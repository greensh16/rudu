//! Utility functions for the `rudu` disk usage tool.
//!
//! This module provides:
//! - Accurate disk usage calculation via `libc::lstat` (symlinks never followed, like `du`)
//! - Directory depth comparison
//! - File/directory owner name resolution
//! - Glob-based exclusion pattern parsing
//!
//! All functions are platform-aware and safe to use with Unix filesystems.
//! Used throughout the main binary for performance and filtering.

use crate::cli::SortKey;
use crate::data::FileEntry;
use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use libc::{c_char, getpwuid_r, lstat as libc_lstat, passwd, stat};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;
use std::sync::Mutex;
use std::{ffi::CStr, ffi::CString, path::Path};

/// Returns the actual disk usage (in bytes) of a file or directory.
///
/// Uses the `st_blocks` field from `lstat()` multiplied by 512 to get
/// the actual disk space used, similar to the `du` command. Like `du`,
/// symbolic links are NOT followed — a symlink reports its own (tiny)
/// size, never its target's.
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
    let result = unsafe { libc_lstat(c_path.as_ptr(), stat_buf.as_mut_ptr()) };

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

/// Sorts entries based on the provided sort key.
///
/// Uses rayon's parallel stable sort — same ordering guarantees as
/// `slice::sort_by`, but the final sort is no longer a single-core tail
/// phase on multi-million-entry results.
///
/// # Arguments
/// * `entries` - A mutable reference to the vector of entries to sort
/// * `sort_key` - The sorting criterion to use
///
/// # Behavior
/// * `SortKey::Size` - Sorts by size in descending order (largest first)
/// * `SortKey::Name` - Sorts by path name in ascending order
pub fn sort_entries(entries: &mut [FileEntry], sort_key: SortKey) {
    use rayon::prelude::*;
    match sort_key {
        SortKey::Size => entries.par_sort_by(|a, b| b.size.cmp(&a.size)),
        SortKey::Name => entries.par_sort_by(|a, b| a.path.cmp(&b.path)),
    }
}

// Global cache for UID to username mapping to avoid repeated lookups
static UID_CACHE: std::sync::LazyLock<Mutex<HashMap<u32, String>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Fallback function to resolve UID to username using the getent command.
/// This is used when getpwuid_r fails but getent works (e.g. some HPC/NSS
/// configurations where the passwd database is not reachable via libc).
fn resolve_uid_with_getent(uid: u32) -> Option<String> {
    // Prefer absolute paths so a hostile entry earlier in PATH cannot be
    // executed; fall back to PATH lookup only if neither location exists
    // (getent is not present at all on e.g. macOS).
    let output = ["/usr/bin/getent", "/bin/getent", "getent"]
        .iter()
        .find_map(|cmd| {
            Command::new(cmd)
                .arg("passwd")
                .arg(uid.to_string())
                .output()
                .ok()
        })?;

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
/// Uses the thread-safe `getpwuid_r` to resolve user ID to a username, with
/// a `getent` fallback for environments where the passwd database is not
/// reachable via libc (some HPC/NSS setups). If the username cannot be
/// resolved at all, returns the numeric UID as a string. Results are cached
/// per UID.
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
    let result = unsafe { libc_lstat(c_path.as_ptr(), stat_buf.as_mut_ptr()) };

    if result != 0 {
        return None;
    }

    let stat_buf = unsafe { stat_buf.assume_init() };
    Some(resolve_uid(stat_buf.st_uid))
}

