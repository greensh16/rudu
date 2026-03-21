# rudu v1.3.0 Fact-Sheet

## Project Overview
- **Name**: rudu
- **Version**: 1.3.0 (Released 2025-07-15)
- **Purpose**: High-performance, Rust-powered replacement for Unix `du` command
- **License**: GNU General Public License
- **Repository**: https://github.com/greensh16/rudu
- **DOI**: 10.5281/zenodo.15914603

## Key Value Propositions
- **Fast**: Uses multithreading (rayon) for parallel directory traversal
- **Safe**: Memory-safe by design, no segfaults or undefined behavior
- **Extensible**: Easy to add new flags, filters, and output formats
- **Accurate**: Reports true disk usage (allocated blocks), not just file sizes
- **Cross-platform**: Works on Unix-like systems (macOS, Linux, BSD)

## Core Features (✅ Implemented)

### Scanning & Traversal
- Recursive disk usage scanning
- Parallelized file traversal using rayon
- Real disk usage calculation (st_blocks * 512)
- Cross-platform compatibility
- Memory safety (Rust-powered)

### Filtering & Display
- Directory depth filtering (`--depth N`)
- File exclusion patterns (`--exclude PATTERN`)
- File visibility control (`--show-files true|false`)
- Clear output labeling ([DIR] and [FILE])

### Sorting & Organization
- Flexible sorting (`--sort size|name`)
- Size-based ordering for identifying largest directories/files

### Ownership & Metadata
- Ownership information display (`--show-owner`)
- Inode usage showing file/subdirectory counts (`--show-inodes`)
- Automatic fallback when getpwuid_r() fails

### Output Formats
- Terminal output with clean formatting
- CSV export (`--output report.csv`)
- Modular output system with pluggable formatters

### Performance & Control
- Thread control (`--threads N`)
- Real-time progress indicator
- Resource efficiency optimization
- Performance profiling (`--profile`)

## Enhanced Features (v1.3.0)

### Intelligent Caching System
- **Memory-mapped cache files** for O(1) load times
- **Automatic cache invalidation** based on directory modification times
- **Configurable TTL** with `--cache-ttl` option (default: 7 days)
- **Cache location fallback** from local directory to XDG cache directory
- **Graceful cache corruption handling** with automatic fallback

### Incremental Scanning
- **Skip unchanged directories** based on metadata comparison (mtime, nlink)
- **Preserves cached aggregated values** for unchanged subtrees
- **Dramatic performance improvements** for repeated scans (3-10x faster)
- **Intelligent cache hit/miss tracking** with profiling integration

### Advanced Threading
- **Work-stealing algorithms** for uneven directory structures
- **Local thread pool optimization** when `--threads` is specified
- **Multiple thread pool strategies** (experimental `--threads-strategy`)
- **NUMA-aware processing** improvements

### Performance Profiling
- **Detailed timing breakdowns** with `--profile` flag
- **Memory usage tracking** (RSS) for each phase
- **Cache hit/miss statistics** for optimization insights
- **JSON export support** for automated performance analysis
- **Phase-by-phase analysis** (Setup, Cache-load, WalkDir, Disk I/O, etc.)

## Command-Line Interface

### Basic Usage
```bash
rudu [PATH] [OPTIONS]
```

### Available Options
- `--depth N` - Limit output to directories up to N levels deep
- `--sort [name|size]` - Sort output by name or size (default: name)
- `--show-files [true|false]` - Show individual files (default: true)
- `--exclude PATTERN` - Exclude entries matching patterns
- `--show-owner` - Display file/directory owners
- `--output FILE` - Write output to CSV file
- `--threads N` - Specify number of CPU threads
- `--show-inodes` - Show number of files/subdirectories
- `--no-cache` - Disable caching and force full rescan
- `--cache-ttl SECONDS` - Cache TTL in seconds (default: 604800 = 7 days)
- `--profile` - Enable performance profiling
- `--threads-strategy` - Thread pool strategy (hidden experimental)

## Benchmark Results

### Performance Comparison (v1.3.0)
| Test Case | Size | Files | `du` Time | `rudu` Time | `rudu` (cached) | Speedup |
|-----------|------|-------|-----------|-------------|-----------------|---------| 
| Small project | 50 MB | 1,000 | 0.010s | 0.015s | 0.005s | 1.5x (3x cached) |
| Medium project | 500 MB | 10,000 | 0.052s | 0.038s | 0.012s | 1.4x (4.3x cached) |
| Large codebase | 2 GB | 50,000 | 0.180s | 0.095s | 0.025s | 1.9x (7.2x cached) |
| Very large | 10 GB | 200,000 | 0.820s | 0.340s | 0.080s | 2.4x (10.3x cached) |

