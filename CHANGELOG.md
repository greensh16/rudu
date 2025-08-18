# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[1.4.0]: https://github.com/greensh16/rudu/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/greensh16/rudu/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/greensh16/rudu/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/greensh16/rudu/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/greensh16/rudu/releases/tag/v1.0.0
