use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rudu::cli::SortKey;
use rudu::scan::scan_files_and_dirs;
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

criterion_group!(
    benches,
    benchmark_scan_small_directory,
    benchmark_scan_deep_directory,
    benchmark_scan_with_owner_info
);
criterion_main!(benches);
