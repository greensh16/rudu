//! CLI interface definitions for the `rudu` application.
//!
//! This module defines command-line arguments using [`clap`] and exposes:
//!
//! - [`Args`]: the main struct parsed from CLI inputs
//! - [`SortKey`]: an enum for sorting output by `size` or `name`
//!
//! The `Args` struct is used in `main.rs` and other modules to control behavior
//! such as filtering, depth limits, file visibility, and output formatting.
//!
//! # Example
//!
//! ```bash
//! rudu --depth 2 --exclude target node_modules --sort size --output disk.csv
//! ```
//!
//! # Dependencies
//! - [`clap`] for argument parsing and help generation

use crate::thread_pool::ThreadPoolStrategy;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Command-line arguments for the `rudu` disk usage calculator.
///
/// This struct defines all available command-line options and flags
/// for controlling the behavior of the file system scan and output formatting.
#[derive(Parser, Debug, Clone)]
#[command(name = "rudu", author = "Sam Green", version = env!("CARGO_PKG_VERSION"), about)]
pub struct Args {
    /// Path to scan (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Limit output to directories up to N levels deep
    #[arg(long)]
    pub depth: Option<usize>,

    /// Sort output by name or size
    #[arg(long, value_enum, default_value_t = SortKey::Name)]
    pub sort: SortKey,

    /// Show individual files at the target depth (default: true)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub show_files: bool,

    /// Exclude entries with matching names (e.g., '.git', 'node_modules')
    #[arg(long, value_name = "PATTERN", num_args = 1.., action = clap::ArgAction::Append)]
    pub exclude: Vec<String>,

    /// Show owner (username) of each file/directory
    #[arg(long, default_value_t = false)]
    pub show_owner: bool,

    /// Write output to a CSV file instead of stdout
    #[arg(long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Limit the number of CPU threads used (default: use all available)
    #[arg(long, value_name = "N")]
    pub threads: Option<usize>,

    /// Show inode usage (i.e., number of files/subdirectories in each dir)
    #[arg(long, default_value_t = false)]
    pub show_inodes: bool,

    /// Thread pool strategy for performance optimization (hidden experimental flag)
    #[arg(long = "threads-strategy", value_enum, default_value_t = ThreadPoolStrategy::Default, hide = true)]
    pub threads_strategy: ThreadPoolStrategy,

    /// Disable caching and force a full rescan
    #[arg(long, default_value_t = false)]
    pub no_cache: bool,

    /// Cache TTL in seconds (default: 604800 = 7 days)
    #[arg(long, default_value_t = 604800)]
    pub cache_ttl: u64,

    /// Enable performance profiling and show timing summary
    #[arg(long, default_value_t = false)]
    pub profile: bool,

    /// Set memory usage limit in megabytes (MB)
    #[arg(long, value_name = "MB")]
    pub memory_limit: Option<u64>,

    /// Show last access time (atime) and access age in days
    ///
    /// For directories this reports the *oldest* entry in the subtree — the one
    /// that will be purged first — plus the bytes beneath it already past the
    /// `--purge-days` threshold.
    #[arg(long, default_value_t = false)]
    pub show_atime: bool,

    /// Access-age threshold in days for "at risk" reporting (NCI /scratch: 100)
    #[arg(long, value_name = "DAYS", default_value_t = crate::atime::DEFAULT_PURGE_DAYS)]
    pub purge_days: u64,

    /// Only show entries not accessed for at least DAYS days (implies --show-atime)
    #[arg(long, value_name = "DAYS")]
    pub older_than: Option<u64>,

    /// Hide entries smaller than SIZE (e.g. 10MB, 1.5GiB, 4096)
    ///
    /// A display filter only: hidden entries still count toward their parent
    /// directories' totals, so the sizes shown stay correct. Units are decimal
    /// by default (KB/MB/GB = powers of 1000), matching the sizes rudu prints;
    /// use KiB/MiB/GiB for powers of 1024.
    #[arg(long, value_name = "SIZE", value_parser = crate::utils::parse_size)]
    pub min_size: Option<u64>,

    /// Memory check interval in milliseconds for memory monitoring (hidden experimental flag)
    #[arg(
        long = "memory-check-interval-ms",
        value_name = "MS",
        default_value_t = 200,
        hide = true
    )]
    pub memory_check_interval_ms: u64,
}

impl Default for Args {
    /// The same values `rudu` runs with when given no arguments.
    ///
    /// Derived by parsing an empty argument list rather than repeating the
    /// literals, so this can never drift from the `#[arg(default_value_t)]`
    /// attributes above. Every field has a default, so the parse cannot fail.
    ///
    /// Benchmarks and tests build `Args` with `..Default::default()` so that
    /// adding a new flag does not break every call site.
    fn default() -> Self {
        Args::try_parse_from(["rudu"]).expect("Args has a default for every field")
    }
}

/// Enum for specifying how to sort scan results.
///
/// # Variants
/// * `Name` - Sort entries alphabetically by path name
/// * `Size` - Sort entries by size in descending order (largest first)
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum SortKey {
    Name,
    Size,
}

