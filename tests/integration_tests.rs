use rudu::cli::{Args, SortKey};
use rudu::data::EntryType;
use rudu::memory::MemoryMonitor;
use rudu::scan::{
    scan_files_and_dirs, scan_files_and_dirs_incremental, scan_files_and_dirs_with_memory_monitor,
};
use rudu::thread_pool::ThreadPoolStrategy;
use rudu::utils::{build_exclude_matcher, expand_exclude_patterns, get_dir_metadata, path_depth};
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
        show_atime: false,
        purge_days: 100,
        older_than: None,
        min_size: None,
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
        show_atime: false,
        purge_days: 100,
        older_than: None,
        min_size: None,
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
        show_atime: false,
        purge_days: 100,
        older_than: None,
        min_size: None,
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
    let file1_content = "a".repeat(4 * 1024); // 4 KB
    let file2_content = "b".repeat(64 * 1024); // 64 KB

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
        show_atime: false,
        purge_days: 100,
        older_than: None,
        min_size: None,
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
        show_atime: false,
        purge_days: 100,
        older_than: None,
        min_size: None,
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
    assert!(
        result.is_ok(),
        "scan_files_and_dirs_with_memory_monitor should not error"
    );
    let scan_result = result.unwrap();

    // memory_limit_hit and memory_status must be consistent with each other
    match scan_result.memory_status {
        rudu::scan::MemoryLimitStatus::MemoryLimitHit => {
            assert!(
                scan_result.memory_limit_hit,
                "status is MemoryLimitHit but flag is false"
            );
        }
        _ => {
            assert!(
                !scan_result.memory_limit_hit,
                "flag is true but status is not MemoryLimitHit"
            );
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
        show_atime: false,
        purge_days: 100,
        older_than: None,
        min_size: None,
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher = build_exclude_matcher(&exclude_patterns).unwrap();

    let result = scan_files_and_dirs_incremental(root, &args, &exclude_matcher, args.sort);
    assert!(
        result.is_ok(),
        "incremental scan should not error: {:?}",
        result
    );

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

/// Serialises tests that set `RUDU_CACHE_DIR`, which is process-global state.
static CACHE_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Builds default scan args for a root with caching disabled.
fn no_cache_args(root: &std::path::Path) -> Args {
    Args {
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
        show_atime: false,
        purge_days: 100,
        older_than: None,
        min_size: None,
    }
}

#[test]
fn test_hard_links_counted_once_in_totals() {
    // Regression test for CODE_REVIEW.md H4: a file with two hard links inside
    // the tree must contribute to directory totals exactly once, like `du`.
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    let dir1 = root.join("dir1");
    fs::create_dir(&dir1).unwrap();
    let original = dir1.join("a.txt");
    let link = dir1.join("b.txt");
    fs::write(&original, vec![0u8; 8192]).unwrap();
    fs::hard_link(&original, &link).expect("Failed to create hard link");

    let args = no_cache_args(root);
    let exclude_matcher = build_exclude_matcher(&expand_exclude_patterns(&[])).unwrap();
    let scan = scan_files_and_dirs(root, &args, &exclude_matcher, args.sort).unwrap();

    let size_of = |p: &std::path::Path| {
        scan.entries
            .iter()
            .find(|e| e.path == p)
            .map(|e| e.size)
            .unwrap_or_else(|| panic!("entry {:?} missing", p))
    };

    // Both link names are listed, each showing the inode's size
    let a_size = size_of(&original);
    let b_size = size_of(&link);
    assert!(a_size > 0);
    assert_eq!(a_size, b_size);

    // ...but the directory total counts the inode once, not twice.
    // Totals also include each directory's own blocks (du parity), so
    // compare against file size + the directories' own disk usage.
    let dir1_own = rudu::utils::get_dir_metadata(&dir1)
        .expect("dir1 metadata")
        .size;
    let root_own = rudu::utils::get_dir_metadata(root)
        .expect("root metadata")
        .size;
    assert_eq!(
        size_of(&dir1),
        a_size + dir1_own,
        "hard-linked inode must be counted once in the directory total"
    );
    assert_eq!(
        size_of(root),
        a_size + dir1_own + root_own,
        "hard-linked inode must be counted once in the root total"
    );
}

#[test]
fn test_symlinks_are_leaf_entries_and_not_followed() {
    // Regression test for CODE_REVIEW.md H2: symlinks were classified as
    // directories, and their metadata was taken from the *target* via stat().
    // They must be leaf (File) entries with their own (lstat) size, and a link
    // pointing at a big external directory must not inflate totals.
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    // External target: a directory holding a 64 KB file, outside the scan root
    let target_dir = TempDir::new().expect("Failed to create target temp dir");
    fs::write(target_dir.path().join("big.bin"), vec![0u8; 65536]).unwrap();

    fs::write(root.join("top.txt"), vec![0u8; 4096]).unwrap();
    let link = root.join("link_to_external");
    std::os::unix::fs::symlink(target_dir.path(), &link).expect("Failed to create symlink");

    let args = no_cache_args(root);
    let exclude_matcher = build_exclude_matcher(&expand_exclude_patterns(&[])).unwrap();
    let scan = scan_files_and_dirs(root, &args, &exclude_matcher, args.sort).unwrap();

    let link_entry = scan
        .entries
        .iter()
        .find(|e| e.path == link)
        .expect("symlink entry missing");

    // Classified as a leaf entry, not a directory
    assert_eq!(
        link_entry.entry_type,
        EntryType::File,
        "symlink must not be classified as a directory"
    );

    // The link's own size is tiny — never the 64 KB target
    assert!(
        link_entry.size < 65536,
        "symlink size must be its own, not its target's (got {})",
        link_entry.size
    );

    // Root total: top.txt + the link's own blocks, far below the target's 64 KB
    let root_size = scan
        .entries
        .iter()
        .find(|e| e.path == root)
        .map(|e| e.size)
        .expect("root entry missing");
    assert!(
        root_size < 65536,
        "root total must not include the symlink target's contents (got {})",
        root_size
    );
}

#[test]
fn test_incremental_scan_second_run_uses_cache() {
    // Running the scan twice on the same unchanged directory should produce
    // a non-zero cache_total on the second run (entries were cached after the first).
    let _env_lock = CACHE_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    let subdir = root.join("cached_dir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("data.txt"), vec![1u8; 4096]).unwrap();

    // Use a dedicated cache dir so the test is isolated
    let cache_dir = TempDir::new().expect("Failed to create cache temp dir");
    // SAFETY: RUDU_CACHE_DIR access is serialised by CACHE_ENV_LOCK;
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
        show_atime: false,
        purge_days: 100,
        older_than: None,
        min_size: None,
    };

    let exclude_patterns = expand_exclude_patterns(&[]);
    let exclude_matcher = build_exclude_matcher(&exclude_patterns).unwrap();

    // First scan — populates the cache
    let first =
        scan_files_and_dirs_incremental(root, &make_args(), &exclude_matcher, SortKey::Name)
            .expect("first scan should succeed");

    // Second scan — should see cache entries
    let second =
        scan_files_and_dirs_incremental(root, &make_args(), &exclude_matcher, SortKey::Name)
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

#[test]
fn test_incremental_scan_cache_correctness() {
    // Regression test for CODE_REVIEW.md C1-C3:
    // - C2: a cached second run must return the same entries as the first,
    //   including files (they previously vanished on cache-hit subtrees).
    // - C3: ancestor totals (root, intermediate dirs) must include the sizes
    //   of cache-hit subtrees (they were previously omitted).
    // - C1: a file added deep in the tree between runs must be detected even
    //   though only its immediate parent's mtime changes (shallow validation
    //   previously missed it, and a root-level hit skipped the scan entirely).
    let _env_lock = CACHE_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    // root/
    // ├── outer/
    // │   └── inner/
    // │       └── a.txt (8 KB)
    // └── top.txt (4 KB)
    let outer = root.join("outer");
    let inner = outer.join("inner");
    fs::create_dir_all(&inner).unwrap();
    fs::write(inner.join("a.txt"), vec![0u8; 8192]).unwrap();
    fs::write(root.join("top.txt"), vec![0u8; 4096]).unwrap();

    let cache_dir = TempDir::new().expect("Failed to create cache temp dir");
    // SAFETY: RUDU_CACHE_DIR access is serialised by CACHE_ENV_LOCK;
    // the variable is restored before the test returns.
    unsafe { std::env::set_var("RUDU_CACHE_DIR", cache_dir.path()) };

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
        no_cache: false, // caching enabled — that's what we're testing
        cache_ttl: 604800,
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
        show_atime: false,
        purge_days: 100,
        older_than: None,
        min_size: None,
    };

    let exclude_matcher = build_exclude_matcher(&expand_exclude_patterns(&[])).unwrap();

    let first = scan_files_and_dirs_incremental(root, &args, &exclude_matcher, SortKey::Name)
        .expect("first scan should succeed");
    let second = scan_files_and_dirs_incremental(root, &args, &exclude_matcher, SortKey::Name)
        .expect("second scan should succeed");

    let sorted_paths = |scan: &rudu::scan::ScanResult| {
        let mut v: Vec<_> = scan.entries.iter().map(|e| e.path.clone()).collect();
        v.sort();
        v
    };

    // C2: identical entry sets across uncached and cached runs (files included)
    assert_eq!(
        sorted_paths(&first),
        sorted_paths(&second),
        "cached run must return the same entries (incl. files) as the first run"
    );
    assert!(
        second
            .entries
            .iter()
            .any(|e| e.path == inner.join("a.txt") && e.entry_type == EntryType::File),
        "file inside a cache-hit subtree must still be listed"
    );

    // C3: sizes of every directory must match between runs — in particular the
    // root and `outer`, whose totals come from the cache-hit subtree
    let size_of = |scan: &rudu::scan::ScanResult, p: &std::path::Path| {
        scan.entries
            .iter()
            .find(|e| e.path == p)
            .map(|e| e.size)
            .unwrap_or_else(|| panic!("entry {:?} missing", p))
    };
    assert_eq!(size_of(&first, root), size_of(&second, root));
    assert_eq!(size_of(&first, &outer), size_of(&second, &outer));
    assert_eq!(size_of(&first, &inner), size_of(&second, &inner));
    assert!(size_of(&second, root) >= size_of(&second, &outer));

    // C1: add a file deep inside — only `inner`'s mtime changes; `outer` and
    // the root are untouched. Deep validation must reject the stale hit.
    fs::write(inner.join("b.txt"), vec![1u8; 8192]).unwrap();
    let third = scan_files_and_dirs_incremental(root, &args, &exclude_matcher, SortKey::Name)
        .expect("third scan should succeed");

    // SAFETY: restoring the env var we set above.
    unsafe { std::env::remove_var("RUDU_CACHE_DIR") };

    assert!(
        third.entries.iter().any(|e| e.path == inner.join("b.txt")),
        "file added deep inside a cached subtree must be detected"
    );
    assert!(
        size_of(&third, &outer) > size_of(&second, &outer),
        "outer's total must grow after a file is added to inner"
    );
    assert!(
        size_of(&third, root) > size_of(&second, root),
        "root total must grow after a file is added deep in the tree"
    );
}

// ---------------------------------------------------------------------------
// Access-age reporting (--show-atime / --older-than / --purge-days)
// ---------------------------------------------------------------------------

/// Backdates a path's access time by `days`, preserving its mtime.
///
/// mtime must survive untouched or the change would invalidate the incremental
/// cache, which is exactly what the cache-hit test below needs to keep intact.
fn set_atime_days_ago(path: &std::path::Path, days: u64) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let meta = get_dir_metadata(path).expect("stat failed");
    let now = rudu::atime::now_unix() as i64;
    let target = now - (days as i64) * 86_400;

    // get_dir_metadata reports mtime in nanoseconds; utimes takes microseconds.
    let mtime_secs = (meta.mtime / 1_000_000_000) as i64;
    let mtime_usec = ((meta.mtime % 1_000_000_000) / 1_000) as i64;

    let times = [
        libc::timeval {
            tv_sec: target,
            tv_usec: 0,
        },
        libc::timeval {
            tv_sec: mtime_secs,
            tv_usec: mtime_usec as libc::suseconds_t,
        },
    ];
    let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
    let rc = unsafe { libc::utimes(c_path.as_ptr(), times.as_ptr()) };
    assert_eq!(rc, 0, "utimes failed for {}", path.display());

    // Read it straight back: a filesystem mounted `noatime` silently ignores
    // the update, and a test asserting on access ages would then be asserting
    // on nothing.
    let after = get_dir_metadata(path).expect("stat failed after utimes");
    assert!(
        after.atime.abs_diff(target as u64) < 60,
        "filesystem did not honour the atime update for {}          (mounted noatime?): wanted {}, got {}",
        path.display(),
        target,
        after.atime
    );
}

/// Builds a tree with one cold subtree, one hot subtree, and one of each.
fn make_aged_tree(root: &std::path::Path) {
    fs::create_dir(root.join("cold")).unwrap();
    fs::create_dir(root.join("hot")).unwrap();

    fs::write(root.join("cold/ancient.dat"), vec![0u8; 8192]).unwrap();
    fs::write(root.join("cold/old.dat"), vec![0u8; 4096]).unwrap();
    fs::write(root.join("hot/fresh.dat"), vec![0u8; 4096]).unwrap();

    set_atime_days_ago(&root.join("cold/ancient.dat"), 250);
    set_atime_days_ago(&root.join("cold/old.dat"), 150);
    set_atime_days_ago(&root.join("hot/fresh.dat"), 2);
}

fn atime_args(root: &std::path::Path) -> Args {
    let mut args = no_cache_args(root);
    args.show_atime = true;
    args
}

#[test]
fn test_directory_reports_oldest_leaf_not_its_own_atime() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    make_aged_tree(root);

    let args = atime_args(root);
    let matcher = build_exclude_matcher(&[]).unwrap();
    let mut entries = scan_files_and_dirs(root, &args, &matcher, args.sort)
        .unwrap()
        .entries;

    let now = rudu::atime::now_unix();
    rudu::atime::apply_rollup(&mut entries, root, 100, now);

    let find = |suffix: &str| {
        entries
            .iter()
            .find(|e| e.path.ends_with(suffix))
            .unwrap_or_else(|| panic!("{} missing from scan", suffix))
    };

    // The scan just read every directory, so `cold`'s own atime is ~now. It must
    // report its oldest leaf (250d) instead.
    let cold_age = rudu::atime::age_days(find("cold").atime.unwrap(), now);
    assert!(
        (249..=251).contains(&cold_age),
        "cold should report its 250-day-old leaf, got {cold_age}d"
    );

    let hot_age = rudu::atime::age_days(find("hot").atime.unwrap(), now);
    assert!(hot_age <= 3, "hot should report ~2d, got {hot_age}d");

    // Files keep their own access time.
    let file_age = rudu::atime::age_days(find("cold/old.dat").atime.unwrap(), now);
    assert!(
        (149..=151).contains(&file_age),
        "old.dat should report ~150d, got {file_age}d"
    );

    // And the rollup reaches the root.
    let root_entry = entries.iter().find(|e| e.path == root).unwrap();
    let root_age = rudu::atime::age_days(root_entry.atime.unwrap(), now);
    assert!(
        (249..=251).contains(&root_age),
        "root should report the oldest leaf in the whole tree, got {root_age}d"
    );
}

