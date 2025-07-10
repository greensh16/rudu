//! CSV output formatter for file system scan results.
//!
//! This module provides functionality to export file system scan results
//! to CSV format for further processing or analysis.

use crate::cli::Args;
use crate::data::FileEntry;
use anyhow::Result;
use csv::Writer;
use std::fs::File;
use std::io;

/// Renders file entries to CSV format.
///
/// # Arguments
/// * `entries` - A slice of already-filtered and sorted file entries to render
/// * `args` - Command line arguments that control output formatting
///
/// # Returns
/// * `Result<()>` - Ok if rendering succeeded, Err if there was an issue
///
/// # Note
/// This function accepts pre-filtered and sorted entries and contains no business logic.
/// It simply serializes the entries using the csv::Writer and FileEntry's Serialize derive.
pub fn render(entries: &[FileEntry], args: &Args) -> Result<()> {
    let writer: Box<dyn io::Write> = if let Some(output_file) = &args.output {
        Box::new(File::create(output_file)?)
    } else {
        Box::new(io::stdout())
    };

    let mut csv_writer = Writer::from_writer(writer);

    // Serialize each entry directly using the serde::Serialize derive
    for entry in entries {
        csv_writer.serialize(entry)?;
    }

    csv_writer.flush()?;

    if let Some(output_file) = &args.output {
        eprintln!("CSV output written to: {}", output_file);
    }

    Ok(())
}
