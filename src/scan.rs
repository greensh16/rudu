//! File system scanning module for `rudu`.
//!
//! This module handles:
//! - Recursive directory traversal using `WalkDir`
//! - Disk usage measurement using `libc::lstat` (symlinks never followed, like `du`)
//! - Parallel size aggregation for directories using `DashMap` and `rayon`
//! - Filtering via glob-based exclude rules
//! - Progress spinner via `indicatif`
//!
//! The main entry point is [`scan_files_and_dirs`], which returns:
//! - A unified list of [`FileEntry`] objects containing both files and directories
//! - Each directory entry includes cached inode counts computed during the scan
//! - All entries include precomputed owner information and sizes
//!
//! Sorting behavior is controlled by the [`SortKey`] provided from the CLI.
//!
//! Performance optimizations:
//! - Inode counts are cached during the initial walk to avoid repeated directory traversal
//! - Directory sizes are accumulated efficiently using parent path caching
//! - Single-pass processing reduces memory allocations and improves cache locality

use crate::Args;
use crate::cache::{CacheEntry, CacheEntryParams, load_cache, save_cache_with_mtime};
use crate::cli::SortKey;
use crate::data::{EntryType, FileEntry};
use crate::memory::MemoryMonitor;
use crate::metrics::{PhaseResult, PhaseTimer};
use crate::utils::{get_dir_metadata, resolve_uid, sort_entries};
use anyhow::{Context, Result};
use dashmap::{DashMap, DashSet};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use walkdir::WalkDir;

/// Recursively restores cached entries (directories *and* files) for a
/// directory cache hit.
///
/// Uses an O(depth) descent through a pre-built parent→children index rather
/// than scanning all cache entries per hit.
///
/// No depth filtering happens here: output depth limiting is applied later by
/// `process_entries`, and restoring the full subtree keeps deep entries alive
/// in the saved cache even on `--depth`-limited runs.
#[allow(clippy::too_many_arguments)]
fn restore_subtree(
    path: &Path,
    children_index: &HashMap<PathBuf, Vec<PathBuf>>,
    cache: &HashMap<PathBuf, CacheEntry>,
    show_inodes: bool,
    exclude_matcher: &globset::GlobSet,
    exclude_patterns: &[String],
    dir_totals: &mut HashMap<PathBuf, u64>,
    directory_children: &mut HashMap<PathBuf, u64>,
    restored_entries: &mut HashMap<PathBuf, CacheEntry>,
    new_cache_entries: &mut HashMap<PathBuf, CacheEntry>,
) {
    let children = match children_index.get(path) {
        Some(c) => c,
        None => return,
    };
    for child_path in children {
        if exclude_matcher.is_match(child_path) {
            continue;
        }
        if child_path.components().any(|c| {
            exclude_patterns
                .iter()
                .any(|x| c.as_os_str() == OsStr::new(x))
        }) {
            continue;
        }
        if let Some(cached_subentry) = cache.get(child_path) {
            restored_entries.insert(child_path.clone(), cached_subentry.clone());
            new_cache_entries.insert(child_path.clone(), cached_subentry.clone());
            if cached_subentry.entry_type == EntryType::Dir {
                dir_totals.insert(child_path.clone(), cached_subentry.size);
                if show_inodes && let Some(inode_count) = cached_subentry.inode_cnt {
                    directory_children.insert(child_path.clone(), inode_count);
                }
                restore_subtree(
                    child_path,
                    children_index,
                    cache,
                    show_inodes,
                    exclude_matcher,
                    exclude_patterns,
                    dir_totals,
                    directory_children,
                    restored_entries,
                    new_cache_entries,
                );
            }
        }
    }
}

