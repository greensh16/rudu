//! Terminal output formatter for file system scan results.
//!
//! This module provides functionality to display file system scan results
//! in a human-readable format directly to the terminal.

use crate::cli::Args;
use crate::data::{EntryType, FileEntry};
use anyhow::Result;
use humansize::{DECIMAL, format_size};
use std::path::Path;

/// Renders file entries to terminal output.
///
/// # Arguments
/// * `entries` - A slice of already-filtered and sorted file entries to render
/// * `args` - Command line arguments that control output formatting
/// * `root` - The root path used to strip path prefixes from output
///
/// # Returns
/// * `Result<()>` - Ok if rendering succeeded, Err if there was an issue
pub fn render(entries: &[FileEntry], args: &Args, root: &Path) -> Result<()> {
    for entry in entries {
        let owner = if args.show_owner {
            entry.owner.clone().unwrap_or_else(|| "unknown".to_string())
        } else {
            "".to_string()
        };

        let display_path = entry.path.strip_prefix(root).unwrap_or(&entry.path);

        match entry.entry_type {
            EntryType::Dir => {
                if args.show_inodes {
                    println!(
                        "[DIR]  {:<12} {:<10} {:<6} {}",
                        format_size(entry.size, DECIMAL),
                        owner,
                        entry.inodes.unwrap_or(0),
                        display_path.display()
                    );
                } else {
                    println!(
                        "[DIR]  {:<12} {:<10} {}",
                        format_size(entry.size, DECIMAL),
                        owner,
                        display_path.display()
                    );
                }
            }
            EntryType::File => {
                println!(
                    "[FILE] {:<12} {:<10} {}",
                    format_size(entry.size, DECIMAL),
                    owner,
                    display_path.display()
                );
            }
        }
    }

    Ok(())
}
