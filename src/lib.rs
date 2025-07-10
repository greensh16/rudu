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
//! - [`data`]: Core data structures (`FileEntry`, `EntryType`)
//! - [`cli`]: Command-line interface definitions
//! - [`output`]: Modular output formatters (terminal, CSV)
//! - [`scan`]: File system scanning functionality
//! - [`utils`]: Utility functions for disk usage and file operations

pub mod cli;
pub mod data;
pub mod output;
pub mod scan;
pub mod utils;

pub use cli::Args;
pub use data::{EntryType, FileEntry};
