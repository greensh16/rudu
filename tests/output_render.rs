use rudu::cli::{Args, SortKey};
use rudu::data::{EntryType, FileEntry};
use rudu::output::{csv, terminal};
use rudu::thread_pool::ThreadPoolStrategy;
use std::path::PathBuf;
#[test]
fn test_csv_rendering() {
    let entries = vec![FileEntry {
        path: PathBuf::from("/mnt/data/file.txt"),
        size: 1024,
        owner: Some("johndoe".to_string()),
        inodes: Some(1),
        entry_type: EntryType::File,
    }];

    let args = Args {
        path: PathBuf::from("/mnt/data"),
        depth: None,
        sort: SortKey::Name,
        show_files: true,
        exclude: vec![],
        show_owner: true,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: false,
        cache_ttl: 604800,
    };

    let result = csv::render(&entries, &args);
    assert!(result.is_ok());
}

#[test]
fn test_terminal_rendering() {
    let entries = vec![FileEntry {
        path: PathBuf::from("/mnt/data/file.txt"),
        size: 1024,
        owner: Some("johndoe".to_string()),
        inodes: Some(1),
        entry_type: EntryType::File,
    }];

    let args = Args {
        path: PathBuf::from("/mnt/data"),
        depth: Some(2),
        sort: SortKey::Size,
        show_files: true,
        exclude: vec![],
        show_owner: true,
        output: None,
        threads: None,
        show_inodes: true,
        threads_strategy: ThreadPoolStrategy::Default,
        no_cache: false,
        cache_ttl: 604800,
    };

    let result = terminal::render(&entries, &args);
    assert!(result.is_ok());
}
