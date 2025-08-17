use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rudu::cache::save_cache;
use rudu::cli::SortKey;
use rudu::scan::{scan_files_and_dirs, scan_files_and_dirs_incremental};
use rudu::thread_pool::ThreadPoolStrategy;
use rudu::utils::build_exclude_matcher;
use rudu::Args;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use walkdir::WalkDir;

fn create_test_directory_structure(dir: &Path, depth: usize, files_per_dir: usize) {
    if depth == 0 {
        return;
    }

    // Create files in current directory
    for i in 0..files_per_dir {
        let file_path = dir.join(format!("file_{}.txt", i));
        fs::write(&file_path, format!("Content of file {}", i)).unwrap();
    }

    // Create subdirectories
    for i in 0..3 {
        let subdir_path = dir.join(format!("subdir_{}", i));
        fs::create_dir_all(&subdir_path).unwrap();
        create_test_directory_structure(&subdir_path, depth - 1, files_per_dir);
    }
}

/// Create a deep tree structure optimized for incremental scanning tests
fn create_deep_tree_structure(dir: &Path, depth: usize, files_per_dir: usize) {
    if depth == 0 {
        return;
    }

    // Create more files at leaf levels to simulate real directory structures
    let leaf_multiplier = if depth <= 2 { 3 } else { 1 };
    for i in 0..(files_per_dir * leaf_multiplier) {
        let file_path = dir.join(format!("deep_file_{}.txt", i));
        fs::write(
            &file_path,
            format!("Deep content of file {} at depth {}", i, depth),
        )
        .unwrap();
    }

    // Create more subdirectories at deeper levels
    let subdirs = if depth > 5 { 5 } else { 3 };
    for i in 0..subdirs {
        let subdir_path = dir.join(format!("deep_subdir_{}", i));
        fs::create_dir_all(&subdir_path).unwrap();
        create_deep_tree_structure(&subdir_path, depth - 1, files_per_dir);
    }
}

/// Create a cache for the given directory structure
fn create_cache_for_structure(
    root: &Path,
    args: &Args,
) -> HashMap<std::path::PathBuf, rudu::cache::CacheEntry> {
    // First scan to populate cache
    let exclude_matcher = build_exclude_matcher(&[]).unwrap();
    let entries = scan_files_and_dirs(root, args, &exclude_matcher, SortKey::Size).unwrap();

    // Convert entries to cache format
    let mut cache = HashMap::new();
    for entry in entries.entries {
        let metadata = fs::metadata(&entry.path).unwrap();
        let owner_u32 = entry.owner.as_ref().and_then(|s| s.parse::<u32>().ok());
        let cache_entry = rudu::cache::CacheEntry::new(
            rudu::utils::path_hash(&entry.path),
            entry.path.clone(),
            entry.size,
            metadata
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            1, // nlink - simplified for benchmark
            entry.inodes,
            owner_u32,
            entry.entry_type,
        );
        cache.insert(entry.path, cache_entry);
    }

    // Save cache to disk
    save_cache(root, &cache).unwrap();
    cache
}

/// Modify a percentage of files in a directory structure to simulate cache misses
fn modify_files_in_structure(dir: &Path, percentage: f32) {
    let walker = WalkDir::new(dir);
    let mut files: Vec<_> = walker
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    let modify_count = (files.len() as f32 * percentage / 100.0).ceil() as usize;
    files.truncate(modify_count);

    for file in files {
        let content = format!(
            "Modified content - {}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );
        fs::write(file.path(), content).unwrap();
    }
}

fn benchmark_scan_small_directory(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a small directory structure: 3 levels deep, 5 files per directory
    create_test_directory_structure(root, 3, 5);

    let args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: rudu::cli::SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: false,
        cache_ttl: 604800, // 7 days
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("scan_small_directory", |b| {
        b.iter(|| {
            scan_files_and_dirs(
                black_box(root),
                black_box(&args),
                black_box(&exclude_matcher),
                black_box(SortKey::Size),
            )
            .unwrap()
        })
    });
}

fn benchmark_scan_deep_directory(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a deeper directory structure: 5 levels deep, 10 files per directory
    create_test_directory_structure(root, 5, 10);

    let args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: rudu::cli::SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: false,
        cache_ttl: 604800, // 7 days
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("scan_deep_directory", |b| {
        b.iter(|| {
            scan_files_and_dirs(
                black_box(root),
                black_box(&args),
                black_box(&exclude_matcher),
                black_box(SortKey::Size),
            )
            .unwrap()
        })
    });
}

fn benchmark_scan_with_owner_info(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a directory structure with owner info enabled
    create_test_directory_structure(root, 4, 8);

    let args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: rudu::cli::SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: true,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: false,
        cache_ttl: 604800, // 7 days
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("scan_with_owner_info", |b| {
        b.iter(|| {
            scan_files_and_dirs(
                black_box(root),
                black_box(&args),
                black_box(&exclude_matcher),
                black_box(SortKey::Size),
            )
            .unwrap()
        })
    });
}

/// Benchmark scan with 100% cache hit rate
fn benchmark_scan_with_cache_hit(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a moderate directory structure
    create_test_directory_structure(root, 4, 8);

    let args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: rudu::cli::SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: false,
        cache_ttl: 604800, // 7 days
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    // Create and populate cache
    create_cache_for_structure(root, &args);

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("scan_with_cache_hit", |b| {
        b.iter(|| {
            scan_files_and_dirs_incremental(
                black_box(root),
                black_box(&args),
                black_box(&exclude_matcher),
                black_box(SortKey::Size),
            )
            .unwrap()
        })
    });
}

/// Benchmark scan with 50% cache miss rate (50% of files modified)
fn benchmark_scan_with_cache_miss(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a moderate directory structure
    create_test_directory_structure(root, 4, 8);

    let args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: rudu::cli::SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: false,
        cache_ttl: 604800, // 7 days
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    // Create and populate cache
    create_cache_for_structure(root, &args);

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("scan_with_cache_miss", |b| {
        b.iter_batched(
            || {
                // Setup: modify 50% of files for each iteration
                modify_files_in_structure(root, 50.0);
            },
            |_| {
                scan_files_and_dirs_incremental(
                    black_box(root),
                    black_box(&args),
                    black_box(&exclude_matcher),
                    black_box(SortKey::Size),
                )
                .unwrap()
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

/// Benchmark incremental scan on deep tree with 10% modified leaves
fn benchmark_scan_incremental_deep(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a deep directory structure: 8 levels deep, 6 files per directory
    create_deep_tree_structure(root, 8, 6);

    let args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: rudu::cli::SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: false,
        cache_ttl: 604800, // 7 days
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    // Create and populate cache
    create_cache_for_structure(root, &args);

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("scan_incremental_deep", |b| {
        b.iter_batched(
            || {
                // Setup: modify 10% of files for each iteration
                modify_files_in_structure(root, 10.0);
            },
            |_| {
                scan_files_and_dirs_incremental(
                    black_box(root),
                    black_box(&args),
                    black_box(&exclude_matcher),
                    black_box(SortKey::Size),
                )
                .unwrap()
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    benchmark_scan_small_directory,
    benchmark_scan_deep_directory,
    benchmark_scan_with_owner_info
);

criterion_group!(
    cache_benchmarks,
    benchmark_scan_with_cache_hit,
    benchmark_scan_with_cache_miss,
    benchmark_scan_incremental_deep
);

criterion_main!(benches, cache_benchmarks);