/// Verifies that every cached *directory* beneath `path` is still structurally
/// unchanged (same mtime and nlink) before a cache hit is trusted.
///
/// A directory's mtime/nlink only reflect changes to its **direct** children,
/// so validating just the top directory would miss files or directories
/// added/removed deeper in the subtree. Statting every cached directory in the
/// subtree (but no files) catches structural changes at any depth, at a small
/// fraction of the cost of a full rescan (directories are typically a 10-20x
/// minority of entries, and no readdir is needed).
///
/// Known limitation: an in-place modification of an existing file changes no
/// directory mtime and therefore cannot be detected by any directory-level
/// check; such changes are only picked up when the cache TTL expires or
/// `--no-cache` is used.
fn validate_cached_subtree(
    path: &Path,
    children_index: &HashMap<PathBuf, Vec<PathBuf>>,
    cache: &HashMap<PathBuf, CacheEntry>,
    exclude_matcher: &globset::GlobSet,
    exclude_patterns: &[String],
) -> bool {
    let children = match children_index.get(path) {
        Some(c) => c,
        None => return true,
    };
    for child_path in children {
        if exclude_matcher.is_match(child_path) {
            continue;
        }
        if child_path.components().any(|c| {
            exclude_patterns
                .iter()
                .any(|x| c.as_os_str() == OsStr::new(x))
        }) {
            continue;
        }
        if let Some(cached) = cache.get(child_path)
            && cached.entry_type == EntryType::Dir
        {
            match get_dir_metadata(child_path) {
                Some(meta) if cached.is_valid(meta.mtime, meta.nlink) => {
                    if !validate_cached_subtree(
                        child_path,
                        children_index,
                        cache,
                        exclude_matcher,
                        exclude_patterns,
                    ) {
                        return false;
                    }
                }
                // Directory deleted, inaccessible, or changed — reject the hit.
                _ => return false,
            }
        }
    }
    true
}

/// Memory limit status for scanning operations
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryLimitStatus {
    /// Scan completed normally without memory pressure
    Normal,
    /// Scan completed but was nearing memory limit (disabled some features)
    NearingLimit,
    /// Scan was terminated due to memory limit being exceeded
    MemoryLimitHit,
}

/// Result of a scan operation including entries and cache statistics
#[derive(Debug)]
pub struct ScanResult {
    pub entries: Vec<FileEntry>,
    pub cache_hits: u64,
    pub cache_total: u64,
    pub memory_limit_hit: bool,
    pub phase_timings: Vec<PhaseResult>,
    #[allow(dead_code)]
    pub memory_status: MemoryLimitStatus,
}

impl Default for ScanResult {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            cache_hits: 0,
            cache_total: 0,
            memory_limit_hit: false,
            phase_timings: Vec::new(),
            memory_status: MemoryLimitStatus::Normal,
        }
    }
}

/// Reads the atime out of a stat result, or `None` when the stat failed.
///
/// Callers gate on `--show-atime` themselves; this only unwraps.
fn atime_of(meta: &Option<crate::utils::DirMetadata>) -> Option<u64> {
    meta.as_ref().map(|m| m.atime)
}

/// Lightweight job struct to minimize per-entry allocation during parallel processing
#[derive(Debug)]
struct ScanJob {
    path: PathBuf,
    /// True only for real directories. Everything else — regular files,
    /// symlinks, FIFOs, sockets, device nodes — is a leaf entry; symlinks and
    /// special files were previously misclassified as directories.
    is_dir: bool,
    /// For leaves: the entry's own disk usage. For directories: the
    /// directory's *own* blocks (its children are aggregated separately).
    size: u64,
    /// Metadata captured with the same single `lstat` used for `size`; reused
    /// for cache-entry creation, owner resolution, and hard-link detection.
    /// (Ancestor paths are walked on the fly during aggregation rather than
    /// materialised per entry — the previous per-file `Vec<PathBuf>` of every
    /// ancestor was the largest single allocation of the scan.)
    meta: Option<crate::utils::DirMetadata>,
}

