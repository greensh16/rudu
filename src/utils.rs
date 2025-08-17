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
use libc::{c_char, getpwuid_r, passwd, stat as libc_stat, stat};
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
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

// Global cache for UID to username mapping to avoid repeated segfaults
static UID_CACHE: std::sync::LazyLock<Mutex<HashMap<u32, String>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

// Flag to track if we've encountered getpwuid issues
static GETPWUID_BROKEN: AtomicBool = AtomicBool::new(false);

/// Fallback function to resolve UID to username using getent command
/// This is used when getpwuid_r fails but getent works
fn resolve_uid_with_getent(uid: u32) -> Option<String> {
    let output = Command::new("getent")
        .arg("passwd")
        .arg(uid.to_string())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let output_str = String::from_utf8(output.stdout).ok()?;
    let line = output_str.trim();

    // Parse passwd format: username:password:uid:gid:gecos:home:shell
    let parts: Vec<&str> = line.split(':').collect();
    if !parts.is_empty() {
        Some(parts[0].to_string())
    } else {
        None
    }
}

/// Returns the username (or UID as a string) for the file or directory owner.
///
/// Uses `libc::getpwuid` to resolve user ID to a username. If the username
/// cannot be resolved, returns the numeric UID as a string.
///
/// This function implements several safety measures:
/// - Thread-safe caching to avoid repeated calls for the same UID
/// - Panic handling to prevent segfaults
/// - Fallback to UID strings when getpwuid is broken
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

    // Check if getpwuid is known to be broken
    if GETPWUID_BROKEN.load(Ordering::Relaxed) {
        return Some(uid.to_string());
    }

    // Try to get from cache first
    if let Ok(cache) = UID_CACHE.lock() {
        if let Some(cached_name) = cache.get(&uid) {
            return Some(cached_name.clone());
        }
    }

    // Try to resolve the UID to a username using thread-safe getpwuid_r
    let resolved_name = match std::panic::catch_unwind(|| {
        // Use thread-safe getpwuid_r instead of getpwuid
        let mut pwd = MaybeUninit::<passwd>::uninit();
        let mut buf = [0u8; 4096]; // Buffer for getpwuid_r
        let mut result: *mut passwd = std::ptr::null_mut();

        let ret = unsafe {
            getpwuid_r(
                uid,
                pwd.as_mut_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                &mut result,
            )
        };

        // Check if getpwuid_r succeeded
        if ret != 0 || result.is_null() {
            return None;
        }

        // Safe to dereference result now
        unsafe {
            let pw_name = (*result).pw_name;
            if pw_name.is_null() {
                return None;
            }

            // Try to create a CStr from the pointer
            let name = CStr::from_ptr(pw_name);
            name.to_str().ok().map(String::from)
        }
    }) {
        Ok(Some(username)) => username,
        Ok(None) => {
            // Try fallback to getent command
            if let Some(username) = resolve_uid_with_getent(uid) {
                static FIRST_SUCCESS: std::sync::Once = std::sync::Once::new();
                FIRST_SUCCESS.call_once(|| {
                    eprintln!("Info: getpwuid_r failed but getent works. Using getent as fallback for UID resolution.");
                });
                username
            } else {
                // Both methods failed - warn but continue
                static FIRST_WARN: std::sync::Once = std::sync::Once::new();
                FIRST_WARN.call_once(|| {
                    eprintln!("Warning: Failed to resolve username for UID {} (both getpwuid_r and getent failed). Further warnings will be suppressed.", uid);
                });
                uid.to_string()
            }
        }
        Err(_) => {
            // Panic occurred - mark getpwuid as broken and fallback to UID strings
            GETPWUID_BROKEN.store(true, Ordering::Relaxed);
            eprintln!(
                "Warning: getpwuid() is causing segfaults. Falling back to UID display for all remaining files."
            );
            uid.to_string()
        }
    };

    // Cache the result
    if let Ok(mut cache) = UID_CACHE.lock() {
        cache.insert(uid, resolved_name.clone());
    }

    Some(resolved_name)
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
