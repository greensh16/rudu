use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rudu::Args;
use rudu::cli::SortKey;
use rudu::scan::scan_files_and_dirs;
use rudu::thread_pool::ThreadPoolStrategy;
use rudu::utils::build_exclude_matcher;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Creates a large directory structure specifically designed to test work-stealing
fn create_large_directory_structure(dir: &Path, large_dirs: usize, files_per_large_dir: usize) {
    // Create multiple large directories (each with > 10k files)
    for i in 0..large_dirs {
        let large_dir = dir.join(format!("large_dir_{}", i));
        fs::create_dir_all(&large_dir).unwrap();

        // Create many files in each large directory
        for j in 0..files_per_large_dir {
            let file_path = large_dir.join(format!("file_{:05}.txt", j));
            fs::write(
                &file_path,
                format!("Content of file {} in large dir {}", j, i),
            )
            .unwrap();
        }
    }

    // Create some smaller directories for comparison
    for i in 0..5 {
        let small_dir = dir.join(format!("small_dir_{}", i));
        fs::create_dir_all(&small_dir).unwrap();

        // Create fewer files in small directories
        for j in 0..100 {
            let file_path = small_dir.join(format!("file_{:03}.txt", j));
            fs::write(
                &file_path,
                format!("Content of file {} in small dir {}", j, i),
            )
            .unwrap();
        }
    }
}

/// Creates an extremely uneven directory structure
fn create_uneven_directory_structure(dir: &Path) {
    // Create one very large directory
    let huge_dir = dir.join("huge_directory");
    fs::create_dir_all(&huge_dir).unwrap();

    // Create 15,000 files in the huge directory
    for i in 0..15_000 {
        let file_path = huge_dir.join(format!("huge_file_{:05}.txt", i));
        fs::write(&file_path, format!("Content of huge file {}", i)).unwrap();
    }

    // Create another large directory
    let large_dir = dir.join("large_directory");
    fs::create_dir_all(&large_dir).unwrap();

    // Create 12,000 files in the large directory
    for i in 0..12_000 {
        let file_path = large_dir.join(format!("large_file_{:05}.txt", i));
        fs::write(&file_path, format!("Content of large file {}", i)).unwrap();
    }

    // Create many small directories
    for i in 0..20 {
        let small_dir = dir.join(format!("small_dir_{}", i));
        fs::create_dir_all(&small_dir).unwrap();

        for j in 0..50 {
            let file_path = small_dir.join(format!("small_file_{:03}.txt", j));
            fs::write(
                &file_path,
                format!("Content of small file {} in dir {}", j, i),
            )
            .unwrap();
        }
    }
}

fn work_stealing_benchmark(c: &mut Criterion) {
    let num_cpus = num_cpus::get();

    // Create test directories
    let large_temp_dir = TempDir::new().unwrap();
    let large_root = large_temp_dir.path();
    // Create 3 directories with 12k files each (total 36k files)
    create_large_directory_structure(large_root, 3, 12_000);

    let uneven_temp_dir = TempDir::new().unwrap();
    let uneven_root = uneven_temp_dir.path();
    create_uneven_directory_structure(uneven_root);

    let strategies = vec![
        ThreadPoolStrategy::Default,
        ThreadPoolStrategy::Fixed,
        ThreadPoolStrategy::IOHeavy,
        ThreadPoolStrategy::WorkStealingUneven,
    ];

    let mut group = c.benchmark_group("work_stealing_comparison");
    // Use longer sample and measurement time for large directories
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(60));

    for strategy in strategies {
        let threads = match strategy {
            ThreadPoolStrategy::Default => num_cpus,
            ThreadPoolStrategy::Fixed => num_cpus,
            ThreadPoolStrategy::IOHeavy => num_cpus * 2,
            ThreadPoolStrategy::WorkStealingUneven => num_cpus,
            _ => num_cpus,
        };

        // Test with large directories
        let large_args = Args {
            path: large_root.to_path_buf(),
            depth: None,
            sort: SortKey::Size,
            show_files: false,
            exclude: vec![],
            show_owner: false,
            output: None,
            threads: Some(threads),
            show_inodes: true,
            threads_strategy: strategy,
            no_cache: false,
            cache_ttl: 604800,
            profile: false,
        };

        let exclude_matcher = build_exclude_matcher(&[]).unwrap();

        group.bench_with_input(
            BenchmarkId::new("large_dirs", strategy.as_str()),
            &strategy,
            |b, _strategy| {
                b.iter(|| {
                    scan_files_and_dirs(
                        black_box(large_root),
                        black_box(&large_args),
                        black_box(&exclude_matcher),
                        black_box(SortKey::Size),
                    )
                    .unwrap()
                })
            },
        );

        // Test with uneven directories
        let uneven_args = Args {
            path: uneven_root.to_path_buf(),
            depth: None,
            sort: SortKey::Size,
            show_files: false,
            exclude: vec![],
            show_owner: false,
            output: None,
            threads: Some(threads),
            show_inodes: true,
            threads_strategy: strategy,
            no_cache: false,
            cache_ttl: 604800,
            profile: false,
        };

        group.bench_with_input(
            BenchmarkId::new("uneven_dirs", strategy.as_str()),
            &strategy,
            |b, _strategy| {
                b.iter(|| {
                    scan_files_and_dirs(
                        black_box(uneven_root),
                        black_box(&uneven_args),
                        black_box(&exclude_matcher),
                        black_box(SortKey::Size),
                    )
                    .unwrap()
                })
            },
        );
    }

    group.finish();
}

fn work_stealing_scalability_benchmark(c: &mut Criterion) {
    let num_cpus = num_cpus::get();

    // Create an extremely uneven directory structure
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    create_uneven_directory_structure(root);

    let thread_counts = vec![1, 2, 4, num_cpus, num_cpus * 2];

    let mut group = c.benchmark_group("work_stealing_scalability");
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(45));

    for threads in thread_counts {
        // Test default strategy
        let default_args = Args {
            path: root.to_path_buf(),
            depth: None,
            sort: SortKey::Size,
            show_files: false,
            exclude: vec![],
            show_owner: false,
            output: None,
            threads: Some(threads),
            show_inodes: true,
            threads_strategy: ThreadPoolStrategy::Default,
            no_cache: false,
            cache_ttl: 604800,
            profile: false,
        };

        // Test work-stealing strategy
        let work_stealing_args = Args {
            path: root.to_path_buf(),
            depth: None,
            sort: SortKey::Size,
            show_files: false,
            exclude: vec![],
            show_owner: false,
            output: None,
            threads: Some(threads),
            show_inodes: true,
            threads_strategy: ThreadPoolStrategy::WorkStealingUneven,
            no_cache: false,
            cache_ttl: 604800,
            profile: false,
        };

        let exclude_matcher = build_exclude_matcher(&[]).unwrap();

        group.bench_with_input(
            BenchmarkId::new("default", threads),
            &threads,
            |b, _threads| {
                b.iter(|| {
                    scan_files_and_dirs(
                        black_box(root),
                        black_box(&default_args),
                        black_box(&exclude_matcher),
                        black_box(SortKey::Size),
                    )
                    .unwrap()
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("work_stealing", threads),
            &threads,
            |b, _threads| {
                b.iter(|| {
                    scan_files_and_dirs(
                        black_box(root),
                        black_box(&work_stealing_args),
                        black_box(&exclude_matcher),
                        black_box(SortKey::Size),
                    )
                    .unwrap()
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    work_stealing_benchmark,
    work_stealing_scalability_benchmark
);
criterion_main!(benches);
