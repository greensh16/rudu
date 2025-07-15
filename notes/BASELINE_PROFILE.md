# Baseline Profiling Results

This document contains the baseline performance measurements for the `scan_files_and_dirs` function before any optimizations.

## Test Environment
- Platform: macOS (Apple Silicon)
- Rust version: 1.x (release mode)
- Criterion version: 0.5

## Benchmark Results

### Small Tree Test (105 entries)
- **Wall time**: ~980 µs (107.0 Kelem/s)
- **Peak RSS**: ~5.5 MB
- **Configuration**: 3 depth, 3 width, 5 files per directory

### Wide Tree Test (331 entries)
- **Wall time**: ~2.55 ms (128.0 Kelem/s)
- **Peak RSS**: ~18.7 MB
- **Configuration**: 2 depth, 10 width, 20 files per directory

### Deep Tree Test (1,786 entries)
- **Wall time**: ~12.6 ms (141.0 Kelem/s)
- **Peak RSS**: ~26.7 MB
- **Configuration**: 8 depth, 2 width, 5 files per directory

### With Owner Info Test (105 entries)
- **Wall time**: ~1.19 ms (88.4 Kelem/s)
- **Peak RSS**: ~27.3 MB
- **Configuration**: Same as small tree but with owner info enabled

### Real-World Sample Test (/usr/share, 10,071 entries)
- **Wall time**: ~74.1 ms (136.0 Kelem/s)
- **Peak RSS**: ~37.9 MB
- **Configuration**: Real directory limited to depth 3

### Scaling Tests
- **100 files**: ~1.23 ms
- **500 files**: ~2.57 ms
- **1000 files**: ~4.76 ms
- **2000 files**: ~8.27 ms

## Performance Characteristics

### Observed Patterns
1. **Memory Usage**: Roughly scales with the number of entries, with owner info adding overhead
2. **CPU Time**: Shows good throughput (100-140 Kelem/s range) across different tree shapes
3. **Scalability**: Appears to scale linearly with input size in the scaling tests

### Potential Bottlenecks
1. **Owner Info**: Significant performance impact (~22% slower wall time, ~5x memory increase)
2. **Deep Trees**: Memory usage increases with depth, suggesting directory caching overhead
3. **Memory Scaling**: Peak RSS grows significantly with tree size

## Validation Notes
- All tests completed successfully
- Memory tracking works on macOS using `ps` command
- Criterion baseline saved for future comparisons
- Real-world test uses `/usr/share` which contains typical system files

## Next Steps
Future optimizations should focus on:
1. Reducing memory overhead for owner info lookups
2. Optimizing directory caching for deep trees
3. Improving memory efficiency for large file counts

---
Generated: $(date)
Commit: $(git rev-parse HEAD)
