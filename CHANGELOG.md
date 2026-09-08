# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.5.0] - 2026-09-08

### Features

#### Size Filtering
- **`--min-size SIZE`** hides entries below a threshold — "if it's smaller than
  N bytes, I don't want to know about it". Accepts a bare byte count (`4096`),
  decimal units (`10MB`, `1.5GB`, or bare `10M`), and binary units (`1GiB`).
  Decimal is the default because rudu *prints* decimal sizes, so `--min-size
  819kB` matches a row the table showed as `819.20 kB`.
- **It is a display filter, not a scan filter**: hidden entries still count
  toward their parent directories' totals, so reported sizes stay correct and
  keep matching `du`. It applies to directories as well as files, and hiding a
  small directory can never hide anything wanted — a directory's size is its
  whole subtree, so if it is under the threshold everything beneath it is too.
- Composes with `--depth`, `--exclude`, and `--older-than` (an entry must
  satisfy all of them). No scan or cache changes.
- An unparseable threshold is rejected at the CLI boundary rather than falling
  back to zero, which would look like the flag had been ignored.

#### Access-Age Reporting for Scratch Purge Policies
- **`--show-atime`** reports each entry's last access time and age in days, for
  judging how close data is to an HPC scratch purge policy (NCI `/scratch`
  deletes files unaccessed for 100 days). Access time comes from the `lstat`
  the scan already performs, so the flag costs no extra syscalls.
- **Directories report the oldest entry in their subtree**, not their own atime.
  A directory's own atime is useless for this question — merely listing a
  directory updates it, and the scan does exactly that — so a directory row
  answers "what under here will be purged first?". A directory containing no
  files at all reports `-` rather than a misleading "0 days".
- **`--purge-days N`** (default 100) sets the at-risk threshold. Directory rows
  gain an `AT RISK` column: the bytes beneath them belonging to files already
  past the threshold, and what share of the directory that is. Hard links are
  counted once per link name in this rollup but once per inode in directory
  totals, so the value is clamped to the directory size rather than exceeding
  100%.
- **`--older-than N`** filters output to entries not accessed for at least N
  days (implying `--show-atime`), so `--older-than 100 --sort size` lists
  exactly what a purge will take, largest first. A directory survives the filter
  when anything beneath it is old enough, making it a drill-down that combines
  with `--depth`.
- **An access-age summary** is printed to stderr — bytes and file counts bucketed
  by age, with the purgeable band flagged. It is computed over the complete scan
  before `--depth` and `--older-than` filtering, so the totals describe the data,
  not the visible rows, and it goes to stderr so it never contaminates a piped
  table or `--output` CSV.
- **CSV export** gains `atime`, `atime_unix`, `age_days`, and `at_risk_bytes`
  columns. They are empty unless `--show-atime` was given, so the schema shape is
  stable between runs.
