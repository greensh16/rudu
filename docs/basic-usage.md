# Basic Usage & Core Functionality

This guide covers the fundamental usage patterns and core functionality of `rudu`, including invocation syntax, default behavior, and key features.

## Invocation Syntax

```bash
rudu [PATH] [OPTIONS]
```

- **`[PATH]`**: The directory path to scan. Defaults to current directory (`.`) if not specified.
- **`[OPTIONS]`**: Command-line options to control scanning behavior, output format, and filtering.

## Command-Line Options

| Option | Description |
|--------|-------------|
| `--depth <N>` | Limit output to directories up to N levels deep |
| `--sort <name\|size>` | Sort output by name or size (default: name) |
| `--show-files <true\|false>` | Show individual files at target depth (default: true) |
| `--exclude <PATTERN>` | Exclude entries matching patterns (e.g., '.git', 'node_modules') |
| `--show-owner` | Show owner (username) of each file/directory |
| `--output <FILE>` | Write output to CSV file instead of stdout |
| `--threads <N>` | Limit number of CPU threads used |
| `--show-inodes` | Show inode usage (number of files/subdirectories) |
| `--no-cache` | Disable caching and force full rescan |
| `--cache-ttl <SECONDS>` | Cache TTL in seconds (default: 604800 = 7 days) |
| `--profile` | Enable performance profiling and show timing summary |
| `--min-size <SIZE>` | Hide entries smaller than SIZE (e.g. `10MB`, `1.5GiB`, `4096`) |
| `--show-atime` | Show last access time and access age in days |
| `--purge-days <DAYS>` | Access-age threshold for "at risk" reporting (default: 100) |
| `--older-than <DAYS>` | Only show entries not accessed for at least DAYS days (implies `--show-atime`) |
| `--memory-limit <MB>` | Limit memory usage, for HPC batch jobs |

## Default Behavior

### Sample Directory Structure

For demonstration purposes, consider this sample project structure:

```
rudu_demo_project/
├── config/
│   └── settings.toml
├── data/
│   ├── cache/
│   │   └── cache.dat
│   ├── logs/
│   │   └── app.log
│   └── temp/
│       └── temp_file.tmp
├── docs/
│   └── README.md
└── src/
    ├── main/
    │   └── main.rs
    ├── tests/
    │   └── unit_tests.rs
    └── utils/
        └── helpers.rs
```

### Basic Scan

```bash
rudu /tmp/rudu_demo_project
```

**Output:**
```plaintext
------------------------------------------------------------------
        .______       __    __   _______   __    __  
        |   _  \     |  |  |  | |       \ |  |  |  | 
        |  |_)  |    |  |  |  | |  .--.  ||  |  |  | 
        |      /     |  |  |  | |  |  |  ||  |  |  | 
        |  |\  \----.|  `--'  | |  '--'  ||  `--'  | 
        | _| `._____| \______/  |_______/  \______/
                    Rust-based du tool
------------------------------------------------------------------            

🔧 Using default thread pool strategy (10 threads)
📦 No cache found, performing full scan
⠏ Incremental scan in progress... [0s]

[DIR]  28.67 kB                0      
[DIR]  4.10 kB                 0      config
[FILE] 4.10 kB                 config/settings.toml
[DIR]  8.19 kB                 0      data
[DIR]  4.10 kB                 0      data/cache
[FILE] 4.10 kB                 data/cache/cache.dat
[DIR]  4.10 kB                 0      data/logs
[FILE] 4.10 kB                 data/logs/app.log
[DIR]  0 B                     0      data/temp
[FILE] 0 B                     data/temp/temp_file.tmp
[DIR]  4.10 kB                 0      docs
[FILE] 4.10 kB                 docs/README.md
[DIR]  12.29 kB                0      src
[DIR]  4.10 kB                 0      src/main
[FILE] 4.10 kB                 src/main/main.rs
[DIR]  4.10 kB                 0      src/tests
[FILE] 4.10 kB                 src/tests/unit_tests.rs
[DIR]  4.10 kB                 0      src/utils
[FILE] 4.10 kB                 src/utils/helpers.rs
```

