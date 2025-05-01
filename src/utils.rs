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

use libc::{stat as libc_stat, stat, getpwuid};
use std::{
    ffi::CString,
    ffi::CStr,
    path::Path,
};
use std::os::unix::ffi::OsStrExt;
use globset::{Glob, GlobSet, GlobSetBuilder};

/// Returns the actual disk usage (in bytes) of a file or directory,
/// based on allocated blocks (`st_blocks * 512`), similar to `du`.
pub fn disk_usage(path: &Path) -> u64 {
    let c_path = match CString::new(path.as_os_str().as_bytes()) {
        Ok(p) => p,
        Err(_) => return 0,
    };

    let mut stat_buf: stat = unsafe { std::mem::zeroed() };
    if unsafe { libc_stat(c_path.as_ptr(), &mut stat_buf) } != 0 {
        return 0;
    }

    (stat_buf.st_blocks as u64) * 512
}

/// Calculates how many path components lie between `root` and `path`.
/// This is used to determine directory depth relative to the scan root.
pub fn path_depth(root: &Path, path: &Path) -> usize {
    path.strip_prefix(root)
        .map(|p| p.components().count())
        .unwrap_or(0)
}

/// Returns the username (or UID as a string) for the file or directory owner.
/// Uses `libc::getpwuid` to resolve user ID to a username.
pub fn get_owner(path: &Path) -> Option<String> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat_buf: stat = unsafe { std::mem::zeroed() };
    if unsafe { libc_stat(c_path.as_ptr(), &mut stat_buf) } != 0 {
        return None;
    }

    let pw = unsafe { getpwuid(stat_buf.st_uid) };
    if pw.is_null() {
        return Some(stat_buf.st_uid.to_string());
    }

    let name = unsafe { CStr::from_ptr((*pw).pw_name) };
    name.to_str().ok().map(String::from)
}

/// Expands exclude patterns into common glob forms:
/// For example, "node_modules" becomes:
///   - `**/node_modules`
///   - `**/node_modules/**`
/// unless the pattern already includes glob symbols or extensions.
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
pub fn build_exclude_matcher(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        match Glob::new(pattern) {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(_) => {
                eprintln!("Warning: Invalid pattern '{}'", pattern);
            }
        }
    }
    builder.build().unwrap()
}
