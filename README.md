# rudu

![workflow](https://github.com/greensh16/rudu/actions/workflows/rust_check.yml/badge.svg)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.15914603.svg)](https://doi.org/10.5281/zenodo.15914603)


<div align="center">
  <img src="https://github.com/user-attachments/assets/721ab886-1d01-4572-9f9f-63dc77ef2698" width="200" height="200" />
</div>

**`rudu`** is a high-performance, Rust-powered replacement for the traditional Unix `du` (disk usage) command. It was built to provide a safer, faster, and more extensible alternative for scanning and analyzing directory sizes — especially for large-scale or deep filesystem structures.

While `du` has been a reliable tool for decades, it's single-threaded, limited in extensibility, and not always ideal for custom workflows or integration with modern systems. `rudu` takes advantage of Rust's memory safety and concurrency to provide a tool that is:

- **Fast** — uses multithreading (`rayon`) to speed up directory traversal and size aggregation.
- **Safe** — memory-safe by design, no segfaults or undefined behavior.
- **Extensible** — easy to add new flags, filters, and output formats as the tool grows.
- **Accurate** — by default, `rudu` reports true disk usage (allocated blocks), not just file sizes.
- **Memory-aware** — configurable memory limits for resource-constrained environments.

---

## Features

### Core Functionality
- ✅ **Recursive disk usage scanning** - Traverse directories and calculate disk usage
- ✅ **Parallelized file traversal** - Uses multithreading (`rayon`) for faster scanning of large directories
- ✅ **Real disk usage calculation** - Reports actual disk usage via `st_blocks * 512`, just like `du`
- ✅ **Cross-platform compatibility** - Works on Unix-like systems (macOS, Linux, BSD)
- ✅ **Platform-specific memory monitoring** - Reliable RSS tracking on Linux/macOS; best-effort on Windows
- ✅ **Memory safety** - Built with Rust for zero segfaults and memory leaks

### Memory Management (New in v1.4.0)
- ✅ **Memory usage limits** (`--memory-limit MB`) - Set maximum memory usage in megabytes
- ✅ **Graceful memory handling** - Automatically disables memory-intensive features when approaching limit
- ✅ **Early termination** - Stops scan when memory limit is exceeded to prevent system issues
- ✅ **Platform-aware monitoring** - Bypasses limits gracefully on platforms without RSS support
- ✅ **HPC cluster support** - Designed for resource-constrained computing environments

### Filtering and Display Options
- ✅ **Directory depth filtering** (`--depth N`) - Limit output to directories up to N levels deep
- ✅ **File exclusion** (`--exclude PATTERN`) - Exclude entries matching patterns (e.g., `.git`, `node_modules`)
- ✅ **File visibility control** (`--show-files true|false`) - Toggle display of individual files
- ✅ **Clear output labeling** - `[DIR]` and `[FILE]` labels for easy identification

### Sorting and Organization
- ✅ **Flexible sorting** (`--sort size|name`) - Sort output by file size or name
- ✅ **Size-based ordering** - Easily identify the largest directories and files

### Ownership and Metadata
- ✅ **Ownership information** (`--show-owner`) - Display file/directory owners
- ✅ **Inode usage** (`--show-inodes`) - Show number of files/subdirectories in each directory

### Output Formats
- ✅ **Terminal output** - Clean, formatted output for interactive use
- ✅ **CSV export** (`--output report.csv`) - Export results to CSV for analysis
- ✅ **Modular output system** - Pluggable formatters (terminal, CSV) for extensibility

### Performance and Control
- ✅ **Thread control** (`--threads N`) - Specify number of CPU threads to use
- ✅ **Progress indicator** - Real-time progress bar during scanning
- ✅ **Resource efficiency** - Optimized for both speed and memory usage
- ✅ **Intelligent caching** - Automatically caches scan results for faster subsequent runs
- ✅ **Incremental scanning** - Only rescans changed directories, skipping unchanged subtrees
- ✅ **Performance profiling** (`--profile`) - Detailed timing breakdowns for optimization
- ✅ **Cache control** (`--no-cache`, `--cache-ttl`) - Fine-grained cache management
---

## Example Usage

### Basic Usage
```bash
# Scan current directory, default settings
rudu

# Scan a target directory
rudu /data

# Scan with progress indicator
rudu /large/directory
```

### Memory-Limited Scanning (New in v1.4.0)
```bash
# Limit memory usage to 512MB (useful for HPC clusters)
rudu /large/dataset --memory-limit 512

# Very memory-constrained environment (128MB limit)
rudu /project --memory-limit 128 --no-cache

# Combine memory limits with other options
rudu /data --memory-limit 256 --depth 3 --threads 2

# Profile memory usage during scan
rudu /large/directory --memory-limit 1024 --profile
```

### Filtering and Depth Control
```bash
# Show only top-level directories (depth = 1)
rudu /data --depth 1

# Exclude common directories
rudu /project --exclude .git --exclude node_modules --exclude target

# Hide individual files in output
rudu /data --show-files=false
```

> 📖 **For comprehensive exclusion examples**: See the [complete `--exclude` tutorial](docs/exclude_tutorial.md) with real-world patterns, troubleshooting, and best practices.

### Sorting and Analysis
```bash
# Sort by size (largest first)
rudu /data --sort size

# Show ownership information
rudu /data --show-owner

Note: Automatic Fallback: When getpwuid_r() fails, automatically falls back to using the getent command as a subprocess

# Show inode usage (file/directory counts)
rudu /data --show-inodes
```

### Output Formats
```bash
# Export to CSV for analysis
rudu /data --output report.csv

# Combine multiple options
rudu /project --depth 2 --sort size --show-owner --exclude .git
```

### Performance Tuning
```bash
# Use specific number of threads
rudu /large/directory --threads 4

# Single-threaded for comparison
rudu /data --threads 1
```

### Caching and Incremental Scanning
```bash
# Enable caching for faster subsequent scans
rudu /large/directory  # Automatically caches results

# Disable caching for fresh scan
rudu /large/directory --no-cache

# Set custom cache TTL (time-to-live) in seconds
rudu /data --cache-ttl 3600  # Cache valid for 1 hour

# Incremental scanning (only scans changed directories)
rudu /project  # Uses cache to skip unchanged directories
```

### Performance Profiling
```bash
# Enable detailed performance profiling
rudu /large/directory --profile

# Combine profiling with other options
rudu /project --profile --threads 8 --depth 2
```

---

## Memory Limiting for HPC Clusters

**New in v1.4.0:** `rudu` now supports memory usage limits, making it suitable for use in High-Performance Computing (HPC) environments where memory resources are strictly controlled.

### Why Memory Limiting?

In HPC clusters, jobs are typically allocated specific amounts of memory, and exceeding these limits can result in:
- Job termination by the scheduler (SLURM, PBS, etc.)
- Node instability affecting other users
- Poor cluster performance due to memory pressure

Traditional tools like `du` don't provide memory usage controls, making them risky for large-scale filesystem analysis in shared computing environments.

### How It Works

`rudu`'s memory limiting system:

1. **Real-time monitoring**: Continuously tracks RSS (Resident Set Size) memory usage
2. **Graceful degradation**: When approaching 95% of limit, disables memory-intensive features like caching
3. **Early termination**: If memory limit is exceeded, stops scanning and returns partial results
4. **Platform awareness**: Automatically disables monitoring on platforms without RSS support

### Usage Examples

```bash
# Basic memory-limited scan (512MB limit)
rudu /shared/datasets --memory-limit 512

# HPC job with strict memory constraints
#!/bin/bash
#SBATCH --mem=1G
#SBATCH --job-name=rudu-scan
rudu /lustre/project --memory-limit 900 --no-cache --threads 4

# Memory-conscious deep scan with profiling
rudu /large/filesystem --memory-limit 256 --depth 5 --profile

# Combine with other resource controls
rudu /data --memory-limit 128 --threads 1 --no-cache
```

### Memory Monitoring Behavior

| Memory Usage | Behavior |
|--------------|----------|
| < 95% limit  | Normal operation with all features enabled |
| 95-100% limit | Disables caching, reduces memory allocations |
| > 100% limit | Terminates scan early, returns partial results |
| Platform unsupported | Disables monitoring, continues normally |

### Platform Support

- **Linux/macOS**: Full memory monitoring with accurate RSS tracking
- **FreeBSD/NetBSD/OpenBSD**: Full support using system-specific APIs
- **Windows**: Best-effort support (may not be available on all versions)
- **Other platforms**: Memory limiting is disabled, but scan continues normally

### Best Practices for HPC

1. **Set conservative limits**: Use 80-90% of allocated job memory
   ```bash
   # For a 2GB job allocation
   rudu /data --memory-limit 1800
   ```

2. **Disable caching for one-time scans**: Saves memory in constrained environments
   ```bash
   rudu /data --memory-limit 512 --no-cache
   ```

3. **Use fewer threads in memory-constrained jobs**: Reduces per-thread memory overhead
   ```bash
   rudu /data --memory-limit 256 --threads 2
   ```

4. **Enable profiling to understand memory patterns**:
   ```bash
   rudu /data --memory-limit 1024 --profile
   ```

5. **Test with smaller datasets first** to understand memory requirements

---

## Benchmark Results

Performance comparison between `rudu` and traditional `du` on macOS:

| Directory Type | Files/Dirs | `du` Time | `rudu` Time | Speedup |
|---|---|---|---|---|
| Small (1K files) | ~1,000 | 0.010s | 0.619s | 0.02x* |
| Medium (/usr/bin) | ~1,400 | 0.017s | 0.015s | 1.13x |
| Large (project) | ~10,000 | 0.106s | 0.052s | 2.04x |

*Note: For very small directories, `rudu`'s startup and threading overhead can make it slower than `du`. The performance benefits become apparent with larger directory structures.*

### Key Performance Characteristics
- **Parallelization**: `rudu` uses multiple CPU cores (shown by 350%+ CPU usage)
- **Memory Safety**: No risk of segfaults or memory leaks
- **Scalability**: Performance improves significantly with larger directory trees
- **Thread Control**: Adjustable thread count for optimal performance
- **Memory Awareness**: Configurable limits prevent resource exhaustion

### When to Use `rudu` vs `du`
- **Use `rudu` for**:
  - Large directory structures (>5,000 files)
  - Complex filtering requirements
  - CSV output for analysis
  - Safety-critical environments
  - Integration with modern toolchains
  - Repeated scans (caching benefits)
  - Performance analysis and optimization
  - **HPC clusters and memory-constrained environments**
  - **Jobs with strict resource limits**

- **Use `du` for**:
  - Very small directories (<1,000 files)
  - Simple, quick size checks
  - Systems where Rust binaries aren't available
  - Legacy script compatibility

### Performance Documentation

For detailed performance analysis, optimization strategies, and benchmarking results, see the [Performance Guide](docs/performance.md).

### Platform Support

For platform-specific behavior, memory monitoring limitations, and RSS tracking details, see the [Platform Support Guide](docs/PLATFORM_SUPPORT.md).

---

## Installation

### From Source
```bash
# Clone the repository
git clone https://github.com/greensh16/rudu.git
cd rudu

# Build release version
cargo build --release

# Install to system
cargo install --path .
```

### Using Cargo
```bash
cargo install rudu
```

---

## Future Roadmap

### Planned Features
- **Output formats**: JSON export (`--format json`)
- **Size filtering**: Minimum size threshold (`--min-size N`)
- **Time-based filtering**: Filter by modification time
- **Compression analysis**: Detect compressible files
- **Network filesystems**: Optimized handling for NFS/SMB
- **Interactive mode**: TUI for exploring directory structures

### Potential Enhancements
- **Plugin system**: Custom analyzers and formatters
- **Cloud integration**: Direct analysis of cloud storage
- **Watch mode**: Real-time monitoring of directory changes
- **Compression analysis**: Identify highly compressible files
- **Advanced memory management**: NUMA-aware allocation strategies

---

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

### Development Setup
```bash
# Clone the repository
git clone https://github.com/greensh16/rudu.git
cd rudu

# Run tests
cargo test

# Check code formatting
cargo fmt --check

# Run linter
cargo clippy --all-targets -- -D warnings

# Build documentation
cargo doc --open
```

### Code Quality
- All code is automatically formatted with `rustfmt`
- Clippy linting is enforced with zero warnings
- Comprehensive test coverage for all core functionality
- GitHub Actions CI ensures quality on every commit

---

## License

This project is licensed under the GNU GENERAL PUBLIC LICENSE - see the [LICENSE](LICENSE) file for details.

---

## Acknowledgments

- Inspired by the classic Unix `du` command
- Built with [Rust](https://www.rust-lang.org/) for safety and performance
- Uses [Rayon](https://github.com/rayon-rs/rayon) for parallel processing
- Command-line interface powered by [Clap](https://github.com/clap-rs/clap)
- Progress indicators via [Indicatif](https://github.com/console-rs/indicatif)
- Memory monitoring via [sysinfo](https://github.com/GuillaumeGomez/sysinfo) crate
