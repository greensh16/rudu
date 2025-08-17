use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rudu::Args;
use rudu::cli::SortKey;
use rudu::memory::MemoryMonitor;
use rudu::scan::{scan_files_and_dirs, scan_files_and_dirs_with_memory_monitor};
use rudu::thread_pool::ThreadPoolStrategy;
use rudu::utils::build_exclude_matcher;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

fn create_large_directory_structure(dir: &Path, depth: usize, files_per_dir: usize) {
    if depth == 0 {
        return;
    }

    // Create many files to create a realistic overhead test scenario
    for i in 0..files_per_dir {
        let file_path = dir.join(format!("test_file_{}.txt", i));
        let content = format!("Test content for file {} in directory {:?}", i, dir);
        fs::write(&file_path, content).unwrap();
    }

    // Create subdirectories
    let subdirs = if depth > 4 { 4 } else { 3 };
    for i in 0..subdirs {
        let subdir_path = dir.join(format!("subdir_{}", i));
        fs::create_dir_all(&subdir_path).unwrap();
        create_large_directory_structure(&subdir_path, depth - 1, files_per_dir);
    }
}

fn memory_monitoring_overhead_benchmark(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a substantial directory structure for realistic overhead testing
    // This creates approximately 20,000+ files across multiple directory levels
    create_large_directory_structure(root, 6, 50);

    let base_args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: true, // Disable cache to ensure consistent benchmark
        cache_ttl: 604800,
        profile: false,
        memory_limit: Some(1000),      // 1GB limit (generous for this test)
        memory_check_interval_ms: 200, // Default interval
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    // Benchmark group for comparing different memory check intervals
    let mut group = c.benchmark_group("memory_monitoring_overhead");

    // Test different memory check intervals to find optimal settings
    for &interval_ms in &[50, 100, 200, 500, 1000] {
        let mut args_with_interval = base_args.clone();
        args_with_interval.memory_check_interval_ms = interval_ms;

        group.bench_with_input(
            BenchmarkId::new("with_memory_monitor", interval_ms),
            &interval_ms,
            |b, &_interval| {
                b.iter(|| {
                    let monitor = Arc::new(Mutex::new(MemoryMonitor::new_with_interval(
                        1000, // 1GB limit
                        interval_ms,
                    )));

                    scan_files_and_dirs_with_memory_monitor(
                        black_box(root),
                        black_box(&args_with_interval),
                        black_box(&exclude_matcher),
                        black_box(SortKey::Size),
                        Some(monitor),
                    )
                    .unwrap()
                });
            },
        );
    }

    // Baseline benchmark without memory monitoring
    group.bench_function("without_memory_monitor", |b| {
        b.iter(|| {
            scan_files_and_dirs(
                black_box(root),
                black_box(&base_args),
                black_box(&exclude_matcher),
                black_box(SortKey::Size),
            )
            .unwrap()
        })
    });

    group.finish();
}

fn memory_monitoring_accuracy_benchmark(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a moderate directory structure
    create_large_directory_structure(root, 5, 30);

    let args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: true,
        cache_ttl: 604800,
        profile: false,
        memory_limit: Some(50), // Very low limit to test monitoring accuracy
        memory_check_interval_ms: 100, // Frequent checks for accuracy
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    c.bench_function("memory_monitoring_accuracy", |b| {
        b.iter(|| {
            let monitor = Arc::new(Mutex::new(MemoryMonitor::new_with_interval(50, 100)));

            let result = scan_files_and_dirs_with_memory_monitor(
                black_box(root),
                black_box(&args),
                black_box(&exclude_matcher),
                black_box(SortKey::Size),
                Some(monitor),
            )
            .unwrap();

            // The scan should complete but may hit memory limits with such a low threshold
            // This tests that memory monitoring works correctly under pressure
            black_box(result);
        })
    });
}

fn memory_check_interval_tuning_benchmark(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a very large directory structure to maximize memory monitoring calls
    create_large_directory_structure(root, 7, 100);

    let base_args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: true,
        cache_ttl: 604800,
        profile: false,
        memory_limit: Some(2000), // Large enough to not interfere
        memory_check_interval_ms: 200,
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    let mut group = c.benchmark_group("memory_check_interval_tuning");

    // Test a wide range of intervals to find the sweet spot
    for &interval_ms in &[25, 50, 100, 200, 400, 800] {
        let mut args_with_interval = base_args.clone();
        args_with_interval.memory_check_interval_ms = interval_ms;

        group.bench_with_input(
            BenchmarkId::new("interval_ms", interval_ms),
            &interval_ms,
            |b, &_interval| {
                b.iter(|| {
                    let monitor = Arc::new(Mutex::new(MemoryMonitor::new_with_interval(
                        2000,
                        interval_ms,
                    )));

                    scan_files_and_dirs_with_memory_monitor(
                        black_box(root),
                        black_box(&args_with_interval),
                        black_box(&exclude_matcher),
                        black_box(SortKey::Size),
                        Some(monitor),
                    )
                    .unwrap()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark to verify memory monitoring has ≤1% overhead
fn one_percent_overhead_validation_benchmark(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a realistic large-scale directory structure
    // This represents a typical large scan scenario where overhead matters
    create_large_directory_structure(root, 6, 75);

    let args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: SortKey::Size,
        show_files: false,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: true, // Disable cache for consistent measurements
        cache_ttl: 604800,
        profile: false,
        memory_limit: Some(4000), // High limit to avoid triggering limits
        memory_check_interval_ms: 200, // Default interval
    };

    let exclude_matcher = build_exclude_matcher(&[]).unwrap();

    let mut group = c.benchmark_group("one_percent_overhead_validation");

    // Configure longer measurement time for accurate results
    group.sample_size(20);
    group.measurement_time(std::time::Duration::from_secs(30));

    // Baseline: scan without memory monitoring
    group.bench_function("baseline_no_monitoring", |b| {
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

    // Test: scan with memory monitoring (default 200ms interval)
    group.bench_function("with_monitoring_200ms", |b| {
        b.iter(|| {
            let monitor = Arc::new(Mutex::new(MemoryMonitor::new_with_interval(4000, 200)));

            scan_files_and_dirs_with_memory_monitor(
                black_box(root),
                black_box(&args),
                black_box(&exclude_matcher),
                black_box(SortKey::Size),
                Some(monitor),
            )
            .unwrap()
        })
    });

    // Test: scan with memory monitoring (optimized 500ms interval)
    group.bench_function("with_monitoring_500ms", |b| {
        b.iter(|| {
            let monitor = Arc::new(Mutex::new(MemoryMonitor::new_with_interval(4000, 500)));

            scan_files_and_dirs_with_memory_monitor(
                black_box(root),
                black_box(&args),
                black_box(&exclude_matcher),
                black_box(SortKey::Size),
                Some(monitor),
            )
            .unwrap()
        })
    });

    group.finish();
}

criterion_group!(
    overhead_benchmarks,
    memory_monitoring_overhead_benchmark,
    memory_monitoring_accuracy_benchmark,
    memory_check_interval_tuning_benchmark,
    one_percent_overhead_validation_benchmark
);

criterion_main!(overhead_benchmarks);
