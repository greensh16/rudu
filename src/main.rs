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
use std::path::Path;

// All modules live in the library crate (src/lib.rs). Previously main.rs
// declared its own copies of every module, compiling the whole tree twice
// and giving the binary distinct copies of library statics.
use rudu::atime;
use rudu::cli::Args;
use rudu::data::{EntryType, FileEntry};
use rudu::metrics::{
    PhaseTimer, ProfileData, print_profile_summary, rss_after_phase, save_stats_json,
};
use rudu::scan::{self, scan_files_and_dirs};
use rudu::thread_pool::{ThreadPoolStrategy, configure_pool};
use rudu::utils::{build_exclude_matcher, expand_exclude_patterns, path_depth};
use rudu::{memory, output};

/// Sets up the thread pool configuration based on CLI arguments.
fn setup_thread_pool(args: &Args) -> Result<()> {
    // An explicit --threads N takes precedence over any strategy: build the
    // global rayon pool with exactly that many threads. (Previously this
    // branch returned without configuring anything, silently running on all
    // cores — including in --memory-limit "HPC" mode.)
    if let Some(n) = args.threads {
        if n == 0 {
            anyhow::bail!("--threads must be greater than 0");
        }
        configure_pool(ThreadPoolStrategy::Fixed, n)?;
        return Ok(());
    }

    // Use the thread pool configuration system for the selected strategy
    let n_threads = match args.threads_strategy {
        ThreadPoolStrategy::Default => num_cpus::get(),
        ThreadPoolStrategy::Fixed => {
            // Unreachable with --threads set (handled above); without it,
            // fall back to all CPUs with a warning.
            eprintln!(
                "Warning: --threads-strategy fixed requires --threads N; \
                 falling back to all CPUs."
            );
            num_cpus::get()
        }
        ThreadPoolStrategy::NumCpusMinus1 => std::cmp::max(1, num_cpus::get() - 1),
        ThreadPoolStrategy::IOHeavy => num_cpus::get() * 2,
        ThreadPoolStrategy::WorkStealingUneven => num_cpus::get(),
    };

    configure_pool(args.threads_strategy, n_threads)?;
    Ok(())
}

/// Processes raw file entries by applying depth, access-age, and show_files filters.
///
/// `--older-than` is applied here rather than during the scan because a
/// directory's access age is a rollup over its whole subtree: the entries that
/// justify keeping a directory row may themselves be filtered out of the
/// display. A directory therefore survives the filter when *anything* beneath
/// it is old enough, which is what makes `--older-than` usable as a drill-down
/// alongside `--depth`.
///
/// A file whose atime could not be read is kept: dropping it would silently hide
/// data from a report whose purpose is to account for everything at risk. A
/// *directory* with no access age has no leaves beneath it and so nothing at
/// risk, and is dropped rather than padding the report with empty directories.
///
/// `--min-size` applies to files and directories alike. Hiding a small directory
/// never hides anything the user asked to see: a directory's size is the total
/// of its whole subtree, so if it is under the threshold every entry beneath it
/// is too.
fn process_entries(root: &Path, args: &Args, raw: Vec<FileEntry>, now: u64) -> Vec<FileEntry> {
    raw.into_iter()
        .filter(|entry| {
            // Apply depth filtering
            let depth = path_depth(root, &entry.path);
            let depth_ok = match entry.entry_type {
                EntryType::Dir => args.depth.map(|d| depth <= d).unwrap_or(true),
                EntryType::File => {
                    args.show_files && args.depth.map(|d| depth <= d).unwrap_or(true)
                }
            };

            let age_ok = match (args.older_than, entry.atime) {
                (None, _) => true,
                (Some(min_days), Some(at)) => atime::age_days(at, now) >= min_days,
                (Some(_), None) => entry.entry_type == EntryType::File,
            };

            let size_ok = args.min_size.is_none_or(|min| entry.size >= min);

            depth_ok && age_ok && size_ok
        })
        .collect()
}

/// Outputs the results either to CSV file or terminal based on CLI arguments.
///
/// Delegates to the modular output formatters in [`output`] so that both
/// code paths share the same serialisation logic and schema.
fn output_results(entries: &[FileEntry], args: &Args, root: &Path, now: u64) -> Result<()> {
    if args.output.is_some() {
        output::render_csv(entries, args, now)
    } else {
        output::render_terminal(entries, args, root, now)
    }
}

