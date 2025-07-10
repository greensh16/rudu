# Benchmark Results: rudu vs du

## Test Environment
- **System**: macOS
- **CPU**: 10 cores available  
- **Test Directory**: GitHub folder (~9.4GB, multiple projects)
- **rudu Version**: 1.2.0 (release build)
- **du Version**: macOS built-in du command

## Performance Comparison

### Test 1: Basic Total Size Calculation

| Command | Time | CPU Usage | Notes |
|---------|------|-----------|-------|
| `du -sh .` | 0.181s | 99% (single-core) | Simple, fast for basic totals |
| `rudu --depth 0` | 0.260s | 258% (multi-core) | Slower for simple totals, but uses parallelization |

### Test 2: Directory Listing with Depth

| Command | Time | CPU Usage | Notes |
|---------|------|-----------|-------|
| `du -d 2 .` | 0.217s | 98% (single-core) | Lists 130 directories |
| `rudu --depth 2` | 0.260s | 273% (multi-core) | Lists 375 entries (includes files) |

### Test 3: Threading Performance (rudu only)

| Configuration | Time | CPU Usage | Notes |
|---------------|------|-----------|-------|
| `rudu --threads 1` | 0.370s | 99% (single-core) | Single-threaded performance |
| `rudu` (all cores) | 0.227s | 291% (multi-core) | **38% faster** with parallelization |

### Test 4: Advanced Features (rudu-specific)

| Feature | Time | CPU Usage | Notes |
|---------|------|-----------|-------|
| `rudu --show-owner` | 0.309s | 359% | Adds file ownership information |
| `rudu --exclude .git --exclude node_modules` | 0.038s | 266% | **83% faster** when excluding large dirs |

## Key Findings

### Performance Analysis

1. **Simple Operations**: `du` is faster for basic total size calculations (~30% faster)
   - `du` is highly optimized for simple operations
   - `rudu` has overhead from Rust's safety checks and parallelization setup

2. **Complex Operations**: `rudu` shows competitive performance and better scalability
   - Multi-threading provides significant benefits (38% improvement over single-threaded)
   - Better performance with exclusions when dealing with large directories

3. **Feature Set**: `rudu` provides enhanced functionality not available in `du`
   - File ownership display
   - Advanced glob-based exclusion patterns  
   - CSV export capability
   - Progress indicators
   - Configurable threading

### CPU Utilization

- **du**: Single-threaded, ~99% CPU usage (1 core)
- **rudu**: Multi-threaded, 250-360% CPU usage (2.5-3.6 cores effectively)

### Memory and I/O

Both tools are I/O bound for large directory structures. `rudu`'s parallel processing helps when:
- Many small files need processing
- Complex filtering/exclusion is needed
- Additional metadata (ownership, inodes) is required

## Recommendations

**Use `du` when:**
- You need the fastest possible basic directory size calculation
- Working with scripts that expect traditional `du` output format
- System resources are limited (single-core systems)

**Use `rudu` when:**
- You need advanced features (exclusions, ownership, CSV export)
- Working with complex directory structures
- You want parallel processing benefits
- You prefer modern, safe Rust-based tools
- You need glob-based pattern matching for exclusions

## Conclusion

While `du` remains faster for basic operations, `rudu` provides a compelling alternative with:
- **30% overhead** for simple operations
- **Competitive performance** for complex operations  
- **Significant additional functionality**
- **Better scalability** through parallelization
- **Enhanced safety** through Rust's memory safety guarantees

The trade-off between raw speed and enhanced features makes `rudu` particularly valuable for complex analysis tasks and modern development workflows.