/// Scans a directory using work-stealing for large subdirectories.
///
/// Properties:
/// - Single WalkDir traversal: `walker_entries` is collected once and reused for both
///   the accumulation phase and the FileEntry construction phase.
/// - Single `lstat` call per entry: sizes and owner UIDs are captured during the
///   accumulation scope and read back when building FileEntry objects.
/// - The rayon scope is used exclusively for accumulation; FileEntry construction runs
///   after the scope exits (guaranteeing all accumulation tasks are complete).
///
/// Note: unlike the default incremental path, this experimental strategy does
/// not use the cache and does not support memory monitoring.
fn scan_with_work_stealing(
    root: &Path,
    args: &Args,
    exclude_matcher: &globset::GlobSet,
    sort_key: SortKey,
) -> Result<ScanResult> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner} Scanning files with work-stealing... [{elapsed}]")
            .context("Failed to set progress template")?,
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let mut phase_timings = Vec::new();
    let walkdir_timer = PhaseTimer::new("WalkDir");

    // Single WalkDir pass — reused for both accumulation and FileEntry creation.
    let walker_entries: Vec<walkdir::DirEntry> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            !exclude_matcher.is_match(e.path())
                && !e
                    .path()
                    .components()
                    .any(|c| args.exclude.iter().any(|x| c.as_os_str() == OsStr::new(x)))
        })
        // No manual tick per entry: enable_steady_tick already animates the
        // spinner from its own thread, and each tick is an atomic RMW on the
        // walk thread (PARALLELISM_REVIEW.md P8).
        .filter_map(|e| e.ok())
        .collect();

    phase_timings.push(walkdir_timer.finish());
    let accumulation_timer = PhaseTimer::new("Aggregation");

    // Group entries by their immediate parent to identify large directories.
    let mut dir_entry_counts: HashMap<PathBuf, usize> = HashMap::new();
    for entry in &walker_entries {
        if let Some(parent) = entry.path().parent() {
            *dir_entry_counts.entry(parent.to_path_buf()).or_insert(0) += 1;
        }
    }

    // Collect the set of large-directory paths (> 10,000 direct children).
    let large_dirs: std::collections::HashSet<PathBuf> = dir_entry_counts
        .iter()
        .filter(|(_, count)| **count > 10_000)
        .map(|(path, _)| path.clone())
        .collect();

    eprintln!(
        "🔍 Found {} large directories (>10k entries) to process with work-stealing",
        large_dirs.len()
    );

    // Accumulation maps — populated during the scope, read after it exits.
    let dir_totals: DashMap<PathBuf, u64> = DashMap::new();
    let directory_children: DashMap<PathBuf, u64> = DashMap::new();
    // Per-leaf sizes stored here so we never lstat the same entry twice.
    let file_sizes: DashMap<PathBuf, u64> = DashMap::new();
    // Owner UIDs captured from the same lstat, so --show-owner does not need
    // a second syscall per entry when building FileEntry objects.
    let entry_owners: DashMap<PathBuf, u32> = DashMap::new();
    // Access times from that same lstat, for --show-atime / --older-than.
    let entry_atimes: DashMap<PathBuf, u64> = DashMap::new();
    // Inodes of multi-link files already counted in totals (hard-link dedup).
    let seen_inodes: DashSet<(u64, u64)> = DashSet::new();

    // Shared accumulation logic for both the large-directory tasks and the
    // remaining-entries pass. A single lstat per entry supplies size, owner,
    // and inode identity; symlinks report their own size (never the target's);
    // each hard-linked inode contributes to totals exactly once; and each
    // directory's own blocks count toward its total, all matching `du`.
    let accumulate = |entry: &walkdir::DirEntry| {
        let path = entry.path().to_path_buf();
        let meta = get_dir_metadata(&path);
        let size = meta.as_ref().map(|m| m.size).unwrap_or(0);

        if args.show_owner
            && let Some(uid) = meta.as_ref().and_then(|m| m.owner)
        {
            entry_owners.insert(path.clone(), uid);
        }

        if let Some(at) = meta.as_ref().map(|m| m.atime) {
            entry_atimes.insert(path.clone(), at);
        }

        let is_dir = entry.file_type().is_dir();
        let counts_toward_totals = if is_dir {
            // The directory's own blocks count toward its own total…
            dir_totals
                .entry(path.clone())
                .and_modify(|v| *v += size)
                .or_insert(size);
            // …and propagate to ancestors below (unless this is the root).
            path != root
        } else {
            file_sizes.insert(path.clone(), size);
            match meta.as_ref() {
                // insert() returns false if another link of this inode was
                // already counted by a concurrent task.
                Some(m) if m.nlink > 1 => seen_inodes.insert((m.dev, m.ino)),
                _ => true,
            }
        };

        if counts_toward_totals {
            let mut cur = path.parent();
            while let Some(p) = cur {
                dir_totals
                    .entry(p.to_path_buf())
                    .and_modify(|v| *v += size)
                    .or_insert(size);
                if p == root {
                    break;
                }
                cur = p.parent();
            }
        }

        if args.show_inodes
            && let Some(parent) = path.parent()
        {
            *directory_children.entry(parent.to_path_buf()).or_insert(0) += 1;
        }
    };

    // Group large-directory entries by index in a single O(n) pass —
    // previously every large directory re-scanned (and cloned from) the whole
    // walker_entries vec, an O(large_dirs × n) pattern with duplicated
    // DirEntry allocations.
    let mut large_dir_groups: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (idx, entry) in walker_entries.iter().enumerate() {
        if let Some(parent) = entry.path().parent()
            && large_dirs.contains(parent)
        {
            large_dir_groups
                .entry(parent.to_path_buf())
                .or_default()
                .push(idx);
        }
    }

    // Accumulation phase: the scope guarantees all spawned tasks complete before we
    // proceed to FileEntry construction, so dir_totals / file_sizes are fully populated.
    rayon::scope(|scope| {
        // Spawn a task per large directory so its entries are processed in parallel
        // with the "remaining" par_iter below, using rayon's work-stealing scheduler.
        for indices in large_dir_groups.values() {
            let accumulate_ref = &accumulate;
            let walker_entries_ref = &walker_entries;
            scope.spawn(move |_| {
                indices
                    .par_iter()
                    .for_each(|&idx| accumulate_ref(&walker_entries_ref[idx]));
            });
        }

        // Process the remaining entries (those not in large directories) in parallel.
        // This runs concurrently with the scope.spawn'd tasks above via work-stealing.
        // (A named reference: `accumulate` itself cannot be moved here because the
        // spawned tasks above still borrow it for the lifetime of the scope.)
        let accumulate_ref = &accumulate;
        walker_entries
            .par_iter()
            .filter(|e| {
                e.path()
                    .parent()
                    .map(|p| !large_dirs.contains(p))
                    .unwrap_or(true)
            })
            .for_each(accumulate_ref);
    });

    phase_timings.push(accumulation_timer.finish());

    pb.finish_with_message("Work-stealing scan complete");

    // Build FileEntry objects from the already-collected walker_entries.
    // Sizes and owners come from the accumulation pass — no second lstat call.
    let mut final_entries: Vec<FileEntry> = walker_entries
        .par_iter()
        .map(|entry| {
            let path = entry.path().to_path_buf();
            // Copy the UID out before resolving so the DashMap shard guard is
            // released before resolve_uid takes the UID-cache mutex.
            let owner_uid = if args.show_owner {
                entry_owners.get(&path).map(|v| *v)
            } else {
                None
            };
            let owner = owner_uid.map(resolve_uid);
            let atime = entry_atimes.get(&path).map(|v| *v);
            // Only real directories are Dir entries; symlinks and special
            // files are leaf entries (previously misclassified as Dir).
            if !entry.file_type().is_dir() {
                FileEntry {
                    path: path.clone(),
                    size: file_sizes.get(&path).map(|v| *v).unwrap_or(0),
                    owner,
                    inodes: None,
                    entry_type: EntryType::File,
                    atime,
                    at_risk_bytes: None,
                }
            } else {
                let size = dir_totals.get(&path).map(|v| *v).unwrap_or(0);
                let inode_count = if args.show_inodes {
                    directory_children.get(&path).map(|v| *v).unwrap_or(0)
                } else {
                    0
                };
                FileEntry {
                    path: path.clone(),
                    size,
                    owner,
                    inodes: if args.show_inodes {
                        Some(inode_count)
                    } else {
                        None
                    },
                    entry_type: EntryType::Dir,
                    // Replaced by the subtree rollup in `atime::apply_rollup`.
                    atime,
                    at_risk_bytes: None,
                }
            }
        })
        .collect();

    sort_entries(&mut final_entries, sort_key);

    Ok(ScanResult {
        entries: final_entries,
        cache_hits: 0,
        cache_total: 0,
        memory_limit_hit: false,
        phase_timings,
        memory_status: MemoryLimitStatus::Normal,
    })
}

