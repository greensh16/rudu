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
///
/// # Examples
///
/// ```rust
/// use rudu::Args;
/// use clap::Parser;
///
/// // Parse arguments from command line
/// let args = Args::parse();
///
/// // Check if CSV output is requested
/// if args.output.is_some() {
///     println!("CSV output enabled");
/// }
/// ```
#[derive(Parser, Debug)]
#[command(name = "rudu", author = "Sam Green", version = "1.3.0", about)]
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
