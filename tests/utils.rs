use rudu::cli::SortKey;
use rudu::data::{EntryType, FileEntry};
use rudu::utils::{
    build_exclude_matcher, expand_exclude_patterns, filter_by_depth, path_depth, sort_entries,
};
use std::path::PathBuf;

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
fn test_filter_by_depth() {
    let root = PathBuf::from("/home/user");
    let entries = vec![
        FileEntry {
            path: PathBuf::from("/home/user/documents"),
            size: 1024,
            owner: Some("user".to_string()),
            inodes: Some(5),
            entry_type: EntryType::Dir,
        },
        FileEntry {
            path: PathBuf::from("/home/user/documents/work"),
            size: 2048,
            owner: Some("user".to_string()),
            inodes: Some(3),
            entry_type: EntryType::Dir,
        },
        FileEntry {
            path: PathBuf::from("/home/user/documents/work/file.txt"),
            size: 512,
            owner: Some("user".to_string()),
            inodes: None,
            entry_type: EntryType::File,
        },
    ];

    // Test depth filtering for directories only
    let filtered = filter_by_depth(&entries, &root, Some(1), false);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].path, PathBuf::from("/home/user/documents"));

    // Test depth filtering including files
    let filtered_with_files = filter_by_depth(&entries, &root, Some(2), true);
    assert_eq!(filtered_with_files.len(), 2);

    // Test no depth limit
    let all_filtered = filter_by_depth(&entries, &root, None, true);
    assert_eq!(all_filtered.len(), 3);
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
