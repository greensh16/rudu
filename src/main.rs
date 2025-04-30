use clap::{Parser, ValueEnum};
use humansize::{format_size, DECIMAL};
use libc::{stat as libc_stat, stat};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    ffi::CString,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;
use std::os::unix::ffi::OsStrExt;

/// Rust-powered disk usage calculator (like `du`, but faster and safer)
#[derive(Parser, Debug)]
#[command(name = "rudu", author, version, about)]
struct Args {
    /// Path to scan (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Limit output to directories up to N levels deep
    #[arg(long)]
    depth: Option<usize>,

    /// Sort output by name or size
    #[arg(long, value_enum, default_value_t = SortKey::Name)]
    sort: SortKey,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum SortKey {
    Name,
    Size,
}

/// Get true disk usage of a file (in bytes), using st_blocks * 512
fn disk_usage(path: &Path) -> u64 {
    let c_path = match CString::new(path.as_os_str().as_bytes()) {
        Ok(p) => p,
        Err(_) => return 0,
    };

    let mut stat_buf: stat = unsafe { std::mem::zeroed() };
    let result = unsafe { libc_stat(c_path.as_ptr(), &mut stat_buf) };

    if result == 0 {
        (stat_buf.st_blocks as u64) * 512
    } else {
        0
    }
}

/// Count levels between root and child
fn path_depth(root: &Path, path: &Path) -> usize {
    path.strip_prefix(root)
        .map(|p| p.components().count())
        .unwrap_or(0)
}

fn main() {
    let args = Args::parse();
    let root = &args.path;

    // Step 1: Walk all files
    let entries: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    // Step 2: Collect sizes
    let file_data: Vec<(PathBuf, u64)> = entries
        .par_iter()
        .map(|entry| {
            let path = entry.path().to_path_buf();
            let size = disk_usage(&path);
            (path, size)
        })
        .collect();

    // Step 3: Aggregate sizes per directory
    let mut dir_totals: HashMap<PathBuf, u64> = HashMap::new();
    for (file_path, size) in &file_data {
        let mut current = file_path.parent();
        while let Some(path) = current {
            *dir_totals.entry(path.to_path_buf()).or_insert(0) += size;
            if path == root {
                break;
            }
            current = path.parent();
        }
    }

    // Step 4: Sort directories
    let mut sorted_dirs: Vec<_> = dir_totals.iter().collect();
    match args.sort {
        SortKey::Size => sorted_dirs.sort_by(|a, b| b.1.cmp(a.1)),
        SortKey::Name => sorted_dirs.sort_by_key(|(k, _)| (*k).clone()),
    }

    // Step 5: Print directories within depth
    for (dir, size) in &sorted_dirs {
        if args
            .depth
            .map(|d| path_depth(root, dir) > d)
            .unwrap_or(false)
        {
            continue;
        }

        println!(
            "{:<12} {}",
            format_size(**size, DECIMAL),
            dir.strip_prefix(root).unwrap_or(dir).display()
        );
    }

    // Step 6: Print files at exact depth (not parent!)
    for (file_path, size) in &file_data {
        if args
            .depth
            .map(|d| path_depth(root, file_path) != d)
            .unwrap_or(true)
        {
            continue;
        }

        println!(
            "{:<12} {}",
            format_size(*size, DECIMAL),
            file_path
                .strip_prefix(root)
                .unwrap_or(file_path)
                .display()
        );
    }
}