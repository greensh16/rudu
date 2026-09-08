//! Access-time (atime) reporting: how close data is to a scratch purge policy.
//!
//! HPC scratch filesystems commonly delete files that have not been *accessed*
//! for some number of days — NCI's `/scratch` uses 100 days, which is the
//! default for [`DEFAULT_PURGE_DAYS`]. This module turns the raw `st_atime`
//! captured during the scan into the three things a user actually needs:
//!
//! 1. **Per-entry access age.** For a file, the age of its own `st_atime`.
//! 2. **Per-directory rollups.** A directory's own atime is useless for this
//!    question — merely listing a directory updates it, and this scan does
//!    exactly that. Instead a directory reports the *oldest* leaf beneath it
//!    (the entry that will be purged first) plus the total bytes beneath it
//!    that are already past the threshold.
//! 3. **A whole-scan summary** bucketed by access age.
//!
//! # Accuracy caveats
//!
//! - **Cached atimes are conservative.** Reading a file updates its atime but
//!   not its parent directory's mtime, so a cache-hit subtree can report an
//!   atime older than reality. Because atime only ever moves forward, a stale
//!   value always *over*-states age and therefore over-states purge risk — it
//!   never reports at-risk data as safe. Use `--no-cache` for exact values.
//! - **Hard links are counted per link here**, unlike directory totals, which
//!   deduplicate by `(dev, ino)`. At-risk bytes can therefore exceed a
//!   directory's total size in a tree full of hard links; the reported value is
//!   clamped to the directory size so the percentage stays meaningful.
//! - **`relatime`/`noatime` mounts.** Most Linux filesystems mount `relatime`,
//!   which only updates atime when it is older than mtime or more than 24h
//!   stale — fine at day granularity. A `noatime` mount freezes atime
//!   entirely, and no userspace tool can recover it; on such a mount these
//!   numbers are meaningless (as is any purge policy built on them).

use crate::data::{EntryType, FileEntry};
use humansize::{DECIMAL, format_size};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// NCI `/scratch` deletes files unaccessed for 100 days; other sites differ,
/// hence `--purge-days`.
pub const DEFAULT_PURGE_DAYS: u64 = 100;

const SECS_PER_DAY: u64 = 86_400;

/// Current wall-clock time in seconds since the Unix epoch.
///
/// Captured once per run so every age in a single report is computed against
/// the same instant.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Whole days between `atime` and `now`, floored.
///
/// An atime in the future (clock skew, or a file touched by a host whose clock
/// runs ahead) saturates to 0 rather than wrapping.
pub fn age_days(atime: u64, now: u64) -> u64 {
    now.saturating_sub(atime) / SECS_PER_DAY
}