/// Resolves a UID to a username (or the numeric UID as a string if unresolvable).
///
/// This is the resolution core of [`get_owner`], exposed separately so callers
/// that already have a UID (e.g. from a cache entry or an earlier `stat`) can
/// resolve it without an extra syscall. Uses the same thread-safe UID cache,
/// `getpwuid_r`, and `getent` fallback.
pub fn resolve_uid(uid: u32) -> String {
    // Try to get from cache first
    if let Ok(cache) = UID_CACHE.lock()
        && let Some(cached_name) = cache.get(&uid)
    {
        return cached_name.clone();
    }

    // Resolve via the thread-safe getpwuid_r.
    //
    // Historical note: an earlier version wrapped this in
    // `std::panic::catch_unwind` with comments claiming it prevented
    // segfaults. It never could — `catch_unwind` intercepts Rust panics, not
    // SIGSEGV; a genuine segfault in libc aborts the process regardless. The
    // actual mitigation for broken NSS setups is using `getpwuid_r` (not the
    // non-reentrant `getpwuid`) plus the `getent` fallback below.
    let getpwuid_result = {
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

        if ret != 0 || result.is_null() {
            None
        } else {
            // Safe to dereference result now
            unsafe {
                let pw_name = (*result).pw_name;
                if pw_name.is_null() {
                    None
                } else {
                    CStr::from_ptr(pw_name).to_str().ok().map(String::from)
                }
            }
        }
    };

    let resolved_name = match getpwuid_result {
        Some(username) => username,
        None => {
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
    };

    // Cache the result
    if let Ok(mut cache) = UID_CACHE.lock() {
        cache.insert(uid, resolved_name.clone());
    }

    resolved_name
}

/// Expands exclude patterns into glob forms that match at any depth.
///
/// Any pattern that does not contain a `/` — a bare name (`node_modules`),
/// a dot-name (`.git`), or a bare glob (`*.log`) — is expanded to:
///   - `**/<pattern>`     (the entry itself, at any depth)
///   - `**/<pattern>/**`  (anything beneath it)
///
/// A trailing `/` is stripped first, so `temp/` behaves like `temp`.
/// Patterns containing `/` are treated as path-anchored globs and used as
/// given.
///
/// Previously, patterns containing `*` or `.` were passed through unexpanded;
/// since globset's `*` does not cross `/`, `--exclude '*.log'` silently
/// matched almost nothing, and `.git` was only excluded via a separate
/// literal component comparison.
pub fn expand_exclude_patterns(patterns: &[String]) -> Vec<String> {
    let mut expanded = Vec::new();

    for pat in patterns {
        let pat = pat.trim().trim_end_matches('/');
        if pat.is_empty() {
            continue;
        }
        if pat.contains('/') {
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
    /// Modification time in nanoseconds since the Unix epoch (only compared
    /// for equality against cached values).
    pub mtime: u64,
    pub nlink: u64,
    /// Disk usage in bytes (`st_blocks * 512`) — same value `disk_usage` returns.
    pub size: u64,
    pub owner: Option<u32>,
    /// Device ID — combined with `ino`, identifies the underlying inode so
    /// hard-linked files can be counted once in totals, as `du` does.
    pub dev: u64,
    /// Inode number.
    pub ino: u64,
    /// Last access time in whole seconds since the Unix epoch.
    ///
    /// Whole seconds, unlike `mtime`: atime is only ever rendered at day
    /// granularity for access-age reporting, never compared for equality the
    /// way cache validation compares mtime.
    pub atime: u64,
}

/// Get entry metadata (mtime, atime, nlink, size, owner, dev/ino) for caching.
///
/// Despite the historical name, this works for any path (file, directory,
/// symlink, or special file) — it is a single `lstat` call, so callers can
/// obtain size, mtime, atime, nlink, owner, and inode identity with one syscall.
/// Symbolic links are NOT followed (matching `du`): a symlink reports its
/// own metadata, never its target's.
pub fn get_dir_metadata(path: &Path) -> Option<DirMetadata> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;

    // Use MaybeUninit to avoid undefined behavior with zeroed stat struct
    let mut stat_buf = std::mem::MaybeUninit::<stat>::uninit();
    let result = unsafe { libc_lstat(c_path.as_ptr(), stat_buf.as_mut_ptr()) };

    if result != 0 {
        return None;
    }

    let stat_buf = unsafe { stat_buf.assume_init() };

    // `st_nlink` and `st_dev` are platform-dependent: on macOS they are `u16`
    // and `i32`, on Linux both are already `u64`. These casts are therefore
    // load-bearing on macOS and no-ops on Linux, where clippy flags them as
    // redundant — hence the targeted allow rather than removing them, which
    // would break the macOS build. `u64::from` is not an alternative either:
    // `st_dev` is signed on macOS.
    #[allow(clippy::unnecessary_cast)]
    let nlink = stat_buf.st_nlink as u64;
    #[allow(clippy::unnecessary_cast)]
    let dev = stat_buf.st_dev as u64;

    Some(DirMetadata {
        // Nanosecond-precision mtime: with whole seconds only, a change made in
        // the same second as the caching scan would compare as "unchanged" and
        // be missed by cache validation. Only ever compared for equality.
        mtime: (stat_buf.st_mtime as u64)
            .wrapping_mul(1_000_000_000)
            .wrapping_add(stat_buf.st_mtime_nsec as u64),
        nlink,
        size: (stat_buf.st_blocks as u64) * 512,
        owner: Some(stat_buf.st_uid),
        dev,
        ino: stat_buf.st_ino,
        // Negative atimes (pre-1970, or a filesystem returning garbage) clamp
        // to the epoch so age arithmetic stays in unsigned range.
        atime: stat_buf.st_atime.max(0) as u64,
    })
}

/// Parses a human-friendly size into a byte count.
///
/// Accepts a bare byte count (`1024`) or a number with a unit suffix:
/// decimal `B`/`KB`/`MB`/`GB`/`TB`/`PB` (powers of 1000) and binary
/// `KiB`/`MiB`/`GiB`/`TiB`/`PiB` (powers of 1024). Units are case-insensitive,
/// the bare forms `K`/`M`/`G`/`T`/`P` are accepted as decimal, fractional
/// values work (`1.5GB`), and `_` may be used as a digit separator.
///
/// **Decimal units are the default deliberately.** rudu prints sizes through
/// humansize's `DECIMAL` formatter, so the table shows `819.20 kB`; if
/// `--min-size 819kB` meant 1024-based kilobytes it would not match the row the
/// user just read. The `i` forms are there for anyone who wants powers of 1024.
///
/// # Errors
///
/// Returns a message suitable for direct display when the number is malformed,
/// negative, the unit is unrecognised, or the result exceeds `u64`.
pub fn parse_size(input: &str) -> Result<u64, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty size value".to_string());
    }

    // Split the leading numeric run from the unit. `-`/`+` are included so a
    // negative value reaches the explicit check below with a clear message
    // rather than failing as an unparseable empty number.
    let split = trimmed
        .find(|c: char| !c.is_ascii_digit() && !matches!(c, '.' | '_' | '-' | '+'))
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let number = number.replace('_', "");
    let unit = unit.trim();

    let value: f64 = number
        .parse()
        .map_err(|_| format!("invalid size '{input}': '{number}' is not a number"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!(
            "invalid size '{input}': size must be a non-negative number"
        ));
    }

    let multiplier: f64 = match unit.to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kb" => 1e3,
        "m" | "mb" => 1e6,
        "g" | "gb" => 1e9,
        "t" | "tb" => 1e12,
        "p" | "pb" => 1e15,
        "ki" | "kib" => 1024.0,
        "mi" | "mib" => 1024f64.powi(2),
        "gi" | "gib" => 1024f64.powi(3),
        "ti" | "tib" => 1024f64.powi(4),
        "pi" | "pib" => 1024f64.powi(5),
        other => {
            return Err(format!(
                "invalid size '{input}': unknown unit '{other}' (expected B, KB, \
                 MB, GB, TB, PB, or the 1024-based KiB, MiB, GiB, TiB, PiB)"
            ));
        }
    };

    let bytes = value * multiplier;
    if bytes > u64::MAX as f64 {
        return Err(format!("invalid size '{input}': value is too large"));
    }
    Ok(bytes.round() as u64)
}

