//! Display filters: `--depth`, `--show-files`, `--older-than`, `--min-size`,
//! and `--max-size`.
//!
//! These hide *rows*, never bytes. They run after the scan, over the complete
//! entry list, so every hidden entry still counts toward its parent
//! directories' totals and the sizes shown keep matching `du`. Filtering
//! during the walk would make the numbers wrong. (`--exclude` is the exception:
//! it prunes the walk itself, so excluded data is genuinely not counted.)

use crate::atime;
use crate::cli::Args;
use crate::data::{EntryType, FileEntry};
use crate::utils::path_depth;
use std::path::Path;

/// Applies the display filters to a fully-scanned entry list.
///
/// All filters are ANDed: an entry must pass every one that was given.
///
/// `--older-than` uses a directory's *rolled-up* access age (its oldest leaf,
/// see [`atime::apply_rollup`]), so a directory survives when anything beneath
/// it is old enough — which is what makes `--older-than` usable as a drill-down
/// alongside `--depth`. Run the rollup before calling this.
///
/// A file whose atime could not be read is kept by `--older-than`: dropping it
/// would silently hide data from a report whose purpose is to account for
/// everything at risk. A *directory* with no access age has no leaves beneath
/// it and so nothing at risk, and is dropped rather than padding the report
/// with empty directories.
///
/// `--min-size` and `--max-size` apply to files and directories alike, each
/// compared on its own displayed size. Hiding a small directory never hides
/// anything the user asked to see: a directory's size is its whole subtree, so
/// if it is under the minimum, every entry beneath it is too. `--max-size` has
/// no such guarantee — a large directory can be hidden while small files inside
/// it are shown — which is the point: it answers "show me the small stuff".
pub fn process_entries(root: &Path, args: &Args, raw: Vec<FileEntry>, now: u64) -> Vec<FileEntry> {
    raw.into_iter()
        .filter(|entry| {
            let depth = path_depth(root, &entry.path);
            let depth_ok = match entry.entry_type {
                EntryType::Dir => args.depth.is_none_or(|d| depth <= d),
                EntryType::File => args.show_files && args.depth.is_none_or(|d| depth <= d),
            };

            let age_ok = match (args.older_than, entry.atime) {
                (None, _) => true,
                (Some(min_days), Some(at)) => atime::age_days(at, now) >= min_days,
                (Some(_), None) => entry.entry_type == EntryType::File,
            };

            let size_ok = args.min_size.is_none_or(|min| entry.size >= min)
                && args.max_size.is_none_or(|max| entry.size <= max);

            depth_ok && age_ok && size_ok
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(path: &str, size: u64, entry_type: EntryType) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            size,
            owner: None,
            inodes: None,
            entry_type,
            atime: None,
            at_risk_bytes: None,
            link_id: None,
        }
    }

    fn sizes(entries: &[FileEntry]) -> Vec<u64> {
        entries.iter().map(|e| e.size).collect()
    }

    #[test]
    fn test_size_band_keeps_only_entries_inside_it() {
        let raw = vec![
            entry("/r", 1_500, EntryType::Dir),
            entry("/r/tiny", 10, EntryType::File),
            entry("/r/mid", 500, EntryType::File),
            entry("/r/big", 990, EntryType::File),
        ];
        let args = Args {
            min_size: Some(100),
            max_size: Some(1_000),
            ..Default::default()
        };
        let kept = process_entries(Path::new("/r"), &args, raw, 0);
        // Bounds are inclusive; the root's subtree total is above the band.
        assert_eq!(sizes(&kept), [500, 990]);
    }

    #[test]
    fn test_max_size_can_hide_a_directory_but_show_its_small_files() {
        let raw = vec![
            entry("/r", 2_000, EntryType::Dir),
            entry("/r/d", 2_000, EntryType::Dir),
            entry("/r/d/small", 100, EntryType::File),
            entry("/r/d/large", 1_900, EntryType::File),
        ];
        let args = Args {
            max_size: Some(1_000),
            ..Default::default()
        };
        let kept = process_entries(Path::new("/r"), &args, raw, 0);
        assert_eq!(sizes(&kept), [100]);
    }
}