/// Formats a Unix timestamp as a local-time `YYYY-MM-DD` date.
pub fn format_date(atime: u64) -> String {
    match chrono::DateTime::from_timestamp(atime as i64, 0) {
        Some(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string(),
        None => "?".to_string(),
    }
}

/// Fills in the directory rollups on a fully-assembled entry list.
///
/// Must be called with **every** entry from the scan — files included, and
/// before any `--depth` or `--older-than` filtering — or the rollups will be
/// computed from a partial tree and under-report. Entries whose `atime` is
/// `None` (the stat failed) contribute nothing.
///
/// After this returns, every `EntryType::Dir` entry carries:
/// - `atime`: the minimum atime over all leaves in its subtree, or `None` when
///   the subtree contains no leaves at all. A directory's *own* atime is
///   deliberately discarded: this scan just read every directory it walked, so
///   that value is always "now" and reporting it would show an empty directory
///   as freshly used.
/// - `at_risk_bytes`: the summed size of leaves in its subtree whose age is at
///   or past `purge_days`, clamped to the directory's own total.
///
/// Cost is O(entries × depth) hash operations — the same shape as the existing
/// size aggregation, and negligible beside the one `lstat` per entry.
pub fn apply_rollup(entries: &mut [FileEntry], root: &Path, purge_days: u64, now: u64) {
    let mut min_atime: HashMap<PathBuf, u64> = HashMap::new();
    let mut at_risk: HashMap<PathBuf, u64> = HashMap::new();

    for entry in entries.iter() {
        if entry.entry_type != EntryType::File {
            continue;
        }
        let Some(atime) = entry.atime else { continue };
        let risky = age_days(atime, now) >= purge_days;

        // Walk the parent chain in place, up to and including the root.
        let mut cur = entry.path.parent();
        while let Some(parent) = cur {
            min_atime
                .entry(parent.to_path_buf())
                .and_modify(|v| *v = (*v).min(atime))
                .or_insert(atime);
            if risky {
                *at_risk.entry(parent.to_path_buf()).or_insert(0) += entry.size;
            }
            if parent == root {
                break;
            }
            cur = parent.parent();
        }
    }

    for entry in entries.iter_mut() {
        if entry.entry_type != EntryType::Dir {
            continue;
        }
        // Overwrite unconditionally, including with None: the directory's own
        // atime was updated by this very scan's readdir, so it carries no
        // information about how long the data has sat untouched.
        entry.atime = min_atime.get(&entry.path).copied();
        // Hard links are counted once per link name here but once per inode in
        // `size`, so clamp rather than report an impossible >100%.
        let risk = at_risk.get(&entry.path).copied().unwrap_or(0);
        entry.at_risk_bytes = Some(risk.min(entry.size));
    }
}

/// One age band of an [`AgeSummary`].
#[derive(Debug, Clone)]
pub struct AgeBucket {
    /// Human label, e.g. `"30-90d"`.
    pub label: String,
    pub bytes: u64,
    pub files: u64,
}

/// Whole-scan breakdown of file bytes by access age.
#[derive(Debug, Clone, Default)]
pub struct AgeSummary {
    pub buckets: Vec<AgeBucket>,
    /// Total bytes over all leaves with a known atime.
    pub total_bytes: u64,
    /// Bytes in leaves at or past the threshold.
    pub at_risk_bytes: u64,
    pub at_risk_files: u64,
    /// Age in days of the single oldest leaf, if any.
    pub oldest_days: Option<u64>,
    /// Leaves whose atime could not be determined (stat failed).
    pub unknown_files: u64,
}

/// Age bands, as `(label, lower_bound_days_inclusive)`, derived from the
/// threshold so the last band is always "already purgeable" and the one before
/// it is the final 10% warning window.
///
/// With the default 100-day policy this yields `<30d`, `30-90d`, `90-100d`,
/// `>=100d`. Degenerate thresholds (very small `purge_days`) collapse empty
/// bands rather than emitting `30-30d`.
fn bands(purge_days: u64) -> Vec<(String, u64)> {
    let warn = purge_days.saturating_mul(9) / 10;
    let early = purge_days.saturating_mul(3) / 10;

    let mut edges = vec![0, early, warn, purge_days];
    edges.dedup();

    let mut out = Vec::with_capacity(edges.len());
    for (i, &lo) in edges.iter().enumerate() {
        let label = match edges.get(i + 1) {
            Some(&hi) => {
                if i == 0 {
                    format!("<{}d", hi)
                } else {
                    format!("{}-{}d", lo, hi)
                }
            }
            None => format!(">={}d", lo),
        };
        out.push((label, lo));
    }
    out
}

/// Summarises access age across every leaf in the scan.
///
/// Directories are skipped: their sizes are aggregates of the files already
/// counted, so including them would multiply-count every byte.
pub fn summarize(entries: &[FileEntry], purge_days: u64, now: u64) -> AgeSummary {
    let bands = bands(purge_days);
    let mut summary = AgeSummary {
        buckets: bands
            .iter()
            .map(|(label, _)| AgeBucket {
                label: label.clone(),
                bytes: 0,
                files: 0,
            })
            .collect(),
        ..Default::default()
    };

    for entry in entries {
        if entry.entry_type != EntryType::File {
            continue;
        }
        let Some(atime) = entry.atime else {
            summary.unknown_files += 1;
            continue;
        };
        let age = age_days(atime, now);

        summary.total_bytes += entry.size;
        summary.oldest_days = Some(summary.oldest_days.map_or(age, |o: u64| o.max(age)));

        if age >= purge_days {
            summary.at_risk_bytes += entry.size;
            summary.at_risk_files += 1;
        }

        // Last band whose lower bound the age reaches.
        let idx = bands.iter().rposition(|(_, lo)| age >= *lo).unwrap_or(0);
        summary.buckets[idx].bytes += entry.size;
        summary.buckets[idx].files += 1;
    }

    summary
}

/// Renders an [`AgeSummary`] as a short multi-line report.
///
/// Written to stderr by the caller so it never contaminates piped table output
/// or a CSV stream, matching how the banner and cache statistics are reported.
pub fn render_summary(summary: &AgeSummary, purge_days: u64) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "--- Access age summary (policy: {} days) ---\n",
        purge_days
    ));

    if summary.total_bytes == 0 && summary.unknown_files == 0 {
        out.push_str("  no files scanned\n");
        return out;
    }

    for bucket in &summary.buckets {
        let pct = if summary.total_bytes > 0 {
            bucket.bytes as f64 * 100.0 / summary.total_bytes as f64
        } else {
            0.0
        };
        let marker = if bucket.label.starts_with(">=") && bucket.bytes > 0 {
            "  <-- purgeable"
        } else {
            ""
        };
        out.push_str(&format!(
            "  {:<10} {:>10}  ({:>5.1}%)  {:>9} files{}\n",
            bucket.label,
            format_size(bucket.bytes, DECIMAL),
            pct,
            bucket.files,
            marker
        ));
    }

    if let Some(oldest) = summary.oldest_days {
        out.push_str(&format!("  oldest access: {} days ago\n", oldest));
    }
    if summary.unknown_files > 0 {
        out.push_str(&format!(
            "  {} files with unreadable access time (not counted)\n",
            summary.unknown_files
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000 * SECS_PER_DAY;

    fn days_ago(days: u64) -> u64 {
        NOW - days * SECS_PER_DAY
    }

    fn file(path: &str, size: u64, age: u64) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            size,
            owner: None,
            inodes: None,
            entry_type: EntryType::File,
            atime: Some(days_ago(age)),
            at_risk_bytes: None,
        }
    }

    fn dir(path: &str, size: u64) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            size,
            owner: None,
            inodes: None,
            entry_type: EntryType::Dir,
            atime: Some(days_ago(0)),
            at_risk_bytes: None,
        }
    }

    #[test]
    fn age_days_floors_and_saturates() {
        assert_eq!(age_days(days_ago(0), NOW), 0);
        assert_eq!(age_days(days_ago(1) + 3600, NOW), 0); // 23h -> 0 days
        assert_eq!(age_days(days_ago(100), NOW), 100);
        // atime in the future must not wrap around
        assert_eq!(age_days(NOW + 10 * SECS_PER_DAY, NOW), 0);
    }

    #[test]
    fn dir_reports_oldest_leaf_in_subtree_not_its_own_atime() {
        let root = PathBuf::from("/s");
        let mut entries = vec![
            dir("/s", 3000),
            dir("/s/a", 2000),
            file("/s/a/old.nc", 1000, 190),
            file("/s/a/new.nc", 1000, 3),
            file("/s/fresh.nc", 1000, 1),
        ];
        apply_rollup(&mut entries, &root, 100, NOW);

        // /s/a reports its oldest leaf (190d), not the 0d own-atime it started with.
        let a = entries.iter().find(|e| e.path.ends_with("a")).unwrap();
        assert_eq!(age_days(a.atime.unwrap(), NOW), 190);
        // The root rolls the same oldest leaf all the way up.
        let root_entry = entries.iter().find(|e| e.path == root).unwrap();
        assert_eq!(age_days(root_entry.atime.unwrap(), NOW), 190);
        // Files keep their own atime untouched.
        let fresh = entries
            .iter()
            .find(|e| e.path.ends_with("fresh.nc"))
            .unwrap();
        assert_eq!(age_days(fresh.atime.unwrap(), NOW), 1);
    }

    #[test]
    fn at_risk_bytes_count_only_leaves_past_threshold() {
        let root = PathBuf::from("/s");
        let mut entries = vec![
            dir("/s", 3000),
            dir("/s/a", 2000),
            file("/s/a/old.nc", 1000, 190),
            file("/s/a/new.nc", 1000, 3),
            file("/s/fresh.nc", 1000, 1),
        ];
        apply_rollup(&mut entries, &root, 100, NOW);

        let a = entries.iter().find(|e| e.path.ends_with("a")).unwrap();
        assert_eq!(a.at_risk_bytes, Some(1000));
        let root_entry = entries.iter().find(|e| e.path == root).unwrap();
        assert_eq!(root_entry.at_risk_bytes, Some(1000));
    }

    #[test]
    fn at_risk_is_clamped_to_directory_size() {
        // Two hard links to one 1000-byte inode: totals count it once, the
        // atime rollup sees two leaves. The report must not exceed 100%.
        let root = PathBuf::from("/s");
        let mut entries = vec![
            dir("/s", 1000),
            file("/s/link_a", 1000, 190),
            file("/s/link_b", 1000, 190),
        ];
        apply_rollup(&mut entries, &root, 100, NOW);

        let root_entry = entries.iter().find(|e| e.path == root).unwrap();
        assert_eq!(root_entry.at_risk_bytes, Some(1000));
    }

    #[test]
    fn directory_with_no_leaves_reports_no_access_age() {
        // The scan's own readdir updates a directory's atime, so its own value
        // is always ~now. Reporting it would show an untouched empty directory
        // as freshly accessed; the honest answer is "no data".
        let root = PathBuf::from("/s");
        let mut empty = dir("/s/empty", 4096);
        empty.atime = Some(days_ago(42));
        let mut entries = vec![dir("/s", 4096), empty];
        apply_rollup(&mut entries, &root, 100, NOW);

        let e = entries.iter().find(|e| e.path.ends_with("empty")).unwrap();
        assert_eq!(e.atime, None);
        assert_eq!(e.at_risk_bytes, Some(0));
    }

    #[test]
    fn default_bands_match_the_hundred_day_policy() {
        let labels: Vec<String> = bands(100).into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels, vec!["<30d", "30-90d", "90-100d", ">=100d"]);
    }

    #[test]
    fn summary_buckets_files_and_ignores_directories() {
        let entries = vec![
            dir("/s", 10_000), // must not be counted: it aggregates the files
            file("/s/a", 1000, 5),
            file("/s/b", 2000, 50),
            file("/s/c", 4000, 95),
            file("/s/d", 8000, 150),
        ];
        let s = summarize(&entries, 100, NOW);

        assert_eq!(s.total_bytes, 15_000);
        assert_eq!(s.buckets[0].bytes, 1000); // <30d
        assert_eq!(s.buckets[1].bytes, 2000); // 30-90d
        assert_eq!(s.buckets[2].bytes, 4000); // 90-100d
        assert_eq!(s.buckets[3].bytes, 8000); // >=100d
        assert_eq!(s.at_risk_bytes, 8000);
        assert_eq!(s.at_risk_files, 1);
        assert_eq!(s.oldest_days, Some(150));
    }

    #[test]
    fn summary_counts_unreadable_atimes_separately() {
        let mut unknown = file("/s/x", 500, 0);
        unknown.atime = None;
        let entries = vec![file("/s/a", 1000, 5), unknown];
        let s = summarize(&entries, 100, NOW);

        assert_eq!(s.unknown_files, 1);
        assert_eq!(s.total_bytes, 1000); // unknown bytes are not attributed to a band
    }
}
