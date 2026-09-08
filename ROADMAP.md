# rudu Roadmap — v1.5.0 → v2.0.0

Current release: **v1.5.0**

Each minor version below represents a focused release theme. Patch releases (e.g. v1.5.x) will carry bug fixes and small improvements between minors. Breaking changes are reserved for v2.0.0.

---

## v1.5.0 — Output Formats & Filtering

*Theme: make rudu's output programmable and its input filters precise.*

**Output**
- `--format json` — structured JSON output with `scan_info`, `entries`, and `summary` blocks; schema documented in `docs/json-schema.md`
- `--format csv` flag to complement the existing `--output file.csv` shorthand; both routes produce identical schemas

**Filtering**
- ~~`--min-size`~~ — **done**: human-friendly units (`10KB`, `100MB`, `5GiB`), decimal by default to match printed sizes; a display filter, so totals are unaffected
- `--max-size` — the upper-bound counterpart, same unit parsing (`utils::parse_size`)
- Size filters compose with `--exclude`, `--depth`, and `--show-files` — done for `--min-size`

**Defaults & UX**
- Smart auto-exclusions for common noise directories (`.git`, `node_modules`, `target`, `__pycache__`, etc.); opt out with `--no-auto-exclude`
- Smart thread count: scales with detected directory size rather than always using all CPUs

**Quality**
- Property-based tests (`proptest`) for size parsing and filter edge cases
- Performance regression tests added to CI; JSON output overhead target < 5% vs CSV

---

## v1.6.0 — Time Filtering & Configuration

*Theme: give users control over when things were last touched, and a place to save their preferences.*

**Time-based filtering**
- `--newer-than` / `--older-than` — relative (`7d`, `2w`, `3m`, `1y`) and absolute (`2025-01-01`) formats
- `--modified-since` alias for `--newer-than` to match `find` conventions
- Timestamps surfaced in JSON output

**Configuration file**
- `~/.rudu.toml` — persistent defaults and named profiles
- `rudu --init-config` to generate a commented starter file
- `--profile hpc` / `--profile dev` to activate a named profile
- CLI flags always override the config file

**Reliability**
- Structured error reporting: permission errors, I/O failures, and partial scans all produce a machine-readable summary when `--format json` is active
- Graceful Ctrl+C: partial results are written to the output file rather than discarded
- Network filesystem retry logic with exponential backoff

---

## v1.7.0 — Interactive TUI

*Theme: explore large directory trees without running rudu repeatedly.*

- `--interactive` launches a full-screen terminal UI (built on `ratatui`)
- Tree navigation with arrow keys; Enter to descend, Backspace to go up
- Live re-sort by size or name, live filter by `--min-size` and `--exclude` pattern
- Press `/` to search by path fragment
- Press `x` to mark a directory excluded from the current view
- Press `e` to export the current view to CSV or JSON
- Memory and scan-progress indicator in the status bar
- `--interactive` is a feature flag in `Cargo.toml`; binary size unaffected when not compiled in

---

## v1.8.0 — Analysis & Advanced Memory

*Theme: answer "what is taking up space and can I recover any of it?" without leaving rudu.*

**Compression & duplicate analysis**
- `--analyze-compression` — estimates compressibility per file using magic-number type detection; reports potential savings in terminal and JSON output
- Duplicate detection: files with identical size + partial hash are flagged in a `duplicates` block in JSON output
- File type distribution summary (text, source, binary, media, archives)

**Advanced memory management**
- Container-aware limits: reads `cgroup` memory limits on Linux so `--memory-limit` defaults sensibly inside Docker/Kubernetes
- NUMA-aware thread pool: pins worker threads to local NUMA nodes when available
- Dynamic thread scaling: sheds threads when memory pressure rises above 80% of limit

---

## v1.9.0 — Network Filesystems & Watch Mode

*Theme: extend rudu to filesystems that are slow, remote, or changing.*

**Network filesystem support**
- Configurable I/O timeout and retry count for NFS/SMB mounts
- Metadata-only mode (`--stat-only`) that skips block reads on remote filesystems, trading accuracy for speed
- Per-mount-point thread limits to avoid saturating remote servers

**Watch mode**
- `--watch` polls the scanned directory at a configurable interval and re-renders the output when sizes change
- Works in both terminal table and TUI modes
- Integrates with the existing cache so only modified subtrees are rescanned on each tick

**Plugin system (preview)**
- Stable `rudu_plugin` crate with a trait for custom output formatters and analysis passes
- Plugins loaded as shared libraries via `--plugin path/to/plugin.so`
- API is marked `#[unstable]` — breaking changes allowed until v2.0.0 stabilises it

---

## v2.0.0 — Library Mode & Stable API

*Theme: rudu graduates from a CLI tool to a reusable component.*

**Breaking changes**
- CLI flag renames and removals accumulated since v1.5.0 land here (a migration guide will be published alongside the release)
- Minimum supported Rust edition bumped

**Library crate**
- `rudu` is split into `rudu-cli` (the binary) and `rudu-lib` (the library crate, published separately)
- `rudu-lib` exposes a stable public API: `Scanner`, `ScanResult`, `FilterSet`, `OutputRenderer`
- Full documentation and examples for embedding rudu in other tools

**Plugin API stabilisation**
- The `rudu_plugin` trait from v1.9.0 is stabilised and semver-guaranteed from v2.0.0 onward

**Platform parity**
- Windows promoted to first-class: RSS monitoring, block-size calculation, and owner lookup all implemented natively (no WSL requirement)
- WASM target: `rudu-lib` compiles to `wasm32-unknown-unknown` for use in browser-based filesystem tools

**Cloud storage (experimental)**
- `--source s3://bucket/prefix` and `--source gs://bucket/prefix` list object sizes from S3 and GCS without downloading data; requires optional `rudu-cloud` feature flag

---

## Dependency Additions by Version

| Version | New dependencies |
|---------|-----------------|
| v1.5.0 | `proptest`, `regex` |
| v1.6.0 | `toml`, `chrono` |
| v1.7.0 | `ratatui`, `crossterm` (feature-gated) |
| v1.8.0 | `sha2`, `flate2` |
| v1.9.0 | `notify` (watch mode) |
| v2.0.0 | `rudu-lib` split; optional `aws-sdk-s3`, `google-cloud-storage` |

---

## Compatibility Policy

- **Patch releases** (v1.x.y): backwards-compatible bug fixes only.
- **Minor releases** (v1.x.0): new features; all v1.4.x CLI options remain valid through v1.x.
- **v2.0.0**: breaking changes are permitted. A migration guide and a deprecation period of at least one minor release will precede any removal.
- **Cache files**: on-disk cache format is stable within a major version. v2.0.0 will ship a cache migration path.
