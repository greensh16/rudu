//! HTML stocktake report (`--report FILE --source PATH...`).
//!
//! Answers "who owns the data across these directories, how much of it has
//! gone untouched past the purge threshold, and who is using the inodes?" for
//! several data sources at once — e.g. a handful of NCI projects under
//! `/g/data`. Each source is scanned in turn, folded into per-owner totals by
//! [`Stocktake::add_source`], and its entry list dropped before the next scan,
//! so memory is bounded by the largest single source rather than their sum.
//!
//! The output is one self-contained HTML file: the aggregated numbers are
//! embedded as JSON in `template.html`, which renders the summary tiles,
//! per-source bars, the per-owner chart, and a sortable table in the browser.
//!
//! # What is counted
//!
//! - **Bytes** are summed over leaf (`FILE`) entries, attributed to each leaf's
//!   own owner. A directory's own blocks are not attributed to anyone, and hard
//!   links count once per link name (directory totals elsewhere in rudu dedup
//!   by inode, but a per-owner rollup has no single right owner for a shared
//!   inode). Totals can therefore differ slightly from `du` on the same tree.
//! - **Inodes** are files plus directories, each attributed to its own owner.
//! - **Stale bytes** are leaves whose access age is at or past `--purge-days`.
//!   A leaf with no readable atime is never counted stale. As with the rest of
//!   [`crate::atime`], cached atimes only ever over-state age.

use crate::atime::age_days;
use crate::data::{EntryType, FileEntry};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const TEMPLATE: &str = include_str!("template.html");
const DATA_PLACEHOLDER: &str = "__RUDU_REPORT_DATA__";
const TITLE_PLACEHOLDER: &str = "__RUDU_REPORT_TITLE__";

/// Owner name used when a uid could not be resolved or was not captured.
const UNKNOWN_OWNER: &str = "(unknown)";

/// One data source named on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Short name shown in the report (e.g. `gb02`).
    pub label: String,
    pub path: PathBuf,
}

/// Parses a `--source` value: either `PATH` or `LABEL=PATH`.
///
/// Only a prefix without a `/` is treated as a label, so a path that happens
/// to contain `=` further along (`/data/run=3`) is still read as a path. With
/// no label, the directory's own name is used (`/g/data/gb02` → `gb02`).
pub fn parse_source(input: &str) -> Result<Source, String> {
    if let Some((label, path)) = input.split_once('=')
        && !label.is_empty()
        && !label.contains('/')
        && !path.is_empty()
    {
        return Ok(Source {
            label: label.to_string(),
            path: PathBuf::from(path),
        });
    }
    if input.is_empty() {
        return Err("source path is empty".to_string());
    }
    let path = PathBuf::from(input);
    Ok(Source {
        label: default_label(&path),
        path,
    })
}

/// The last path component, resolving `.`/`..` against the filesystem so that
/// `rudu --report r.html .` is labelled with the real directory name.
pub fn default_label(path: &Path) -> String {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    resolved
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| resolved.display().to_string())
}

/// Checks that every source exists, is a directory, and has a unique label.
///
/// Run before any scanning so a typo in the fifth path doesn't surface only
/// after the first four have taken an hour.
pub fn validate_sources(sources: &[Source]) -> Result<()> {
    if sources.is_empty() {
        anyhow::bail!("--report needs at least one source");
    }
    let mut seen = BTreeMap::new();
    for s in sources {
        if !s.path.is_dir() {
            anyhow::bail!("source '{}' is not a directory", s.path.display());
        }
        if let Some(prev) = seen.insert(s.label.clone(), &s.path) {
            anyhow::bail!(
                "sources '{}' and '{}' would both be labelled '{}'; \
                 name one explicitly with --source LABEL=PATH",
                prev.display(),
                s.path.display(),
                s.label
            );
        }
    }
    Ok(())
}

/// Per-owner totals, serialised with the short keys the template reads.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct OwnerTotals {
    /// Owner name.
    pub o: String,
    /// Bytes by source label (sources with no bytes are omitted).
    pub b: BTreeMap<String, u64>,
    /// Inodes by source label (sources with no inodes are omitted).
    pub i: BTreeMap<String, u64>,
    /// Bytes at or past the purge threshold, over all sources.
    pub stale: u64,
    pub files: u64,
    pub dirs: u64,
}

/// Per-source totals for the "By source" strip.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct SourceTotals {
    pub label: String,
    pub path: String,
    pub bytes: u64,
    pub stale: u64,
    pub inodes: u64,
    pub files: u64,
    pub dirs: u64,
    /// The scan was cut short by `--memory-limit`; numbers are a lower bound.
    pub partial: bool,
}

