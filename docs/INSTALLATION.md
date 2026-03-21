# rudu Installation Guide

This guide provides step-by-step installation instructions for `rudu`, a high-performance Rust-powered replacement for the Unix `du` command.

## Prerequisites

### Operating System Support
- **macOS** (Intel and Apple Silicon)
- **Linux** (x86_64, ARM64)
- **BSD variants** (FreeBSD, OpenBSD, NetBSD)
- **Windows** (via WSL or native with limited functionality)

### Required Software

#### Rust Toolchain
`rudu` requires **Rust 1.74.0 or later** with Cargo package manager.

**Check if Rust is installed:**
```bash
rustc --version
cargo --version
```

**Expected output:**
```
rustc 1.74.0 (79e9716c9 2023-11-13)
cargo 1.74.0 (1e8ebca0a 2023-11-08)
```

**Install Rust (if not present):**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### System Build Tools

**On macOS:**
```bash
# Install Xcode Command Line Tools
xcode-select --install
```

**On Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install build-essential pkg-config
```

**On CentOS/RHEL/Fedora:**
```bash
# CentOS/RHEL 7/8
sudo yum groupinstall "Development Tools"
sudo yum install pkgconfig

# Fedora/RHEL 9+
sudo dnf groupinstall "Development Tools"
sudo dnf install pkgconfig
```

**On Arch Linux:**
```bash
sudo pacman -S base-devel
```

## Installation Methods

### Method 1: Install from crates.io (Recommended)

This is the easiest method for most users.

```bash
cargo install rudu
```

**Expected output:**
```
    Updating crates.io index
  Downloaded rudu v1.3.0
  Downloaded 1 crate (78.5 KB)
  Installing rudu v1.3.0
   Compiling libc v0.2.149
   Compiling walkdir v2.5.0
   Compiling rayon-core v1.12.0
   ...
   Compiling rudu v1.3.0
    Finished release [optimized] target(s) in 45.23s
  Installing ~/.cargo/bin/rudu
   Installed package `rudu v1.3.0` to `~/.cargo/bin/rudu`
```

**Verify installation:**
```bash
rudu --version
```

**Expected output:**
```
rudu 1.3.0
```

### Method 2: Build from Source

This method gives you the latest development version and full control over the build process.

```bash
# Clone the repository
git clone https://github.com/greensh16/rudu.git
cd rudu

# Build release version (optimized)
cargo build --release

# Install to system (optional)
cargo install --path .
```

**Expected output:**
```
   Compiling rudu v1.3.0 (/path/to/rudu)
    Finished release [optimized] target(s) in 3.30s
  Installing /home/user/.cargo/bin/rudu
   Installed package `rudu v1.3.0` to `/home/user/.cargo/bin/rudu`
```

**Alternative: Run without installing:**
```bash
# Run directly from build directory
./target/release/rudu --help
```

## Post-Installation Setup

### PATH Configuration

Ensure `~/.cargo/bin` is in your PATH. Add this to your shell configuration file:

**For Bash (~/.bashrc):**
```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

**For Zsh (~/.zshrc):**
```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

**For Fish (~/.config/fish/config.fish):**
```fish
echo 'set -gx PATH $HOME/.cargo/bin $PATH' >> ~/.config/fish/config.fish
source ~/.config/fish/config.fish
```

### Verify Installation

Test that `rudu` is working correctly:

```bash
# Basic functionality test
rudu --version
rudu --help
```

**Expected help output (partial):**
```
Command-line arguments for the `rudu` disk usage calculator.

Usage: rudu [OPTIONS] [PATH]

Arguments:
  [PATH]  Path to scan (defaults to current directory) [default: .]

