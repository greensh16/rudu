//! Property-based tests for size parsing and the display filters.
//!
//! Example-based tests elsewhere pin down specific behaviour; these check the
//! invariants that must hold for *every* input — no panics on garbage, units
//! that round-trip with what rudu prints, and filters that only ever hide rows.

use humansize::{DECIMAL, format_size};
use proptest::prelude::*;
use rudu::cli::Args;
use rudu::data::{EntryType, FileEntry};
use rudu::filter::process_entries;
use rudu::utils::parse_size;
use std::path::{Path, PathBuf};

const DECIMAL_UNITS: &[(&str, u64)] = &[
    ("", 1),
    ("B", 1),
    ("K", 1_000),
    ("KB", 1_000),
    ("MB", 1_000_000),
    ("GB", 1_000_000_000),
    ("TB", 1_000_000_000_000),
];

const BINARY_UNITS: &[(&str, u64)] = &[
    ("KiB", 1 << 10),
    ("MiB", 1 << 20),
    ("GiB", 1 << 30),
    ("TiB", 1 << 40),
];

proptest! {
    #[test]
    fn parse_size_never_panics(input in ".*") {
        let _ = parse_size(&input);
    }

    #[test]
    fn parse_size_whole_numbers_with_units(
        n in 0u64..1_000_000,
        unit in prop::sample::select([DECIMAL_UNITS, BINARY_UNITS].concat()),
        lowercase in any::<bool>(),
        space in any::<bool>(),
    ) {
        let (suffix, mult) = unit;
        let suffix = if lowercase { suffix.to_ascii_lowercase() } else { suffix.to_string() };
        let sep = if space && !suffix.is_empty() { " " } else { "" };
        prop_assert_eq!(parse_size(&format!("{n}{sep}{suffix}")), Ok(n * mult));
    }

    #[test]
    fn parse_size_ignores_digit_separators(n in 1_000u64..u32::MAX as u64) {
        let digits = n.to_string();
        let (head, tail) = digits.split_at(digits.len() - 3);
        prop_assert_eq!(parse_size(&format!("{head}_{tail}")), Ok(n));
    }

    #[test]
    fn parse_size_rejects_negatives(n in 1u64..1_000_000) {
        let input = format!("-{n}");
        prop_assert!(parse_size(&input).is_err());
    }

    /// What rudu prints, rudu can read back: pasting a displayed size into
    /// `--min-size` selects (to display precision) the row it came from.
    #[test]
    fn parse_size_reads_back_printed_sizes(n in 0u64..1_000_000_000_000_000) {
        let printed = format_size(n, DECIMAL);
        let parsed = parse_size(&printed).unwrap();
        // humansize prints two decimals of the leading unit: at most 0.5% off.
        let err = (parsed as f64 - n as f64).abs();
        prop_assert!(err <= n as f64 * 0.005 + 0.5, "{} -> {:?} -> {}", n, printed, parsed);
    }
}

fn arb_entries() -> impl Strategy<Value = Vec<FileEntry>> {
    prop::collection::vec((0u64..10_000, any::<bool>(), 0usize..4), 0..40).prop_map(|specs| {
        specs
            .into_iter()
            .enumerate()
            .map(|(i, (size, is_dir, depth))| {
                let mut path = PathBuf::from("/root");
                for d in 0..depth {
                    path.push(format!("d{d}"));
                }
                path.push(format!("e{i}"));
                FileEntry {
                    path,
                    size,
                    owner: None,
                    inodes: None,
                    entry_type: if is_dir {
                        EntryType::Dir
                    } else {
                        EntryType::File
                    },
                    atime: Some(0),
                    at_risk_bytes: None,
                    link_id: None,
                }
            })
            .collect()
    })
}

fn filter(entries: &[FileEntry], args: &Args) -> Vec<FileEntry> {
    process_entries(Path::new("/root"), args, entries.to_vec(), 0)
}

fn paths(entries: &[FileEntry]) -> Vec<PathBuf> {
    entries.iter().map(|e| e.path.clone()).collect()
}

proptest! {
    #[test]
    fn no_filters_keep_everything(entries in arb_entries()) {
        prop_assert_eq!(paths(&filter(&entries, &Args::default())), paths(&entries));
    }

    /// Filters hide rows; they never reorder, invent, or alter entries.
    #[test]
    fn filters_return_an_unchanged_subsequence(
        entries in arb_entries(),
        min in prop::option::of(0u64..10_000),
        max in prop::option::of(0u64..10_000),
        depth in prop::option::of(0usize..5),
        show_files in any::<bool>(),
    ) {
        let args = Args { min_size: min, max_size: max, depth, show_files, ..Default::default() };
        let kept = filter(&entries, &args);
        let mut it = entries.iter();
        for k in &kept {
            prop_assert!(
                it.any(|e| e.path == k.path && e.size == k.size && e.entry_type == k.entry_type),
                "kept entry {:?} is not from the input, in order", k.path
            );
        }
    }

    #[test]
    fn size_filters_respect_their_bounds(
        entries in arb_entries(),
        min in prop::option::of(0u64..10_000),
        max in prop::option::of(0u64..10_000),
    ) {
        let args = Args { min_size: min, max_size: max, ..Default::default() };
        let kept = filter(&entries, &args);
        for e in &kept {
            prop_assert!(min.is_none_or(|m| e.size >= m));
            prop_assert!(max.is_none_or(|m| e.size <= m));
        }
        // ...and nothing inside the bounds was dropped.
        let expected = entries
            .iter()
            .filter(|e| min.is_none_or(|m| e.size >= m) && max.is_none_or(|m| e.size <= m))
            .count();
        prop_assert_eq!(kept.len(), expected);
    }

    /// Tightening a bound can only hide more.
    #[test]
    fn raising_min_size_never_adds_rows(entries in arb_entries(), a in 0u64..10_000, b in 0u64..10_000) {
        let (lo, hi) = (a.min(b), a.max(b));
        let loose = filter(&entries, &Args { min_size: Some(lo), ..Default::default() });
        let tight = filter(&entries, &Args { min_size: Some(hi), ..Default::default() });
        prop_assert!(tight.len() <= loose.len());
    }

    #[test]
    fn an_empty_band_hides_everything(entries in arb_entries(), a in 1u64..10_000) {
        let args = Args { min_size: Some(a), max_size: Some(a - 1), ..Default::default() };
        prop_assert!(filter(&entries, &args).is_empty());
    }
}
