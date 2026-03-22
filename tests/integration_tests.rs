use rudu::cli::{Args, SortKey};
use rudu::memory::MemoryMonitor;
use rudu::scan::{scan_files_and_dirs, scan_files_and_dirs_incremental, scan_files_and_dirs_with_memory_monitor};
use rudu::thread_pool::ThreadPoolStrategy;
use rudu::data::EntryType;
use rudu::utils::{build_exclude_matcher, expand_exclude_patterns, path_depth};
use std::fs;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

#[test]
fn test_inode_counting_with_tempdir() {
    // Create a temporary directory structure for testing
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root_path = temp_dir.path();

    // Create test directory structure:
    // temp/
    // ├── dir1/
    // │   ├── file1.txt
    // │   └── file2.txt
    // ├── dir2/
    // │   ├── subdir/
    // │   │   └── file3.txt
    // │   └── file4.txt
    // └── file5.txt

    let dir1 = root_path.join("dir1");
    let dir2 = root_path.join("dir2");
    let subdir = dir2.join("subdir");

    fs::create_dir(&dir1).expect("Failed to create dir1");
    fs::create_dir(&dir2).expect("Failed to create dir2");
    fs::create_dir(&subdir).expect("Failed to create subdir");

    // Create files
    fs::write(dir1.join("file1.txt"), "content1").expect("Failed to write file1");
    fs::write(dir1.join("file2.txt"), "content2").expect("Failed to write file2");
    fs::write(subdir.join("file3.txt"), "content3").expect("Failed to write file3");
    fs::write(dir2.join("file4.txt"), "content4").expect("Failed to write file4");
    fs::write(root_path.join("file5.txt"), "content5").expect("Failed to write file5");

    // Set up args for scanning
    let args = Args {
        path: root_path.to_path_buf(),
        depth: None,
        sort: SortKey::Name,
        show_files: true,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: true,
        cache_ttl: 604800,
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher =
        build_exclude_matcher(&exclude_patterns).expect("Failed to build exclude matcher");

    // Scan the directory
    let entries = scan_files_and_dirs(root_path, &args, &exclude_matcher, args.sort)
        .expect("Failed to scan directory");

    // Verify the results
    assert!(!entries.entries.is_empty());

    // Find directory entries and verify inode counts
    let dir1_entry = entries
        .entries
        .iter()
        .find(|e| e.path == dir1)
        .expect("dir1 not found");
    let dir2_entry = entries
        .entries
        .iter()
        .find(|e| e.path == dir2)
        .expect("dir2 not found");
    let subdir_entry = entries
        .entries
        .iter()
        .find(|e| e.path == subdir)
        .expect("subdir not found");

    // dir1 should have 2 files (inodes = 2)
    assert_eq!(dir1_entry.inodes, Some(2));

    // dir2 should have 2 entries: subdir and file4.txt (inodes = 2)
    assert_eq!(dir2_entry.inodes, Some(2));

    // subdir should have 1 file (inodes = 1)
    assert_eq!(subdir_entry.inodes, Some(1));

    // Verify all files are present
    let file_paths: Vec<_> = entries
        .entries
        .iter()
        .filter(|e| e.entry_type == rudu::data::EntryType::File)
        .map(|e| e.path.file_name().unwrap().to_str().unwrap())
        .collect();

    assert!(file_paths.contains(&"file1.txt"));
    assert!(file_paths.contains(&"file2.txt"));
    assert!(file_paths.contains(&"file3.txt"));
    assert!(file_paths.contains(&"file4.txt"));
    assert!(file_paths.contains(&"file5.txt"));
}

#[test]
fn test_exclude_patterns_with_tempdir() {
    // Create a temporary directory structure for testing exclusion
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root_path = temp_dir.path();

    // Create test directory structure:
    // temp/
    // ├── node_modules/
    // │   └── package.json
    // ├── src/
    // │   └── main.rs
    // └── target/
    //     └── debug/
    //         └── app

    let node_modules = root_path.join("node_modules");
    let src = root_path.join("src");
    let target = root_path.join("target");
    let debug = target.join("debug");

    fs::create_dir(&node_modules).expect("Failed to create node_modules");
    fs::create_dir(&src).expect("Failed to create src");
    fs::create_dir(&target).expect("Failed to create target");
    fs::create_dir(&debug).expect("Failed to create debug");

    fs::write(node_modules.join("package.json"), "{}").expect("Failed to write package.json");
    fs::write(src.join("main.rs"), "fn main() {}").expect("Failed to write main.rs");
    fs::write(debug.join("app"), "binary").expect("Failed to write app");

    // Set up args with exclusions
    let args = Args {
        path: root_path.to_path_buf(),
        depth: None,
        sort: SortKey::Name,
        show_files: true,
        exclude: vec!["node_modules".to_string(), "target".to_string()],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: true,
        cache_ttl: 604800,
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher =
        build_exclude_matcher(&exclude_patterns).expect("Failed to build exclude matcher");

    // Scan the directory
    let entries = scan_files_and_dirs(root_path, &args, &exclude_matcher, args.sort)
        .expect("Failed to scan directory");

    // Verify that excluded directories are not present
    let paths: Vec<_> = entries.entries.iter().map(|e| &e.path).collect();

    assert!(!paths.contains(&&node_modules));
    assert!(!paths.contains(&&target));
    assert!(!paths.contains(&&debug));

    // Verify that src directory is present
    assert!(paths.contains(&&src));

    // Verify that files in excluded directories are not present
    let file_names: Vec<_> = entries
        .entries
        .iter()
        .filter(|e| e.entry_type == rudu::data::EntryType::File)
        .map(|e| e.path.file_name().unwrap().to_str().unwrap())
        .collect();

    assert!(!file_names.contains(&"package.json"));
    assert!(!file_names.contains(&"app"));
    assert!(file_names.contains(&"main.rs"));
}

#[test]
fn test_depth_filtering_with_tempdir() {
    // Create a temporary directory structure for testing depth filtering
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root_path = temp_dir.path();

    // Create test directory structure:
    // temp/                        (depth 0)
    // ├── level1/                  (depth 1)
    // │   ├── file_at_level2.txt   (depth 2) - in level1/
    // │   └── level2/              (depth 2)
    // │       └── level3/          (depth 3)
    // │           └── deep_file.txt (depth 4)
    // └── file_at_level1.txt       (depth 1)

    let level1 = root_path.join("level1");
    let level2 = level1.join("level2");
    let level3 = level2.join("level3");

    fs::create_dir(&level1).expect("Failed to create level1");
    fs::create_dir(&level2).expect("Failed to create level2");
    fs::create_dir(&level3).expect("Failed to create level3");

    fs::write(level3.join("deep_file.txt"), "deep content").expect("Failed to write deep_file.txt");
    fs::write(level1.join("file_at_level2.txt"), "level2 content")
        .expect("Failed to write file_at_level2.txt");
    fs::write(root_path.join("file_at_level1.txt"), "level1 content")
        .expect("Failed to write file_at_level1.txt");

    // Test with depth limit of 2
    let args = Args {
        path: root_path.to_path_buf(),
        depth: Some(2),
        sort: SortKey::Name,
        show_files: true,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: true,
        cache_ttl: 604800,
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher =
        build_exclude_matcher(&exclude_patterns).expect("Failed to build exclude matcher");

    // Scan the directory (returns all entries; depth filtering is a display concern)
    let entries = scan_files_and_dirs(root_path, &args, &exclude_matcher, args.sort)
        .expect("Failed to scan directory");

    // Apply depth filtering inline using path_depth (filter_by_depth was removed in Fix #15)
    let depth_limit = args.depth.unwrap();
    let filtered_entries: Vec<_> = entries
        .entries
        .iter()
        .filter(|e| {
            let d = path_depth(root_path, &e.path);
            match e.entry_type {
                EntryType::Dir => d <= depth_limit,
                EntryType::File => args.show_files && d <= depth_limit,
            }
        })
        .collect();

    // Verify that level3 directory is not included (depth 3 > limit 2)
    let paths: Vec<_> = filtered_entries.iter().map(|e| &e.path).collect();
    assert!(!paths.contains(&&level3));

    // Verify that level1 and level2 directories are included
    assert!(paths.contains(&&level1));
    assert!(paths.contains(&&level2));

    // Verify that files within the depth limit are included/excluded correctly
    let file_names: Vec<_> = filtered_entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .map(|e| e.path.file_name().unwrap().to_str().unwrap())
        .collect();
    assert!(file_names.contains(&"file_at_level2.txt")); // depth 2 — within limit
    assert!(!file_names.contains(&"deep_file.txt")); // depth 4 (level1/level2/level3/deep_file.txt) — excluded
}

#[test]
fn test_size_calculation_with_tempdir() {
    // Create a temporary directory structure for testing size calculation
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root_path = temp_dir.path();

    // Create test files with clearly different disk usage.
    // Using a 16x size ratio (4 KB vs 64 KB) ensures file2 occupies more blocks than file1
    // even on filesystems with large minimum block sizes (e.g. APFS 4 KB blocks).
    let file1_content = "a".repeat(4 * 1024);   // 4 KB
    let file2_content = "b".repeat(64 * 1024);  // 64 KB

    fs::write(root_path.join("file1.txt"), &file1_content).expect("Failed to write file1");
    fs::write(root_path.join("file2.txt"), &file2_content).expect("Failed to write file2");

    // Set up args for scanning
    let args = Args {
        path: root_path.to_path_buf(),
        depth: None,
        sort: SortKey::Size,
        show_files: true,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: false,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: true,
        cache_ttl: 604800,
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher =
        build_exclude_matcher(&exclude_patterns).expect("Failed to build exclude matcher");

    // Scan the directory
    let entries = scan_files_and_dirs(root_path, &args, &exclude_matcher, args.sort)
        .expect("Failed to scan directory");

    // Find file entries
    let file_entries: Vec<_> = entries
        .entries
        .iter()
        .filter(|e| e.entry_type == rudu::data::EntryType::File)
        .collect();

    // Verify files are sorted by size (largest first)
    assert!(file_entries.len() >= 2);

    // Due to sorting by size, file2 (~8KB disk usage) should come before file1 (~4KB disk usage)
    let file2_entry = file_entries
        .iter()
        .find(|e| e.path.file_name().unwrap() == "file2.txt")
        .unwrap();
    let file1_entry = file_entries
        .iter()
        .find(|e| e.path.file_name().unwrap() == "file1.txt")
        .unwrap();

    assert!(file2_entry.size > file1_entry.size);
}

#[test]
fn test_memory_limit_with_small_temp_dir() {
    // Create a small temporary directory structure
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root_path = temp_dir.path();

    // Create test directory structure with just a few files
    // temp/
    // ├── dir1/
    // │   └── file1.txt
    // └── file2.txt

    let dir1 = root_path.join("dir1");
    fs::create_dir(&dir1).expect("Failed to create dir1");

    // Create some small files
    fs::write(dir1.join("file1.txt"), "small content 1").expect("Failed to write file1");
    fs::write(root_path.join("file2.txt"), "small content 2").expect("Failed to write file2");

    // Set up args for scanning with a very low memory limit (1 MB)
    let args = Args {
        path: root_path.to_path_buf(),
        depth: None,
        sort: SortKey::Name,
        show_files: true,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: false,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: true, // Disable cache to simplify the test
        cache_ttl: 604800,
        profile: false,
        memory_limit: Some(1),        // 1 MB limit - very low
        memory_check_interval_ms: 50, // Check very frequently
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher =
        build_exclude_matcher(&exclude_patterns).expect("Failed to build exclude matcher");

    // Create a memory monitor with the specified limit
    let memory_monitor = Arc::new(Mutex::new(MemoryMonitor::new(1))); // 1 MB limit

    // Scan the directory with memory monitoring
    let result = scan_files_and_dirs_with_memory_monitor(
        root_path,
        &args,
        &exclude_matcher,
        args.sort,
        Some(memory_monitor.clone()),
    );

    // Primary goal: verify the scan completes without panicking regardless of
    // whether the 1 MB limit is hit. With such a low limit the process will
    // almost always exceed it, so we don't assert on the specific memory status.
    assert!(result.is_ok(), "scan_files_and_dirs_with_memory_monitor should not error");
    let scan_result = result.unwrap();

    // memory_limit_hit and memory_status must be consistent with each other
    match scan_result.memory_status {
        rudu::scan::MemoryLimitStatus::MemoryLimitHit => {
            assert!(scan_result.memory_limit_hit, "status is MemoryLimitHit but flag is false");
        }
        _ => {
            assert!(!scan_result.memory_limit_hit, "flag is true but status is not MemoryLimitHit");
        }
    }
}

// ── scan_files_and_dirs_incremental ──────────────────────────────────────────

#[test]
fn test_incremental_scan_returns_correct_entries() {
    // Build a small, known directory tree and verify incremental scan finds
    // all expected entries with plausible sizes.
    //
    // Layout:
    //   tmp/
    //   ├── alpha/
    //   │   ├── a.txt   (4 KB)
    //   │   └── b.txt   (4 KB)
    //   └── beta/
    //       └── c.txt   (4 KB)

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    let alpha = root.join("alpha");
    let beta = root.join("beta");
    fs::create_dir(&alpha).unwrap();
    fs::create_dir(&beta).unwrap();
    fs::write(alpha.join("a.txt"), vec![0u8; 4096]).unwrap();
    fs::write(alpha.join("b.txt"), vec![0u8; 4096]).unwrap();
    fs::write(beta.join("c.txt"), vec![0u8; 4096]).unwrap();

    let args = Args {
        path: root.to_path_buf(),
        depth: None,
        sort: SortKey::Name,
        show_files: true,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: false,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: true,
        cache_ttl: 604800,
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher = build_exclude_matcher(&exclude_patterns).unwrap();

    let result = scan_files_and_dirs_incremental(root, &args, &exclude_matcher, args.sort);
    assert!(result.is_ok(), "incremental scan should not error: {:?}", result);

    let scan = result.unwrap();
    assert!(
        !scan.entries.is_empty(),
        "incremental scan should return at least one entry"
    );

    // All returned paths must be descendants of (or equal to) the root
    for entry in &scan.entries {
        assert!(
            entry.path.starts_with(root),
            "entry path {:?} should be under root {:?}",
            entry.path,
            root
        );
    }

    // Both subdirectories should appear
    let paths: Vec<_> = scan.entries.iter().map(|e| e.path.as_path()).collect();
    assert!(
        paths.iter().any(|p| p.ends_with("alpha")),
        "alpha dir should appear in results"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("beta")),
        "beta dir should appear in results"
    );
}

#[test]
fn test_incremental_scan_second_run_uses_cache() {
    // Running the scan twice on the same unchanged directory should produce
    // a non-zero cache_total on the second run (entries were cached after the first).
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    let subdir = root.join("cached_dir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("data.txt"), vec![1u8; 4096]).unwrap();

    // Use a dedicated cache dir so the test is isolated
    let cache_dir = TempDir::new().expect("Failed to create cache temp dir");
    // SAFETY: this test is single-threaded with respect to RUDU_CACHE_DIR;
    // the variable is restored before the test returns.
    unsafe { std::env::set_var("RUDU_CACHE_DIR", cache_dir.path()) };

    let make_args = || Args {
        path: root.to_path_buf(),
        depth: None,
        sort: SortKey::Name,
        show_files: true,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: false,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: false, // enable caching
        cache_ttl: 604800,
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    };

    let exclude_patterns = expand_exclude_patterns(&[]);
    let exclude_matcher = build_exclude_matcher(&exclude_patterns).unwrap();

    // First scan — populates the cache
    let first = scan_files_and_dirs_incremental(root, &make_args(), &exclude_matcher, SortKey::Name)
        .expect("first scan should succeed");

    // Second scan — should see cache entries
    let second = scan_files_and_dirs_incremental(root, &make_args(), &exclude_matcher, SortKey::Name)
        .expect("second scan should succeed");

    // SAFETY: restoring the env var we set above.
    unsafe { std::env::remove_var("RUDU_CACHE_DIR") };

    // After a successful first scan, cache_total on the second should be > 0
    assert!(
        second.cache_total > 0,
        "second scan cache_total should be > 0 (first={} second={})",
        first.cache_total,
        second.cache_total,
    );
}
