use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use rudu::Args;
use rudu::cli::SortKey;
use rudu::scan::scan_files_and_dirs;
use rudu::thread_pool::ThreadPoolStrategy;
use rudu::utils::build_exclude_matcher;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tempfile::TempDir;
use walkdir;

#[cfg(target_os = "linux")]
use procfs::process::Process;

use std::process::Command;

/// Memory usage tracker for profiling
struct MemoryTracker {
    initial_rss: f64,
}

impl MemoryTracker {
    fn new() -> Self {
        Self {
            initial_rss: Self::current_rss_mb(),
        }
    }

    fn current_rss_mb() -> f64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(process) = Process::myself() {
                if let Ok(stat) = process.stat() {
                    // RSS is in pages, convert to MB
                    return stat.rss as f64 * 4.0 / 1024.0; // Assuming 4KB pages
                }
            }
            0.0
        }
        #[cfg(target_os = "macos")]
        {
            // Use ps command on macOS
            if let Ok(output) = Command::new("ps")
                .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
                .output()
            {
                if let Ok(rss_str) = String::from_utf8(output.stdout) {
                    if let Ok(rss_kb) = rss_str.trim().parse::<f64>() {
                        return rss_kb / 1024.0; // Convert kB to MB
                    }
                }
            }
            0.0
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            0.0
        }
    }

    fn peak_rss_mb(&self) -> f64 {
        let current = Self::current_rss_mb();
        current.max(self.initial_rss)
    }
}

/// Create a synthetic directory structure for testing
/// Reused from existing scan_benchmark.rs with modifications
fn create_synthetic_tree(dir: &Path, depth: usize, width: usize, files_per_dir: usize) {
    if depth == 0 {
        return;
    }

    // Create files in current directory
    for i in 0..files_per_dir {
        let file_path = dir.join(format!("file_{}.txt", i));
        let content = format!("Content of file {} - {}", i, "x".repeat(100 * i));
        fs::write(&file_path, content).unwrap();
    }

    // Create subdirectories
    for i in 0..width {
        let subdir_path = dir.join(format!("subdir_{}", i));
        fs::create_dir_all(&subdir_path).unwrap();
        create_synthetic_tree(&subdir_path, depth - 1, width, files_per_dir);
    }
}

/// Create a small tree (low depth, few files) - baseline
fn create_small_tree(dir: &Path) {
    create_synthetic_tree(dir, 3, 3, 5);
}

/// Create a wide tree (shallow but many directories)
fn create_wide_tree(dir: &Path) {
    create_synthetic_tree(dir, 2, 10, 20);
}

/// Create a deep tree (many levels)
fn create_deep_tree(dir: &Path) {
    create_synthetic_tree(dir, 8, 2, 5);
}

/// Helper function to create Args with common settings
fn create_args(path: PathBuf) -> Args {
    Args {
        path,
        depth: None,
        sort: SortKey::Size,
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
    }
}

/// Benchmark function with profiling
fn profile_scan_function<F>(
    c: &mut Criterion,
    name: &str,
    setup: F,
    show_owner: bool,
    show_inodes: bool,
) where
    F: Fn() -> TempDir,
{
    let mut group = c.benchmark_group(name);

    // Setup the test directory
    let temp_dir = setup();
    let root = temp_dir.path();

    // Calculate total entries for throughput measurement
    let total_entries = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .count();

    group.throughput(Throughput::Elements(total_entries as u64));

    let mut args = create_args(root.to_path_buf());
    args.show_owner = show_owner;
    args.show_inodes = show_inodes;

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    group.bench_function("scan", |b| {
        b.iter_custom(|iters| {
            let mut total_duration = std::time::Duration::new(0, 0);
            let mut peak_memory = 0.0;

            for _i in 0..iters {
                let memory_tracker = MemoryTracker::new();
                let start = Instant::now();

                let result = scan_files_and_dirs(
                    black_box(root),
                    black_box(&args),
                    black_box(&exclude_matcher),
                    black_box(SortKey::Size),
                );

                let duration = start.elapsed();
                total_duration += duration;

                // Track peak memory
                let current_memory = memory_tracker.peak_rss_mb();
                if current_memory > peak_memory {
                    peak_memory = current_memory;
                }

                // Ensure the result is used
                black_box(result.unwrap());
            }

            // Print memory usage info
            if peak_memory > 0.0 {
                eprintln!(
                    "Benchmark {} - Peak RSS: {:.2} MB, Total entries: {}",
                    name, peak_memory, total_entries
                );
            }

            total_duration
        });
    });

    group.finish();
}

