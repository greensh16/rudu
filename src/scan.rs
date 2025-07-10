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

use crate::cli::SortKey;
use crate::data::{EntryType, FileEntry};
use crate::utils::{disk_usage, get_owner, sort_entries};
use crate::Args;
use anyhow::{Context, Result};
use dashmap::DashMap;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::{DirEntry, WalkDir};

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
) -> Result<Vec<FileEntry>> {
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

    // Cache parent paths to avoid repeated allocations
    let parent_cache: DashMap<PathBuf, Vec<PathBuf>> = DashMap::new();

    // Collect all entries and process them in a single pass
    let entries: Vec<DirEntry> = WalkDir::new(root)
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

    pb.finish_with_message("Scan complete ✅");

    // Pre-compute inode counts for directories during initial processing
    // This is more efficient than doing separate walkdir calls later
    let directory_children: DashMap<PathBuf, u64> = DashMap::new();

    if args.show_inodes {
        // Count direct children for each directory
        for entry in &entries {
            if let Some(parent) = entry.path().parent() {
                *directory_children.entry(parent.to_path_buf()).or_insert(0) += 1;
            }
        }
    }

    // Separate files and directories for parallel processing
    let files: Vec<&DirEntry> = entries.iter().filter(|e| e.file_type().is_file()).collect();
    let dirs: Vec<&DirEntry> = entries.iter().filter(|e| e.file_type().is_dir()).collect();

    // Pre-compute parent paths for all files to avoid repeated traversal
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

    // Compute disk usage for each file and accumulate directory totals
    let file_entries: Vec<FileEntry> = files
        .par_iter()
        .map(|entry| {
            let path = entry.path().to_path_buf();
            let size = disk_usage(&path);

            // Use cached parent paths or compute and cache them
            let parent_paths = parent_cache
                .entry(path.clone())
                .or_insert_with(|| get_parent_paths(&path))
                .clone();

            // Accumulate size in parent directories
            for parent_path in parent_paths {
                dir_totals
                    .entry(parent_path)
                    .and_modify(|v| *v += size)
                    .or_insert(size);
            }

            FileEntry {
                path,
                size,
                owner: if args.show_owner {
                    get_owner(entry.path())
                } else {
                    None
                },
                inodes: None, // Files don't have inode counts
                entry_type: EntryType::File,
            }
        })
        .collect();

    // Create directory entries with cached inode counts
    let dir_entries: Vec<FileEntry> = dirs
        .par_iter()
        .map(|entry| {
            let path = entry.path().to_path_buf();
            let size = dir_totals.get(&path).map(|v| *v).unwrap_or(0);

            // Use pre-computed inode count from our single-pass scan
            let inode_count = if args.show_inodes {
                directory_children.get(&path).map(|v| *v).unwrap_or(0)
            } else {
                0
            };

            FileEntry {
                path,
                size,
                owner: if args.show_owner {
                    get_owner(entry.path())
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
        })
        .collect();

    // Combine files and directories into one vector
    let mut all_entries = Vec::new();
    all_entries.extend(file_entries);
    all_entries.extend(dir_entries);

    // Sort the entries based on selected criteria
    sort_entries(&mut all_entries, sort_key);

    Ok(all_entries)
}