#[test]
fn test_at_risk_bytes_count_only_leaves_past_the_threshold() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    make_aged_tree(root);

    let args = atime_args(root);
    let matcher = build_exclude_matcher(&[]).unwrap();
    let mut entries = scan_files_and_dirs(root, &args, &matcher, args.sort)
        .unwrap()
        .entries;

    let now = rudu::atime::now_unix();
    rudu::atime::apply_rollup(&mut entries, root, 100, now);

    let cold = entries.iter().find(|e| e.path.ends_with("cold")).unwrap();
    let hot = entries.iter().find(|e| e.path.ends_with("hot")).unwrap();

    // Both files under cold are past 100 days; nothing under hot is.
    assert!(
        cold.at_risk_bytes.unwrap() >= 12_288,
        "both cold files should be at risk, got {:?}",
        cold.at_risk_bytes
    );
    assert_eq!(hot.at_risk_bytes, Some(0));

    // Raising the threshold past the older file must shrink the at-risk total,
    // proving the rollup is derived per run rather than baked in.
    let mut entries2 = scan_files_and_dirs(root, &args, &matcher, args.sort)
        .unwrap()
        .entries;
    rudu::atime::apply_rollup(&mut entries2, root, 200, now);
    let cold2 = entries2.iter().find(|e| e.path.ends_with("cold")).unwrap();
    assert!(
        cold2.at_risk_bytes.unwrap() < cold.at_risk_bytes.unwrap(),
        "a 200-day threshold should exclude the 150-day file"
    );
}