/// Recursively scans a directory tree and returns a list of file and directory entries.
///
/// This function performs a comprehensive file system scan, including:
/// - Recursive directory traversal
/// - Disk usage calculation for files and directories
/// - Owner information resolution
/// - Inode count calculation for directories
/// - Filtering based on exclusion patterns
/// - Sorting by name or size
///
/// # Arguments
/// * `root` - The root path to start scanning from
/// * `args` - Command line arguments controlling scan behavior
/// * `exclude_matcher` - Compiled glob patterns for excluding files/directories
/// * `sort_key` - How to sort the resulting entries (by name or size)
///
/// # Returns
/// * `Result<ScanResult>` - Scan entries plus cache statistics on success
///
/// # Errors
/// Returns an error if:
/// - The root path is inaccessible
/// - Progress bar template configuration fails
/// - File system access errors occur during scanning
pub fn scan_files_and_dirs(
    root: &Path,
    args: &Args,
    exclude_matcher: &globset::GlobSet,
    sort_key: SortKey,
) -> Result<ScanResult> {
    // Use work-stealing strategy for uneven trees if selected
    if args.threads_strategy == crate::thread_pool::ThreadPoolStrategy::WorkStealingUneven {
        return scan_with_work_stealing(root, args, exclude_matcher, sort_key);
    }

    // Use incremental scanning by default (unless work-stealing is selected)
    scan_files_and_dirs_incremental(root, args, exclude_matcher, sort_key)
}