fn benchmark_small_tree(c: &mut Criterion) {
    profile_scan_function(
        c,
        "small_tree",
        || {
            let temp_dir = TempDir::new().unwrap();
            create_small_tree(temp_dir.path());
            temp_dir
        },
        false,
        true,
    );
}

fn benchmark_wide_tree(c: &mut Criterion) {
    profile_scan_function(
        c,
        "wide_tree",
        || {
            let temp_dir = TempDir::new().unwrap();
            create_wide_tree(temp_dir.path());
            temp_dir
        },
        false,
        true,
    );
}

fn benchmark_deep_tree(c: &mut Criterion) {
    profile_scan_function(
        c,
        "deep_tree",
        || {
            let temp_dir = TempDir::new().unwrap();
            create_deep_tree(temp_dir.path());
            temp_dir
        },
        false,
        true,
    );
}

fn benchmark_with_owner_info(c: &mut Criterion) {
    profile_scan_function(
        c,
        "with_owner_info",
        || {
            let temp_dir = TempDir::new().unwrap();
            create_small_tree(temp_dir.path());
            temp_dir
        },
        true,
        true,
    );
}

fn benchmark_real_world_sample(c: &mut Criterion) {
    // Check for environment variable to specify real-world path
    let real_world_path =
        env::var("RUDU_BENCHMARK_PATH").unwrap_or_else(|_| "/usr/share".to_string());

    let path = Path::new(&real_world_path);

    // Only run this benchmark if the path exists
    if path.exists() {
        let mut group = c.benchmark_group("real_world_sample");

        // Calculate total entries for throughput measurement
        let total_entries = walkdir::WalkDir::new(path)
            .max_depth(3) // Limit depth to avoid extremely long runs
            .into_iter()
            .filter_map(|e| e.ok())
            .count();

        group.throughput(Throughput::Elements(total_entries as u64));

        let mut args = create_args(path.to_path_buf());
        args.depth = Some(3); // Limit depth for real-world benchmark

        let exclude_matcher = build_exclude_matcher(&[]).unwrap();

        group.bench_function("scan", |b| {
            b.iter_custom(|iters| {
                let mut total_duration = std::time::Duration::new(0, 0);
                let mut peak_memory = 0.0;

                for _i in 0..iters {
                    let memory_tracker = MemoryTracker::new();
                    let start = Instant::now();

                    let result = scan_files_and_dirs(
                        black_box(path),
                        black_box(&args),
                        black_box(&exclude_matcher),
                        black_box(SortKey::Size),
                    );

                    let duration = start.elapsed();
                    total_duration += duration;

                    // Track peak memory
                    let current_memory = memory_tracker.peak_rss_mb();
                    if current_memory > peak_memory {
                        peak_memory = current_memory;
                    }

                    // Ensure the result is used
                    black_box(result.unwrap());
                }

                // Print memory usage info
                if peak_memory > 0.0 {
                    eprintln!(
                        "Real-world benchmark ({}) - Peak RSS: {:.2} MB, Total entries: {}",
                        real_world_path, peak_memory, total_entries
                    );
                }

                total_duration
            });
        });

        group.finish();
    } else {
        eprintln!(
            "Real-world benchmark path '{}' does not exist, skipping...",
            real_world_path
        );
    }
}

/// Comprehensive scaling benchmark
fn benchmark_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling");

    // Test different sizes
    for size in [100, 500, 1000, 2000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let temp_dir = TempDir::new().unwrap();
            let root = temp_dir.path();

            // Create a structure with 'size' files
            create_synthetic_tree(root, 3, 3, size / 9);

            let args = create_args(root.to_path_buf());
            let exclude_matcher = build_exclude_matcher(&[]).unwrap();

            b.iter(|| {
                let result = scan_files_and_dirs(
                    black_box(root),
                    black_box(&args),
                    black_box(&exclude_matcher),
                    black_box(SortKey::Size),
                );
                black_box(result.unwrap());
            });
        });
    }

    group.finish();
}

criterion_group!(
    profiling_benches,
    benchmark_small_tree,
    benchmark_wide_tree,
    benchmark_deep_tree,
    benchmark_with_owner_info,
    benchmark_real_world_sample,
    benchmark_scaling,
);

criterion_main!(profiling_benches);