#[test]
fn test_atime_survives_a_cache_hit() {
    // A cache-hit subtree is never walked, so its files' access times can only
    // come from the cache. Without cached atimes the second run would report
    // nothing for exactly the data the report exists to account for.
    let _env_lock = CACHE_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    make_aged_tree(root);

    let cache_dir = TempDir::new().unwrap();
    // SAFETY: RUDU_CACHE_DIR access is serialised by CACHE_ENV_LOCK;
    // the variable is removed before the test returns.
    unsafe { std::env::set_var("RUDU_CACHE_DIR", cache_dir.path()) };

    let mut args = atime_args(root);
    args.no_cache = false;
    let matcher = build_exclude_matcher(&[]).unwrap();
    let now = rudu::atime::now_unix();

    let run = |args: &Args| {
        let mut entries = scan_files_and_dirs_incremental(root, args, &matcher, args.sort)
            .unwrap()
            .entries;
        rudu::atime::apply_rollup(&mut entries, root, 100, now);
        entries
    };

    let first = run(&args);
    let second = run(&args);

    let age_of = |entries: &[rudu::data::FileEntry], suffix: &str| {
        entries
            .iter()
            .find(|e| e.path.ends_with(suffix))
            .and_then(|e| e.atime)
            .map(|at| rudu::atime::age_days(at, now))
    };

    for suffix in ["cold/ancient.dat", "cold/old.dat", "cold", "hot"] {
        assert_eq!(
            age_of(&first, suffix),
            age_of(&second, suffix),
            "access age for {suffix} changed across a cached rerun"
        );
        assert!(
            age_of(&second, suffix).is_some(),
            "{suffix} lost its access time on the cached rerun"
        );
    }

    unsafe { std::env::remove_var("RUDU_CACHE_DIR") };
}

