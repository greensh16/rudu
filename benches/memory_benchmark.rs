use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rudu::cli::SortKey;
use rudu::scan::{scan_files_and_dirs, scan_files_and_dirs_incremental};
use rudu::thread_pool::ThreadPoolStrategy;
use rudu::utils::build_exclude_matcher;
use rudu::Args;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[cfg(target_os = "linux")]
use procfs::process::Process;

/// Memory usage tracker for benchmarks
#[derive(Debug, Clone)]
struct MemoryUsage {
    peak_rss: u64,  // Peak resident set size in bytes
    peak_vms: u64,  // Peak virtual memory size in bytes
    start_rss: u64, // RSS at start
    start_vms: u64, // VMS at start
}

impl MemoryUsage {
    fn new() -> Self {
        let (start_rss, start_vms) = get_memory_usage();
        Self {
            peak_rss: start_rss,
            peak_vms: start_vms,
            start_rss,
            start_vms,
        }
    }

    fn update(&mut self) {
        let (current_rss, current_vms) = get_memory_usage();
        if current_rss > self.peak_rss {
            self.peak_rss = current_rss;
        }
        if current_vms > self.peak_vms {
            self.peak_vms = current_vms;
        }
    }

    fn peak_memory_mb(&self) -> f64 {
        (self.peak_rss - self.start_rss) as f64 / 1024.0 / 1024.0
    }
}

#[cfg(target_os = "linux")]
fn get_memory_usage() -> (u64, u64) {
    if let Ok(process) = Process::myself() {
        if let Ok(stat) = process.stat() {
            return (stat.rss * 4096, stat.vsize); // RSS is in pages, VMS is in bytes
        }
    }
    (0, 0)
}

#[cfg(not(target_os = "linux"))]
fn get_memory_usage() -> (u64, u64) {
    // Fallback for non-Linux systems - could use platform-specific APIs
    // For now, return 0 to indicate unsupported
    (0, 0)
}

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

fn create_large_directory_structure(dir: &Path, depth: usize, files_per_dir: usize) {
    if depth == 0 {
        return;
    }

    // Create many files to stress memory usage
    for i in 0..files_per_dir {
        let file_path = dir.join(format!("large_file_{}.txt", i));
        // Create larger files to increase memory pressure
        let content = format!("Large file content {} - {}", i, "x".repeat(1000));
        fs::write(&file_path, content).unwrap();
    }

    // Create more subdirectories for larger tree
    let subdirs = if depth > 3 { 5 } else { 4 };
    for i in 0..subdirs {
        let subdir_path = dir.join(format!("large_subdir_{}", i));
        fs::create_dir_all(&subdir_path).unwrap();
        create_large_directory_structure(&subdir_path, depth - 1, files_per_dir);
    }
}

fn memory_benchmark_small_scan(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a small directory structure
    create_test_directory_structure(root, 3, 10);

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
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("memory_small_scan", |b| {
        b.iter_custom(|iters| {
            let mut total_duration = std::time::Duration::from_secs(0);
            let mut memory_tracker = MemoryUsage::new();

            for _i in 0..iters {
                let start = std::time::Instant::now();

                let _result = scan_files_and_dirs(
                    black_box(root),
                    black_box(&args),
                    black_box(&exclude_matcher),
                    black_box(SortKey::Size),
                )
                .unwrap();

                memory_tracker.update();
                total_duration += start.elapsed();
            }

            // Report memory usage
            println!(
                "Small scan peak memory usage: {:.2} MB",
                memory_tracker.peak_memory_mb()
            );
            total_duration
        })
    });
}

fn memory_benchmark_large_scan(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a large directory structure to stress memory
    create_large_directory_structure(root, 5, 20);

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
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("memory_large_scan", |b| {
        b.iter_custom(|iters| {
            let mut total_duration = std::time::Duration::from_secs(0);
            let mut memory_tracker = MemoryUsage::new();

            for _i in 0..iters {
                let start = std::time::Instant::now();

                let _result = scan_files_and_dirs(
                    black_box(root),
                    black_box(&args),
                    black_box(&exclude_matcher),
                    black_box(SortKey::Size),
                )
                .unwrap();

                memory_tracker.update();
                total_duration += start.elapsed();
            }

            // Report memory usage
            println!(
                "Large scan peak memory usage: {:.2} MB",
                memory_tracker.peak_memory_mb()
            );
            total_duration
        })
    });
}

fn memory_benchmark_cache_operations(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a moderate directory structure
    create_test_directory_structure(root, 4, 15);

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
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("memory_cache_operations", |b| {
        b.iter_custom(|iters| {
            let mut total_duration = std::time::Duration::from_secs(0);
            let mut memory_tracker = MemoryUsage::new();

            for _i in 0..iters {
                let start = std::time::Instant::now();

                // First scan to create cache
                let _result = scan_files_and_dirs(
                    black_box(root),
                    black_box(&args),
                    black_box(&exclude_matcher),
                    black_box(SortKey::Size),
                )
                .unwrap();

                memory_tracker.update();

                // Second scan using cache
                let _result = scan_files_and_dirs_incremental(
                    black_box(root),
                    black_box(&args),
                    black_box(&exclude_matcher),
                    black_box(SortKey::Size),
                )
                .unwrap();

                memory_tracker.update();
                total_duration += start.elapsed();

                // Clean up cache for next iteration
                let _ = std::fs::remove_file(root.join(".rudu-cache.bin"));
            }

            // Report memory usage
            println!(
                "Cache operations peak memory usage: {:.2} MB",
                memory_tracker.peak_memory_mb()
            );
            total_duration
        })
    });
}

fn memory_benchmark_threaded_scan(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a large directory structure
    create_large_directory_structure(root, 4, 25);

    let args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: rudu::cli::SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: true, // Enable owner info to stress memory more
        output: None,
        threads: Some(num_cpus::get()),
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::WorkStealingUneven,
        no_cache: false,
        cache_ttl: 604800, // 7 days
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("memory_threaded_scan", |b| {
        b.iter_custom(|iters| {
            let mut total_duration = std::time::Duration::from_secs(0);
            let mut memory_tracker = MemoryUsage::new();

            for _i in 0..iters {
                let start = std::time::Instant::now();

                let _result = scan_files_and_dirs(
                    black_box(root),
                    black_box(&args),
                    black_box(&exclude_matcher),
                    black_box(SortKey::Size),
                )
                .unwrap();

                memory_tracker.update();
                total_duration += start.elapsed();
            }

            // Report memory usage
            println!(
                "Threaded scan peak memory usage: {:.2} MB",
                memory_tracker.peak_memory_mb()
            );
            total_duration
        })
    });
}

criterion_group!(
    memory_benchmarks,
    memory_benchmark_small_scan,
    memory_benchmark_large_scan,
    memory_benchmark_cache_operations,
    memory_benchmark_threaded_scan
);

criterion_main!(memory_benchmarks);