/// Scan files and directories with memory monitoring support
///
/// This function accepts an optional memory monitor that will check memory usage
/// during the scan and adjust behavior accordingly:
/// - When nearing the limit: disables caching and other memory-heavy features
/// - When exceeding the limit: terminates the scan early and returns partial results
///
/// # Arguments
/// * `root` - The root path to start scanning from
/// * `args` - Command line arguments controlling scan behavior
/// * `exclude_matcher` - Compiled glob patterns for excluding files/directories
/// * `sort_key` - How to sort the resulting entries (by name or size)
/// * `monitor` - Optional memory monitor for limiting memory usage
///
/// # Returns
/// * `Result<ScanResult>` - Scan results with memory status information
pub fn scan_files_and_dirs_with_memory_monitor(
    root: &Path,
    args: &Args,
    exclude_matcher: &globset::GlobSet,
    sort_key: SortKey,
    monitor: Option<Arc<Mutex<MemoryMonitor>>>,
) -> Result<ScanResult> {
    scan_files_and_dirs_with_monitor(root, args, exclude_matcher, sort_key, monitor)
}

/// Incremental scanning with caching support
///
/// This function implements the incremental scanning algorithm:
/// 1. Load existing cache if available and not disabled
/// 2. For each directory during WalkDir traversal (except the root, which is
///    always rescanned):
///    - Fetch directory metadata (mtime, nlink) and compare against the cache
///    - If unchanged AND every cached directory beneath it is also unchanged,
///      skip walking the subtree and restore its cached dirs and files
///    - Otherwise, perform a full scan of the subtree and update the cache
/// 3. Propagate cached subtree totals to ancestor directories
/// 4. Save updated cache (directories and files) to disk
///
/// Limitation: in-place file modifications do not change any directory mtime
/// and are only detected after the cache TTL expires or with `--no-cache`.
pub fn scan_files_and_dirs_incremental(
    root: &Path,
    args: &Args,
    exclude_matcher: &globset::GlobSet,
    sort_key: SortKey,
) -> Result<ScanResult> {
    scan_files_and_dirs_with_monitor(root, args, exclude_matcher, sort_key, None)
}