#[test]
fn test_older_than_filter_end_to_end() {
    // --older-than lives in the binary's display pipeline, so drive the CLI.
    //
    // This deliberately uses threshold extremes rather than backdated atimes:
    // some platforms (macOS Spotlight indexing its temp directories) touch
    // files between the backdate and the spawn, which would make an
    // age-dependent assertion flaky here while passing on Linux CI. The age
    // arithmetic itself is covered deterministically in `atime`'s unit tests
    // and by the in-process rollup tests above.
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    make_aged_tree(root);

    let cache_dir = TempDir::new().unwrap();
    let run = |threshold: &str| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rudu"))
            .arg(root)
            .args(["--older-than", threshold, "--no-cache"])
            .env("RUDU_CACHE_DIR", cache_dir.path())
            .output()
            .expect("failed to run rudu");
        assert!(
            output.status.success(),
            "rudu exited with {}",
            output.status
        );
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    // Everything is at least 0 days old, so nothing is filtered out.
    let (kept, stderr) = run("0");
    for name in ["ancient.dat", "old.dat", "fresh.dat"] {
        assert!(
            kept.contains(name),
            "--older-than 0 must keep {name}:\n{kept}"
        );
    }

    // --older-than implies --show-atime, so the columns appear without it.
    assert!(
        kept.contains("LAST ACCESS") && kept.contains("AT RISK"),
        "--older-than should imply --show-atime:\n{kept}"
    );

    // Nothing is 100000 days old, so every row is filtered out...
    let (dropped, _) = run("100000");
    for name in ["ancient.dat", "old.dat", "fresh.dat"] {
        assert!(
            !dropped.contains(name),
            "--older-than 100000 must drop {name}:\n{dropped}"
        );
    }

    // ...but the summary still accounts for the whole scan, because it is
    // computed before the display filter rather than from the visible rows.
    assert!(
        stderr.contains("Access age summary"),
        "summary missing from stderr:\n{stderr}"
    );
    let (_, stderr_filtered) = run("100000");
    // Sum across bands rather than expecting a particular band to hold all
    // three: which band each file lands in depends on its real access age,
    // which is exactly what this test must not depend on. The total is what
    // proves the summary ignored the display filter.
    let counted: u64 = stderr_filtered
        .lines()
        .filter_map(|line| {
            let (before, _) = line.split_once(" files")?;
            before.split_whitespace().next_back()?.parse::<u64>().ok()
        })
        .sum();
    assert_eq!(
        counted, 3,
        "summary must account for all 3 files even though every row was \
         filtered out of the table:\n{stderr_filtered}"
    );
}

