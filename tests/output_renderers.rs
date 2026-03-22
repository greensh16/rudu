use rudu::cli::{Args, SortKey};
use rudu::data::{EntryType, FileEntry};
use rudu::output::{csv, terminal};
use rudu::thread_pool::ThreadPoolStrategy;
use std::io::Read;
use std::path::PathBuf;
use tempfile::NamedTempFile;

fn make_test_entries() -> Vec<FileEntry> {
    vec![
        FileEntry {
            path: PathBuf::from("/test/dir1"),
            size: 1024,
            owner: Some("testuser".to_string()),
            inodes: Some(5),
            entry_type: EntryType::Dir,
        },
        FileEntry {
            path: PathBuf::from("/test/file1.txt"),
            size: 512,
            owner: Some("testuser".to_string()),
            inodes: None,
            entry_type: EntryType::File,
        },
    ]
}

fn make_args(root: PathBuf) -> Args {
    Args {
        path: root,
        depth: None,
        sort: SortKey::Name,
        show_files: true,
        exclude: Vec::new(),
        show_owner: true,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: false,
        cache_ttl: 604800,
        profile: false,
        memory_limit: None,
        memory_check_interval_ms: 200,
    }
}

#[test]
fn test_csv_renderer_produces_expected_schema() {
    let entries = make_test_entries();
    let tmp = NamedTempFile::new().expect("Failed to create temp file");
    let tmp_path = tmp.path().to_path_buf();

    let mut args = make_args(PathBuf::from("/test"));
    args.output = Some(tmp_path.to_string_lossy().into_owned());

    let result = csv::render(&entries, &args);
    assert!(
        result.is_ok(),
        "csv::render returned an error: {:?}",
        result
    );

    // Read and verify the CSV content
    let mut buf = String::new();
    std::fs::File::open(&tmp_path)
        .unwrap()
        .read_to_string(&mut buf)
        .unwrap();

    // Header row must contain all expected column names
    let header = buf.lines().next().expect("CSV output is empty");
    assert!(header.contains("entry_type"), "missing 'entry_type' column");
    assert!(header.contains("size_bytes"), "missing 'size_bytes' column");
    assert!(header.contains("size_human"), "missing 'size_human' column");
    assert!(header.contains("owner"), "missing 'owner' column");
    assert!(header.contains("path"), "missing 'path' column");
    assert!(header.contains("inodes"), "missing 'inodes' column");

    // Should have header + 2 data rows
    let line_count = buf.lines().count();
    assert_eq!(
        line_count, 3,
        "Expected 1 header + 2 data rows, got {line_count}"
    );

    // Verify data rows contain expected values (EntryType::as_str() returns "DIR" / "FILE")
    assert!(buf.contains("DIR"), "Dir entry should have type 'DIR'");
    assert!(buf.contains("FILE"), "File entry should have type 'FILE'");
    assert!(
        buf.contains("testuser"),
        "Owner 'testuser' should appear in output"
    );
}

#[test]
fn test_csv_renderer_handles_none_owner_and_inodes() {
    // Entries where optional fields are None must not panic and must produce
    // valid CSV rows (empty cells for the missing columns).
    let entries = vec![
        FileEntry {
            path: PathBuf::from("/test/no-owner.txt"),
            size: 256,
            owner: None,
            inodes: None,
            entry_type: EntryType::File,
        },
        FileEntry {
            path: PathBuf::from("/test/dir-no-meta"),
            size: 0,
            owner: None,
            inodes: None,
            entry_type: EntryType::Dir,
        },
    ];

    let tmp = NamedTempFile::new().expect("Failed to create temp file");
    let tmp_path = tmp.path().to_path_buf();

    let mut args = make_args(PathBuf::from("/test"));
    args.output = Some(tmp_path.to_string_lossy().into_owned());

    let result = csv::render(&entries, &args);
    assert!(
        result.is_ok(),
        "csv::render should not error on None fields: {:?}",
        result
    );

    let mut buf = String::new();
    std::fs::File::open(&tmp_path)
        .unwrap()
        .read_to_string(&mut buf)
        .unwrap();

    // Should still produce header + 2 rows
    let line_count = buf.lines().count();
    assert_eq!(
        line_count, 3,
        "Expected 1 header + 2 data rows, got {line_count}"
    );

    // The paths must appear even when owner/inodes are absent
    assert!(buf.contains("no-owner.txt"), "path should appear in output");
    assert!(buf.contains("dir-no-meta"), "path should appear in output");
}

#[test]
fn test_csv_renderer_writes_to_stdout_when_no_output_path() {
    // When args.output is None, csv::render should succeed (writes to stdout).
    // We cannot easily capture stdout in a test, so we just verify no error.
    let entries = make_test_entries();
    let args = make_args(PathBuf::from("/test")); // output: None

    let result = csv::render(&entries, &args);
    assert!(
        result.is_ok(),
        "csv::render with output=None should succeed: {:?}",
        result
    );
}

#[test]
fn test_terminal_renderer_works() {
    let entries = make_test_entries();
    let root = PathBuf::from("/test");
    let args = make_args(root.clone());

    // terminal::render writes to stdout; verify it doesn't error
    let result = terminal::render(&entries, &args, &root);
    assert!(
        result.is_ok(),
        "terminal::render returned an error: {:?}",
        result
    );
}
