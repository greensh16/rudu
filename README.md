# rudu

![workflow](https://github.com/greensh16/rudu/actions/workflows/rust_check.yml/badge.svg)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.15321605.svg)](https://doi.org/10.5281/zenodo.15321605)

<div align="center">
  <img src="https://github.com/user-attachments/assets/721ab886-1d01-4572-9f9f-63dc77ef2698" width="200" height="200" />
</div>

**`rudu`** is a high-performance, Rust-powered replacement for the traditional Unix `du` (disk usage) command. It was built to provide a safer, faster, and more extensible alternative for scanning and analyzing directory sizes — especially for large-scale or deep filesystem structures.

While `du` has been a reliable tool for decades, it's single-threaded, limited in extensibility, and not always ideal for custom workflows or integration with modern systems. `rudu` takes advantage of Rust’s memory safety and concurrency to provide a tool that is:

- **Fast** — uses multithreading (`rayon`) to speed up directory traversal and size aggregation.
- **Safe** — memory-safe by design, no segfaults or undefined behavior.
- **Extensible** — easy to add new flags, filters, and output formats as the tool grows.
- **Accurate** — by default, `rudu` reports true disk usage (allocated blocks), not just file sizes.

---

## Features

### Core Functionality
- ✅ **Recursive disk usage scanning** - Traverse directories and calculate disk usage
- ✅ **Parallelized file traversal** - Uses multithreading (`rayon`) for faster scanning of large directories
- ✅ **Real disk usage calculation** - Reports actual disk usage via `st_blocks * 512`, just like `du`
- ✅ **Cross-platform compatibility** - Works on Unix-like systems (macOS, Linux, BSD)
- ✅ **Memory safety** - Built with Rust for zero segfaults and memory leaks

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

### Filtering and Depth Control
```bash
# Show only top-level directories (depth = 1)
rudu /data --depth 1

# Exclude common directories
rudu /project --exclude .git --exclude node_modules --exclude target

# Hide individual files in output
rudu /data --show-files=false
```

### Sorting and Analysis
```bash
# Sort by size (largest first)
rudu /data --sort size

# Show ownership information
rudu /data --show-owner

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

### When to Use `rudu` vs `du`
- **Use `rudu` for**:
  - Large directory structures (>5,000 files)
  - Complex filtering requirements
  - CSV output for analysis
  - Safety-critical environments
  - Integration with modern toolchains

- **Use `du` for**:
  - Very small directories (<1,000 files)
  - Simple, quick size checks
  - Systems where Rust binaries aren't available
  - Legacy script compatibility

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
- **Caching**: Speed up repeated scans
- **Incremental scanning**: Only scan changed directories
- **Cloud integration**: Direct analysis of cloud storage

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

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## Acknowledgments

- Inspired by the classic Unix `du` command
- Built with [Rust](https://www.rust-lang.org/) for safety and performance
- Uses [Rayon](https://github.com/rayon-rs/rayon) for parallel processing
- Command-line interface powered by [Clap](https://github.com/clap-rs/clap)
- Progress indicators via [Indicatif](https://github.com/console-rs/indicatif)
