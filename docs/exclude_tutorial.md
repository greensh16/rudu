# The `--exclude` Option: Complete Tutorial

The `--exclude` option in rudu allows you to filter out specific files and directories from disk usage scans. This is essential for getting accurate disk usage measurements by excluding temporary files, build artifacts, version control directories, and other irrelevant content.

## Table of Contents
- [Basic Usage](#basic-usage)
- [How Exclude Patterns Work](#how-exclude-patterns-work)
- [Pattern Types](#pattern-types)
- [Real-World Examples](#real-world-examples)
- [Advanced Patterns](#advanced-patterns)
- [Best Practices](#best-practices)
- [Performance Considerations](#performance-considerations)
- [Troubleshooting](#troubleshooting)

## Basic Usage

The `--exclude` option accepts one or more patterns that specify which files and directories to skip during scanning:

```bash
# Exclude a single directory
rudu . --exclude node_modules

# Exclude multiple directories
rudu . --exclude node_modules target .git

# Exclude files with specific extensions
rudu . --exclude "*.tmp" "*.log"

# Exclude mixed patterns
rudu . --exclude node_modules "*.pyc" __pycache__ target
```

## How Exclude Patterns Work

Rudu uses glob patterns for exclusions, which are automatically expanded for common use cases:

### Automatic Pattern Expansion

When you provide a simple directory name like `node_modules`, rudu automatically expands it into two glob patterns:

```bash
# This command:
rudu . --exclude node_modules

# Automatically becomes:
rudu . --exclude "**/node_modules" "**/node_modules/**"
```

This expansion ensures that:
- `**/node_modules` matches any `node_modules` directory at any depth
- `**/node_modules/**` matches all contents inside any `node_modules` directory

### Manual Glob Patterns

If your pattern already contains glob symbols (`*`, `?`, `[`), ends with `/`, or contains `.`, rudu uses it as-is without expansion:

```bash
# These patterns are used exactly as written:
rudu . --exclude "*.log"           # Files ending in .log
rudu . --exclude "temp/"           # Directories named temp
rudu . --exclude "cache*"          # Files/dirs starting with cache
```

## Pattern Types

### 1. Directory Names (Auto-Expanded)

**Purpose**: Exclude entire directories by name, regardless of location.

```bash
# Exclude common build/dependency directories
rudu . --exclude node_modules target dist build __pycache__

# Exclude version control directories
rudu . --exclude .git .svn .hg

# Exclude IDE/editor directories
rudu . --exclude .vscode .idea .vs
```

**What happens**: Each name gets expanded to match the directory anywhere in the tree and all its contents.

### 2. File Extensions

**Purpose**: Exclude files based on their extension.

```bash
# Exclude temporary and log files
rudu . --exclude "*.tmp" "*.log" "*.bak"

# Exclude compiled files
rudu . --exclude "*.o" "*.pyc" "*.class"

# Exclude backup files
rudu . --exclude "*~" "*.swp" "*.orig"
```

**Note**: Extensions must be quoted to prevent shell expansion.

### 3. Exact Paths

**Purpose**: Exclude specific files or directories at exact locations.

```bash
# Exclude specific files
rudu . --exclude "./specific_file.txt"

# Exclude directories at specific depths
rudu . --exclude "src/temp" "docs/build"
```

### 4. Wildcard Patterns

**Purpose**: More flexible matching using glob patterns.

```bash
# Exclude all cache directories (regardless of exact name)
rudu . --exclude "cache*" "*cache*" "*-cache"

# Exclude test files
rudu . --exclude "*_test.rs" "test_*.py" "*.test.js"

# Exclude files with multiple extensions
rudu . --exclude "*.tar.gz" "*.tar.bz2"
```

## Real-World Examples

### Web Development Project

```bash
# Scan a web project excluding common artifacts
rudu . --exclude node_modules dist build coverage .next .nuxt

# Or for a more comprehensive exclusion:
rudu . --exclude \
  node_modules \
  dist \
  build \
  coverage \
  .next \
  .nuxt \
  "*.log" \
  ".env.*"
```

### Rust Project

```bash
# Exclude Rust build artifacts and dependencies
rudu . --exclude target Cargo.lock

# More comprehensive Rust exclusion:
rudu . --exclude \
  target \
  "*.rs.bk" \
  "Cargo.lock" \
  ".cargo"
```

### Python Project

```bash
# Exclude Python cache and virtual environments
rudu . --exclude __pycache__ venv env .venv "*.pyc"

# Comprehensive Python project scan:
rudu . --exclude \
  __pycache__ \
  venv \
  env \
  .venv \
  .env \
  "*.pyc" \
  "*.pyo" \
  ".pytest_cache" \
  ".mypy_cache" \
  dist \
  build \
  "*.egg-info"
```

### Multi-Language Development Environment

```bash
# Exclude artifacts from multiple languages and tools
rudu . --exclude \
  node_modules \
  target \
  __pycache__ \
  .git \
  .vscode \
  .idea \
  dist \
  build \
  "*.log" \
  "*.tmp" \
  "*.swp" \
  ".DS_Store"
```

### System Administration

```bash
# Scan system directories excluding common temporary locations
rudu /var --exclude \
  log \
  cache \
  tmp \
  spool \
  run \
  "*.lock"

# Scan user home excluding caches and temporary files
rudu ~ --exclude \
  .cache \
  .tmp \
  Downloads \
  "*.log" \
  "*.tmp" \
  ".DS_Store" \
  "Thumbs.db"
```

## Advanced Patterns

### Character Classes

Use character classes for more flexible matching:

```bash
# Exclude files with numeric extensions
rudu . --exclude "*.[0-9]" "*.[0-9][0-9]"

# Exclude backup files with date stamps
rudu . --exclude "*.backup.[0-9][0-9][0-9][0-9]*"
```

### Negation (Not Directly Supported)

Rudu doesn't support negation patterns directly, but you can achieve similar results with depth limiting and multiple scans:

```bash
# Instead of "exclude everything except src/"
# Use depth limiting:
rudu src --depth 10

# Or scan specific directories:
rudu src && rudu docs && rudu tests
```

### Complex Glob Patterns

```bash
# Exclude files matching complex patterns
rudu . --exclude \
  "*.{tmp,log,bak}" \       # Multiple extensions (shell expansion)
  "temp*" \                 # Files starting with temp
  "*temp*" \                # Files containing temp
  "cache/**" \              # Everything in cache directories
  "**/*.lock"               # All .lock files at any depth
```

## Best Practices

### 1. Start Simple, Then Refine

Begin with basic exclusions and add more as needed:

```bash
# Start with the essentials
rudu . --exclude .git node_modules target

# Add more as you discover what to exclude
rudu . --exclude .git node_modules target __pycache__ "*.log" .vscode
```

### 2. Use Project-Specific Exclusion Lists

Create shell aliases or scripts for different project types:

```bash
# In your .bashrc or .zshrc
alias rudu-web='rudu . --exclude node_modules dist build .next .nuxt coverage'
alias rudu-rust='rudu . --exclude target "*.rs.bk" .cargo'
alias rudu-python='rudu . --exclude __pycache__ venv .venv "*.pyc" .pytest_cache'
```

### 3. Combine with Other Options

Exclusions work well with other rudu options:

```bash
# Exclude artifacts and limit depth for quick overview
rudu . --exclude node_modules target --depth 2

# Exclude artifacts and show file details
rudu . --exclude .git node_modules --show-files --show-owner

# Exclude artifacts and export to CSV for analysis
rudu . --exclude node_modules target --output analysis.csv
```

### 4. Quote Patterns with Special Characters

Always quote patterns containing special characters:

```bash
# Good - quoted patterns
rudu . --exclude "*.log" "*.tmp" "cache*"

# Bad - unquoted patterns may be expanded by shell
rudu . --exclude *.log *.tmp cache*
```

### 5. Test Your Exclusions

Use the `--depth 1` option to quickly verify your exclusions are working:

```bash
# Quick check to see what's being excluded
rudu . --exclude node_modules target --depth 1
```

## Performance Considerations

### Exclusion Impact on Performance

- **Early Exclusion**: Rudu excludes directories as soon as they're encountered, preventing unnecessary traversal
- **Glob Compilation**: Pattern compilation happens once at startup
- **Memory Usage**: Excluding large directories significantly reduces memory usage
- **Scan Speed**: Excluding directories like `node_modules` can speed up scans by orders of magnitude

### Optimization Tips

```bash
# Exclude the largest/most numerous directories first
rudu . --exclude node_modules target .git   # Good order

# Use specific patterns rather than overly broad ones
rudu . --exclude "*.log"                    # Better than "*log*"

# Combine exclusions in a single command rather than multiple scans
rudu . --exclude node_modules target        # Better than separate scans
```

## Troubleshooting

### Common Issues

#### 1. Pattern Not Matching

**Problem**: Your exclusion pattern isn't working as expected.

**Solutions**:
```bash
# Check if the pattern needs quotes
rudu . --exclude "*.log"          # Not: *.log

# Verify the pattern syntax
rudu . --exclude "**/cache"       # Match cache at any depth
rudu . --exclude "cache/**"       # Match contents of cache

# Test with a simple pattern first
rudu . --exclude cache            # Basic directory name
```

#### 2. Shell Expansion Interfering

**Problem**: The shell expands your pattern before rudu sees it.

**Solutions**:
```bash
# Use single quotes to prevent shell expansion
rudu . --exclude '*.log' 'temp*'

# Or escape special characters
rudu . --exclude \*.log temp\*
```

#### 3. Unexpected Files Still Showing

**Problem**: Files you expected to exclude are still appearing.

**Debugging Steps**:
```bash
# Check exact file/directory names
ls -la | grep -E "(node_modules|target)"

# Test with a broader pattern
rudu . --exclude "*node_modules*" "*target*"

# Use absolute patterns for specific paths
rudu . --exclude "./specific/path/to/exclude"
```

#### 4. Performance Still Slow

**Problem**: Scan is still slow even with exclusions.

**Solutions**:
```bash
# Add more common directories to exclusions
rudu . --exclude node_modules target .git __pycache__ .cache

# Check if you're missing major directories
ls -la | grep ^d

# Use depth limiting along with exclusions
rudu . --exclude node_modules --depth 3
```

### Debug Mode

To understand what's being excluded, you can use shell debugging:

```bash
# Show what patterns are being used
echo "Excluding: node_modules target .git"
rudu . --exclude node_modules target .git
```

## Integration Examples

### Makefile Integration

```makefile
# Add disk usage analysis to your build process
.PHONY: disk-usage
disk-usage:
	rudu . --exclude node_modules target .git --output disk_usage.csv
	@echo "Disk usage report saved to disk_usage.csv"
```

### CI/CD Integration

```yaml
# GitHub Actions example
- name: Analyze disk usage
  run: |
    cargo install rudu
    rudu . --exclude target node_modules .git --depth 3
```

### Shell Script Integration

```bash
#!/bin/bash
# Project cleanup and analysis script

# Define exclusions for different project types
COMMON_EXCLUDES="node_modules target __pycache__ .git .vscode"
WEB_EXCLUDES="$COMMON_EXCLUDES dist build .next coverage"
RUST_EXCLUDES="$COMMON_EXCLUDES .cargo"

# Function to analyze project
analyze_project() {
    local project_type=$1
    local path=${2:-.}
    
    case $project_type in
        web)
            rudu "$path" --exclude $WEB_EXCLUDES --depth 2
            ;;
        rust)
            rudu "$path" --exclude $RUST_EXCLUDES --depth 2
            ;;
        *)
            rudu "$path" --exclude $COMMON_EXCLUDES --depth 2
            ;;
    esac
}

# Usage
analyze_project web /path/to/web/project
```

---

## Summary

The `--exclude` option is one of rudu's most powerful features for getting accurate and relevant disk usage information. Key takeaways:

1. **Simple directory names** are automatically expanded to match anywhere in the tree
2. **Glob patterns** with special characters are used as-is
3. **Quote your patterns** to prevent shell expansion
4. **Combine exclusions** with other rudu options for powerful analysis
5. **Test your patterns** on small directories first
6. **Use project-specific exclusion lists** for common workflows

By mastering the `--exclude` option, you can quickly identify real disk usage patterns while filtering out the noise of build artifacts, caches, and temporary files.
