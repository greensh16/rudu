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
use std::sync::{Arc, Mutex};

// All modules live in the library crate (src/lib.rs). Previously main.rs
// declared its own copies of every module, compiling the whole tree twice
// and giving the binary distinct copies of library statics.
use rudu::atime;
use rudu::cli::{Args, OutputFormat};
use rudu::data::{EntryType, FileEntry};
use rudu::filter::process_entries;
use rudu::metrics::{
    PhaseTimer, ProfileData, print_profile_summary, rss_after_phase, save_stats_json,
};
use rudu::scan::{self, scan_files_and_dirs};
use rudu::thread_pool::{ThreadPoolStrategy, configure_pool};
use rudu::utils::{
    AUTO_EXCLUDES, NETWORK_FS_THREADS, build_exclude_matcher, expand_exclude_patterns,
    network_fs_type, with_auto_excludes,
};
use rudu::{memory, output, report};

/// Sets up the thread pool configuration based on CLI arguments.
///
/// `roots` are the directories about to be scanned. When no `--threads` or
/// strategy was given and any root sits on a network or parallel filesystem,
/// the pool is capped at [`NETWORK_FS_THREADS`] rather than all cores.
fn setup_thread_pool(args: &Args, roots: &[&Path]) -> Result<()> {
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

    if args.threads_strategy == ThreadPoolStrategy::Default
        && let Some((root, fs)) = roots
            .iter()
            .find_map(|r| network_fs_type(r).map(|fs| (r, fs)))
    {
        let n = num_cpus::get().min(NETWORK_FS_THREADS);
        eprintln!(
            "{} is on {}, a network filesystem: using {} threads to spare the \
             metadata servers (--threads N to override)",
            root.display(),
            fs,
            n
        );
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

/// Writes the displayed rows in the selected `--format`.
///
/// CSV and JSON share one row conversion ([`output::csv::row`]), so the two
/// can never disagree on a field.
fn output_results(
    entries: &[FileEntry],
    args: &Args,
    root: &Path,
    now: u64,
    json: &output::json::JsonContext,
) -> Result<()> {
    match args.output_format()? {
        OutputFormat::Table => output::render_terminal(entries, args, root, now),
        OutputFormat::Csv => output::render_csv(entries, args, now),
        OutputFormat::Json => output::render_json(entries, args, now, json),
    }
}

/// The `--report` sources: `--source` values, or the positional PATH alone.
fn report_sources(args: &Args) -> Vec<report::Source> {
    if args.source.is_empty() {
        vec![report::Source {
            label: report::default_label(&args.path),
            path: args.path.clone(),
        }]
    } else {
        args.source.clone()
    }
}

/// `--report`: scans each source in turn and writes the HTML stocktake.
///
/// Each source's entries are folded into the running totals and dropped before
/// the next scan starts, so peak memory is that of the largest source alone.
fn run_report(
    report_path: &Path,
    sources: &[report::Source],
    args: &Args,
    exclude_matcher: &globset::GlobSet,
    memory_monitor: Option<Arc<Mutex<memory::MemoryMonitor>>>,
    now: u64,
) -> Result<()> {
    // The report is a per-owner rollup, so owners are always resolved.
    let mut scan_args = args.clone();
    scan_args.show_owner = true;

    let mut stocktake = report::Stocktake::new(args.purge_days, now);
    for (n, source) in sources.iter().enumerate() {
        eprintln!(
            "[{}/{}] Scanning {} ({})",
            n + 1,
            sources.len(),
            source.label,
            source.path.display()
        );
        let result = if memory_monitor.is_some() {
            scan::scan_files_and_dirs_with_memory_monitor(
                &source.path,
                &scan_args,
                exclude_matcher,
                scan_args.sort,
                memory_monitor.clone(),
            )?
        } else {
            scan_files_and_dirs(&source.path, &scan_args, exclude_matcher, scan_args.sort)?
        };
        if result.memory_limit_hit {
            eprintln!(
                "WARNING: Memory limit reached while scanning {}; its totals are partial.",
                source.label
            );
        }
        stocktake.add_source(source, &result.entries, result.memory_limit_hit);
    }

    let meta = report::ReportMeta {
        title: args.report_title.clone(),
        command: std::env::args().collect::<Vec<_>>().join(" "),
    };
    stocktake.write(report_path, &meta)?;

    for s in stocktake.sources() {
        eprintln!(
            "{:>12}  {:>10}  {:>3}% not accessed for {}+ days  {} inodes",
            s.label,
            humansize::format_size(s.bytes, humansize::DECIMAL),
            (s.stale * 100).checked_div(s.bytes).unwrap_or(0),
            args.purge_days,
            s.inodes
        );
    }
    eprintln!("Report written to {}", report_path.display());
    Ok(())
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
    // Reject `--format table --output FILE` before a potentially long scan.
    args.output_format()?;

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

    // Report sources are validated before anything else runs, so a typo in
    // the fifth path fails now rather than an hour into the first four scans.
    let sources = if args.report.is_some() {
        let sources = report_sources(&args);
        report::validate_sources(&sources)?;
        sources
    } else {
        Vec::new()
    };
    let roots: Vec<&Path> = if args.report.is_some() {
        sources.iter().map(|s| s.path.as_path()).collect()
    } else {
        vec![root.as_path()]
    };
    setup_thread_pool(&modified_args, &roots)?;

    if args.auto_exclude {
        modified_args.exclude = with_auto_excludes(&modified_args.exclude);
        eprintln!("Auto-excluding: {}", AUTO_EXCLUDES.join(" "));
    }

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
        Some(Arc::new(Mutex::new(monitor)))
    } else {
        None
    };

    if let Some(report_path) = &args.report {
        return run_report(
            report_path,
            &sources,
            &modified_args,
            &exclude_matcher,
            memory_monitor,
            now,
        );
    }

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

    // Whole-scan totals for `--format json`, also taken before filtering.
    let totals = output::json::ScanTotals {
        total_bytes: scan_entries
            .iter()
            .find(|e| e.path == *root)
            .map_or(0, |e| e.size),
        files: scan_entries
            .iter()
            .filter(|e| e.entry_type == EntryType::File)
            .count() as u64,
        dirs: scan_entries
            .iter()
            .filter(|e| e.entry_type == EntryType::Dir)
            .count() as u64,
        cache_hits: scan_result.cache_hits,
        cache_total: scan_result.cache_total,
        partial: scan_result.memory_limit_hit,
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

    let json_ctx = output::json::JsonContext {
        totals: &totals,
        age_summary: age_summary.as_ref(),
    };
    output_results(&processed_entries, &args, root, now, &json_ctx)?;

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
