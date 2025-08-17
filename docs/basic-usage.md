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
