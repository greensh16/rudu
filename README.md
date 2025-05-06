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

- ✅ Recursive disk usage scanning
- ✅ Parallelized file traversal for large directories
- ✅ Real disk usage via `st_blocks * 512`, just like `du`
- ✅ Directory depth filtering (`--depth`)
- ✅ Sort output by size or name (`--sort size|name`)
- ✅ Optional file listing (`--show-files=true|false`)
- ✅ `[DIR]` and `[FILE]` labels for clear output
- ✅ Cross-platform compatible (POSIX-style filesystem; currently optimized for Unix-like systems)
- ✅ Option to exclude specific directories and files (`--exclude .git, excepts globbed .g*`)
- ✅ Option to show directory and file ownership (`--show-owner`)
- ✅ Option to output to csv file (`--output report.csv`)
- ✅ Option to specify number of threads to use in parallel file scanning (`--threads #`)
---

## Example Usage

```bash
# Scan current directory, default settings
rudu

# Scan a target directory
rudu /data

# Show only top-level directories (depth = 1)
rudu /data --depth 1

# Sort by size, not name
rudu /data --sort size

# Hide individual files in output
rudu /data --show-files=false
```

## Planned features

- --format json|csv|plain
- --min-size N
