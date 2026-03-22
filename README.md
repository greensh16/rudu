# rudu

![workflow](https://github.com/greensh16/rudu/actions/workflows/rust_check.yml/badge.svg)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.15914603.svg)](https://doi.org/10.5281/zenodo.19160335)

<div align="center">
  <img src="https://github.com/user-attachments/assets/721ab886-1d01-4572-9f9f-63dc77ef2698" width="200" height="200" />
</div>

**`rudu`** is a high-performance, Rust-powered replacement for the traditional Unix `du` command. It uses multithreading (`rayon`) for fast parallel directory traversal, reports true disk usage (`st_blocks × 512`), and adds filtering, CSV export, incremental caching, and memory limiting — all with Rust's memory-safety guarantees.

---

## Quick Start

```bash
rudu                                          # scan current directory
rudu /data --depth 2 --sort size              # top 2 levels, largest first
rudu /project --exclude .git --output out.csv # exclude .git, export to CSV
rudu /lustre --memory-limit 900 --no-cache    # HPC cluster with 1 GB job limit
```

For the full options reference and annotated examples, see [docs/basic-usage.md](docs/basic-usage.md).

---

## Features

- **Parallel scanning** — work-stealing thread pool via `rayon`; configurable with `--threads N`
- **True disk usage** — `st_blocks × 512`, same as `du`
- **Depth & exclusion filtering** — `--depth N`, `--exclude PATTERN`
- **Flexible output** — terminal table or `--output report.csv`
- **Owner & inode info** — `--show-owner`, `--show-inodes`
- **Incremental caching** — skips unchanged subtrees on repeat scans; `--no-cache`, `--cache-ttl`
- **Memory limiting** — `--memory-limit MB` for HPC/SLURM jobs; graceful degradation at 95 % of limit
- **Performance profiling** — `--profile` prints per-phase timing

---

## Documentation

| Guide | Description |
|-------|-------------|
| [Basic Usage](docs/basic-usage.md) | Invocation syntax, all flags, annotated examples |
| [Exclude Tutorial](docs/exclude_tutorial.md) | Pattern exclusion in depth, with real-world examples |
| [Performance Guide](docs/performance.md) | Benchmarks, tuning, thread-pool strategy |
| [Platform Support](docs/PLATFORM_SUPPORT.md) | macOS / Linux / BSD / Windows notes, RSS tracking |

---

## Benchmarks

| Directory | Files | `du` | `rudu` | Speedup |
|-----------|-------|------|--------|---------|
| Small (1 K files) | ~1,000 | 0.010 s | 0.619 s | 0.02×* |
| Medium (`/usr/bin`) | ~1,400 | 0.017 s | 0.015 s | 1.13× |
| Large (project) | ~10,000 | 0.106 s | 0.052 s | 2.04× |

\* For very small directories, startup and threading overhead outweighs the benefit. Performance gains scale with directory size.

---

## Installation

```bash
# From source
git clone https://github.com/greensh16/rudu.git
cd rudu
cargo build --release
cargo install --path .

# From crates.io
cargo install rudu
```

---

## Contributing

Contributions are welcome. For major changes please open an issue first.

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo doc --open
```

---

## License

GNU General Public License — see [LICENSE](LICENSE).

## Acknowledgments

Built with [Rust](https://www.rust-lang.org/) · parallel processing via [Rayon](https://github.com/rayon-rs/rayon) · CLI via [Clap](https://github.com/clap-rs/clap) · progress via [Indicatif](https://github.com/console-rs/indicatif) · memory monitoring via [sysinfo](https://github.com/GuillaumeGomez/sysinfo)