Options:
      --depth <DEPTH>        Limit output to directories up to N levels deep
      --sort <SORT>          Sort output by name or size [default: name]
      --show-files <SHOW_FILES>  Show individual files [default: true]
      --exclude <PATTERN>... Exclude entries with matching names
      --show-owner           Show owner (username) of each file/directory
      --output <FILE>        Write output to a CSV file instead of stdout
      --threads <N>          Limit the number of CPU threads used
      --show-inodes          Show inode usage
      --no-cache             Disable caching and force a full rescan
      --cache-ttl <CACHE_TTL>  Cache TTL in seconds [default: 604800]
      --profile              Enable performance profiling
  -h, --help                 Print help
  -V, --version              Print version
```

**Test basic functionality:**
```bash
# Scan current directory
rudu .
```

**Expected output format:**
```
------------------------------------------------------------------
        .______       __    __   _______   __    __  
        |   _  \     |  |  |  | |       \ |  |  |  | 
        |  |_)  |    |  |  |  | |  .--.  ||  |  |  | 
        |      /     |  |  |  | |  |  |  ||  |  |  | 
        |  |\  \----.|  `--'  | |  '--'  ||  `--'  | 
        | _| `._____| \______/  |_______/  \______/
                    Rust-based du tool
------------------------------------------------------------------            

🔧 Using default thread pool strategy (8 threads)
⠋ Incremental scan in progress... [1s]
[DIR]  156.3 MB           src/
[DIR]  45.2 MB            target/
[FILE] 2.1 kB             Cargo.toml
[FILE] 1.5 kB             README.md
```

## Shell Completions (Optional)

Currently, `rudu` does not include built-in shell completion generation. However, you can create basic completions manually:

### Bash Completion
Create `~/.bash_completion.d/rudu`:
```bash
_rudu_completions() {
    local cur prev
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    
    case ${prev} in
        --sort)
            COMPREPLY=($(compgen -W "name size" -- ${cur}))
            return 0
            ;;
        --show-files)
            COMPREPLY=($(compgen -W "true false" -- ${cur}))
            return 0
            ;;
        --output|--cache-ttl|--threads|--depth)
            return 0
            ;;
    esac
    
    COMPREPLY=($(compgen -W "--depth --sort --show-files --exclude --show-owner --output --threads --show-inodes --no-cache --cache-ttl --profile --help --version" -- ${cur}))
}

complete -F _rudu_completions rudu
```

Load the completion:
```bash
source ~/.bash_completion.d/rudu
```

## Troubleshooting

### Common Issues

#### "rudu: command not found"
- Ensure `~/.cargo/bin` is in your PATH
- Restart your terminal or run `source ~/.bashrc` (or equivalent)
- Verify installation: `ls -la ~/.cargo/bin/rudu`

#### "error: Microsoft C++ Build Tools is required"
On Windows, install [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/).

#### Permission denied errors
- On Unix systems, ensure you have read permissions for directories you want to scan
- For system directories, you may need to run with `sudo` (not recommended for regular use)

#### Memory issues on large filesystems
```bash
# Reduce thread count for large scans
rudu /large/directory --threads 2

# Disable caching if memory is limited
rudu /large/directory --no-cache
```

### Performance Optimization

#### Thread Configuration
```bash
# For SSDs (higher thread count)
rudu /path --threads 16

# For HDDs (lower thread count to avoid thrashing)  
rudu /path --threads 2

# Let rudu auto-detect (recommended)
rudu /path
```

#### Cache Settings
```bash
# Enable caching for repeated scans (default)
rudu /large/directory

# Disable cache for one-time scans
rudu /temp/directory --no-cache

# Custom cache TTL (1 hour)
rudu /project --cache-ttl 3600
```

## Uninstallation

To remove `rudu`:

```bash
cargo uninstall rudu
```

**Expected output:**
```
    Removing ~/.cargo/bin/rudu
```

Remove any shell completions you may have added manually.

## Next Steps

- Read the [README.md](README.md) for usage examples
- Check the [Performance Guide](docs/performance.md) for optimization tips
- Explore advanced features like CSV export and filtering options

For issues and questions, visit the [GitHub repository](https://github.com/greensh16/rudu).
