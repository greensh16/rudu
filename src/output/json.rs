//! JSON output formatter (`--format json`).
//!
//! One document with three blocks — `scan_info` (how the scan was run),
//! `entries` (the displayed rows, same fields as CSV), and `summary` (totals
//! over the *whole* scan, before display filters). The schema is documented in
//! `docs/json-schema.md`; bump [`SCHEMA_VERSION`] on any incompatible change.
//!
//! Entries are serialised straight to the writer as they are converted, so the
//! output never exists twice in memory — on a multi-million-entry scan that is
//! the difference between JSON costing about what CSV costs and doubling the
//! peak.

use super::csv::row;
use crate::atime::AgeSummary;
use crate::cli::Args;
use crate::data::FileEntry;
use anyhow::{Context, Result};
use serde::Serialize;
use serde::ser::{SerializeSeq, Serializer};
use std::fs::File;
use std::io::{self, BufWriter, Write};

/// Version of the JSON document layout.
pub const SCHEMA_VERSION: u32 = 1;

/// Totals over the complete scan, computed before display filters run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanTotals {
    /// Disk usage of the scan root (hard links counted once, as `du` does).
    pub total_bytes: u64,
    pub files: u64,
    pub dirs: u64,
    pub cache_hits: u64,
    pub cache_total: u64,
    /// The scan was cut short by `--memory-limit`; totals are a lower bound.
    pub partial: bool,
}

/// Everything the JSON document needs beyond the rows themselves.
pub struct JsonContext<'a> {
    pub totals: &'a ScanTotals,
    pub age_summary: Option<&'a AgeSummary>,
}

#[derive(Serialize)]
struct Filters<'a> {
    depth: Option<usize>,
    show_files: bool,
    min_size: Option<u64>,
    max_size: Option<u64>,
    older_than: Option<u64>,
    exclude: &'a [String],
}

#[derive(Serialize)]
struct ScanInfo<'a> {
    schema_version: u32,
    rudu_version: &'static str,
    root: String,
    generated: String,
    generated_unix: u64,
    show_atime: bool,
    purge_days: Option<u64>,
    filters: Filters<'a>,
}

#[derive(Serialize)]
struct Summary<'a> {
    #[serde(flatten)]
    totals: &'a ScanTotals,
    entries_shown: usize,
    access_age: Option<&'a AgeSummary>,
}

/// Serialises rows lazily, converting each [`FileEntry`] as it is written.
struct Rows<'a> {
    entries: &'a [FileEntry],
    args: &'a Args,
    now: u64,
}

impl Serialize for Rows<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.entries.len()))?;
        for entry in self.entries {
            seq.serialize_element(&row(entry, self.args, self.now))?;
        }
        seq.end()
    }
}

#[derive(Serialize)]
struct Document<'a> {
    scan_info: ScanInfo<'a>,
    entries: Rows<'a>,
    summary: Summary<'a>,
}

/// Writes the JSON document to `--output`, or stdout without one.
///
/// `entries` are the already-filtered, sorted rows to display; totals in
/// `ctx` describe the whole scan.
pub fn render(entries: &[FileEntry], args: &Args, now: u64, ctx: &JsonContext) -> Result<()> {
    let doc = Document {
        scan_info: ScanInfo {
            schema_version: SCHEMA_VERSION,
            rudu_version: env!("CARGO_PKG_VERSION"),
            root: args.path.display().to_string(),
            generated: chrono::DateTime::from_timestamp(now as i64, 0)
                .map(|t| t.to_rfc3339())
                .unwrap_or_default(),
            generated_unix: now,
            show_atime: args.show_atime,
            purge_days: args.show_atime.then_some(args.purge_days),
            filters: Filters {
                depth: args.depth,
                show_files: args.show_files,
                min_size: args.min_size,
                max_size: args.max_size,
                older_than: args.older_than,
                exclude: &args.exclude,
            },
        },
        entries: Rows { entries, args, now },
        summary: Summary {
            totals: ctx.totals,
            entries_shown: entries.len(),
            access_age: ctx.age_summary,
        },
    };

    let writer: Box<dyn Write> = match &args.output {
        Some(path) => {
            Box::new(File::create(path).with_context(|| format!("creating {}", path.display()))?)
        }
        None => Box::new(io::stdout().lock()),
    };
    // A large buffer: JSON repeats every key per row, so it writes roughly
    // twice the bytes CSV does, and 8 KiB writes dominated its overhead.
    let mut writer = BufWriter::with_capacity(1 << 20, writer);
    serde_json::to_writer(&mut writer, &doc)?;
    writer.write_all(b"\n")?;
    writer.flush()?;

    if let Some(path) = &args.output {
        eprintln!("JSON output written to: {}", path.display());
    }
    Ok(())
}
