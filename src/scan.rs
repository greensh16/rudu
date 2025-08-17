//! File system scanning module for `rudu`.
//!
//! This module handles:
//! - Recursive directory traversal using `WalkDir`
//! - Disk usage measurement using `libc::stat`
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
use crate::cache::{CacheEntry, load_cache, save_cache_with_mtime};
use crate::cli::SortKey;
use crate::data::{EntryType, FileEntry};
use crate::memory::MemoryMonitor;
use crate::metrics::{PhaseResult, PhaseTimer};
use crate::utils::{disk_usage, get_dir_metadata, get_owner, path_hash, sort_entries};
use anyhow::{Context, Result};
use dashmap::DashMap;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use walkdir::WalkDir;

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

/// Lightweight job struct to minimize per-entry allocation during parallel processing
#[derive(Debug)]
struct ScanJob {
    path: PathBuf,
    is_file: bool,
    size: u64,
    parent_paths: Vec<PathBuf>,
}

/// Execute a closure with a local thread pool if threads are specified, otherwise use global pool
fn with_thread_pool<F, R>(args: &Args, f: F) -> Result<R>
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    if let Some(n_threads) = args.threads {
        // Use local thread pool to avoid global contention
        let builder = rayon::ThreadPoolBuilder::new().num_threads(n_threads);

        // For work-stealing with uneven trees, we'll handle the optimization in the scan function
        // The main work-stealing logic is implemented in scan_with_work_stealing()

        let pool = builder
            .build()
            .context("Failed to create local thread pool")?;

        Ok(pool.install(f))
    } else {
        // Use global thread pool
        Ok(f())
    }
}

