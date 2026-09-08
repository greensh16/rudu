use rudu::cli::SortKey;
use rudu::data::{EntryType, FileEntry};
use rudu::utils::{
    build_exclude_matcher, disk_usage, expand_exclude_patterns, get_dir_metadata, parse_size,
    path_depth, path_hash, sort_entries,
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
            atime: None,
            at_risk_bytes: None,
        },
        FileEntry {
            path: PathBuf::from("/home/user/a.txt"),
            size: 2048,
            owner: Some("user".to_string()),
            inodes: None,
            entry_type: EntryType::File,
            atime: None,
            at_risk_bytes: None,
        },
        FileEntry {
            path: PathBuf::from("/home/user/c.txt"),
            size: 512,
            owner: Some("user".to_string()),
            inodes: None,
            entry_type: EntryType::File,
            atime: None,
            at_risk_bytes: None,
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

    // Bare names expand to match at any depth
    assert!(expanded.contains(&"**/node_modules".to_string()));
    assert!(expanded.contains(&"**/node_modules/**".to_string()));

    // Bare globs expand too — a plain "*.log" would not match nested paths
    // because globset's `*` does not cross `/`
    assert!(expanded.contains(&"**/*.log".to_string()));
    assert!(expanded.contains(&"**/*.log/**".to_string()));

    // Trailing slash is stripped, then expanded like a bare name
    assert!(expanded.contains(&"**/temp".to_string()));
    assert!(expanded.contains(&"**/temp/**".to_string()));
}

#[test]
fn test_exclude_glob_matches_nested_paths() {
    // Regression test for CODE_REVIEW.md M2: `--exclude '*.log'` must exclude
    // log files at any depth, and path-anchored patterns pass through as-is.
    let expanded = expand_exclude_patterns(&["*.log".to_string()]);
    let matcher = build_exclude_matcher(&expanded).unwrap();

    assert!(matcher.is_match("x.log"));
    assert!(matcher.is_match("deep/nested/dir/x.log"));
    assert!(!matcher.is_match("deep/nested/x.txt"));

    // Patterns containing '/' are used as given
    let expanded = expand_exclude_patterns(&["build/output".to_string()]);
    assert_eq!(expanded, vec!["build/output".to_string()]);
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
    assert!(
        usage > 0,
        "disk_usage should be > 0 for a non-empty file, got {usage}"
    );
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
    assert!(
        meta.is_some(),
        "get_dir_metadata should return Some for a real directory"
    );
    let meta = meta.unwrap();
    // nlink must be at least 2 (the dir itself + ".")
    assert!(meta.nlink >= 2, "nlink should be >= 2, got {}", meta.nlink);
    // mtime is nanoseconds since the epoch — must be past year 2000
    assert!(
        meta.mtime > 946_684_800_000_000_000,
        "mtime looks wrong: {}",
        meta.mtime
    );
    // owner UID should be present
    assert!(
        meta.owner.is_some(),
        "owner UID should be present for a tempdir"
    );
}

#[test]
fn test_get_dir_metadata_returns_none_for_missing_path() {
    let meta = get_dir_metadata(std::path::Path::new("/no/such/directory/ever"));
    assert!(
        meta.is_none(),
        "get_dir_metadata should return None for a missing path"
    );
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
            atime: None,
            at_risk_bytes: None,
        },
        FileEntry {
            path: PathBuf::from("/second"),
            size: 512,
            owner: None,
            inodes: None,
            entry_type: EntryType::File,
            atime: None,
            at_risk_bytes: None,
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
        atime: None,
        at_risk_bytes: None,
    }];
    sort_entries(&mut entries, SortKey::Size);
    assert_eq!(entries[0].path, PathBuf::from("/only"));
}

// ---------------------------------------------------------------------------
// Size parsing (--min-size)
// ---------------------------------------------------------------------------

#[test]
fn test_parse_size_bare_numbers_are_bytes() {
    assert_eq!(parse_size("0").unwrap(), 0);
    assert_eq!(parse_size("4096").unwrap(), 4096);
    assert_eq!(parse_size("100B").unwrap(), 100);
    // Underscores are allowed as digit separators.
    assert_eq!(parse_size("1_048_576").unwrap(), 1_048_576);
}

#[test]
fn test_parse_size_decimal_units_match_what_rudu_prints() {
    // rudu renders sizes with humansize's DECIMAL formatter, so KB/MB/GB here
    // must be powers of 1000 or `--min-size 819kB` would not match a row the
    // table just displayed as "819.20 kB".
    assert_eq!(parse_size("1KB").unwrap(), 1_000);
    assert_eq!(parse_size("10MB").unwrap(), 10_000_000);
    assert_eq!(parse_size("2GB").unwrap(), 2_000_000_000);
    assert_eq!(parse_size("1TB").unwrap(), 1_000_000_000_000);
    assert_eq!(parse_size("1PB").unwrap(), 1_000_000_000_000_000);
    // Bare single letters are decimal too, for consistency.
    assert_eq!(parse_size("5K").unwrap(), 5_000);
    assert_eq!(parse_size("5M").unwrap(), 5_000_000);
}

#[test]
fn test_parse_size_binary_units() {
    assert_eq!(parse_size("1KiB").unwrap(), 1024);
    assert_eq!(parse_size("1MiB").unwrap(), 1_048_576);
    assert_eq!(parse_size("1GiB").unwrap(), 1_073_741_824);
    assert_eq!(parse_size("1TiB").unwrap(), 1_099_511_627_776);
}

#[test]
fn test_parse_size_is_case_insensitive_and_trims() {
    for input in ["10mb", "10MB", "10Mb", " 10MB ", "10 MB"] {
        assert_eq!(
            parse_size(input).unwrap(),
            10_000_000,
            "failed to parse {input:?}"
        );
    }
    assert_eq!(parse_size("1gib").unwrap(), 1_073_741_824);
}

#[test]
fn test_parse_size_accepts_fractions() {
    // Users copy values straight out of rudu's own output, which is fractional.
    assert_eq!(parse_size("1.5GB").unwrap(), 1_500_000_000);
    assert_eq!(parse_size("819.20kB").unwrap(), 819_200);
    assert_eq!(parse_size("0.5MiB").unwrap(), 524_288);
}

#[test]
fn test_parse_size_rejects_bad_input() {
    // Each of these must fail rather than silently becoming 0, which would
    // filter nothing and look like the flag was ignored.
    for bad in [
        "",
        "   ",
        "big",
        "MB",
        "10 flurbs",
        "10XB",
        "1.2.3MB",
        "--5MB",
    ] {
        assert!(
            parse_size(bad).is_err(),
            "{bad:?} should have been rejected"
        );
    }
}

#[test]
fn test_parse_size_rejects_negative_with_a_clear_message() {
    let err = parse_size("-5MB").unwrap_err();
    assert!(
        err.contains("non-negative"),
        "expected a message about negativity, got: {err}"
    );
}

#[test]
fn test_parse_size_rejects_overflow() {
    assert!(parse_size("99999999PB").is_err());
}
