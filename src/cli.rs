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

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Rust-powered disk usage calculator (like `du`, but faster and safer)
#[derive(Parser, Debug)]
#[command(name = "rudu", author = "Sam Green", version = "1.1.0", about)]
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
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum SortKey {
    Name,
    Size,
}

/// A single record of output (used for CSV serialization)
#[derive(Debug, serde::Serialize)]
pub struct CsvEntry {
    pub entry_type: String, // "DIR" or "FILE"
    pub size_bytes: u64,
    pub size_human: String,
    pub owner: Option<String>,
    pub path: String,
}