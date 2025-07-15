use rudu::cli::{Args, SortKey};
use rudu::data::{EntryType, FileEntry};
use rudu::output::{csv, terminal};
use rudu::thread_pool::ThreadPoolStrategy;
use std::path::PathBuf;

#[test]
fn test_csv_renderer_works() {
    let test_entries = vec![
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
    ];

    let args = Args {
        path: PathBuf::from("/test"),
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
    };

    // Test that CSV rendering doesn't panic or error
    let result = csv::render(&test_entries, &args);
    assert!(result.is_ok());
}

#[test]
fn test_terminal_renderer_works() {
    let test_entries = vec![
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
    ];

    let args = Args {
        path: PathBuf::from("/test"),
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
    };

    // Test that terminal rendering doesn't panic or error
    let result = terminal::render(&test_entries, &args);
    assert!(result.is_ok());
}
