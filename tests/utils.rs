use rudu::cli::SortKey;
use rudu::data::{EntryType, FileEntry};
use rudu::utils::{
    build_exclude_matcher, disk_usage, expand_exclude_patterns, get_dir_metadata, path_depth,
    path_hash, sort_entries,
};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_path_depth() {
    let root = PathBuf::from("/home/user");
    let path1 = PathBuf::from("/home/user/documents");
    let path2 = PathBuf::from("/home/user/documents/work/project");

    assert_eq!(path_depth(&root, &path1), 1);
    assert_eq!(path_depth(&root, &path2), 3);
    assert_eq!(path_depth(&root, &root), 0);
}


#[test]
fn test_sort_entries() {
    let entries = vec![
        FileEntry {
            path: PathBuf::from("/home/user/b.txt"),
            size: 1024,
            owner: Some("user".to_string()),
            inodes: None,
            entry_type: EntryType::File,
        },
        FileEntry {
            path: PathBuf::from("/home/user/a.txt"),
            size: 2048,
            owner: Some("user".to_string()),
            inodes: None,
            entry_type: EntryType::File,
        },
        FileEntry {
            path: PathBuf::from("/home/user/c.txt"),
            size: 512,
            owner: Some("user".to_string()),
            inodes: None,
            entry_type: EntryType::File,
        },
    ];

    // Test sorting by name
    let mut name_sorted = entries.clone();
    sort_entries(&mut name_sorted, SortKey::Name);
    assert_eq!(name_sorted[0].path, PathBuf::from("/home/user/a.txt"));
    assert_eq!(name_sorted[1].path, PathBuf::from("/home/user/b.txt"));
    assert_eq!(name_sorted[2].path, PathBuf::from("/home/user/c.txt"));

    // Test sorting by size (largest first)
    let mut size_sorted = entries.clone();
    sort_entries(&mut size_sorted, SortKey::Size);
    assert_eq!(size_sorted[0].size, 2048);
    assert_eq!(size_sorted[1].size, 1024);
    assert_eq!(size_sorted[2].size, 512);
}

#[test]
fn test_expand_exclude_patterns() {
    let patterns = vec![
        "node_modules".to_string(),
        "*.log".to_string(),
        "temp/".to_string(),
    ];

    let expanded = expand_exclude_patterns(&patterns);

    // Should expand "node_modules" to multiple patterns
    assert!(expanded.contains(&"**/node_modules".to_string()));
    assert!(expanded.contains(&"**/node_modules/**".to_string()));

    // Should keep "*.log" as-is (contains glob)
    assert!(expanded.contains(&"*.log".to_string()));

    // Should keep "temp/" as-is (ends with slash)
    assert!(expanded.contains(&"temp/".to_string()));
}

#[test]
fn test_build_exclude_matcher() {
    let patterns = vec!["*.log".to_string(), "**/node_modules/**".to_string()];

    let matcher = build_exclude_matcher(&patterns);
    assert!(matcher.is_ok());

    let matcher = matcher.unwrap();
    assert!(matcher.is_match("debug.log"));
    assert!(matcher.is_match("project/node_modules/package.json"));
    assert!(!matcher.is_match("src/main.rs"));
}

#[test]
fn test_build_exclude_matcher_invalid_pattern() {
    let patterns = vec![
        "[invalid".to_string(), // Invalid glob pattern
    ];

    let matcher = build_exclude_matcher(&patterns);
    assert!(matcher.is_err());
}

// ── disk_usage ────────────────────────────────────────────────────────────────

#[test]
fn test_disk_usage_nonzero_for_real_file() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("sample.txt");
    // Write enough data that the OS allocates at least one block
    std::fs::write(&file_path, "x".repeat(4096)).unwrap();
    let usage = disk_usage(&file_path);
    assert!(usage > 0, "disk_usage should be > 0 for a non-empty file, got {usage}");
}

#[test]
fn test_disk_usage_zero_for_missing_path() {
    let usage = disk_usage(std::path::Path::new("/nonexistent/path/that/cannot/exist"));
    assert_eq!(usage, 0, "disk_usage should return 0 for a missing path");
}

// ── path_hash ─────────────────────────────────────────────────────────────────

#[test]
fn test_path_hash_is_deterministic() {
    let path = std::path::Path::new("/home/user/documents/report.txt");
    assert_eq!(
        path_hash(path),
        path_hash(path),
        "path_hash must return the same value for the same path"
    );
}

#[test]
fn test_path_hash_differs_for_different_paths() {
    let a = std::path::Path::new("/home/user/a.txt");
    let b = std::path::Path::new("/home/user/b.txt");
    assert_ne!(
        path_hash(a),
        path_hash(b),
        "path_hash should differ for distinct paths"
    );
}

// ── get_dir_metadata ──────────────────────────────────────────────────────────

#[test]
fn test_get_dir_metadata_returns_some_for_real_dir() {
    let tmp = TempDir::new().unwrap();
    let meta = get_dir_metadata(tmp.path());
    assert!(meta.is_some(), "get_dir_metadata should return Some for a real directory");
    let meta = meta.unwrap();
    // nlink must be at least 2 (the dir itself + ".")
    assert!(meta.nlink >= 2, "nlink should be >= 2, got {}", meta.nlink);
    // mtime must be a plausible Unix timestamp (> year 2000)
    assert!(meta.mtime > 946_684_800, "mtime looks wrong: {}", meta.mtime);
    // owner UID should be present
    assert!(meta.owner.is_some(), "owner UID should be present for a tempdir");
}

#[test]
fn test_get_dir_metadata_returns_none_for_missing_path() {
    let meta = get_dir_metadata(std::path::Path::new("/no/such/directory/ever"));
    assert!(meta.is_none(), "get_dir_metadata should return None for a missing path");
}

// ── sort_entries edge cases ───────────────────────────────────────────────────

#[test]
fn test_sort_entries_size_ties_are_stable_by_relative_order() {
    // Two entries with identical sizes — relative order should not swap under
    // a stable sort; we use `sort_by` so Rust guarantees stability.
    let mut entries = vec![
        FileEntry {
            path: PathBuf::from("/first"),
            size: 512,
            owner: None,
            inodes: None,
            entry_type: EntryType::File,
        },
        FileEntry {
            path: PathBuf::from("/second"),
            size: 512,
            owner: None,
            inodes: None,
            entry_type: EntryType::File,
        },
    ];
    sort_entries(&mut entries, SortKey::Size);
    // Both have the same size; stability means /first stays before /second
    assert_eq!(entries[0].path, PathBuf::from("/first"));
    assert_eq!(entries[1].path, PathBuf::from("/second"));
}

#[test]
fn test_sort_entries_empty_slice_does_not_panic() {
    let mut entries: Vec<FileEntry> = vec![];
    sort_entries(&mut entries, SortKey::Name);
    sort_entries(&mut entries, SortKey::Size);
    // No assertions needed — reaching here without panic is the goal
}

#[test]
fn test_sort_entries_single_entry_unchanged() {
    let mut entries = vec![FileEntry {
        path: PathBuf::from("/only"),
        size: 1024,
        owner: None,
        inodes: None,
        entry_type: EntryType::Dir,
    }];
    sort_entries(&mut entries, SortKey::Size);
    assert_eq!(entries[0].path, PathBuf::from("/only"));
}