## Core Functionality

### 1. Recursive Scanning

`rudu` performs **recursive directory traversal** by default, scanning all subdirectories and files to calculate disk usage. The tool uses multithreading (via Rust's `rayon` crate) to parallelize the scanning process for improved performance on large directory structures.

### 2. Size Units

- **Default unit**: Kilobytes (kB)
- **Calculation method**: Reports actual disk usage via `st_blocks * 512`, similar to the traditional `du` command
- **Precision**: Shows bytes (B) for very small files

### 3. Labels and Output Format

- **`[DIR]`**: Indicates directory entries
- **`[FILE]`**: Indicates individual files
- **Size column**: Shows disk usage in human-readable format
- **Owner column**: Shows when `--show-owner` is used
- **Inode column**: Shows file/directory count when `--show-inodes` is used
- **Path column**: Relative path from the scanned root

## Usage Examples

### Depth Control

**Limit to top-level directories only:**
```bash
rudu /path/to/scan --depth 1
```

**Show directories up to 2 levels deep:**
```bash
rudu /path/to/scan --depth 2
```

### Sorting Options

**Sort by size (largest first):**
```bash
rudu /tmp/rudu_demo_project --sort size
```

**Output:**
```plaintext
[DIR]  28.67 kB                0      
[DIR]  12.29 kB                0      src
[DIR]  8.19 kB                 0      data
[DIR]  4.10 kB                 0      config
[FILE] 4.10 kB                 config/settings.toml
[DIR]  4.10 kB                 0      docs
[FILE] 4.10 kB                 docs/README.md
[DIR]  4.10 kB                 0      data/cache
[FILE] 4.10 kB                 data/cache/cache.dat
[DIR]  4.10 kB                 0      data/logs
[FILE] 4.10 kB                 data/logs/app.log
[DIR]  4.10 kB                 0      src/tests
[FILE] 4.10 kB                 src/tests/unit_tests.rs
[DIR]  4.10 kB                 0      src/utils
[FILE] 4.10 kB                 src/utils/helpers.rs
[DIR]  4.10 kB                 0      src/main
[FILE] 4.10 kB                 src/main/main.rs
[DIR]  0 B                     0      data/temp
[FILE] 0 B                     data/temp/temp_file.tmp
```

### File Visibility Control

**Hide individual files, show only directories:**
```bash
rudu /path/to/scan --show-files=false
```

### Pattern Exclusion

**Exclude common build/cache directories:**
```bash
rudu /project --exclude .git --exclude node_modules --exclude target
```

**Exclude temporary files:**
```bash
rudu /data --exclude temp --exclude cache
```

### Owner Information

**Display file owners:**
```bash
rudu /tmp/rudu_demo_project --show-owner
```

**Output:**
```plaintext
[DIR]  28.67 kB     green      0      
[DIR]  4.10 kB      green      0      config
[FILE] 4.10 kB      green      config/settings.toml
[DIR]  8.19 kB      green      0      data
[DIR]  4.10 kB      green      0      data/cache
[FILE] 4.10 kB      green      data/cache/cache.dat
[DIR]  4.10 kB      green      0      data/logs
[FILE] 4.10 kB      green      data/logs/app.log
[DIR]  0 B          green      0      data/temp
[FILE] 0 B          green      data/temp/temp_file.tmp
[DIR]  4.10 kB      green      0      docs
[FILE] 4.10 kB      green      docs/README.md
[DIR]  12.29 kB     green      0      src
[DIR]  4.10 kB      green      0      src/main
[FILE] 4.10 kB      green      src/main/main.rs
[DIR]  4.10 kB      green      0      src/tests
[FILE] 4.10 kB      green      src/tests/unit_tests.rs
[DIR]  4.10 kB      green      0      src/utils
[FILE] 4.10 kB      green      src/utils/helpers.rs
```

### Performance and Threading

**Use specific number of threads:**
```bash
rudu /large/directory --threads 4
```

**Single-threaded operation:**
```bash
rudu /data --threads 1
```

### Caching Features

**Disable caching for fresh scan:**
```bash
rudu /path/to/scan --no-cache
```

**Set custom cache TTL (1 hour):**
```bash
rudu /data --cache-ttl 3600
```

### CSV Export

**Export results to CSV:**
```bash
rudu /data --output analysis.csv
```

### Performance Profiling

**Enable detailed timing information:**
```bash
rudu /large/directory --profile
```

### Size Filtering

`--min-size` hides entries below a threshold — "if it's smaller than N bytes, I
don't want to know about it":

```bash
rudu /project --min-size 10MB --sort size
```

```
[DIR]  1.10 MB                 
[DIR]  1.08 MB                 data
[FILE] 921.60 kB               data/big.nc
[FILE] 155.65 kB               data/medium.nc
```

Key properties:

- **It is a display filter, not a scan filter.** Hidden entries still count
  toward their parent directories' totals, so the sizes shown stay correct and
  keep matching `du`. In the example above, the root total is identical with and
  without the flag — the six 4 kB log files it hides are still in it.
- **It applies to directories too**, and hiding a small directory can never hide
  something you wanted: a directory's size is the total of its whole subtree, so
  if it is under the threshold, everything beneath it is too.
- **It composes** with `--depth`, `--exclude`, and `--older-than`; an entry must
  satisfy all of them to be shown.

**Units.** A bare number is bytes. Suffixes are case-insensitive and may be
fractional (`1.5GB`):

| Form | Base | Example |
|------|------|---------|
| `B`, `KB`, `MB`, `GB`, `TB`, `PB` (or bare `K`, `M`, `G`, `T`, `P`) | 1000 | `--min-size 10MB` = 10,000,000 bytes |
| `KiB`, `MiB`, `GiB`, `TiB`, `PiB` | 1024 | `--min-size 1GiB` = 1,073,741,824 bytes |

Decimal is the default on purpose: rudu *prints* sizes in decimal units
(`819.20 kB`), so `--min-size 819kB` matches the row you just read. Use the `i`
forms when you want powers of 1024.

An unparseable threshold is rejected outright rather than silently treated as
zero, which would look like the flag was ignored:

```
$ rudu . --min-size "10 flurbs"
error: invalid value '10 flurbs' for '--min-size <SIZE>': invalid size
'10 flurbs': unknown unit 'flurbs' (expected B, KB, MB, GB, TB, PB, or the
1024-based KiB, MiB, GiB, TiB, PiB)
```

### Access Age and Scratch Purge Policies

HPC scratch filesystems commonly delete files that have not been *accessed* for
some number of days — NCI's `/scratch` uses 100 days, which is `rudu`'s default
threshold. `--show-atime` reports how close data is to that deadline:

```bash
rudu /scratch/ab12/$USER --show-atime --sort size
```

```
       SIZE                    LAST ACCESS        AT RISK (>=100d) PATH
[DIR]  2.15 MB                 2026-01-01  250d   1.84 MB (86%)
[DIR]  1.23 MB                 2026-01-01  250d   1.23 MB (100%)   cold
[FILE] 819.20 kB               2026-03-02  190d                    cold/run2019.nc
[DIR]  716.80 kB               2026-04-11  150d   614.40 kB (86%)  mixed
[FILE] 614.40 kB               2026-04-11  150d                    mixed/stale.nc
[DIR]  204.80 kB               2026-09-06    2d   -                hot
```

Read the columns like this:

- **For a file**, `LAST ACCESS` is its own atime and the age in days.
- **For a directory**, it is the *oldest* entry anywhere in the subtree — the
  one that will be purged first. `cold` showing `250d` means something under it
  is 150 days past the policy. A directory containing no files at all shows `-`,
  because its own atime is meaningless: the scan itself just read it.
- **`AT RISK`** is the bytes beneath a directory belonging to files already past
  the threshold, and what share of the directory that is. `cold` is 100% doomed;
  `mixed` loses 86% of its bytes but keeps a recently-read file.

A summary of the whole scan is printed to stderr, so it survives piping the
table to `less` or `grep` and never contaminates `--output` CSV:

```
--- Access age summary (policy: 100 days) ---
  <30d        307.20 kB  ( 14.3%)          2 files
  30-90d            0 B  (  0.0%)          0 files
  90-100d           0 B  (  0.0%)          0 files
  >=100d        1.84 MB  ( 85.7%)          3 files  <-- purgeable
  oldest access: 250 days ago
```

**Find exactly what the policy will take**, largest first:

```bash
rudu /scratch/ab12/$USER --older-than 100 --sort size
```

`--older-than` implies `--show-atime`. It keeps any entry at least that old,
which for a directory means "something beneath it is that old" — so it works as
a drill-down alongside `--depth`. The stderr summary still describes the whole
scan rather than the filtered rows, so the totals stay honest.

**Sites with a different policy window:**

```bash
rudu /scratch/proj --show-atime --purge-days 30     # 30-day policy
rudu /scratch/proj --older-than 30 --purge-days 30  # and list what it takes
```

**Export for a report:**

```bash
rudu /scratch/ab12/$USER --show-atime --output access-audit.csv
```

adds `atime`, `atime_unix`, `age_days`, and `at_risk_bytes` columns.

#### Accuracy caveats

- **Cached access times are conservative.** Reading a file updates its atime but
  not its parent directory's mtime, so an unchanged subtree still counts as a
  cache hit and can report an atime older than reality. Because atime only moves
  forward, this over-states age and therefore purge risk — it never reports
  at-risk data as safe. Use `--no-cache` when you need exact values.
- **Hard links** are counted once per link name in the at-risk rollup but once
  per inode in directory totals, so at-risk bytes are clamped to the directory
  size rather than exceeding 100%.
- **`noatime` mounts.** If a filesystem is mounted `noatime`, access times never
  update and no tool can recover them; the numbers are meaningless there (as is
  any purge policy built on them). `relatime`, the common Linux default, updates
  atime at least daily and is accurate at this granularity.
- **Running `rudu` does not protect data.** Walking a tree updates *directory*
  access times, not file ones, and scratch policies purge on file atime.

## Cross-Platform Notes

### Supported Platforms
- **macOS**: Full support with native file system optimizations
- **Linux**: Full support across all major distributions
- **BSD variants**: Compatible with FreeBSD, OpenBSD, NetBSD

### Platform-Specific Considerations

1. **File System Differences**:
   - `rudu` automatically adapts to different file systems (ext4, APFS, ZFS, etc.)
   - Block size calculations are handled transparently across platforms

2. **Permission Handling**:
   - Gracefully handles permission-denied scenarios
   - Owner information display depends on system user database availability

3. **Thread Pool Optimization**:
   - Automatically detects available CPU cores
   - Adjusts thread pool size based on system capabilities

### Memory Safety
- **Zero segfaults**: Rust's memory safety guarantees prevent crashes
- **No memory leaks**: Automatic memory management ensures efficient resource usage
- **Safe concurrency**: Thread-safe operations without data races

## Performance Characteristics

### When to Use `rudu`
- **Large directory structures** (>5,000 files)
- **Complex filtering requirements**
- **Repeated scans** (benefits from caching)
- **Safety-critical environments**
- **Performance analysis needs**

### Performance Benefits
- **Parallelization**: Utilizes all available CPU cores
- **Intelligent caching**: Avoids redundant scans of unchanged directories
- **Incremental scanning**: Only rescans modified parts of the directory tree
- **Memory efficiency**: Optimized memory usage patterns

## Summary

`rudu` provides a modern, safe, and high-performance alternative to the traditional `du` command with enhanced features:

- **Flexible depth control** for focused analysis
- **Multiple sorting options** for different use cases  
- **Pattern-based exclusion** for filtering unwanted files
- **Owner and inode information** for detailed analysis
- **CSV export capabilities** for data processing
- **Intelligent caching system** for performance optimization
- **Cross-platform compatibility** with consistent behavior

The tool's Rust implementation ensures memory safety while delivering excellent performance through parallel processing and smart caching mechanisms.
