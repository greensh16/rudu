use clap::{Parser, ValueEnum};
use humansize::{format_size, DECIMAL};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    path::PathBuf,
};
use walkdir::WalkDir;

mod utils;
use utils::{disk_usage, path_depth, get_owner, expand_exclude_patterns, build_exclude_matcher};

/// Rust-powered disk usage calculator (like `du`, but faster and safer)
#[derive(Parser, Debug)]
#[command(name = "rudu", author = "Sam Green", version = "1.0.0", about)]
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

    /// Show individual files at the target depth (default: true)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    show_files: bool,

    /// Exclude entries with matching names (e.g., '.git', 'node_modules')
    #[arg(long, value_name = "PATTERN", num_args = 1.., action = clap::ArgAction::Append)]
    exclude: Vec<String>,

    /// Show owner (username) of each file/directory
    #[arg(long, default_value_t = false)]
    show_owner: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum SortKey {
    Name,
    Size,
}

fn main() {
    let args = Args::parse();
    let root = &args.path;
    let expanded_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher = build_exclude_matcher(&expanded_patterns);

    let entries: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let path = entry.path();

            // Check exact match on this path
            if exclude_matcher.is_match(path) {
                return false;
            }

            // Check parent path too — to catch '**/dir/**'
            for ancestor in path.ancestors() {
                if exclude_matcher.is_match(ancestor) {
                    return false;
                }
            }

            true
        })
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
            .unwrap_or(false) {
            continue;
        }
    
        let owner = if args.show_owner {
            get_owner(dir)
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            "".to_string()
        };
    
        println!(
            "[DIR]  {:<12} {:<10} {}",
            format_size(**size, DECIMAL),
            owner,
            dir.strip_prefix(root).unwrap_or(dir).display()
        );
    }

    // Step 6: Print files at exact depth (not parent!)
    if args.show_files {
        for (file_path, size) in &file_data {
            if args
                .depth
                .map(|d| path_depth(root, file_path) != d)
                .unwrap_or(true) {
                continue;
            }
    
            let owner = if args.show_owner {
                get_owner(file_path)
                    .unwrap_or_else(|| "unknown".to_string())
            } else {
                "".to_string()
            };
    
            println!(
                "[FILE] {:<12} {:<10} {}",
                format_size(*size, DECIMAL),
                owner,
                file_path.strip_prefix(root).unwrap_or(file_path).display()
            );
        }
    }
}