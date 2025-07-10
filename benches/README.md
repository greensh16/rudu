# Benchmarks

This directory contains benchmarks for the `rudu` disk usage analyzer.

## Available Benchmarks

### 1. `scan_benchmark.rs`
The original benchmark focused on basic performance testing of the `scan_files_and_dirs` function.

### 2. `profiling.rs`
A comprehensive profiling benchmark that includes:
- **Memory usage tracking** (Peak RSS)
- **Wall-time measurement** 
- **Throughput calculation** (elements/second)
- **Multiple test scenarios**

## Profiling Benchmark Test Cases

### Synthetic Trees
- **Small Tree**: 3 levels deep, 3 directories wide, 5 files per directory (~105 entries)
- **Wide Tree**: 2 levels deep, 10 directories wide, 20 files per directory (~331 entries)
- **Deep Tree**: 8 levels deep, 2 directories wide, 5 files per directory (~1,786 entries)

### Feature Tests
- **Owner Info**: Tests performance impact of owner information lookup
- **Scaling**: Tests performance across different file counts (100, 500, 1000, 2000 files)

### Real-World Test
- **Real-World Sample**: Tests against actual filesystem (default: `/usr/share`)

## Running Benchmarks

### Basic Usage
```bash
# Run all profiling benchmarks
cargo bench --bench profiling

# Run with quick mode (fewer iterations)
cargo bench --bench profiling -- --quick

# Save baseline for comparison
cargo bench --bench profiling -- --save-baseline baseline

# Compare against baseline
cargo bench --bench profiling -- --baseline baseline
```

### Environment Variables

#### `RUDU_BENCHMARK_PATH`
Set this to specify a custom path for real-world testing:
```bash
export RUDU_BENCHMARK_PATH="/path/to/test/directory"
cargo bench --bench profiling
```

**Examples:**
- `/usr/share` (default) - System files
- `/usr/local` - Local installations
- `/home/user/projects` - User projects
- `/tmp/large_test_dir` - Custom test directory

## Memory Tracking

The benchmark includes cross-platform memory tracking:
- **Linux**: Uses `procfs` to read RSS from `/proc/self/stat`
- **macOS**: Uses `ps` command to get RSS
- **Other platforms**: Basic tracking (may return 0)

## Output Format

The benchmark outputs:
- Standard Criterion timing results
- Memory usage information printed to stderr
- Throughput measurements in elements/second
- HTML reports (if gnuplot is available)

Example output:
```
small_tree/scan         time:   [973.68 µs 979.65 µs 981.15 µs]
                        thrpt:  [107.02 Kelem/s 107.18 Kelem/s 107.84 Kelem/s]

Benchmark small_tree - Peak RSS: 5.50 MB, Total entries: 105
```

## Using Results

### Baseline Comparison
1. Run benchmark with `--save-baseline initial`
2. Make optimizations to the code
3. Run benchmark with `--baseline initial` to compare

### CI Integration
The benchmark can be integrated into CI pipelines:
```bash
# In CI script
export RUDU_BENCHMARK_PATH="/usr"
cargo bench --bench profiling -- --quick
```

## Interpretation

### Performance Metrics
- **Wall-time**: Total time spent in function
- **Throughput**: Files/directories processed per second
- **Peak RSS**: Maximum resident set size (memory usage)

### What to Look For
- **Memory leaks**: RSS should not grow unbounded
- **Scalability**: Performance should scale reasonably with input size
- **Feature overhead**: Owner info adds significant overhead (~22% slower)
- **Tree shape impact**: Deep trees use more memory than wide trees

### Optimization Targets
Based on baseline measurements, focus on:
1. **Owner info optimization**: Biggest performance impact
2. **Memory efficiency**: RSS grows significantly with tree size
3. **Directory caching**: Deep trees show high memory usage

## Troubleshooting

### Common Issues

1. **"Real-world benchmark path does not exist"**
   - The specified path doesn't exist or isn't accessible
   - Solution: Set `RUDU_BENCHMARK_PATH` to a valid directory

2. **Memory tracking returns 0**
   - Platform doesn't support memory tracking
   - Solution: Run on Linux or macOS for memory measurements

3. **Very slow benchmarks**
   - Large directories can take significant time
   - Solution: Use `--quick` flag or limit directory depth

### Platform Notes
- **Linux**: Full profiling capabilities with `procfs`
- **macOS**: Memory tracking via `ps` command
- **Windows**: Basic timing only (no memory tracking)

## Contributing

When adding new benchmarks:
1. Follow the existing pattern in `profiling.rs`
2. Include memory tracking in custom benchmarks
3. Add appropriate documentation
4. Update this README with new test cases