/// A single record of output (used for CSV serialization).
///
/// # Fields
/// * `entry_type` - "DIR" or "FILE"
/// * `size_bytes` - Size in bytes
/// * `size_human` - Human-readable size (e.g., "1.2 MB")
/// * `owner` - Optional owner username
/// * `path` - Full path to the file or directory
/// * `inodes` - Optional inode count for directories
/// * `atime` - Last access date (`YYYY-MM-DD`), only with `--show-atime`
/// * `atime_unix` - Last access time as Unix seconds, only with `--show-atime`
/// * `age_days` - Whole days since last access, only with `--show-atime`
/// * `at_risk_bytes` - Directories only: bytes past the `--purge-days`
///   threshold beneath this directory
#[derive(Debug, serde::Serialize)]
pub struct CsvEntry {
    pub entry_type: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub owner: Option<String>,
    pub path: String,
    pub inodes: Option<u64>,
    pub atime: Option<String>,
    pub atime_unix: Option<u64>,
    pub age_days: Option<u64>,
    pub at_risk_bytes: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_memory_limit_parsing() {
        // Test with memory limit specified
        let args = Args::try_parse_from(["rudu", "--memory-limit", "512"]).unwrap();
        assert_eq!(args.memory_limit, Some(512));

        // Test without memory limit (should be None)
        let args = Args::try_parse_from(["rudu"]).unwrap();
        assert_eq!(args.memory_limit, None);

        // Test with invalid memory limit (should fail)
        let result = Args::try_parse_from(["rudu", "--memory-limit", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_values() {
        let args = Args::try_parse_from(["rudu"]).unwrap();

        assert_eq!(args.path, PathBuf::from("."));
        assert_eq!(args.depth, None);
        assert_eq!(args.sort, SortKey::Name);
        assert!(args.show_files);
        assert_eq!(args.exclude, Vec::<String>::new());
        assert!(!args.show_owner);
        assert_eq!(args.output, None);
        assert_eq!(args.threads, None);
        assert!(!args.show_inodes);
        assert!(!args.no_cache);
        assert_eq!(args.cache_ttl, 604800);
        assert!(!args.profile);
        assert_eq!(args.memory_limit, None);
        assert_eq!(args.memory_check_interval_ms, 200);
        assert!(!args.show_atime);
        assert_eq!(args.purge_days, 100);
        assert_eq!(args.older_than, None);
        assert_eq!(args.min_size, None);
    }

    #[test]
    fn test_default_impl_matches_cli_defaults() {
        // Guards the `..Default::default()` used across benches and tests: if
        // Default ever drifted from the parsed defaults, those call sites would
        // silently benchmark something other than what `rudu` actually runs.
        let parsed = Args::try_parse_from(["rudu"]).unwrap();
        let defaulted = Args::default();

        assert_eq!(parsed.path, defaulted.path);
        assert_eq!(parsed.sort, defaulted.sort);
        assert_eq!(parsed.show_files, defaulted.show_files);
        assert_eq!(parsed.cache_ttl, defaulted.cache_ttl);
        assert_eq!(parsed.purge_days, defaulted.purge_days);
        assert_eq!(
            parsed.memory_check_interval_ms,
            defaulted.memory_check_interval_ms
        );
    }

    #[test]
    fn test_min_size_flag_parsing() {
        let args = Args::try_parse_from(["rudu", "--min-size", "10MB"]).unwrap();
        assert_eq!(args.min_size, Some(10_000_000));

        // Binary units are available for anyone who wants powers of 1024.
        let args = Args::try_parse_from(["rudu", "--min-size", "1GiB"]).unwrap();
        assert_eq!(args.min_size, Some(1_073_741_824));

        // A bare number is a byte count.
        let args = Args::try_parse_from(["rudu", "--min-size", "4096"]).unwrap();
        assert_eq!(args.min_size, Some(4096));

        // The parser rejects nonsense at the CLI boundary rather than silently
        // filtering on 0 and showing everything.
        assert!(Args::try_parse_from(["rudu", "--min-size", "big"]).is_err());
        assert!(Args::try_parse_from(["rudu", "--min-size", "10 flurbs"]).is_err());
    }

    #[test]
    fn test_atime_flag_parsing() {
        let args = Args::try_parse_from(["rudu", "--show-atime", "--older-than", "100"]).unwrap();
        assert!(args.show_atime);
        assert_eq!(args.older_than, Some(100));

        // A site with a different policy window
        let args = Args::try_parse_from(["rudu", "--purge-days", "30"]).unwrap();
        assert_eq!(args.purge_days, 30);

        assert!(Args::try_parse_from(["rudu", "--older-than", "soon"]).is_err());
    }

    #[test]
    fn test_memory_limit_with_other_args() {
        let args = Args::try_parse_from([
            "rudu",
            "--memory-limit",
            "1024",
            "--depth",
            "3",
            "--threads",
            "4",
            "/some/path",
        ])
        .unwrap();

        assert_eq!(args.memory_limit, Some(1024));
        assert_eq!(args.depth, Some(3));
        assert_eq!(args.threads, Some(4));
        assert_eq!(args.path, PathBuf::from("/some/path"));
    }

    #[test]
    fn test_memory_check_interval_parsing() {
        // Test with custom memory check interval
        let args = Args::try_parse_from(["rudu", "--memory-check-interval-ms", "500"]).unwrap();
        assert_eq!(args.memory_check_interval_ms, 500);

        // Test default value
        let args = Args::try_parse_from(["rudu"]).unwrap();
        assert_eq!(args.memory_check_interval_ms, 200);

        // Test with invalid value (should fail)
        let result = Args::try_parse_from(["rudu", "--memory-check-interval-ms", "invalid"]);
        assert!(result.is_err());
    }
}
