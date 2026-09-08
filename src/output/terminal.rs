//! Terminal output formatter for file system scan results.
//!
//! This module provides functionality to display file system scan results
//! in a human-readable format directly to the terminal.

use crate::atime::{age_days, format_date};
use crate::cli::Args;
use crate::data::{EntryType, FileEntry};
use anyhow::Result;
use humansize::{DECIMAL, format_size};
use std::path::Path;

/// Width of the last-access column (`YYYY-MM-DD` plus the age in days).
const ATIME_COL: usize = 18;
/// Width of the at-risk column (`1.23 GB (45%)`).
const RISK_COL: usize = 16;

/// Renders the access-age columns for one entry.
///
/// Returns an empty string unless `--show-atime` was given, so the default
/// output shape is byte-for-byte unchanged.
fn atime_cells(entry: &FileEntry, args: &Args, now: u64) -> String {
    if !args.show_atime {
        return String::new();
    }

    let access = match (entry.atime, entry.entry_type) {
        (Some(at), _) => format!("{} {:>4}d", format_date(at), age_days(at, now)),
        // A directory with no access age holds no leaves; a file with none
        // could not be stat'd. Different facts, different markers.
        (None, EntryType::Dir) => "-".to_string(),
        (None, EntryType::File) => "unknown".to_string(),
    };

    // Only directories carry a rollup; a file's own size is its entire risk.
    let risk = match (entry.entry_type, entry.at_risk_bytes) {
        (EntryType::Dir, Some(bytes)) if bytes > 0 => {
            let pct = if entry.size > 0 {
                bytes as f64 * 100.0 / entry.size as f64
            } else {
                0.0
            };
            format!("{} ({:.0}%)", format_size(bytes, DECIMAL), pct)
        }
        (EntryType::Dir, _) => "-".to_string(),
        (EntryType::File, _) => String::new(),
    };

    format!(
        "{:<width$} {:<risk$} ",
        access,
        risk,
        width = ATIME_COL,
        risk = RISK_COL
    )
}

/// Prints the column header, but only when `--show-atime` adds columns whose
/// meaning is not obvious from the values alone.
fn print_header(args: &Args) {
    if !args.show_atime {
        return;
    }
    let owner = if args.show_owner {
        format!("{:<10} ", "OWNER")
    } else {
        format!("{:<10} ", "")
    };
    let inodes = if args.show_inodes {
        format!("{:<6} ", "INODES")
    } else {
        String::new()
    };
    println!(
        "       {:<12} {}{:<width$} {:<risk$} {}PATH",
        "SIZE",
        owner,
        "LAST ACCESS",
        format!("AT RISK (>={}d)", args.purge_days),
        inodes,
        width = ATIME_COL,
        risk = RISK_COL,
    );
}

/// Renders file entries to terminal output.
///
/// # Arguments
/// * `entries` - A slice of already-filtered and sorted file entries to render
/// * `args` - Command line arguments that control output formatting
/// * `root` - The root path used to strip path prefixes from output
/// * `now` - Wall-clock reference for access-age arithmetic, so every row in a
///   report is aged against the same instant
///
/// # Returns
/// * `Result<()>` - Ok if rendering succeeded, Err if there was an issue
pub fn render(entries: &[FileEntry], args: &Args, root: &Path, now: u64) -> Result<()> {
    print_header(args);

    for entry in entries {
        let owner = if args.show_owner {
            entry.owner.clone().unwrap_or_else(|| "unknown".to_string())
        } else {
            "".to_string()
        };

        let display_path = entry.path.strip_prefix(root).unwrap_or(&entry.path);
        let access = atime_cells(entry, args, now);

        match entry.entry_type {
            EntryType::Dir => {
                if args.show_inodes {
                    println!(
                        "[DIR]  {:<12} {:<10} {}{:<6} {}",
                        format_size(entry.size, DECIMAL),
                        owner,
                        access,
                        entry.inodes.unwrap_or(0),
                        display_path.display()
                    );
                } else {
                    println!(
                        "[DIR]  {:<12} {:<10} {}{}",
                        format_size(entry.size, DECIMAL),
                        owner,
                        access,
                        display_path.display()
                    );
                }
            }
            EntryType::File => {
                println!(
                    "[FILE] {:<12} {:<10} {}{}",
                    format_size(entry.size, DECIMAL),
                    owner,
                    access,
                    display_path.display()
                );
            }
        }
    }

    Ok(())
}
