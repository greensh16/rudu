//! Library crate for rudu
//!
//! This exposes the modules needed for testing and potential library usage.
//!
//! # Features
//!
//! - **File System Scanning**: Efficiently scan directories with parallel processing
//! - **Modular Output System**: Pluggable output formatters for different use cases
//! - **Data Structures**: Core types like `FileEntry` for representing filesystem entries
//! - **Utilities**: Helper functions for disk usage calculation and file processing
//!
//! # Modules
//!
//! - [`cache`]: Disk-based caching system for improved performance
//! - [`data`]: Core data structures (`FileEntry`, `EntryType`)
//! - [`cli`]: Command-line interface definitions
//! - [`output`]: Modular output formatters (terminal, CSV)
//! - [`scan`]: File system scanning functionality
//! - [`thread_pool`]: Thread pool configuration strategies for performance optimization
//! - [`utils`]: Utility functions for disk usage and file operations

pub mod cache;
pub mod cli;
pub mod data;
pub mod memory;
pub mod metrics;
pub mod output;
pub mod scan;
pub mod thread_pool;
pub mod utils;

pub use cli::Args;
pub use data::{EntryType, FileEntry};