// ---------------------------------------------------------------------------
// Size filtering (--min-size)
// ---------------------------------------------------------------------------

/// Runs the real binary and returns `(path, size_bytes)` for every CSV row.
///
/// CSV rather than the terminal table so the sizes are exact bytes instead of
/// rounded human-readable strings.
fn run_rudu_csv(
    root: &std::path::Path,
    cache_dir: &std::path::Path,
    out_path: &std::path::Path,
    extra: &[&str],
) -> Vec<(String, u64)> {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rudu"))
        .arg(root)
        .args(["--no-cache", "--output"])
        .arg(out_path)
        .args(extra)
        .env("RUDU_CACHE_DIR", cache_dir)
        .output()
        .expect("failed to run rudu");
    assert!(
        output.status.success(),
        "rudu exited with {}",
        output.status
    );

    let csv = fs::read_to_string(out_path).expect("no CSV written");
    csv.lines()
        .skip(1) // header
        .filter_map(|line| {
            // entry_type,size_bytes,size_human,owner,path,inodes,...
            let fields: Vec<&str> = line.split(',').collect();
            Some((fields.get(4)?.to_string(), fields.get(1)?.parse().ok()?))
        })
        .collect()
}

#[test]
fn test_min_size_hides_small_entries_without_changing_totals() {
    // The point of --min-size is to hide noise from the *display* while the
    // numbers stay honest. If a filtered-out file stopped counting toward its
    // parent, rudu would report directory sizes that disagree with `du`.
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    fs::write(root.join("big.dat"), vec![0u8; 200 * 1024]).unwrap();
    fs::create_dir(root.join("noise")).unwrap();
    for i in 0..5 {
        fs::write(root.join(format!("noise/tiny{i}.dat")), vec![0u8; 1024]).unwrap();
    }

    let cache_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let unfiltered = run_rudu_csv(root, cache_dir.path(), &out_dir.path().join("all.csv"), &[]);
    let filtered = run_rudu_csv(
        root,
        cache_dir.path(),
        &out_dir.path().join("big.csv"),
        &["--min-size", "100KB"],
    );

    let named = |rows: &[(String, u64)], name: &str| -> Option<u64> {
        rows.iter()
            .find(|(p, _)| p.ends_with(name))
            .map(|(_, s)| *s)
    };

    // Everything shows up when unfiltered.
    assert!(named(&unfiltered, "big.dat").is_some());
    assert!(named(&unfiltered, "tiny0.dat").is_some());
    assert!(named(&unfiltered, "noise").is_some());

    // The threshold hides the small file and the small directory...
    assert!(
        named(&filtered, "big.dat").is_some(),
        "a 200 KB file must survive --min-size 100KB"
    );
    for i in 0..5 {
        assert!(
            named(&filtered, &format!("tiny{i}.dat")).is_none(),
            "a 1 KB file must be hidden by --min-size 100KB"
        );
    }
    assert!(
        named(&filtered, "noise").is_none(),
        "a directory totalling well under the threshold must be hidden too"
    );

    // ...and the root's reported total is byte-identical, proving the hidden
    // entries still counted toward it.
    let root_str = root.display().to_string();
    let root_before = unfiltered
        .iter()
        .find(|(p, _)| *p == root_str)
        .expect("root row missing")
        .1;
    let root_after = filtered
        .iter()
        .find(|(p, _)| *p == root_str)
        .expect("root row missing when filtered")
        .1;
    assert_eq!(
        root_before, root_after,
        "--min-size must not change any total; it is a display filter"
    );
    assert!(
        root_before > 200 * 1024,
        "root total should include the hidden files"
    );
}

#[test]
fn test_min_size_rejects_an_unparseable_threshold() {
    // Better to fail loudly than to fall back to 0 and appear to ignore the flag.
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = TempDir::new().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rudu"))
        .arg(temp_dir.path())
        .args(["--no-cache", "--min-size", "10 flurbs"])
        .env("RUDU_CACHE_DIR", cache_dir.path())
        .output()
        .expect("failed to run rudu");

    assert!(!output.status.success(), "an invalid --min-size must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("flurbs"),
        "the error should quote the bad input:\n{stderr}"
    );
}