/// Accumulates owner and source totals across several scans.
#[derive(Debug, Clone)]
pub struct Stocktake {
    purge_days: u64,
    now: u64,
    sources: Vec<SourceTotals>,
    owners: BTreeMap<String, OwnerTotals>,
}

/// Run details shown in the report's header and footer.
#[derive(Debug, Clone)]
pub struct ReportMeta {
    pub title: String,
    /// The command line that produced the report.
    pub command: String,
}

#[derive(Serialize)]
struct ReportData<'a> {
    title: &'a str,
    scanned: String,
    rudu_version: &'static str,
    purge_days: u64,
    command: &'a str,
    /// Parent directory shared by every source, if there is one (`/g/data`).
    common_parent: Option<String>,
    sources: &'a [SourceTotals],
    owners: Vec<&'a OwnerTotals>,
}

impl Stocktake {
    pub fn new(purge_days: u64, now: u64) -> Self {
        Stocktake {
            purge_days,
            now,
            sources: Vec::new(),
            owners: BTreeMap::new(),
        }
    }

    /// Folds one source's complete scan into the totals.
    ///
    /// `entries` must be the full, unfiltered scan of `source` with owners
    /// populated (`show_owner`). Directory sizes are ignored — they are
    /// aggregates of the leaves already counted.
    pub fn add_source(&mut self, source: &Source, entries: &[FileEntry], partial: bool) {
        let label = &source.label;
        let mut totals = SourceTotals {
            label: label.clone(),
            path: source.path.display().to_string(),
            partial,
            ..Default::default()
        };

        for entry in entries {
            let name = entry.owner.as_deref().unwrap_or(UNKNOWN_OWNER);
            let owner = self
                .owners
                .entry(name.to_string())
                .or_insert_with(|| OwnerTotals {
                    o: name.to_string(),
                    ..Default::default()
                });
            *owner.i.entry(label.clone()).or_insert(0) += 1;
            totals.inodes += 1;

            match entry.entry_type {
                EntryType::Dir => {
                    owner.dirs += 1;
                    totals.dirs += 1;
                }
                EntryType::File => {
                    owner.files += 1;
                    totals.files += 1;
                    totals.bytes += entry.size;
                    if entry.size > 0 {
                        *owner.b.entry(label.clone()).or_insert(0) += entry.size;
                    }
                    let stale = entry
                        .atime
                        .is_some_and(|at| age_days(at, self.now) >= self.purge_days);
                    if stale {
                        owner.stale += entry.size;
                        totals.stale += entry.size;
                    }
                }
            }
        }

        self.sources.push(totals);
    }

    pub fn sources(&self) -> &[SourceTotals] {
        &self.sources
    }

    pub fn owners(&self) -> impl Iterator<Item = &OwnerTotals> {
        self.owners.values()
    }

    /// Renders the self-contained HTML report.
    pub fn to_html(&self, meta: &ReportMeta) -> Result<String> {
        let data = ReportData {
            title: &meta.title,
            scanned: crate::atime::format_date(self.now),
            rudu_version: env!("CARGO_PKG_VERSION"),
            purge_days: self.purge_days,
            command: &meta.command,
            common_parent: common_parent(&self.sources),
            sources: &self.sources,
            owners: self.owners.values().collect(),
        };
        let json = serde_json::to_string(&data).context("serialising report data")?;
        // `<` only ever occurs inside JSON strings, where `<` is an
        // equivalent escape; this stops a path or owner containing
        // `</script>` from closing the data block early.
        let json = json.replace('<', "\\u003c");

        Ok(TEMPLATE
            .replace(TITLE_PLACEHOLDER, &escape_html(&meta.title))
            .replace(DATA_PLACEHOLDER, &json))
    }

    /// Writes the report to `path`.
    pub fn write(&self, path: &Path, meta: &ReportMeta) -> Result<()> {
        let html = self.to_html(meta)?;
        std::fs::write(path, html).with_context(|| format!("writing report to {}", path.display()))
    }
}

fn common_parent(sources: &[SourceTotals]) -> Option<String> {
    let mut parents = sources.iter().map(|s| Path::new(&s.path).parent());
    let first = parents.next()??;
    if first.as_os_str().is_empty() {
        return None;
    }
    parents
        .all(|p| p == Some(first))
        .then(|| first.display().to_string())
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests;
