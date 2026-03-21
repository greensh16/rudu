//! CSV output formatter for file system scan results.
//!
//! This module provides functionality to export file system scan results
//! to CSV format for further processing or analysis.

use crate::cli::{Args, CsvEntry};
use crate::data::FileEntry;
use anyhow::Result;
use csv::Writer;
use humansize::{DECIMAL, format_size};
use std::fs::File;
use std::io;

/// Renders file entries to CSV format.
///
/// Converts each `FileEntry` to the canonical `CsvEntry` schema (which includes
/// a human-readable size column) so that CSV output is consistent regardless of
/// whether it is written to a file or stdout.
///
/// # Arguments
/// * `entries` - A slice of already-filtered and sorted file entries to render
/// * `args` - Command line arguments that control output formatting
///
/// # Returns
/// * `Result<()>` - Ok if rendering succeeded, Err if there was an issue
pub fn render(entries: &[FileEntry], args: &Args) -> Result<()> {
    let writer: Box<dyn io::Write> = if let Some(output_file) = &args.output {
        Box::new(File::create(output_file)?)
    } else {
        Box::new(io::stdout())
    };

    let mut csv_writer = Writer::from_writer(writer);

    for entry in entries {
        let csv_entry = CsvEntry {
            entry_type: entry.entry_type.as_str().to_string(),
            size_bytes: entry.size,
            size_human: format_size(entry.size, DECIMAL),
            owner: entry.owner.clone(),
            path: entry.path.display().to_string(),
            inodes: entry.inodes,
        };
        csv_writer.serialize(csv_entry)?;
    }

    csv_writer.flush()?;

    if let Some(output_file) = &args.output {
        eprintln!("CSV output written to: {}", output_file);
    }

    Ok(())
}
