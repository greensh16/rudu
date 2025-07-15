# Scan.rs Optimization Summary

## Changes Made

### 1. Combined Pipeline with `par_bridge()`
- **Before**: Two separate `par_iter()` blocks for files and directories
- **After**: Single `WalkDir -> channel -> par_bridge() -> parallel consumer` pipeline
- **Benefit**: Reduces scheduler overhead by combining both processing stages

### 2. Introduced `ScanJob` Struct
```rust
struct ScanJob {
    path: PathBuf,
    is_file: bool,
    size: u64,
    parent_paths: Vec<PathBuf>,
}
```
- **Purpose**: Minimizes per-entry allocation by pre-computing metadata
- **Benefit**: Reduces memory allocations and improves cache locality

### 3. Local Thread Pool with `build_scoped()`
- **Added**: `with_thread_pool()` function using `rayon::ThreadPoolBuilder`
- **Behavior**: Creates local thread pools when `--threads` is specified
- **Benefit**: Avoids global thread pool contention across multiple library users

### 4. Optimized Processing Flow
```
WalkDir -> filter -> par_bridge() -> classify & process -> channel -> collect
```
- **Before**: Sequential collection then parallel processing
- **After**: Parallel processing during collection phase
- **Benefit**: Better parallelization and reduced memory usage

## Performance Improvements

1. **Reduced Scheduler Overhead**: Single parallel pipeline instead of two
2. **Minimized Allocations**: Custom `ScanJob` struct reduces per-entry allocations
3. **Better Thread Management**: Local thread pools when `--threads` specified
4. **Improved Cache Locality**: Pre-computed parent paths reduce repeated traversals

## Usage

The optimizations are transparent to users:
- `rudu --threads 4 .` uses local thread pool with 4 threads
- `rudu .` uses global thread pool with default strategy
- All existing functionality preserved

## Testing

- ✅ Code compiles without warnings
- ✅ Local thread pool message displays when `--threads` used
- ✅ Global thread pool still works for default usage
- ✅ All functionality preserved (files, directories, inodes, owners, CSV export)
