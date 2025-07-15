//! Utility functions for the `rudu` disk usage tool.
//!
//! This module provides:
//! - Accurate disk usage calculation via `libc::stat`
//! - Directory depth comparison
//! - File/directory owner name resolution
//! - Glob-based exclusion pattern parsing
//!
//! All functions are platform-aware and safe to use with Unix filesystems.
//! Used throughout the main binary for performance and filtering.

use crate::cli::SortKey;
use crate::data::{EntryType, FileEntry};
use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use libc::{getpwuid, stat as libc_stat, stat};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::os::unix::ffi::OsStrExt;
use std::{ffi::CStr, ffi::CString, path::Path};

/// Returns the actual disk usage (in bytes) of a file or directory.
///
/// Uses the `st_blocks` field from `stat()` multiplied by 512 to get
/// the actual disk space used, similar to the `du` command.
///
/// # Arguments
/// * `path` - The file or directory path to check
///
/// # Returns
/// * `u64` - The disk usage in bytes, or 0 if the path cannot be accessed
pub fn disk_usage(path: &Path) -> u64 {
    let c_path = match CString::new(path.as_os_str().as_bytes()) {
        Ok(p) => p,
        Err(_) => return 0,
    };

    // Use MaybeUninit to avoid undefined behavior with zeroed stat struct
    let mut stat_buf = std::mem::MaybeUninit::<stat>::uninit();
    let result = unsafe { libc_stat(c_path.as_ptr(), stat_buf.as_mut_ptr()) };
    
    if result != 0 {
        return 0;
    }
    
    let stat_buf = unsafe { stat_buf.assume_init() };
    (stat_buf.st_blocks as u64) * 512
}

/// Calculates how many path components lie between `root` and `path`.
/// This is used to determine directory depth relative to the scan root.
pub fn path_depth(root: &Path, path: &Path) -> usize {
    path.strip_prefix(root)
        .map(|p| p.components().count())
        .unwrap_or(0)
}

/// Filters entries by depth based on the root path and optional depth limit.
#[allow(dead_code)]
pub fn filter_by_depth<'a>(
    entries: &'a [FileEntry],
    root: &Path,
    depth: Option<usize>,
    show_files: bool,
) -> Vec<&'a FileEntry> {
    entries
        .iter()
        .filter(|entry| {
            let entry_depth = path_depth(root, &entry.path);
            match entry.entry_type {
                EntryType::Dir => depth.map(|d| entry_depth <= d).unwrap_or(true),
                EntryType::File => show_files && depth.map(|d| entry_depth == d).unwrap_or(true),
            }
        })
        .collect()
}

/// Sorts entries based on the provided sort key.
///
/// # Arguments
/// * `entries` - A mutable reference to the vector of entries to sort
/// * `sort_key` - The sorting criterion to use
///
/// # Behavior
/// * `SortKey::Size` - Sorts by size in descending order (largest first)
/// * `SortKey::Name` - Sorts by path name in ascending order
pub fn sort_entries(entries: &mut [FileEntry], sort_key: SortKey) {
    match sort_key {
        SortKey::Size => entries.sort_by(|a, b| b.size.cmp(&a.size)),
        SortKey::Name => entries.sort_by(|a, b| a.path.cmp(&b.path)),
    }
}

/// Returns the username (or UID as a string) for the file or directory owner.
///
/// Uses `libc::getpwuid` to resolve user ID to a username. If the username
/// cannot be resolved, returns the numeric UID as a string.
///
/// # Arguments
/// * `path` - The file or directory path to check
///
/// # Returns
/// * `Option<String>` - The username or UID, or None if the path cannot be accessed
pub fn get_owner(path: &Path) -> Option<String> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    
    // Use MaybeUninit to avoid undefined behavior with zeroed stat struct
    let mut stat_buf = std::mem::MaybeUninit::<stat>::uninit();
    let result = unsafe { libc_stat(c_path.as_ptr(), stat_buf.as_mut_ptr()) };
    
    if result != 0 {
        return None;
    }
    
    let stat_buf = unsafe { stat_buf.assume_init() };
    let uid = stat_buf.st_uid;
    
    // Safely handle getpwuid which may fail on HPC systems
    let pw = unsafe { getpwuid(uid) };
    if pw.is_null() {
        return Some(uid.to_string());
    }
    
    // Additional safety check before dereferencing
    unsafe {
        let pw_name = (*pw).pw_name;
        if pw_name.is_null() {
            return Some(uid.to_string());
        }
        
        let name = CStr::from_ptr(pw_name);
        name.to_str().ok().map(String::from)
    }
}

/// Expands exclude patterns into common glob forms:
/// For example, "node_modules" becomes:
///   - `**/node_modules`
///   - `**/node_modules/**`
///     unless the pattern already includes glob symbols or extensions.
pub fn expand_exclude_patterns(patterns: &[String]) -> Vec<String> {
    let mut expanded = Vec::new();

    for pat in patterns {
        let pat = pat.trim();
        if pat.contains('*') || pat.ends_with('/') || pat.contains('.') {
            expanded.push(pat.to_string());
        } else {
            expanded.push(format!("**/{}", pat));
            expanded.push(format!("**/{}/**", pat));
        }
    }

    expanded
}

/// Compiles a list of glob patterns into a `GlobSet` matcher,
/// which can be used to test paths efficiently.
pub fn build_exclude_matcher(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob =
            Glob::new(pattern).with_context(|| format!("Invalid glob pattern: '{}'", pattern))?;
        builder.add(glob);
    }
    builder.build().context("Failed to build glob set")
}

/// Directory metadata for caching purposes
#[derive(Debug, Clone)]
pub struct DirMetadata {
    pub mtime: u64,
    pub nlink: u64,
    #[allow(dead_code)]
    pub size: u64,
    pub owner: Option<u32>,
}

/// Get directory metadata (mtime, nlink, size, owner) for caching
pub fn get_dir_metadata(path: &Path) -> Option<DirMetadata> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    
    // Use MaybeUninit to avoid undefined behavior with zeroed stat struct
    let mut stat_buf = std::mem::MaybeUninit::<stat>::uninit();
    let result = unsafe { libc_stat(c_path.as_ptr(), stat_buf.as_mut_ptr()) };
    
    if result != 0 {
        return None;
    }
    
    let stat_buf = unsafe { stat_buf.assume_init() };
    Some(DirMetadata {
        mtime: stat_buf.st_mtime as u64,
        nlink: stat_buf.st_nlink as u64,
        size: (stat_buf.st_blocks as u64) * 512,
        owner: Some(stat_buf.st_uid),
    })
}

/// Calculate a hash of a path for use in cache lookups
pub fn path_hash(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}