/// Default thread cap on network and parallel filesystems (see
/// [`network_fs_type`]). Each `lstat` there is a metadata-server round trip
/// shared with every other user, so all-cores parallelism on a 64-core node
/// can slow the filesystem for everyone without making the scan much faster.
pub const NETWORK_FS_THREADS: usize = 8;

/// Names the filesystem holding `path` if it is a network or parallel
/// filesystem (Lustre, NFS, GPFS, BeeGFS, CephFS, SMB, …), else `None`.
///
/// One `statfs` call; no pre-scan. Returns `None` when the type cannot be
/// determined, so an unrecognised filesystem keeps the all-cores default.
/// FUSE is deliberately not listed: it fronts local and remote filesystems
/// alike, and its magic number cannot tell them apart.
pub fn network_fs_type(path: &Path) -> Option<&'static str> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut buf = MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: c_path is NUL-terminated and buf is valid for writes; statfs
    // fully initialises buf on success, and we only read it in that case.
    if unsafe { libc::statfs(c_path.as_ptr(), buf.as_mut_ptr()) } != 0 {
        return None;
    }
    let buf = unsafe { buf.assume_init() };
    fs_type_name(&buf)
}

#[cfg(target_os = "linux")]
fn fs_type_name(buf: &libc::statfs) -> Option<&'static str> {
    // `f_type` is `i64` on glibc and `u64` on musl, so the cast is redundant
    // on one of them; magic numbers are 32-bit, so mask after widening.
    #[allow(clippy::unnecessary_cast)]
    let magic = (buf.f_type as u64) & 0xFFFF_FFFF;
    Some(match magic {
        0x0BD0_0BD0 => "lustre",
        0x0000_6969 => "nfs",
        0x4750_4653 => "gpfs",
        0x1983_0326 => "beegfs",
        0x00C3_6400 => "cephfs",
        0xAAD7_AAEA => "panfs",
        0x5346_414F => "afs",
        0x0102_1997 => "9p",
        0x0000_517B => "smb",
        0xFF53_4D42 => "cifs",
        0xFE53_4D42 => "smb2",
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
fn fs_type_name(buf: &libc::statfs) -> Option<&'static str> {
    // SAFETY: the kernel NUL-terminates f_fstypename within its fixed array.
    let name = unsafe { CStr::from_ptr(buf.f_fstypename.as_ptr()) };
    Some(match name.to_bytes() {
        b"nfs" => "nfs",
        b"smbfs" => "smb",
        b"afpfs" => "afp",
        b"webdav" => "webdav",
        _ => return None,
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn fs_type_name(_buf: &libc::statfs) -> Option<&'static str> {
    None
}

/// Directory names skipped by `--auto-exclude`: version-control metadata,
/// dependency trees, build output, and tool caches — things that are
/// regenerable and rarely what a "where did my space go" question is about.
///
/// Opt-in, never a default: excluding these changes totals so they no longer
/// match `du`, and on shared storage a directory called `target` or `build`
/// can just as easily be real data.
pub const AUTO_EXCLUDES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
];

/// Appends [`AUTO_EXCLUDES`] to `patterns`, skipping any already present.
pub fn with_auto_excludes(patterns: &[String]) -> Vec<String> {
    let mut out = patterns.to_vec();
    for name in AUTO_EXCLUDES {
        if !out.iter().any(|p| p.trim().trim_end_matches('/') == *name) {
            out.push((*name).to_string());
        }
    }
    out
}

/// FNV-1a 64-bit hasher, implemented inline (previously the `fnv` crate).
///
/// The offset basis and prime are identical to `fnv::FnvHasher`, so hash
/// values — and therefore on-disk cache file names and entry keys — are
/// unchanged by the dependency removal. FNV-1a is used because it is stable
/// across Rust versions and platforms, unlike `DefaultHasher`.
pub struct Fnv1aHasher(u64);

impl Default for Fnv1aHasher {
    fn default() -> Self {
        Fnv1aHasher(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for Fnv1aHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

/// Calculate a stable, version-independent hash of a path for use in cache lookups.
///
/// Uses FNV-1a rather than `DefaultHasher`, which has no cross-version stability
/// guarantee and would silently invalidate on-disk cache files after a Rust upgrade.
pub fn path_hash(path: &Path) -> u64 {
    let mut hasher = Fnv1aHasher::default();
    path.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod fs_type_tests {
    use super::*;

    fn zeroed_statfs() -> libc::statfs {
        // SAFETY: statfs is a plain C struct; all-zero is a valid value.
        unsafe { std::mem::zeroed() }
    }

    #[test]
    fn test_local_temp_dir_is_not_a_network_filesystem() {
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(network_fs_type(dir.path()), None);
    }

    #[test]
    fn test_unknown_filesystem_type_keeps_the_default() {
        assert_eq!(fs_type_name(&zeroed_statfs()), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_linux_magic_numbers() {
        let mut buf = zeroed_statfs();
        for (magic, name) in [
            (0x0BD0_0BD0u32, "lustre"),
            (0x6969, "nfs"),
            (0x4750_4653, "gpfs"),
            (0xFF53_4D42, "cifs"),
        ] {
            buf.f_type = magic as _;
            assert_eq!(fs_type_name(&buf), Some(name));
        }
        buf.f_type = 0xEF53 as _; // ext4
        assert_eq!(fs_type_name(&buf), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_macos_type_names() {
        let mut buf = zeroed_statfs();
        for (raw, name) in [
            (&b"nfs\0"[..], Some("nfs")),
            (b"smbfs\0", Some("smb")),
            (b"apfs\0", None),
        ] {
            buf.f_fstypename = [0; 16];
            for (dst, &src) in buf.f_fstypename.iter_mut().zip(raw) {
                *dst = src as libc::c_char;
            }
            assert_eq!(fs_type_name(&buf), name);
        }
    }
}