- **Cache format** now stores each entry's raw atime — but never a derived
  rollup, since `--purge-days` can change between runs; rollups are recomputed
  from restored entries every time. atime is cached even without `--show-atime`,
  so a cache built by a plain run still serves a later access-age run. Caches
  written by earlier versions are discarded and rebuilt automatically.
  A restored atime can be older than reality (reading a file updates its atime
  but not its parent's mtime, so the subtree still validates as a hit); because
  atime only moves forward this over-states age and therefore purge risk, and
  never reports at-risk data as safe. `--no-cache` gives exact values.
- Default output is unchanged: without `--show-atime` the terminal table is
  byte-for-byte what it was.

### Maintenance
- **`Args` now implements `Default`**, derived by parsing an empty argument list
  so it cannot drift from the `default_value_t` attributes. Benchmarks build
  their `Args` with `..Default::default()`, so adding a flag no longer breaks
  every call site.
- **Benchmarks compile again.** `cargo bench` and `cargo clippy --all-targets`
  were already failing before this change: `work_stealing_benchmark`,
  `thread_pool_benchmark`, `memory_benchmark`, and `profiling` built `Args`
  without the `memory_limit`/`memory_check_interval_ms` fields added in 1.4.x,
  and `scan_benchmark` still called the positional 8-argument
  `CacheEntry::new` that 1.4.10 replaced with `CacheEntryParams`. All six
  benches now build.
- `libc` added as a dev-dependency: integration tests link only against the
  `rudu` lib and dev-deps, and the access-time tests backdate atimes via
  `utimes`.

## [1.4.10] - 2026-07-17

### Bug Fixes

#### Incremental Cache Correctness (CODE_REVIEW.md C1-C3)
- **Root is never a cache hit.** Previously an unchanged root directory caused the entire walk to be skipped, freezing results on stale data for up to the cache TTL (7 days by default) — changes deeper than the root's direct children were invisible.
- **Deep subtree validation.** A cache hit at directory `D` is now only trusted after re-statting every cached directory beneath `D` (a directory's mtime/nlink only reflect its *direct* children). Structural changes — files or directories added, removed, or renamed at any depth — are now always detected. Known remaining limitation (inherent to directory-level caching): in-place modification of an existing file changes no directory mtime and is only picked up after TTL expiry or with `--no-cache`.
- **Files are now cached and restored.** The cache previously stored only directories, so any cache-hit subtree silently dropped its files from the output (with the default `--show-files=true`, the second run of a scan listed fewer entries than the first). File entries (size, mtime, owner) are now cached and restored on hits, making output identical across runs.
- **Cached sizes propagate to ancestors.** On a cache hit at `D`, `D`'s total was recorded for `D` itself but never added to its parent, grandparent, or the root — partially-cached runs understated every ancestor total and persisted the wrong numbers back into the cache. Hit totals are now propagated up to the root, and the hit directory is counted in its parent's inode count.
- **Nanosecond mtime comparison.** Cache validation previously compared whole-second mtimes, so a change made in the same second as the caching scan was invisible. Cached mtimes are now nanosecond-precision.
- **No cache save on aborted scans (CODE_REVIEW.md H3).** A scan terminated early by `--memory-limit` could previously persist its truncated directory totals as a valid cache; cache saving is now skipped whenever the scan was cut short.

#### `--threads` Was a No-Op (CODE_REVIEW.md H1)
- `--threads N` previously printed "Using local thread pool with N threads" and then configured nothing — the scan ran on all cores. It now builds the global rayon pool with exactly N threads. This also fixes `--memory-limit` "HPC mode", which claimed to limit itself to 2 threads while actually using every core. `--threads 0` is now rejected with an error.

#### Symlinks and Special Files Misclassified as Directories (CODE_REVIEW.md H2)
- Symbolic links, FIFOs, sockets, and device nodes were reported as `[DIR]` entries, and symlink metadata was read from the *target* (via `stat`), so a symlink could acquire its target directory's cached size and mtime. All non-directory entries are now leaf (`FILE`) entries, and all metadata syscalls use `lstat` — symlinks report their own size and are never followed, matching `du`.

#### Directory Totals Excluded the Directories' Own Blocks (CODE_REVIEW.md M1)
- Directory totals were the sum of contained file sizes only; every directory inode's own blocks (typically 4 KB each) were missing, making rudu's numbers diverge visibly from `du` on directory-heavy trees. Each directory's own disk usage now counts toward its total and its ancestors', in both scan paths.

#### Exclude Patterns With `*` or `.` Silently Failed to Match Nested Paths (CODE_REVIEW.md M2)
- `expand_exclude_patterns` skipped expansion for any pattern containing `*` or `.`, and since globset's `*` does not cross `/`, `--exclude '*.log'` matched almost nothing and `.git` was only excluded via a separate literal comparison. Any pattern without a `/` (bare names, dot-names, bare globs) is now expanded to `**/<pat>` and `**/<pat>/**`; a trailing `/` is stripped; patterns containing `/` pass through unchanged.

#### Illusory Segfault Guard Removed From Owner Resolution (CODE_REVIEW.md M3)
- `get_owner`'s `std::panic::catch_unwind` wrapper claimed to prevent segfaults, but `catch_unwind` only intercepts Rust panics — a real SIGSEGV aborts the process regardless, so the `GETPWUID_BROKEN` fallback path was unreachable in the scenario it was built for. Removed; the real mitigations (re-entrant `getpwuid_r`, `getent` fallback, per-UID caching) remain. `getent` is now invoked via absolute paths (`/usr/bin/getent`, `/bin/getent`) before falling back to `PATH`.

#### Hard Links Double-Counted (CODE_REVIEW.md H4)
- A file with multiple hard links inside the scanned tree contributed its size once per link, inflating directory totals (backups, git object stores, package caches). Files with `st_nlink > 1` are now deduplicated by `(st_dev, st_ino)` and counted once in totals, as `du` does. Each link name is still listed individually with the inode's size. Links straddling a cache-hit boundary cannot be deduplicated across the boundary (the cached total is pre-aggregated) — same scan-order dependence `du` itself has.
- Cache format now includes file entries and nanosecond mtimes; the existing version check automatically invalidates caches written by older versions.

### Maintenance
- **Single module tree.** `main.rs` previously declared its own copies of every module alongside `lib.rs`, compiling the whole tree twice and giving the binary distinct copies of library statics (`CACHE_ENABLED`, the UID cache, the cache file lock). The binary now imports from the library crate (CODE_REVIEW.md L1).
- **Four dependencies removed** (CODE_REVIEW.md L4/L6): `once_cell` and `parking_lot` replaced by `std::sync::LazyLock`/`Mutex`; `fnv` replaced by an inline FNV-1a hasher with identical constants (hash values, cache file names, and entry keys are unchanged); `memmap2` removed along with both cache-IO `unsafe` blocks — cache files are now read with `std::fs::read` and written with buffered write + atomic rename (bincode deserializes the whole buffer either way, so mmap bought nothing and carried a SIGBUS hazard). Run `cargo build` once to refresh `Cargo.lock`.
- `sysinfo` usage now does a targeted refresh of the current process instead of `System::new_all()` (which enumerated every process, disk, and network interface); the duplicated Unix/Windows `rss_after_phase` bodies were merged (L6).
- `Args.output` is now `Option<PathBuf>` instead of `Option<String>`; the `--profile` stats file is named after the output file (`results.csv` → `results.stats.json`) instead of a fixed `stats.json` that silently overwrote any existing one (L5).
- `Cache::load_from_file`/`save_to_file` are now test-only (`#[cfg(test)]`); production IO goes through the atomic-rename path in `cache/mod.rs` (L5). `calculate_path_hash` delegates to `utils::path_hash` instead of duplicating it.
- Removed tautological `assert!(x || !x)` tests in `memory.rs` (replaced with a real invariant: exceeding the limit implies nearing it) and deleted the unused `tests/util.rs` helper module (L2).
- Doc fixes (L3): stray code fence in the `Args` doc comment, inaccurate "O(1) memory-mapped load" claim, stale "cache stored in scanned directory" text.

### Performance
- Incremental-path aggregation maps (`dir_totals`, `directory_children`, restored entries) are plain `HashMap`s instead of `DashMap`s — every write is sequential (walk + aggregation phases) and the only concurrent access is read-only, so the sharding/locking overhead bought nothing (PARALLELISM_REVIEW.md P4). Final sorting uses rayon's `par_sort_by` (P6), and redundant per-entry progress-bar ticks were removed from the walk loops (P8).
- Work-stealing path (`--threads-strategy work-stealing-uneven`, experimental): large-directory groups are now built in one O(n) indexing pass instead of re-scanning and cloning the whole entry list per large directory (was O(large_dirs × n)); owner UIDs are captured from the same lstat as sizes so `--show-owner` no longer issues a second syscall per entry; `--profile` now reports WalkDir/Aggregation phase timings (CODE_REVIEW.md M4/M5).
- Incremental path no longer materialises a `Vec<PathBuf>` of every ancestor per file — parent chains are walked in place during aggregation, removing the scan's largest single allocation (CODE_REVIEW.md M6, partial: entries are still collected before aggregation).
- One `stat` per entry during scanning instead of up to three: size, mtime, nlink, and owner are captured in a single call and reused for cache entries and `--show-owner` (previously `disk_usage` and `get_owner` each issued their own `stat`).
- Restored cache entries resolve owners from the cached UID instead of re-statting skipped paths.
- Depth-limited runs (`--depth N`) no longer shrink the saved cache: full subtrees are restored into the cache, and output depth filtering happens at display time as before.

## [1.4.9] - 2026-03-22

### Bug Fixes

#### Test Reliability — Cache mtime Race on Linux CI
- Fixed `test_save_and_load_large_cache` and all other cache tests failing on Linux CI (passing locally on macOS) due to a mtime race condition. `TestCacheGuard` previously used the same temp directory as both `RUDU_CACHE_DIR` and the scan root. Writing the cache `.bin` file updated the directory's mtime, causing `should_invalidate` to see a mismatch on reload. Linux filesystems track mtime with nanosecond precision so the race was always triggered there. Fixed by giving `TestCacheGuard` two separate temp directories — one for `RUDU_CACHE_DIR` (where cache files land) and one exposed as the scan root (never written to by the cache layer). This fix covers all ~15 tests in `cache/tests.rs` in a single change.

#### Dead Code Compile Error (`with_thread_pool`)
- Removed orphaned `with_thread_pool` function from `src/scan.rs` which became dead code after `scan_files_and_dirs_legacy` was deleted, causing a `-D dead-code` compile error.

#### Test Compile Errors
- `tests/utils.rs`: Removed import and deleted `test_filter_by_depth` test after `filter_by_depth` was deleted from `utils.rs`.
- `tests/integration_tests.rs`: Replaced `filter_by_depth` call with inline `path_depth`-based filtering.
- `tests/output_renderers.rs`: Added missing `root: &Path` third argument to `terminal::render` call; deleted duplicate `tests/output_render.rs`.
- `examples/test_cache_disable_demo.rs`: Updated old 8-argument `CacheEntry::new(...)` call to use the `CacheEntryParams` struct constructor.
- `tests/output_renderers.rs`: Fixed `args.output` type mismatch — `Args.output` is `Option<String>` not `Option<PathBuf>`.

#### Failing Test: `test_cache_path_generation`
- Fixed assertion `cache_path.ends_with(".bin")` in `src/cache/model.rs`. `Path::ends_with` checks path *components*, not string suffixes, so it always returned `false` for filenames like `a1b2c3d4.bin`. Changed to `cache_path.extension().and_then(|e| e.to_str()) == Some("bin")`.

#### Failing Test: `test_csv_renderer_produces_expected_schema`
- Fixed assertion `buf.contains("directory")` — `EntryType::as_str()` returns `"DIR"` (uppercase), not `"directory"`. Updated assertions to match `"DIR"` and `"FILE"`.

#### `unsafe` env var mutations (Rust 1.81+)
- Wrapped `std::env::set_var` and `std::env::remove_var` calls in `unsafe` blocks in `tests/integration_tests.rs` after these functions became `unsafe` in Rust 1.81.

### Tests Added

#### `tests/utils.rs`
- `test_disk_usage_nonzero_for_real_file` — verifies `disk_usage` returns > 0 for a file with content.
- `test_disk_usage_zero_for_missing_path` — verifies `disk_usage` returns 0 for a nonexistent path.
- `test_path_hash_is_deterministic` — same path always produces the same FNV hash.
- `test_path_hash_differs_for_different_paths` — distinct paths produce distinct hashes.
- `test_get_dir_metadata_returns_some_for_real_dir` — checks `nlink >= 2`, plausible `mtime`, and `owner` is `Some`.
- `test_get_dir_metadata_returns_none_for_missing_path` — returns `None` for a nonexistent path.
- `test_sort_entries_size_ties_are_stable_by_relative_order` — equal-size entries preserve original order under stable sort.
- `test_sort_entries_empty_slice_does_not_panic` — sorting an empty slice is a no-op.
- `test_sort_entries_single_entry_unchanged` — single-entry slice is unchanged after sort.

#### `tests/output_renderers.rs`
- `test_csv_renderer_handles_none_owner_and_inodes` — entries with `owner: None` and `inodes: None` produce valid CSV rows without panicking.
- `test_csv_renderer_writes_to_stdout_when_no_output_path` — `args.output = None` causes the renderer to write to stdout without error.

#### `tests/integration_tests.rs`
- `test_incremental_scan_returns_correct_entries` — smoke test that `scan_files_and_dirs_incremental` finds all expected directories and constrains all returned paths to within the scanned root.
- `test_incremental_scan_second_run_uses_cache` — runs the scan twice on the same directory and asserts `cache_total > 0` on the second run, confirming the first run populated the cache.

### Other Changes
- Simplified `README.md`: removed exhaustive feature checklist and usage examples (now in `docs/basic-usage.md`); kept elevator pitch, quick-start block, feature bullets, doc index, benchmark table, installation, and contributing sections.
- Added `ROADMAP.md` covering planned releases v1.5.0 through v2.0.0 in 0.1 steps.

---

## [1.4.0] - 2025-08-18

### Major Features Added

#### Memory Limiting System
- **Memory usage limits** with `--memory-limit MB` option for resource-constrained environments
- **Real-time memory monitoring** using RSS (Resident Set Size) tracking
- **Graceful degradation** - automatically disables caching when approaching 95% of memory limit
- **Early termination** - stops scanning when memory limit is exceeded to prevent system issues
- **Platform-aware monitoring** - bypasses limits gracefully on platforms without RSS support
- **Configurable check intervals** with hidden `--memory-check-interval-ms` option for fine-tuning

#### HPC Cluster Support
- **Memory-conscious scanning** designed for High-Performance Computing environments
- **Job scheduler integration** with examples for SLURM, PBS/Torque, and LSF
- **Resource-constrained operation** that respects allocated memory limits
- **Batch job compatibility** with conservative memory usage patterns

### Enhanced Features

#### Memory Management
- **Intelligent cache disabling** when memory pressure is detected
- **Partial result handling** when scans are terminated early due to memory limits
- **Memory status reporting** in scan results with `MemoryLimitStatus` enum
- **Cross-platform compatibility** with fallback behavior on unsupported systems

#### CLI Improvements
- **New `--memory-limit` option** for setting memory usage limits in megabytes
- **Enhanced help text** with clear memory limiting documentation
- **Memory status output** showing when limits are approached or exceeded
- **Profile integration** showing memory usage alongside performance metrics

#### Platform Support
- **Linux/macOS**: Full memory monitoring with accurate RSS tracking
- **FreeBSD/NetBSD/OpenBSD**: Full support using system-specific APIs
- **Windows**: Best-effort support (may not be available on all versions)
- **Other platforms**: Graceful fallback with monitoring disabled

### Performance Improvements

#### Memory Efficiency
- **Reduced memory allocations** when operating under memory constraints
- **Optimized data structures** for memory-limited environments
- **Throttled memory checks** to minimize monitoring overhead (default: 200ms intervals)
- **Smart caching decisions** based on available memory headroom

#### Resource Management
- **Thread pool optimization** when memory limits are active
- **Incremental scanning** with memory-aware cache management
- **Early exit strategies** to prevent resource exhaustion
- **Memory-conscious progress reporting** with reduced overhead

### Use Cases and Examples

#### HPC Integration
```bash
# SLURM job with 2GB memory allocation
#SBATCH --mem=2G
rudu /lustre/project --memory-limit 1800 --threads 4

# PBS job with conservative memory usage
#PBS -l mem=1gb
rudu /data --memory-limit 900 --no-cache

# Memory-constrained deep scan
rudu /filesystem --memory-limit 256 --depth 5 --profile
```

#### Development Workflows
```bash
# Limit memory for CI/CD environments
rudu /repo --memory-limit 512 --no-cache

# Profile memory usage patterns
rudu /project --memory-limit 1024 --profile

# Conservative scanning for shared systems
rudu /shared --memory-limit 128 --threads 1
```

### Memory Monitoring Behavior

| Memory Usage | System Behavior |
|--------------|------------------|
| < 95% limit  | Normal operation with all features enabled |
| 95-100% limit | Disables caching, reduces memory allocations |
| > 100% limit | Terminates scan early, returns partial results |
| Platform unsupported | Disables monitoring, continues normally |

### Documentation Updates

- **New "Memory Limiting for HPC Clusters" section** in README
- **Comprehensive usage examples** for different HPC schedulers
- **Best practices guide** for memory-constrained environments
- **Platform compatibility matrix** for memory monitoring support
- **Integration examples** with SLURM, PBS, and LSF job schedulers

### API Changes

#### New Public APIs
- `MemoryMonitor::new(limit_mb: u64)` - Create memory monitor with limit
- `MemoryMonitor::exceeds_limit()` - Check if memory limit is exceeded
- `MemoryMonitor::nearing_limit()` - Check if approaching memory limit
- `MemoryLimitStatus` enum for tracking memory constraint states
- `ScanResult::memory_status` field for reporting memory-related outcomes

#### CLI Options
- `--memory-limit MB` - Set memory usage limit in megabytes
- `--memory-check-interval-ms MS` - Hidden option for tuning check frequency

### Backward Compatibility

- **Full backward compatibility** with all existing command-line options
- **No breaking changes** to existing APIs or output formats
- **Optional memory limiting** - all existing workflows continue to work unchanged
- **Graceful fallback** on platforms without memory monitoring support

### Performance Benchmarks

Memory-limited scanning performance:

| Dataset Size | Memory Limit | Completion Time | Memory Usage | Status |
|-------------|--------------|-----------------|--------------|--------|
| 100K files | 512MB | 2.3s | 487MB | Normal |
| 500K files | 512MB | 8.1s | 498MB | Nearing limit |
| 1M files | 512MB | 12.7s | 512MB+ | Early termination |

### Bug Fixes and Stability Improvements

#### Caching System Fixes
- **Cache test reliability** improvements with better test isolation and cleanup
- **Cache file handling** robustness improvements for edge cases
- **Memory-mapped cache** stability enhancements
- **Cache invalidation** logic fixes for better reliability

#### Code Quality and Linting
- **Clippy warnings resolved** - all code now passes strict linting requirements
- **Code formatting** standardized across all modules and benchmarks
- **Benchmark consistency** improvements across all performance tests
- **Example code** cleaned up and validated

#### CI/CD Pipeline Improvements
- **GitHub Actions workflow** optimization for faster CI runs
- **Test reliability** improvements with better resource management
- **Build process** streamlining and dependency management

#### Documentation and Examples
- **New tutorial documentation** added in `docs/basic-usage.md`
- **Comprehensive exclusion guide** in `docs/exclude_tutorial.md` with 490+ lines of examples
- **Memory monitor demo example** showing practical memory limiting usage
- **Cache disable demo** for testing memory-constrained environments
- **Enhanced benchmarking** with new overhead benchmark suite

#### Developer Experience
- **Test coverage improvements** with additional integration tests
- **Better error messages** and debugging information
- **Enhanced utilities** for development and testing workflows
- **Improved project structure** with better module organization

### Future Roadmap

Planned enhancements for memory management:

- **NUMA-aware memory allocation** for large-scale systems
- **Memory pressure prediction** using historical usage patterns
- **Dynamic thread scaling** based on memory availability
- **Memory pool optimization** for frequent allocations
- **Container-aware limits** for Docker and Kubernetes environments

---

## [1.3.0] - 2025-07-15

### Major Features Added

#### Intelligent Caching System
- **Memory-mapped cache files** for near-instantaneous repeated scans
- **Automatic cache invalidation** based on directory modification times
- **Configurable TTL** with `--cache-ttl` option (default: 7 days)
- **Cache location fallback** from local directory to XDG cache directory
- **Graceful cache corruption handling** with automatic fallback

#### Incremental Scanning
- **Skip unchanged directories** based on metadata comparison (mtime, nlink)
- **Preserves cached aggregated values** for unchanged subtrees
- **Dramatic performance improvements** for repeated scans (3-10x faster)
- **Intelligent cache hit/miss tracking** with profiling integration

#### Performance Profiling
- **Detailed timing breakdowns** with `--profile` flag
- **Memory usage tracking** (RSS) for each phase
- **Cache hit/miss statistics** for optimization insights
- **JSON export support** for automated performance analysis
- **Phase-by-phase analysis** (Setup, Cache-load, WalkDir, Disk I/O, etc.)

### Enhanced Features

#### Advanced Threading
- **Work-stealing algorithms** for uneven directory structures
- **Local thread pool optimization** when `--threads` is specified
- **Multiple thread pool strategies** (experimental `--threads-strategy`)
- **NUMA-aware processing** improvements

#### Improved CLI
- **New caching options**: `--no-cache`, `--cache-ttl`
- **Performance profiling**: `--profile`
- **Enhanced help text** with performance guidance
- **Better error handling** for cache operations

#### Documentation
- **Comprehensive performance guide** in `docs/performance.md`
- **Detailed benchmark results** with cache performance metrics
- **Optimization strategies** for different use cases
- **Troubleshooting guide** for common performance issues

### Performance Improvements

#### Caching Performance
- **O(1) cache loading** using memory-mapped files
- **Sub-millisecond cache access** for small to medium projects
- **Efficient cache serialization** with bincode
- **Automatic cache compression** for large datasets

#### Scanning Optimizations
- **Reduced memory allocations** through better data structure reuse
- **Improved I/O patterns** for better cache locality
- **Optimized parent path traversal** with caching
- **Single-pass inode counting** during directory traversal

### Benchmark Results

Performance improvements over version 1.2.0:

| Test Case | v1.2.0 | v1.3.0 | v1.3.0 (cached) | Improvement |
|-----------|--------|--------|------------------|-------------|
| Small project (1K files) | 0.015s | 0.015s | 0.005s | 3x (cached) |
| Medium project (10K files) | 0.038s | 0.038s | 0.012s | 3.2x (cached) |
| Large codebase (50K files) | 0.095s | 0.095s | 0.025s | 3.8x (cached) |
| Very large (200K files) | 0.340s | 0.340s | 0.080s | 4.3x (cached) |

### Upgrade Instructions

#### For Existing Users

1. **No breaking changes** - all existing command-line options continue to work
2. **Automatic caching** - caching is enabled by default with sensible defaults
3. **Cache location** - cache files are stored in scanned directories as `.rudu-cache.bin`
4. **Memory usage** - slight increase in memory usage due to caching (typically 10-20MB)

#### New Command-Line Options

```bash
# Disable caching for one-time scans
rudu --no-cache

# Set custom cache TTL (time-to-live)
rudu --cache-ttl 3600  # 1 hour

# Enable performance profiling
rudu --profile

# Combine new options
rudu --profile --cache-ttl 86400  # 24 hours
```

#### Performance Tuning

For optimal performance in v1.3.0:

```bash
# Development environments (frequent scans)
rudu --cache-ttl 3600 --threads 4

# System administration (detailed analysis)
rudu --profile --show-owner --show-inodes

# Large directories (memory-constrained)
rudu --threads 2 --cache-ttl 86400

# Network filesystems
rudu --threads 1 --cache-ttl 86400
```

#### Migration Notes

- **Cache files** are automatically created on first run
- **Existing workflows** continue to work without changes
- **Performance improvements** are automatic for repeated scans
- **Memory usage** may increase slightly due to caching overhead

### Backward Compatibility

- **Full backward compatibility** with v1.2.0
- **All existing flags** continue to work as before
- **Output format** remains unchanged
- **CSV export** format unchanged

### Bug Fixes

- **Improved error handling** for permission-denied scenarios
- **Better fallback behavior** when cache directory is not writable
- **Memory leak fixes** in large directory processing
- **Thread pool cleanup** improvements

### Documentation Updates

- **Updated README** with caching and profiling examples
- **New performance guide** with comprehensive optimization strategies
- **Benchmark results** with detailed performance analysis
- **Usage examples** for new features

### Future Roadmap

Features planned for upcoming releases:

- **JSON output format** (`--format json`)
- **Size filtering** (`--min-size`, `--max-size`)
- **Time-based filtering** (modification time ranges)
- **Interactive TUI mode** for exploring directory structures
- **Watch mode** for real-time directory monitoring

---

## [1.2.0] - 2025-05-01

### Added
- **Thread pool optimization** with work-stealing algorithms
- **Advanced exclusion patterns** with glob support
- **Progress indicators** during scanning
- **CSV export functionality** for analysis
- **Owner information display** with `--show-owner`
- **Inode counting** with `--show-inodes`
- **Comprehensive benchmarking** infrastructure

### Improved
- **Parallel processing** performance for large directories
- **Memory usage optimization** through better data structures
- **Error handling** and user feedback

### Fixed
- **Thread safety** issues in large directory processing
- **Memory leaks** in long-running operations
- **Cross-platform compatibility** improvements

---

## [1.1.0] - 2025-01-15

### Added
- **Multi-threading support** with configurable thread counts
- **Depth limiting** with `--depth` option
- **File exclusion** patterns
- **Sorting options** (by name or size)

### Improved
- **Performance** through parallelization
- **User interface** with better formatting
- **Documentation** with usage examples

---

## [1.0.0] - 2024-11-01

### Added
- **Initial release** of rudu
- **Basic directory scanning** functionality
- **Disk usage calculation** using system calls
- **Cross-platform support** (Unix-like systems)
- **Memory safety** through Rust
- **Simple CLI interface**

[1.5.0]: https://github.com/greensh16/rudu/compare/v1.4.9...v1.5.0
[1.4.9]: https://github.com/greensh16/rudu/compare/v1.4.0...v1.4.9
[1.4.0]: https://github.com/greensh16/rudu/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/greensh16/rudu/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/greensh16/rudu/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/greensh16/rudu/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/greensh16/rudu/releases/tag/v1.0.0
