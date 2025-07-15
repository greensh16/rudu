# Enhanced Benchmark Suite

This document describes the comprehensive benchmark suite for rudu, including cache performance scenarios and cross-platform testing.

## Benchmark Categories

### 1. Core Scanning Benchmarks (`scan_benchmark.rs`)

#### Basic Benchmarks
- **`scan_small_directory`**: Tests performance on small directory structures (3 levels, 5 files per directory)
- **`scan_deep_directory`**: Tests performance on deeper structures (5 levels, 10 files per directory)
- **`scan_with_owner_info`**: Tests scanning with owner information enabled

#### Cache Performance Benchmarks
- **`scan_with_cache_hit`**: Tests incremental scan with 100% cache hit rate
  - Creates a cached directory structure
  - Uses incremental scanning to measure cache performance
  - Simulates optimal caching scenarios

- **`scan_with_cache_miss`**: Tests performance with 50% cache miss rate
  - Modifies 50% of files before each iteration
  - Measures performance when cache is partially invalidated
  - Simulates moderate file system changes

- **`scan_incremental_deep`**: Tests deep tree incremental scanning with 10% modification
  - Creates an 8-level deep directory structure
  - Modifies 10% of leaf files before each iteration
  - Measures incremental scan efficiency on large trees

### 2. Memory Benchmarks (`memory_benchmark.rs`)

#### Memory Tracking Benchmarks
- **`memory_small_scan`**: Tracks memory usage during small directory scans
- **`memory_large_scan`**: Tracks peak memory consumption on large directory structures
- **`memory_cache_operations`**: Measures memory usage during cache creation and loading
- **`memory_threaded_scan`**: Monitors memory usage with multi-threaded scanning

#### Platform Support
- **Linux**: Uses `/proc/self/stat` for accurate RSS measurement
- **macOS**: Falls back to system memory APIs (currently placeholder)
- **Other platforms**: Provides framework for platform-specific memory tracking

### 3. Thread Pool Benchmarks (`thread_pool_benchmark.rs`)

Tests different threading strategies:
- **Default**: Rayon's default configuration
- **Fixed**: Fixed number of threads
- **NumCpusMinus1**: CPU count minus one
- **IOHeavy**: 2x CPU count for I/O-heavy workloads
- **WorkStealingUneven**: Optimized for uneven directory trees

### 4. Work Stealing Benchmarks (`work_stealing_benchmark.rs`)

Specialized benchmarks for work-stealing performance:
- Tests with large directories (12k files each)
- Tests with uneven directory structures
- Scalability analysis across different thread counts

## Cross-Platform CI Configuration

### Enhanced GitHub Actions Workflow

The CI configuration now includes:

#### Build Matrix
- **Operating Systems**: `ubuntu-latest`, `macos-latest`
- **Rust Version**: `stable`
- Parallel builds to catch OS-specific issues

#### Benchmark Jobs
- **Basic benchmarks**: Run on both platforms
- **Memory benchmarks**: Linux-only (due to `/proc` dependency)
- **Cache benchmarks**: Cross-platform cache performance testing

#### Benchmark Scheduling
- **PR/Push triggers**: Build and test validation
- **Weekly schedule**: Full benchmark suite on Sundays at 3 AM UTC
- **Artifact collection**: 30-day retention of benchmark results

#### Performance Comparison
- Automated cross-platform performance comparison
- GitHub Step Summary integration with key metrics
- Baseline tracking for performance regression detection

## Running Benchmarks

### Local Development

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark groups
cargo bench --bench scan_benchmark
cargo bench --bench memory_benchmark
cargo bench --bench scan_benchmark cache_benchmarks

# Run with test mode (faster)
cargo bench --bench scan_benchmark -- --test
```

### Cache-Specific Benchmarks

```bash
# Test cache hit performance
cargo bench --bench scan_benchmark scan_with_cache_hit

# Test cache miss scenarios
cargo bench --bench scan_benchmark scan_with_cache_miss

# Test deep tree incremental scanning
cargo bench --bench scan_benchmark scan_incremental_deep
```

### Memory Benchmarks

```bash
# Run memory tracking benchmarks (Linux recommended)
cargo bench --bench memory_benchmark

# Individual memory tests
cargo bench --bench memory_benchmark memory_small_scan
cargo bench --bench memory_benchmark memory_large_scan
```

## Benchmark Output

### Cache Performance Metrics

- **Cache hit rate**: Percentage of cache hits vs misses
- **Scan time with cache**: Time for incremental scans
- **Cache loading time**: Time to load cache from disk
- **Memory overhead**: Additional memory used by caching

### Memory Usage Tracking

- **Peak RSS**: Maximum resident set size during scan
- **Memory growth**: Memory usage patterns during scanning
- **Thread memory overhead**: Per-thread memory consumption

### Cross-Platform Comparison

- **OS performance differences**: Ubuntu vs macOS performance characteristics
- **File system efficiency**: Platform-specific I/O performance
- **Threading behavior**: OS-specific thread pool performance

## Performance Baselines

### Expected Cache Performance

- **100% cache hit**: 5-10x faster than full scan
- **50% cache miss**: 2-3x faster than full scan
- **Deep tree incremental**: 3-5x faster with 10% changes

### Memory Usage Guidelines

- **Small scans**: < 50MB peak memory
- **Large scans**: Memory usage should scale sub-linearly with file count
- **Cache overhead**: < 20% additional memory for cached data

### Thread Scaling

- **Work-stealing**: Should show better scaling on uneven directory trees
- **I/O heavy**: May benefit from higher thread counts
- **Default strategy**: Should provide good general-purpose performance

## Troubleshooting

### Common Issues

1. **Memory benchmarks not working**: Ensure you're on Linux for full memory tracking
2. **Cache benchmarks failing**: Check disk write permissions in test directories
3. **Thread benchmarks unstable**: System load can affect threading benchmarks

### Platform-Specific Notes

- **Linux**: Full feature support including memory tracking
- **macOS**: Core benchmarks work, memory tracking limited
- **Windows**: Should work but not actively tested in CI

## Contributing

When adding new benchmarks:

1. Follow the existing naming conventions
2. Add appropriate documentation
3. Include both small and large test cases
4. Consider cross-platform compatibility
5. Update this documentation

### Benchmark Guidelines

- Use `black_box()` to prevent optimization
- Include setup/teardown in measurement considerations
- Use appropriate sample sizes for reliable results
- Document expected performance characteristics
