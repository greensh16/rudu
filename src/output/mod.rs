//! Modular output system for the `rudu` application.
//!
//! This module provides a pluggable output system with different formatters
//! for displaying file system scan results in various formats. The modular
//! design allows for easy extension with new output formats.
//!
//! # Available Formatters
//!
//! - **Terminal**: Human-readable output with colored prefixes and formatting
//! - **CSV**: Machine-readable CSV format for data analysis and processing
//!
//! # Usage
//!
//! Each formatter accepts a slice of `FileEntry` objects and command-line
//! arguments to control the output format. The formatters are designed to
//! be independent and stateless, making them easy to test and extend.

pub mod csv;
pub mod terminal;

// Re-export the main render functions for convenience

/// CSV output renderer function.
///
/// See [`csv::render`] for full documentation.
pub use csv::render as render_csv;

/// Terminal output renderer function.
///
/// See [`terminal::render`] for full documentation.
pub use terminal::render as render_terminal;
