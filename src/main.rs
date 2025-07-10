//! Main entry point for the `rudu` CLI application.
//!
//! `rudu` is a fast, Rust-powered replacement for the traditional `du` (disk usage) command.
//! It provides disk usage summaries with support for file filtering, depth control,
//! user ownership display, CSV export, and a progress spinner.
//!
//! # Responsibilities
//! - Parses CLI arguments via [`clap`] using the [`Args`] struct
//! - Sets up glob-based file/directory exclusion rules
//! - Delegates directory traversal and size aggregation to [`scan::scan_files_and_dirs`]
//! - Handles terminal or CSV output formatting and sorting
//!
//! # Output Modes
//! - Terminal table view with size, owner, and type markers
//! - CSV export via `--output <file.csv>`
//!
//! # Flags of Interest
//! - `--depth N`: Limit directory depth in output
//! - `--exclude PATTERN`: Skip matching paths
//! - `--show-owner`: Show username for each entry
//! - `--sort size|name`: Sort output by size or name
//!
//! # Modules
//! - [`scan`] - file system traversal and size aggregation
//! - [`utils`] - helpers for file metadata, ownership, and pattern matching

use anyhow::{Context, Result};
use clap::Parser;
use csv::Writer;
use humansize::{format_size, DECIMAL};
use std::fs::File;
use std::path::Path;

mod utils;
use utils::{build_exclude_matcher, expand_exclude_patterns, path_depth};
mod scan;
use scan::scan_files_and_dirs;
pub mod cli;
use cli::{Args, CsvEntry};
mod data;
pub use data::{EntryType, FileEntry};
pub mod output;

/// Sets up the thread pool configuration based on CLI arguments.
fn setup_thread_pool(args: &Args) -> Result<()> {
    if let Some(n_threads) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n_threads)
            .build_global()
            .context("Failed to configure thread pool")?;
        println!("🔧 Using {} CPU thread(s)", n_threads);
    } else {
        println!("🔧 Using all {} available CPU threads", num_cpus::get());
    }
    Ok(())
}

/// Processes raw file entries by applying depth filtering, sorting, and show_files flags.
fn process_entries(root: &Path, args: &Args, raw: Vec<FileEntry>) -> Vec<FileEntry> {
    raw.into_iter()
        .filter(|entry| {
            // Apply depth filtering
            let depth = path_depth(root, &entry.path);
            match entry.entry_type {
                EntryType::Dir => args.depth.map(|d| depth <= d).unwrap_or(true),
                EntryType::File => {
                    args.show_files && args.depth.map(|d| depth == d).unwrap_or(true)
                }
            }
        })
        .collect()
}

/// Outputs the results either to CSV file or terminal based on CLI arguments.
fn output_results(entries: &[FileEntry], args: &Args, root: &Path) -> Result<()> {
    if let Some(csv_path) = &args.output {
        // CSV output
        let file = File::create(csv_path)?;
        let mut writer = Writer::from_writer(file);

        for entry in entries {
            let csv_entry = CsvEntry {
                entry_type: entry.entry_type.as_str().into(),
                size_bytes: entry.size,
                size_human: format_size(entry.size, DECIMAL),
                owner: entry.owner.clone(),
                path: entry.path.display().to_string(),
                inodes: entry.inodes,
            };
            writer.serialize(csv_entry)?;
        }
        writer.flush()?;
        println!("Output saved to: {}", csv_path);
    } else {
        // Terminal output
        for entry in entries {
            let owner = if args.show_owner {
                entry.owner.clone().unwrap_or_else(|| "unknown".to_string())
            } else {
                "".to_string()
            };

            match entry.entry_type {
                EntryType::Dir => {
                    println!(
                        "[DIR]  {:<12} {:<10} {:<6} {}",
                        format_size(entry.size, DECIMAL),
                        owner,
                        entry.inodes.unwrap_or(0),
                        entry
                            .path
                            .strip_prefix(root)
                            .unwrap_or(&entry.path)
                            .display()
                    );
                }
                EntryType::File => {
                    println!(
                        "[FILE] {:<12} {:<10} {}",
                        format_size(entry.size, DECIMAL),
                        owner,
                        entry
                            .path
                            .strip_prefix(root)
                            .unwrap_or(&entry.path)
                            .display()
                    );
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root = &args.path;

    // Print banner
    println!(
        r#"
------------------------------------------------------------------
        .______       __    __   _______   __    __  
        |   _  \     |  |  |  | |       \ |  |  |  | 
        |  |_)  |    |  |  |  | |  .--.  ||  |  |  | 
        |      /     |  |  |  | |  |  |  ||  |  |  | 
        |  |\  \----.|  `--'  | |  '--'  ||  `--'  | 
        | _| `._____| \______/  |_______/  \______/
                    Rust-based du tool
------------------------------------------------------------------            
                    "#
    );

    // Parse args → setup_thread_pool → scan_files_and_dirs → process_entries → output_results
    setup_thread_pool(&args)?;

    let expanded_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher = build_exclude_matcher(&expanded_patterns)?;

    let raw_entries = scan_files_and_dirs(root, &args, &exclude_matcher, args.sort)?;
    let processed_entries = process_entries(root, &args, raw_entries);
    output_results(&processed_entries, &args, root)?;

    Ok(())
}
