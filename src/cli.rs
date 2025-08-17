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
/// ```
#[derive(Parser, Debug, Clone)]
#[command(name = "rudu", author = "Sam Green", version = "1.4.0", about)]
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
    pub output: Option<String>,

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

    /// Memory check interval in milliseconds for memory monitoring (hidden experimental flag)
    #[arg(
        long = "memory-check-interval-ms",
        value_name = "MS",
        default_value_t = 200,
        hide = true
    )]
    pub memory_check_interval_ms: u64,
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
#[derive(Debug, serde::Serialize)]
pub struct CsvEntry {
    pub entry_type: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub owner: Option<String>,
    pub path: String,
    pub inodes: Option<u64>,
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
        assert_eq!(args.show_files, true);
        assert_eq!(args.exclude, Vec::<String>::new());
        assert_eq!(args.show_owner, false);
        assert_eq!(args.output, None);
        assert_eq!(args.threads, None);
        assert_eq!(args.show_inodes, false);
        assert_eq!(args.no_cache, false);
        assert_eq!(args.cache_ttl, 604800);
        assert_eq!(args.profile, false);
        assert_eq!(args.memory_limit, None);
        assert_eq!(args.memory_check_interval_ms, 200);
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
