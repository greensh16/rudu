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

use anyhow::Result;
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
pub mod cache;
pub mod metrics;
pub mod output;
pub mod thread_pool;
use metrics::{print_profile_summary, rss_after_phase, save_stats_json, PhaseTimer, ProfileData};
use thread_pool::{configure_pool, ThreadPoolStrategy};

/// Sets up the thread pool configuration based on CLI arguments.
fn setup_thread_pool(args: &Args) -> Result<()> {
    // Skip global thread pool setup when --threads is specified
    // as we'll use local thread pools in the scan module instead
    if args.threads.is_some() {
        println!(
            "🔧 Using local thread pool with {} threads",
            args.threads.unwrap()
        );
        return Ok(());
    }

    // Use the new thread pool configuration system for other strategies
    let n_threads = match args.threads_strategy {
        ThreadPoolStrategy::Default => num_cpus::get(),
        ThreadPoolStrategy::Fixed => num_cpus::get(), // Default to num_cpus if not specified
        ThreadPoolStrategy::NumCpusMinus1 => std::cmp::max(1, num_cpus::get() - 1),
        ThreadPoolStrategy::IOHeavy => num_cpus::get() * 2,
        ThreadPoolStrategy::WorkStealingUneven => num_cpus::get(),
    };

    configure_pool(args.threads_strategy, n_threads)?;
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

    // Initialize profiling if enabled
    let mut profile = if args.profile {
        Some(ProfileData::new())
    } else {
        None
    };

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
    let setup_timer = if args.profile {
        Some(PhaseTimer::new("Setup"))
    } else {
        None
    };

    setup_thread_pool(&args)?;

    let expanded_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher = build_exclude_matcher(&expanded_patterns)?;

    if let (Some(ref mut prof), Some(timer)) = (profile.as_mut(), setup_timer) {
        prof.add_phase(timer.finish());
    }

    // Time the scanning phase
    let scan_timer = if args.profile {
        Some(PhaseTimer::new("WalkDir"))
    } else {
        None
    };

    let scan_result = scan_files_and_dirs(root, &args, &exclude_matcher, args.sort)?;

    if let (Some(ref mut prof), Some(timer)) = (profile.as_mut(), scan_timer) {
        let total_scan_time = timer.finish();

        // Add detailed phase timings from scan result, or fallback to total time
        if !scan_result.phase_timings.is_empty() {
            for phase in scan_result.phase_timings {
                prof.add_phase(phase);
            }
        } else {
            prof.add_phase(total_scan_time);
        }

        // Add cache statistics to profile
        prof.set_cache_stats(scan_result.cache_hits, scan_result.cache_total);
    }

    // Time the processing phase
    let process_timer = if args.profile {
        Some(PhaseTimer::new("Filtering"))
    } else {
        None
    };

    let processed_entries = process_entries(root, &args, scan_result.entries);

    if let (Some(ref mut prof), Some(timer)) = (profile.as_mut(), process_timer) {
        prof.add_phase(timer.finish());
    }

    // Time the output phase
    let output_timer = if args.profile {
        Some(PhaseTimer::new("Output"))
    } else {
        None
    };

    output_results(&processed_entries, &args, root)?;

    if let (Some(ref mut prof), Some(timer)) = (profile.as_mut(), output_timer) {
        prof.add_phase(timer.finish());
    }

    // Capture final memory usage and display profile if enabled
    if let Some(mut prof) = profile {
        prof.memory_peak = rss_after_phase();

        // Add metadata about the scan
        prof.add_metadata("entries_processed", &processed_entries.len().to_string());
        prof.add_metadata("root_path", &root.display().to_string());
        if let Some(depth) = args.depth {
            prof.add_metadata("max_depth", &depth.to_string());
        }

        // Display profile summary
        print_profile_summary(&prof);

        // Save stats.json if output is being written to a file
        if let Some(ref output_path) = args.output {
            if let Err(e) = save_stats_json(std::path::Path::new(output_path), &prof) {
                eprintln!("Failed to save stats.json: {}", e);
            }
        }
    }

    Ok(())
}