### Threading Performance
| Threads | Time | CPU Usage | Memory | Notes |
|---------|------|-----------|--------|-------|
| 1 | 0.450s | 100% | 25 MB | Single-threaded baseline |
| 2 | 0.280s | 180% | 28 MB | Good balance for most systems |
| 4 | 0.195s | 350% | 32 MB | Optimal for 4-core systems |
| 8 | 0.140s | 600% | 38 MB | Best for 8+ core systems |
| 16 | 0.138s | 650% | 45 MB | Diminishing returns |

### Cache Performance
| Cache Size | Load Time | Memory Usage | Notes |
|------------|-----------|--------------|-------|
| 1K entries | 1ms | 2 MB | Instant load |
| 10K entries | 5ms | 8 MB | Very fast |
| 100K entries | 20ms | 35 MB | Still fast |
| 1M entries | 150ms | 180 MB | Large projects |

### v1.3.0 Performance Improvements
| Test Case | v1.2.0 | v1.3.0 | v1.3.0 (cached) | Improvement |
|-----------|--------|--------|------------------|-------------|
| Small project (1K files) | 0.015s | 0.015s | 0.005s | 3x (cached) |
| Medium project (10K files) | 0.038s | 0.038s | 0.012s | 3.2x (cached) |
| Large codebase (50K files) | 0.095s | 0.095s | 0.025s | 3.8x (cached) |
| Very large (200K files) | 0.340s | 0.340s | 0.080s | 4.3x (cached) |

## Usage Recommendations

### When to Use rudu vs du
**Use rudu for:**
- Large directory structures (>5,000 files)
- Complex filtering requirements
- CSV output for analysis
- Safety-critical environments
- Integration with modern toolchains
- Repeated scans (caching benefits)
- Performance analysis and optimization

**Use du for:**
- Very small directories (<1,000 files)
- Simple, quick size checks
- Systems where Rust binaries aren't available
- Legacy script compatibility

### Optimization Strategies

#### Thread Configuration
- **I/O-bound workloads**: Use `--threads $(nproc)`
- **CPU-bound workloads**: Use `--threads $(($(nproc) / 2))`
- **Mixed workloads**: Use `--threads $(($(nproc) * 3 / 4))`

#### Cache Management
- **Development projects**: `--cache-ttl 3600` (1 hour)
- **Daily monitoring**: `--cache-ttl 86400` (24 hours)
- **One-off scans**: `--no-cache`

#### Performance Tuning by Use Case
- **Development**: `rudu --threads 4 --cache-ttl 3600 --exclude node_modules --exclude .git`
- **System admin**: `rudu --profile --show-owner --show-inodes --threads 8`
- **Backup verification**: `rudu --cache-ttl 86400 --threads 2`

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

## Installation

### From Source
```bash
git clone https://github.com/greensh16/rudu.git
cd rudu
cargo build --release
cargo install --path .
```

### Using Cargo
```bash
cargo install rudu
```

## Technical Dependencies
- **Rust**: Core language and memory safety
- **Rayon**: Parallel processing
- **Clap**: Command-line interface
- **Indicatif**: Progress indicators
- **Serde**: Serialization for CSV output
- **Bincode**: Cache serialization

## Memory Usage Patterns
- **Base memory**: ~20 MB for small directories
- **Thread overhead**: ~2-3 MB per thread
- **Cache memory**: ~100 bytes per cached entry
- **Peak memory**: Usually 2-3x base memory during processing

## Version History
- **v1.3.0** (2025-07-15): Intelligent caching, incremental scanning, performance profiling
- **v1.2.0** (2025-05-01): Thread optimization, exclusion patterns, CSV export, owner info
- **v1.1.0** (2025-01-15): Multi-threading, depth limiting, file exclusion, sorting
- **v1.0.0** (2024-11-01): Initial release with basic scanning functionality

## Development Quality
- Automatic code formatting with rustfmt
- Clippy linting enforced with zero warnings
- Comprehensive test coverage for core functionality
- GitHub Actions CI for quality assurance
- Cross-platform compatibility testing

## Key Performance Characteristics
- **Parallelization**: Uses multiple CPU cores effectively
- **Memory Safety**: Zero risk of segfaults or memory leaks
- **Scalability**: Performance improves significantly with larger directory trees
- **Caching**: 3-10x performance improvement for repeated scans
- **Thread Control**: Adjustable thread count for optimal performance
