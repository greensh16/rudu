use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rudu::cli::SortKey;
use rudu::scan::scan_files_and_dirs;
use rudu::thread_pool::ThreadPoolStrategy;
use rudu::utils::build_exclude_matcher;
use rudu::Args;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

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

fn create_io_heavy_structure(dir: &Path, depth: usize, files_per_dir: usize) {
    if depth == 0 {
        return;
    }

    // Create more files with larger content for I/O heavy workload
    for i in 0..files_per_dir {
        let file_path = dir.join(format!("large_file_{}.txt", i));
        let content = format!("Large file content {}\n", i).repeat(1000); // ~26KB per file
        fs::write(&file_path, content).unwrap();
    }

    // Create more subdirectories for I/O heavy workload
    for i in 0..5 {
        let subdir_path = dir.join(format!("subdir_{}", i));
        fs::create_dir_all(&subdir_path).unwrap();
        create_io_heavy_structure(&subdir_path, depth - 1, files_per_dir);
    }
}

#[derive(Debug, Clone)]
struct BenchmarkParams {
    strategy: ThreadPoolStrategy,
    n_threads: usize,
    workload: &'static str,
}

impl BenchmarkParams {
    fn id(&self) -> String {
        format!(
            "{}_{}_threads_{}",
            self.strategy.as_str(),
            self.n_threads,
            self.workload
        )
    }
}

fn thread_pool_benchmark(c: &mut Criterion) {
    let num_cpus = num_cpus::get();

    // Create test directories for different workloads
    let small_temp_dir = TempDir::new().unwrap();
    let small_root = small_temp_dir.path();
    create_test_directory_structure(small_root, 3, 5);

    let io_heavy_temp_dir = TempDir::new().unwrap();
    let io_heavy_root = io_heavy_temp_dir.path();
    create_io_heavy_structure(io_heavy_root, 3, 8);

    let deep_temp_dir = TempDir::new().unwrap();
    let deep_root = deep_temp_dir.path();
    create_test_directory_structure(deep_root, 6, 12);

    let strategies = vec![
        ThreadPoolStrategy::Default,
        ThreadPoolStrategy::Fixed,
        ThreadPoolStrategy::NumCpusMinus1,
        ThreadPoolStrategy::IOHeavy,
        ThreadPoolStrategy::WorkStealingUneven,
    ];

    let mut group = c.benchmark_group("thread_pool_strategies");

    for strategy in strategies {
        for n_threads in 1..=num_cpus {
            // Skip invalid combinations
            if strategy == ThreadPoolStrategy::Default && n_threads != num_cpus {
                continue;
            }
            if strategy == ThreadPoolStrategy::NumCpusMinus1
                && n_threads != std::cmp::max(1, num_cpus - 1)
            {
                continue;
            }
            if strategy == ThreadPoolStrategy::IOHeavy && n_threads != num_cpus * 2 {
                continue;
            }
            if strategy == ThreadPoolStrategy::WorkStealingUneven && n_threads != num_cpus {
                continue;
            }

            let params = vec![
                BenchmarkParams {
                    strategy,
                    n_threads,
                    workload: "small",
                },
                BenchmarkParams {
                    strategy,
                    n_threads,
                    workload: "io_heavy",
                },
                BenchmarkParams {
                    strategy,
                    n_threads,
                    workload: "deep",
                },
            ];

            for param in params {
                let (test_root, test_args) = match param.workload {
                    "small" => (
                        small_root,
                        Args {
                            path: small_root.to_path_buf(),
                            depth: None,
                            sort: SortKey::Size,
                            show_files: false,
                            exclude: vec![],
                            show_owner: false,
                            output: None,
                            threads: Some(param.n_threads),
                            show_inodes: true,
                            threads_strategy: param.strategy,
                            no_cache: false,
                            cache_ttl: 604800,
                            profile: false,
                        },
                    ),
                    "io_heavy" => (
                        io_heavy_root,
                        Args {
                            path: io_heavy_root.to_path_buf(),
                            depth: None,
                            sort: SortKey::Size,
                            show_files: false,
                            exclude: vec![],
                            show_owner: true, // Enable owner info for I/O heavy workload
                            output: None,
                            threads: Some(param.n_threads),
                            show_inodes: true,
                            threads_strategy: param.strategy,
                            no_cache: false,
                            cache_ttl: 604800,
                            profile: false,
                        },
                    ),
                    "deep" => (
                        deep_root,
                        Args {
                            path: deep_root.to_path_buf(),
                            depth: None,
                            sort: SortKey::Size,
                            show_files: true,
                            exclude: vec![],
                            show_owner: false,
                            output: None,
                            threads: Some(param.n_threads),
                            show_inodes: true,
                            threads_strategy: param.strategy,
                            no_cache: false,
                            cache_ttl: 604800,
                            profile: false,
                        },
                    ),
                    _ => unreachable!(),
                };

                let exclude_matcher = build_exclude_matcher(&[]).unwrap();

                group.bench_with_input(
                    BenchmarkId::new("thread_pool_performance", param.id()),
                    &param,
                    |b, _param| {
                        b.iter(|| {
                            // Note: We need to reset the thread pool for each iteration
                            // This is a limitation of the current setup, but gives us
                            // comparative results for different strategies
                            scan_files_and_dirs(
                                black_box(test_root),
                                black_box(&test_args),
                                black_box(&exclude_matcher),
                                black_box(SortKey::Size),
                            )
                            .unwrap()
                        })
                    },
                );
            }
        }
    }

    group.finish();
}

// Simplified benchmark that focuses on comparing strategies with optimal thread counts
fn strategy_comparison_benchmark(c: &mut Criterion) {
    let num_cpus = num_cpus::get();

    // Create test directory
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    create_test_directory_structure(root, 4, 8);

    let strategies_with_threads = vec![
        (ThreadPoolStrategy::Default, num_cpus),
        (ThreadPoolStrategy::Fixed, num_cpus),
        (
            ThreadPoolStrategy::NumCpusMinus1,
            std::cmp::max(1, num_cpus - 1),
        ),
        (ThreadPoolStrategy::IOHeavy, num_cpus * 2),
        (ThreadPoolStrategy::WorkStealingUneven, num_cpus),
    ];

    let mut group = c.benchmark_group("strategy_comparison");

    for (strategy, threads) in strategies_with_threads {
        let args = Args {
            path: root.to_path_buf(),
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
            BenchmarkId::new("strategy", strategy.as_str()),
            &strategy,
            |b, _strategy| {
                b.iter(|| {
                    scan_files_and_dirs(
                        black_box(root),
                        black_box(&args),
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
    thread_pool_benchmark,
    strategy_comparison_benchmark
);
criterion_main!(benches);
