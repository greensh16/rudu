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
//! - A list of individual file sizes
//! - A sorted list of total directory sizes
//!
//! Sorting behavior is controlled by the [`SortKey`] provided from the CLI.

use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::{DirEntry, WalkDir};
use dashmap::DashMap;

use crate::cli::SortKey;
use crate::utils::disk_usage;
use crate::Args;

pub fn scan_files_and_dirs(
    root: &Path,
    args: &Args,
    exclude_matcher: &globset::GlobSet,
    sort_key: SortKey,
) -> (Vec<(PathBuf, u64)>, Vec<(PathBuf, u64)>) {

    // Setup a spinner to indicate scanning progress in the terminal
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner} Scanning files... [{elapsed}]")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    // Collect all file entries recursively while filtering by exclusion rules
    let entries: Vec<DirEntry> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            !exclude_matcher.is_match(e.path())
                && !e.path().components().any(|c| {
                    args.exclude.iter().any(|x| c.as_os_str() == OsStr::new(x))
                })
        })
        .filter_map(|e| {
            pb.tick();
            e.ok()
        })
        .filter(|e| e.file_type().is_file())
        .collect();

    pb.finish_with_message("Scan complete ✅");

    // Compute disk usage for each file path in parallel
    let file_data: Vec<(PathBuf, u64)> = entries
        .par_iter()
        .map(|entry| {
            let path = entry.path().to_path_buf();
            let size = disk_usage(&path);
            (path, size)
        })
        .collect();

    // Thread-safe accumulation of total sizes for each parent directory
    let dir_totals: DashMap<PathBuf, u64> = DashMap::new();

    file_data.par_iter().for_each(|(file_path, size)| {
        let mut current = file_path.parent();
        while let Some(path) = current {
            dir_totals
                .entry(path.to_path_buf())
                .and_modify(|v| *v += size)
                .or_insert(*size);
            if path == root {
                break;
            }
            current = path.parent();
        }
    });

    // Sort the final directory list based on selected criteria (size or name)
    let mut sorted_dirs: Vec<(PathBuf, u64)> = dir_totals
        .iter()
        .map(|e| (e.key().clone(), *e.value()))
        .collect();

    match sort_key {
        SortKey::Size => sorted_dirs.sort_by(|a, b| b.1.cmp(&a.1)),
        SortKey::Name => sorted_dirs.sort_by_key(|(k, _)| k.clone()),
    }

    (file_data, sorted_dirs)
}
