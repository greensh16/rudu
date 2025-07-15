# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[1.3.0]: https://github.com/greensh16/rudu/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/greensh16/rudu/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/greensh16/rudu/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/greensh16/rudu/releases/tag/v1.0.0