fn main() -> Result<()> {
    let mut args = Args::parse();
    // Asking to filter by access age without asking to see it would print a
    // mysteriously short table, so --older-than turns the columns on.
    if args.older_than.is_some() {
        args.show_atime = true;
    }
    let args = args;
    let root = &args.path;

    // One reference instant for the whole report: ages computed at different
    // points of a long scan would otherwise disagree across rows.
    let now = atime::now_unix();

    // Initialize profiling if enabled
    let mut profile = if args.profile {
        Some(ProfileData::new())
    } else {
        None
    };

    // Print banner
    eprintln!(
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

    // Apply conservative thread settings if memory limit is specified
    let mut modified_args = args.clone();
    if args.memory_limit.is_some() && args.threads.is_none() {
        // Use at most 2 threads in HPC mode to reduce memory pressure
        modified_args.threads = Some(std::cmp::min(2, num_cpus::get()));
        eprintln!(
            "HPC mode: Using {} threads to minimize memory usage",
            modified_args.threads.unwrap()
        );
    }

    setup_thread_pool(&modified_args)?;

    let expanded_patterns = expand_exclude_patterns(&modified_args.exclude);
    let exclude_matcher = build_exclude_matcher(&expanded_patterns)?;

    if let (Some(ref mut prof), Some(timer)) = (profile.as_mut(), setup_timer) {
        prof.add_phase(timer.finish());
    }

    // Create memory monitor if memory limit is specified
    let memory_monitor = if let Some(memory_limit_mb) = modified_args.memory_limit {
        eprintln!("Memory limit set to {} MB", memory_limit_mb);
        eprintln!(
            "WARNING: HPC mode: Using conservative settings for resource-constrained environments"
        );
        let monitor = memory::MemoryMonitor::new_with_interval(
            memory_limit_mb,
            modified_args.memory_check_interval_ms,
        );
        Some(std::sync::Arc::new(std::sync::Mutex::new(monitor)))
    } else {
        None
    };

    // Time the scanning phase
    let scan_timer = if args.profile {
        Some(PhaseTimer::new("WalkDir"))
    } else {
        None
    };

    let scan_result = if memory_monitor.is_some() {
        scan::scan_files_and_dirs_with_memory_monitor(
            root,
            &modified_args,
            &exclude_matcher,
            modified_args.sort,
            memory_monitor,
        )?
    } else {
        scan_files_and_dirs(root, &modified_args, &exclude_matcher, modified_args.sort)?
    };

    // Check if memory limit was hit during scanning
    if scan_result.memory_limit_hit {
        eprintln!(
            "WARNING: Memory limit reached ({} MB). Showing partial results.",
            modified_args.memory_limit.unwrap()
        );
    }

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

    // Access-age rollups and the summary are computed over the *complete* entry
    // list, before --depth and --older-than filtering: a directory's oldest
    // entry and its at-risk bytes come from leaves that the display filters
    // will often drop, and a summary of the filtered view would be circular.
    let mut scan_entries = scan_result.entries;
    if args.show_atime {
        atime::apply_rollup(&mut scan_entries, root, args.purge_days, now);
    }
    let age_summary = if args.show_atime {
        Some(atime::summarize(&scan_entries, args.purge_days, now))
    } else {
        None
    };

    let processed_entries = process_entries(root, &args, scan_entries, now);

    if let (Some(ref mut prof), Some(timer)) = (profile.as_mut(), process_timer) {
        prof.add_phase(timer.finish());
    }

    // Time the output phase
    let output_timer = if args.profile {
        Some(PhaseTimer::new("Output"))
    } else {
        None
    };

    output_results(&processed_entries, &args, root, now)?;

    // Summary goes to stderr, like the banner and cache statistics, so it never
    // contaminates a piped table or a CSV stream on stdout.
    if let Some(summary) = age_summary {
        eprint!("{}", atime::render_summary(&summary, args.purge_days));
    }

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
        if let Some(ref output_path) = args.output
            && let Err(e) = save_stats_json(output_path, &prof)
        {
            eprintln!("Failed to save stats file: {}", e);
        }
    }

    Ok(())
}
