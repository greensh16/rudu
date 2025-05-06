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

use clap::Parser;
use humansize::{format_size, DECIMAL};
use std::{
    time::Duration,
    fs::File,
};
use indicatif::{ProgressBar, ProgressStyle};
use csv::Writer;
use anyhow::Result;

mod utils;
use utils::{path_depth, get_owner, expand_exclude_patterns, build_exclude_matcher};
mod scan;
use scan::scan_files_and_dirs;
mod cli;
use cli::{Args, CsvEntry};
 
fn main() -> Result<()> {
    let args = Args::parse();
    let root = &args.path;
    let expanded_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher = build_exclude_matcher(&expanded_patterns);

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

    // Create progress spinner
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        .template("{spinner} Scanning files... [{elapsed}]")
        .unwrap());
    pb.enable_steady_tick(Duration::from_millis(100));

    let (file_data, sorted_dirs) = scan_files_and_dirs(root, &args, &exclude_matcher, args.sort);

    // Step 5: Print CSV output
    let mut csv_entries = Vec::new();

    // For directories
    for (dir, size) in &sorted_dirs {
        if args.depth.map(|d| path_depth(root, dir) > d).unwrap_or(false) {
            continue;
        }
    
        let owner = if args.show_owner {
            get_owner(dir)
        } else {
            None
        };
    
        csv_entries.push(CsvEntry {
            entry_type: "DIR".into(),
            size_bytes: *size,
            size_human: format_size(*size, DECIMAL),
            owner,
            path: dir.display().to_string(),
        });
    }

    // For files
    if args.show_files {
        for (file_path, size) in &file_data {
            if args.depth.map(|d| path_depth(root, file_path) != d).unwrap_or(false) {
                continue;
            }
    
            let owner = if args.show_owner {
                get_owner(file_path)
            } else {
                None
            };
    
            csv_entries.push(CsvEntry {
                entry_type: "FILE".into(),
                size_bytes: *size,
                size_human: format_size(*size, DECIMAL),
                owner,
                path: file_path.display().to_string(),
            });
        }
    }

    if let Some(csv_path) = &args.output {
        let file = File::create(csv_path)?;
        let mut writer = Writer::from_writer(file);
        for row in &csv_entries {
            writer.serialize(row)?;
        }
        writer.flush()?;
        println!("Output saved to: {}", csv_path);
    }

    // Step 5: Print directories within depth
    if args.output.is_none() {
        for (dir, size) in &sorted_dirs {
            if args.depth.map(|d| path_depth(root, dir) > d).unwrap_or(false) {
                continue;
            }
    
            let owner = if args.show_owner {
                get_owner(dir).unwrap_or_else(|| "unknown".to_string())
            } else {
                "".to_string()
            };
    
            println!(
                "[DIR]  {:<12} {:<10} {}",
                format_size(*size, DECIMAL),
                owner,
                dir.strip_prefix(root).unwrap_or(dir).display()
            );
        }
    }

    // Step 6: Print files at exact depth (not parent!)
    if args.output.is_none() && args.show_files {
        for (file, size) in &file_data {
            if args.depth.map(|d| path_depth(root, file) != d).unwrap_or(false) {
                continue;
            }
    
            let owner = if args.show_owner {
                get_owner(file).unwrap_or_else(|| "unknown".to_string())
            } else {
                "".to_string()
            };
    
            println!(
                "[FILE] {:<12} {:<10} {}",
                format_size(*size, DECIMAL),
                owner,
                file.strip_prefix(root).unwrap_or(file).display()
            );
        }
    }
    Ok(())
}