/// Incremental scanning with optional memory monitoring
///
/// This is the main implementation that supports memory monitoring.
fn scan_files_and_dirs_with_monitor(
    root: &Path,
    args: &Args,
    exclude_matcher: &globset::GlobSet,
    sort_key: SortKey,
    monitor: Option<Arc<Mutex<MemoryMonitor>>>,
) -> Result<ScanResult> {
    let mut phase_timings = Vec::new();

    // Capture root mtime before any directory modifications
    let root_mtime = crate::cache::model::get_root_mtime(root);

    // Cache loading phase
    let cache_timer = PhaseTimer::new("Cache-load");
    let cache = if args.no_cache {
        eprintln!("Cache disabled, performing full scan");
        std::collections::HashMap::new()
    } else {
        {
            let cache = load_cache(root, args.cache_ttl);
            if cache.is_empty() {
                eprintln!("📦 No cache found, performing full scan");
            }
            cache
        }
    };
    phase_timings.push(cache_timer.finish());

    let cache_hits = std::sync::atomic::AtomicUsize::new(0);
    let cache_misses = std::sync::atomic::AtomicUsize::new(0);

    // Setup progress spinner
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner} Incremental scan in progress... [{elapsed}]")
            .context("Failed to set progress template")?,
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    // Data structures for aggregating results.
    //
    // Plain HashMaps, deliberately: every write happens on the single-threaded
    // walk or aggregation phase, and the only concurrent access is read-only
    // from the FileEntry-construction par_iter (&HashMap is Sync). DashMaps
    // here paid sharding and per-shard locking on the hottest sequential loops
    // for zero concurrency benefit (PARALLELISM_REVIEW.md P4).
    let mut dir_totals: HashMap<PathBuf, u64> = HashMap::new();
    let mut directory_children: HashMap<PathBuf, u64> = HashMap::new();
    let mut new_cache_entries: HashMap<PathBuf, CacheEntry> = HashMap::new();
    // Entries (dirs and files) restored from cache-hit subtrees.
    let mut restored_entries: HashMap<PathBuf, CacheEntry> = HashMap::new();

    // Memory monitoring state
    let mut memory_nearing_limit = false;
    let mut entry_counter = 0;
    // Calculate check interval based on CLI setting - check more frequently if interval is shorter
    let memory_check_interval: usize = if args.memory_check_interval_ms <= 100 {
        500 // Very frequent checks for short intervals
    } else if args.memory_check_interval_ms <= 200 {
        1000 // Normal interval for default setting
    } else {
        2000 // Less frequent checks for longer intervals to reduce overhead
    };

    // WalkDir phase
    let walkdir_timer = PhaseTimer::new("WalkDir");

    // Pre-build parent → children index so that subtree restoration on a cache hit is O(n)
    // overall rather than O(n×k) (iterating all cache entries for each hit).
    let mut children_index: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for cached_path in cache.keys() {
        if let Some(parent) = cached_path.parent() {
            children_index
                .entry(parent.to_path_buf())
                .or_default()
                .push(cached_path.clone());
        }
    }

    let walker_iter = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let path = e.path();

            // Apply exclusion filters
            if exclude_matcher.is_match(path) {
                return false;
            }

            if path
                .components()
                .any(|c| args.exclude.iter().any(|x| c.as_os_str() == OsStr::new(x)))
            {
                return false;
            }

            // For directories, check if we can skip based on cache.
            //
            // The root itself is never treated as a cache hit: a root-level hit
            // would skip the entire walk and freeze results on stale data, since
            // the root's mtime/nlink do not change when anything deeper than its
            // direct children changes.
            if e.file_type().is_dir() && !args.no_cache && path != root {
                // A hit requires the directory itself to be unchanged AND
                // every cached directory beneath it to be unchanged — a
                // directory's own mtime/nlink say nothing about deeper
                // structural changes.
                if let Some(cached_entry) = cache.get(&path.to_path_buf())
                    && let Some(current_metadata) = get_dir_metadata(path)
                    && cached_entry.is_valid(current_metadata.mtime, current_metadata.nlink)
                    && validate_cached_subtree(
                        path,
                        &children_index,
                        &cache,
                        exclude_matcher,
                        &args.exclude,
                    )
                {
                    // Cache hit - we can skip this subtree
                    cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    // Reuse cached aggregated values
                    dir_totals.insert(path.to_path_buf(), cached_entry.size);
                    if args.show_inodes
                        && let Some(inode_count) = cached_entry.inode_cnt
                    {
                        directory_children.insert(path.to_path_buf(), inode_count);
                    }

                    // Store cached entry info for later FileEntry creation
                    restored_entries.insert(path.to_path_buf(), cached_entry.clone());

                    // Add to new cache (preserving valid entries)
                    new_cache_entries.insert(path.to_path_buf(), cached_entry.clone());

                    // Restore cached subtree entries (dirs and files) using
                    // the pre-built children_index.
                    restore_subtree(
                        path,
                        &children_index,
                        &cache,
                        args.show_inodes,
                        exclude_matcher,
                        &args.exclude,
                        &mut dir_totals,
                        &mut directory_children,
                        &mut restored_entries,
                        &mut new_cache_entries,
                    );

                    // Files under this subtree are never walked, so the
                    // cached total must be propagated to every ancestor up
                    // to the root — ancestor totals are otherwise built
                    // only from walked files.
                    let mut cur = path.parent();
                    while let Some(p) = cur {
                        dir_totals
                            .entry(p.to_path_buf())
                            .and_modify(|v| *v += cached_entry.size)
                            .or_insert(cached_entry.size);
                        if p == root {
                            break;
                        }
                        cur = p.parent();
                    }

                    // The skipped directory itself is also invisible to the
                    // aggregation pass — count it as a child of its parent.
                    if args.show_inodes
                        && let Some(parent) = path.parent()
                    {
                        *directory_children.entry(parent.to_path_buf()).or_insert(0) += 1;
                    }

                    return false; // Skip walking into this subtree
                }
                cache_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            true
        });

    // Collect entries with memory monitoring
    let mut walker_entries: Vec<walkdir::DirEntry> = Vec::new();
    let mut memory_exceeded = false;

    // No manual tick per entry: enable_steady_tick already animates the
    // spinner from its own thread (PARALLELISM_REVIEW.md P8).
    for entry in walker_iter.flatten() {
        // Increment counter and check memory every N entries
        entry_counter += 1;
        if entry_counter % memory_check_interval == 0
            && let Some(ref monitor) = monitor
            && let Ok(mut mem_monitor) = monitor.lock()
        {
            if mem_monitor.exceeds_limit() {
                eprintln!("⚠️  Memory limit exceeded, terminating scan early");
                memory_exceeded = true;
                break;
            } else if !memory_nearing_limit && mem_monitor.nearing_limit() {
                eprintln!("⚠️  Memory usage nearing limit, disabling cache and heavy features");
                memory_nearing_limit = true;
                // Disable caching dynamically to reduce memory usage
                crate::cache::set_enabled(false);
            }
        }

        walker_entries.push(entry);
    }

    phase_timings.push(walkdir_timer.finish());

    // Disk I/O phase - process entries that weren't cached
    let disk_io_timer = PhaseTimer::new("Disk-usage I/O");
    let scan_jobs: Vec<ScanJob> = walker_entries
        .par_iter()
        .map(|entry| {
            let path = entry.path().to_path_buf();
            let is_dir = entry.file_type().is_dir();
            // One lstat per entry: size, mtime, nlink, owner, and inode identity
            // all come from the same call and are reused for cache entries,
            // owner display, and hard-link deduplication. Directories record
            // their own blocks so totals include them, as `du` does.
            let meta = get_dir_metadata(&path);
            let size = meta.as_ref().map(|m| m.size).unwrap_or(0);

            ScanJob {
                path,
                is_dir,
                size,
                meta,
            }
        })
        .collect();
    phase_timings.push(disk_io_timer.finish());

    // Aggregation phase
    let aggregation_timer = PhaseTimer::new("Aggregation");

    // Accumulate directory totals from scan jobs. Leaf sizes propagate to all
    // ancestors; each directory also contributes its *own* blocks to its total
    // and its ancestors', matching `du` (previously only file sizes were
    // counted, so every directory inode was missing from the numbers).
    //
    // Hard-link deduplication: a file with st_nlink > 1 reachable via several
    // paths inside the tree must only be counted once in totals, as `du` does.
    // (Links that straddle a cache-hit boundary cannot be deduplicated — the
    // cached subtree's total is a pre-aggregated number; this matches the
    // scan-order dependence `du` itself has.)
    let mut seen_inodes: std::collections::HashSet<(u64, u64)> = std::collections::HashSet::new();
    for job in &scan_jobs {
        if !job.is_dir {
            if let Some(meta) = &job.meta
                && meta.nlink > 1
                && !seen_inodes.insert((meta.dev, meta.ino))
            {
                continue; // Same inode already counted via another link
            }
        } else {
            // The directory's own blocks count toward its own total…
            dir_totals
                .entry(job.path.clone())
                .and_modify(|v| *v += job.size)
                .or_insert(job.size);
            // …and the root's own blocks have no in-tree ancestors.
            if job.path == root {
                continue;
            }
        }
        // Propagate the entry's size to every ancestor up to the root,
        // walking the parent chain in place (no per-entry Vec allocation).
        let mut cur = job.path.parent();
        while let Some(parent_path) = cur {
            dir_totals
                .entry(parent_path.to_path_buf())
                .and_modify(|v| *v += job.size)
                .or_insert(job.size);
            if parent_path == root {
                break;
            }
            cur = parent_path.parent();
        }
    }

    // Count children for inode tracking - skip if memory nearing limit to save memory
    if args.show_inodes && !memory_nearing_limit {
        for job in &scan_jobs {
            if let Some(parent) = job.path.parent() {
                *directory_children.entry(parent.to_path_buf()).or_insert(0) += 1;
            }
        }
    }

    // Create FileEntry objects from scan jobs and collect cache entries
    let scanned_entries: Vec<(FileEntry, Option<CacheEntry>)> = scan_jobs
        .par_iter()
        .map(|job| {
            let owner = if args.show_owner {
                job.meta.as_ref().and_then(|m| m.owner).map(resolve_uid)
            } else {
                None
            };

            let (entry, cache_entry) = if !job.is_dir {
                // Leaf entry: regular file, symlink, FIFO, socket, or device.
                let entry = FileEntry {
                    path: job.path.clone(),
                    size: job.size,
                    owner,
                    inodes: None,
                    entry_type: EntryType::File,
                    atime: atime_of(&job.meta),
                    at_risk_bytes: None,
                };

                // Cache file entries too, so that cache-hit subtrees can restore
                // their files and output stays identical across runs.
                let cache_entry = job.meta.as_ref().map(|metadata| {
                    CacheEntry::new(CacheEntryParams {
                        path: job.path.clone(),
                        size: job.size,
                        mtime: metadata.mtime,
                        nlink: metadata.nlink,
                        inode_cnt: None,
                        owner: metadata.owner,
                        entry_type: EntryType::File,
                        // Always cached, even without --show-atime: otherwise a
                        // cache built by a plain run would leave a later
                        // --show-atime run blind over every hit subtree.
                        atime: Some(metadata.atime),
                    })
                });

                (entry, cache_entry)
            } else {
                let size = dir_totals.get(&job.path).copied().unwrap_or(0);
                let inode_count = if args.show_inodes {
                    directory_children.get(&job.path).copied().unwrap_or(0)
                } else {
                    0
                };

                // Create cache entry for this directory
                let cache_entry = job.meta.as_ref().map(|metadata| {
                    CacheEntry::new(CacheEntryParams {
                        path: job.path.clone(),
                        size,
                        mtime: metadata.mtime,
                        nlink: metadata.nlink,
                        inode_cnt: if args.show_inodes {
                            Some(inode_count)
                        } else {
                            None
                        },
                        owner: metadata.owner,
                        entry_type: EntryType::Dir,
                        atime: Some(metadata.atime),
                    })
                });

                let entry = FileEntry {
                    path: job.path.clone(),
                    size,
                    owner,
                    inodes: if args.show_inodes {
                        Some(inode_count)
                    } else {
                        None
                    },
                    entry_type: EntryType::Dir,
                    // The directory's *own* atime, which this very scan just
                    // updated by reading it. `atime::apply_rollup` overwrites
                    // it with the oldest leaf beneath, keeping this value only
                    // for directories that contain no leaves at all.
                    atime: atime_of(&job.meta),
                    at_risk_bytes: None,
                };

                (entry, cache_entry)
            };

            (entry, cache_entry)
        })
        .collect();

    // Separate entries and cache entries
    let mut file_entries: Vec<FileEntry> = Vec::new();
    for (entry, cache_entry) in scanned_entries {
        let path = entry.path.clone();
        file_entries.push(entry);
        if let Some(cache_entry) = cache_entry {
            new_cache_entries.insert(path, cache_entry);
        }
    }

    // Add entries restored from cache-hit subtrees (dirs and files).
    // Owner comes from the cached UID — no re-stat of skipped paths.
    let cached_entries_vec: Vec<(PathBuf, CacheEntry)> = restored_entries
        .iter()
        .map(|(path, entry)| (path.clone(), entry.clone()))
        .collect();

    let mut cached_entries: Vec<FileEntry> = cached_entries_vec
        .par_iter()
        .map(|(path, cached_entry)| FileEntry {
            path: path.clone(),
            size: cached_entry.size,
            owner: if args.show_owner {
                cached_entry.owner.map(resolve_uid)
            } else {
                None
            },
            inodes: cached_entry.inode_cnt,
            entry_type: cached_entry.entry_type,
            // Conservative by construction: a cached atime can only be older
            // than the truth, so it over-states age and never hides risk.
            atime: cached_entry.atime,
            at_risk_bytes: None,
        })
        .collect();

    // Combine scanned and cached entries
    let mut all_entries = file_entries;
    all_entries.append(&mut cached_entries);

    phase_timings.push(aggregation_timer.finish());

    pb.finish_with_message("Incremental scan complete");

    // Print cache statistics
    let hits = cache_hits.load(std::sync::atomic::Ordering::Relaxed);
    let misses = cache_misses.load(std::sync::atomic::Ordering::Relaxed);
    if hits > 0 || misses > 0 {
        eprintln!(
            "📊 Cache stats: {} hits, {} misses ({}% hit rate)",
            hits,
            misses,
            (hits * 100).checked_div(hits + misses).unwrap_or(0)
        );
    }

    // Save updated cache (unless disabled or memory constrained).
    // A scan terminated early by the memory limit is incomplete — persisting it
    // would poison future runs with truncated directory totals.
    if !args.no_cache && !memory_nearing_limit && !memory_exceeded {
        if let Err(e) = save_cache_with_mtime(root, &new_cache_entries, root_mtime) {
            eprintln!("Failed to save cache: {}", e);
        } else {
            eprintln!("Cache updated with {} entries", new_cache_entries.len());
        }
    } else if memory_nearing_limit || memory_exceeded {
        eprintln!("⚠️  Cache saving disabled due to memory constraints");
    }

    // Sort and return results
    sort_entries(&mut all_entries, sort_key);
    let cache_hits_val = hits;
    let cache_total_val = hits + misses;

    // Determine memory status based on what happened during scan
    let memory_status = if memory_exceeded {
        MemoryLimitStatus::MemoryLimitHit
    } else if memory_nearing_limit {
        MemoryLimitStatus::NearingLimit
    } else {
        MemoryLimitStatus::Normal
    };

    Ok(ScanResult {
        entries: all_entries,
        cache_hits: cache_hits_val as u64,
        cache_total: cache_total_val as u64,
        memory_limit_hit: memory_exceeded,
        phase_timings,
        memory_status,
    })
}