/// Scans a directory using work-stealing for large subdirectories
fn scan_with_work_stealing(
    root: &Path,
    args: &Args,
    exclude_matcher: &globset::GlobSet,
    sort_key: SortKey,
) -> Result<ScanResult> {
    use rayon::prelude::*;

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner} Scanning files with work-stealing... [{elapsed}]")
            .context("Failed to set progress template")?,
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let dir_totals: DashMap<PathBuf, u64> = DashMap::new();
    let directory_children: DashMap<PathBuf, u64> = DashMap::new();

    // Use scope to spawn tasks for large directories
    let all_entries: Vec<FileEntry> = rayon::scope(|scope| {
        // First pass: collect all entries and identify large directories
        let walker_entries: Vec<_> = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                !exclude_matcher.is_match(e.path())
                    && !e
                        .path()
                        .components()
                        .any(|c| args.exclude.iter().any(|x| c.as_os_str() == OsStr::new(x)))
            })
            .filter_map(|e| {
                pb.tick();
                e.ok()
            })
            .collect();

        // Group entries by their parent directory to count entries per directory
        let mut dir_entry_counts: std::collections::HashMap<PathBuf, usize> =
            std::collections::HashMap::new();
        for entry in &walker_entries {
            if let Some(parent) = entry.path().parent() {
                *dir_entry_counts.entry(parent.to_path_buf()).or_insert(0) += 1;
            }
        }

        // Identify large directories (> 10,000 entries)
        let large_dirs: Vec<_> = dir_entry_counts
            .iter()
            .filter(|&(_, count)| *count > 10_000)
            .map(|(path, _)| path.clone())
            .collect();

        println!(
            "🔍 Found {} large directories (>10k entries) to process with work-stealing",
            large_dirs.len()
        );

        // Process large directories as separate tasks
        for large_dir in large_dirs {
            let large_dir_entries: Vec<_> = walker_entries
                .iter()
                .filter(|e| e.path().parent() == Some(&large_dir))
                .cloned()
                .collect();

            let dir_totals_ref = &dir_totals;
            let directory_children_ref = &directory_children;
            let args_ref = args;

            scope.spawn(move |_| {
                // Process this large directory in a separate task
                large_dir_entries.par_iter().for_each(|entry| {
                    let path = entry.path().to_path_buf();
                    let is_file = entry.file_type().is_file();

                    if is_file {
                        let size = disk_usage(&path);

                        // Accumulate in parent directories
                        let mut current = path.parent();
                        while let Some(parent_path) = current {
                            dir_totals_ref
                                .entry(parent_path.to_path_buf())
                                .and_modify(|v| *v += size)
                                .or_insert(size);
                            if parent_path == root {
                                break;
                            }
                            current = parent_path.parent();
                        }
                    }

                    // Count for inode tracking
                    if args_ref.show_inodes {
                        if let Some(parent) = path.parent() {
                            *directory_children_ref
                                .entry(parent.to_path_buf())
                                .or_insert(0) += 1;
                        }
                    }
                });
            });
        }

        // Process remaining entries normally
        let remaining_entries: Vec<_> = walker_entries
            .into_iter()
            .filter(|e| {
                if let Some(parent) = e.path().parent() {
                    dir_entry_counts
                        .get(parent)
                        .is_none_or(|count| *count <= 10_000)
                } else {
                    true
                }
            })
            .collect();

        // Process remaining entries in parallel
        remaining_entries.par_iter().for_each(|entry| {
            let path = entry.path().to_path_buf();
            let is_file = entry.file_type().is_file();

            if is_file {
                let size = disk_usage(&path);

                // Accumulate in parent directories
                let mut current = path.parent();
                while let Some(parent_path) = current {
                    dir_totals
                        .entry(parent_path.to_path_buf())
                        .and_modify(|v| *v += size)
                        .or_insert(size);
                    if parent_path == root {
                        break;
                    }
                    current = parent_path.parent();
                }
            }

            // Count for inode tracking
            if args.show_inodes {
                if let Some(parent) = path.parent() {
                    *directory_children.entry(parent.to_path_buf()).or_insert(0) += 1;
                }
            }
        });

        // Create FileEntry objects for all entries
        let all_walker_entries: Vec<_> = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                !exclude_matcher.is_match(e.path())
                    && !e
                        .path()
                        .components()
                        .any(|c| args.exclude.iter().any(|x| c.as_os_str() == OsStr::new(x)))
            })
            .filter_map(|e| e.ok())
            .collect();

        all_walker_entries
            .par_iter()
            .map(|entry| {
                let path = entry.path().to_path_buf();
                let is_file = entry.file_type().is_file();

                if is_file {
                    FileEntry {
                        path: path.clone(),
                        size: disk_usage(&path),
                        owner: if args.show_owner {
                            get_owner(&path)
                        } else {
                            None
                        },
                        inodes: None,
                        entry_type: crate::data::EntryType::File,
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
                        owner: if args.show_owner {
                            get_owner(&path)
                        } else {
                            None
                        },
                        inodes: if args.show_inodes {
                            Some(inode_count)
                        } else {
                            None
                        },
                        entry_type: crate::data::EntryType::Dir,
                    }
                }
            })
            .collect()
    });

    pb.finish_with_message("Work-stealing scan complete");

    let mut final_entries = all_entries;
    sort_entries(&mut final_entries, sort_key);

    Ok(ScanResult {
        entries: final_entries,
        cache_hits: 0,
        cache_total: 0,
        memory_limit_hit: false,
        phase_timings: Vec::new(),
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
/// * `Result<Vec<FileEntry>>` - A vector of file and directory entries on success
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

/// Legacy scanning function (kept for reference)
#[allow(dead_code)]
fn scan_files_and_dirs_legacy(
    root: &Path,
    args: &Args,
    exclude_matcher: &globset::GlobSet,
    sort_key: SortKey,
) -> Result<ScanResult> {
    // Setup a spinner to indicate scanning progress in the terminal
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner} Scanning files... [{elapsed}]")
            .context("Failed to set progress template")?,
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    // Thread-safe maps for directory totals and inode counts
    let dir_totals: DashMap<PathBuf, u64> = DashMap::new();

    // Create a channel for the optimized pipeline
    let (tx, rx) = mpsc::channel::<ScanJob>();

    // Pre-compute parent paths function
    let get_parent_paths = |path: &Path| -> Vec<PathBuf> {
        let mut parents = Vec::new();
        let mut current = path.parent();
        while let Some(parent_path) = current {
            parents.push(parent_path.to_path_buf());
            if parent_path == root {
                break;
            }
            current = parent_path.parent();
        }
        parents
    };

    // Use local thread pool for parallel processing if specified
    let scan_jobs: Vec<ScanJob> = with_thread_pool(args, || {
        // Optimized WalkDir -> channel -> parallel consumer pipeline
        WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                !exclude_matcher.is_match(e.path())
                    && !e
                        .path()
                        .components()
                        .any(|c| args.exclude.iter().any(|x| c.as_os_str() == OsStr::new(x)))
            })
            .filter_map(|e| {
                pb.tick();
                e.ok()
            })
            .par_bridge()
            .for_each_with(tx, |s, entry| {
                let path = entry.path().to_path_buf();
                let is_file = entry.file_type().is_file();
                let size = if is_file { disk_usage(&path) } else { 0 };
                let parent_paths = if is_file {
                    get_parent_paths(&path)
                } else {
                    Vec::new()
                };

                let job = ScanJob {
                    path,
                    is_file,
                    size,
                    parent_paths,
                };

                // Handle channel send failure gracefully instead of panicking
                if s.send(job).is_err() {
                    // Channel receiver dropped, stop processing
                }
            });

        // Collect all scan jobs from the channel
        rx.into_iter().collect()
    })?;

    pb.finish_with_message("Scan complete");

    // Pre-compute inode counts for directories during initial processing
    // This is more efficient than doing separate walkdir calls later
    let directory_children: DashMap<PathBuf, u64> = DashMap::new();

    // Accumulate directory sizes from file scan jobs
    for job in &scan_jobs {
        if job.is_file {
            // Accumulate size in parent directories
            for parent_path in &job.parent_paths {
                dir_totals
                    .entry(parent_path.clone())
                    .and_modify(|v| *v += job.size)
                    .or_insert(job.size);
            }
        }
    }

    if args.show_inodes {
        // Count direct children for each directory
        for job in &scan_jobs {
            if let Some(parent) = job.path.parent() {
                *directory_children.entry(parent.to_path_buf()).or_insert(0) += 1;
            }
        }
    }

    // Process all scan jobs in parallel to create FileEntry objects
    let mut all_entries: Vec<FileEntry> = with_thread_pool(args, || {
        scan_jobs
            .par_iter()
            .map(|job| {
                if job.is_file {
                    FileEntry {
                        path: job.path.clone(),
                        size: job.size,
                        owner: if args.show_owner {
                            get_owner(&job.path)
                        } else {
                            None
                        },
                        inodes: None, // Files don't have inode counts
                        entry_type: EntryType::File,
                    }
                } else {
                    let size = dir_totals.get(&job.path).map(|v| *v).unwrap_or(0);

                    // Use pre-computed inode count from our single-pass scan
                    let inode_count = if args.show_inodes {
                        directory_children.get(&job.path).map(|v| *v).unwrap_or(0)
                    } else {
                        0
                    };

                    FileEntry {
                        path: job.path.clone(),
                        size,
                        owner: if args.show_owner {
                            get_owner(&job.path)
                        } else {
                            None
                        },
                        inodes: if args.show_inodes {
                            Some(inode_count)
                        } else {
                            None
                        },
                        entry_type: EntryType::Dir,
                    }
                }
            })
            .collect()
    })?;

    // Sort the entries based on selected criteria
    sort_entries(&mut all_entries, sort_key);

    Ok(ScanResult {
        entries: all_entries,
        cache_hits: 0,
        cache_total: 0,
        memory_limit_hit: false,
        phase_timings: Vec::new(),
        memory_status: MemoryLimitStatus::Normal,
    })
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
/// 2. For each directory during WalkDir traversal:
///    - Fetch directory metadata (mtime, nlink)
///    - Compare against cached entry
///    - If unchanged, skip walking into subtree and reuse cached values
///    - If changed, perform full scan and update cache
/// 3. Save updated cache to disk
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
        println!("Cache disabled, performing full scan");
        std::collections::HashMap::new()
    } else {
        {
            let cache = load_cache(root, args.cache_ttl);
            if cache.is_empty() {
                println!("📦 No cache found, performing full scan");
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

    // Data structures for aggregating results
    let dir_totals: DashMap<PathBuf, u64> = DashMap::new();
    let directory_children: DashMap<PathBuf, u64> = DashMap::new();
    let mut new_cache_entries: std::collections::HashMap<PathBuf, CacheEntry> =
        std::collections::HashMap::new();
    let cached_dirs: DashMap<PathBuf, CacheEntry> = DashMap::new();

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

            // For directories, check if we can skip based on cache
            if e.file_type().is_dir() && !args.no_cache {
                if let Some(cached_entry) = cache.get(&path.to_path_buf()) {
                    if let Some(current_metadata) = get_dir_metadata(path) {
                        if cached_entry.is_valid(current_metadata.mtime, current_metadata.nlink) {
                            // Cache hit - we can skip this subtree
                            cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                            // Reuse cached aggregated values
                            dir_totals.insert(path.to_path_buf(), cached_entry.size);
                            if let Some(inode_count) = cached_entry.inode_cnt {
                                directory_children.insert(path.to_path_buf(), inode_count);
                            }

                            // Store cached directory info for later FileEntry creation
                            cached_dirs.insert(path.to_path_buf(), cached_entry.clone());

                            // Add to new cache (preserving valid entries)
                            new_cache_entries.insert(path.to_path_buf(), cached_entry.clone());

                            // Also restore any cached subdirectories within this subtree
                            // that would be visible given the depth constraints and current exclusion patterns
                            for (cached_path, cached_subentry) in cache.iter() {
                                // Check if this is a subdirectory of the current path
                                if cached_path.starts_with(path) && cached_path != path {
                                    let sub_depth = crate::utils::path_depth(root, cached_path);

                                    // Apply current exclusion patterns to cached entries
                                    let excluded_by_glob = exclude_matcher.is_match(cached_path);
                                    let excluded_by_component = cached_path.components().any(|c| {
                                        args.exclude.iter().any(|x| c.as_os_str() == OsStr::new(x))
                                    });

                                    // Only include subdirectories that pass all filters
                                    let should_include = match args.depth {
                                        Some(max_depth) => sub_depth <= max_depth,
                                        None => true,
                                    } && !excluded_by_glob
                                        && !excluded_by_component;

                                    if should_include {
                                        cached_dirs
                                            .insert(cached_path.clone(), cached_subentry.clone());
                                        new_cache_entries
                                            .insert(cached_path.clone(), cached_subentry.clone());

                                        // Also restore the size and inode data for subdirectories
                                        dir_totals
                                            .insert(cached_path.clone(), cached_subentry.size);
                                        if let Some(inode_count) = cached_subentry.inode_cnt {
                                            directory_children
                                                .insert(cached_path.clone(), inode_count);
                                        }
                                    }
                                }
                            }

                            pb.tick();
                            return false; // Skip walking into this subtree
                        }
                    }
                }
                cache_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            true
        });

    // Collect entries with memory monitoring
    let mut walker_entries: Vec<walkdir::DirEntry> = Vec::new();
    let mut memory_exceeded = false;

    for entry in walker_iter.flatten() {
        pb.tick();

        // Increment counter and check memory every N entries
        entry_counter += 1;
        if entry_counter % memory_check_interval == 0 {
            if let Some(ref monitor) = monitor {
                if let Ok(mut mem_monitor) = monitor.lock() {
                    if mem_monitor.exceeds_limit() {
                        println!("⚠️  Memory limit exceeded, terminating scan early");
                        memory_exceeded = true;
                        break;
                    } else if !memory_nearing_limit && mem_monitor.nearing_limit() {
                        println!(
                            "⚠️  Memory usage nearing limit, disabling cache and heavy features"
                        );
                        memory_nearing_limit = true;
                        // Disable caching dynamically to reduce memory usage
                        crate::cache::set_enabled(false);
                    }
                }
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
            let is_file = entry.file_type().is_file();
            let size = if is_file { disk_usage(&path) } else { 0 };

            let parent_paths = if is_file {
                let mut parents = Vec::new();
                let mut current = path.parent();
                while let Some(parent_path) = current {
                    parents.push(parent_path.to_path_buf());
                    if parent_path == root {
                        break;
                    }
                    current = parent_path.parent();
                }
                parents
            } else {
                Vec::new()
            };

            ScanJob {
                path,
                is_file,
                size,
                parent_paths,
            }
        })
        .collect();
    phase_timings.push(disk_io_timer.finish());

    // Aggregation phase
    let aggregation_timer = PhaseTimer::new("Aggregation");

    // Accumulate directory sizes from file scan jobs
    for job in &scan_jobs {
        if job.is_file {
            for parent_path in &job.parent_paths {
                dir_totals
                    .entry(parent_path.clone())
                    .and_modify(|v| *v += job.size)
                    .or_insert(job.size);
            }
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
            let (entry, cache_entry) = if job.is_file {
                let entry = FileEntry {
                    path: job.path.clone(),
                    size: job.size,
                    owner: if args.show_owner {
                        get_owner(&job.path)
                    } else {
                        None
                    },
                    inodes: None,
                    entry_type: EntryType::File,
                };
                (entry, None)
            } else {
                let size = dir_totals.get(&job.path).map(|v| *v).unwrap_or(0);
                let inode_count = if args.show_inodes {
                    directory_children.get(&job.path).map(|v| *v).unwrap_or(0)
                } else {
                    0
                };

                // Create cache entry for this directory
                let cache_entry = get_dir_metadata(&job.path).map(|metadata| {
                    CacheEntry::new(
                        path_hash(&job.path),
                        job.path.clone(),
                        size,
                        metadata.mtime,
                        metadata.nlink,
                        if args.show_inodes {
                            Some(inode_count)
                        } else {
                            None
                        },
                        metadata.owner,
                        EntryType::Dir,
                    )
                });

                let entry = FileEntry {
                    path: job.path.clone(),
                    size,
                    owner: if args.show_owner {
                        get_owner(&job.path)
                    } else {
                        None
                    },
                    inodes: if args.show_inodes {
                        Some(inode_count)
                    } else {
                        None
                    },
                    entry_type: EntryType::Dir,
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

    // Add cached directory entries
    let cached_entries_vec: Vec<(PathBuf, CacheEntry)> = cached_dirs
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().clone()))
        .collect();

    let mut cached_entries: Vec<FileEntry> = cached_entries_vec
        .par_iter()
        .map(|(path, cached_entry)| FileEntry {
            path: path.clone(),
            size: cached_entry.size,
            owner: if args.show_owner {
                get_owner(path)
            } else {
                None
            },
            inodes: cached_entry.inode_cnt,
            entry_type: cached_entry.entry_type,
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
        println!(
            "📊 Cache stats: {} hits, {} misses ({}% hit rate)",
            hits,
            misses,
            if hits + misses > 0 {
                hits * 100 / (hits + misses)
            } else {
                0
            }
        );
    }

    // Save updated cache (unless disabled or memory constrained)
    if !args.no_cache && !memory_nearing_limit {
        if let Err(e) = save_cache_with_mtime(root, &new_cache_entries, root_mtime) {
            eprintln!("Failed to save cache: {}", e);
        } else {
            println!("Cache updated with {} entries", new_cache_entries.len());
        }
    } else if memory_nearing_limit {
        println!("⚠️  Cache saving disabled due to memory constraints");
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
