//! Unit tests for the cache loader and writer with memory-mapped IO

use super::*;
use crate::cache::model::CacheEntry;
use crate::data::EntryType;
use std::collections::HashMap;
use std::sync::Mutex;

// Static mutex to ensure cache tests run sequentially to avoid race conditions
pub static CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

/// A guard struct to ensure proper test isolation by restoring environment state
struct TestCacheGuard {
    temp_dir: tempfile::TempDir,
    original_rudu_cache_dir: Option<String>,
    original_xdg_cache_home: Option<String>,
    original_cache_enabled: bool,
}

impl TestCacheGuard {
    fn new() -> std::io::Result<Self> {
        let temp_dir = tempfile::tempdir()?;

        // Store original values
        let original_rudu_cache_dir = std::env::var("RUDU_CACHE_DIR").ok();
        let original_xdg_cache_home = std::env::var("XDG_CACHE_HOME").ok();
        let original_cache_enabled = crate::cache::is_enabled();

        // Set test environment
        std::env::set_var("RUDU_CACHE_DIR", temp_dir.path());
        crate::cache::set_enabled(true);

        Ok(TestCacheGuard {
            temp_dir,
            original_rudu_cache_dir,
            original_xdg_cache_home,
            original_cache_enabled,
        })
    }

    fn path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }
}

impl Drop for TestCacheGuard {
    fn drop(&mut self) {
        // Restore original environment variables
        match &self.original_rudu_cache_dir {
            Some(value) => std::env::set_var("RUDU_CACHE_DIR", value),
            None => std::env::remove_var("RUDU_CACHE_DIR"),
        }

        match &self.original_xdg_cache_home {
            Some(value) => std::env::set_var("XDG_CACHE_HOME", value),
            None => std::env::remove_var("XDG_CACHE_HOME"),
        }

        // Restore original cache enabled state
        crate::cache::set_enabled(self.original_cache_enabled);
    }
}

/// Sets up a unique temporary cache directory for each test to prevent cross-test interference.
/// This function creates a temporary directory and sets the RUDU_CACHE_DIR environment variable
/// to point to it, ensuring each test gets its own isolated cache space.
/// It also ensures caching is enabled for the test.
fn setup_temp_cache_dir() -> std::io::Result<TestCacheGuard> {
    TestCacheGuard::new()
}

#[test]
fn test_load_cache_nonexistent() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let nonexistent_path = temp_dir.path().join("nonexistent");

    let result = load_cache(&nonexistent_path, 604800);
    assert!(result.is_empty());
}

#[test]
fn test_save_and_load_cache_empty() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let cache = HashMap::new();

    // Save empty cache
    save_cache(temp_dir.path(), &cache).unwrap();

    // Load it back
    let loaded = load_cache(temp_dir.path(), 604800);
    assert_eq!(loaded.len(), 0);
}

#[test]
fn test_save_and_load_cache_with_entries() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let mut cache = HashMap::new();

    // Create some test entries
    let entry1 = CacheEntry::new(
        12345,
        PathBuf::from("test1.txt"),
        1024,
        1234567890,
        2, // nlink
        Some(42),
        Some(1000),
        EntryType::File,
    );

    let entry2 = CacheEntry::new(
        67890,
        PathBuf::from("test2"),
        2048,
        1234567891,
        3, // nlink
        Some(100),
        Some(1001),
        EntryType::Dir,
    );

    cache.insert(PathBuf::from("test1.txt"), entry1.clone());
    cache.insert(PathBuf::from("test2"), entry2.clone());

    // Save cache
    println!("DEBUG: temp_dir.path() = {:?}", temp_dir.path());
    println!(
        "DEBUG: RUDU_CACHE_DIR = {:?}",
        std::env::var("RUDU_CACHE_DIR")
    );
    save_cache(temp_dir.path(), &cache).unwrap();

    // Load it back
    let loaded = load_cache(temp_dir.path(), 604800);
    assert_eq!(loaded.len(), 2);

    // Verify entries are preserved
    let loaded_entry1 = loaded.get(&PathBuf::from("test1.txt")).unwrap();
    assert_eq!(loaded_entry1.size, 1024);
    assert_eq!(loaded_entry1.mtime, 1234567890);
    assert_eq!(loaded_entry1.entry_type, EntryType::File);

    let loaded_entry2 = loaded.get(&PathBuf::from("test2")).unwrap();
    assert_eq!(loaded_entry2.size, 2048);
    assert_eq!(loaded_entry2.mtime, 1234567891);
    assert_eq!(loaded_entry2.entry_type, EntryType::Dir);
}

#[test]
fn test_save_and_load_large_cache() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let mut cache = HashMap::new();

    // Create a large cache with many entries
    for i in 0..10000 {
        let path = PathBuf::from(format!("file_{}", i));
        let entry = CacheEntry::new(
            i,
            path.clone(),
            i * 1024,
            1234567890 + i,
            i + 2, // nlink
            Some(i),
            Some(1000),
            if i % 2 == 0 {
                EntryType::File
            } else {
                EntryType::Dir
            },
        );
        cache.insert(path, entry);
    }

    // Get the current directory mtime and use it for cache to avoid mtime mismatch
    let current_mtime = crate::cache::model::get_root_mtime(temp_dir.path());

    // Save cache with the directory's actual mtime
    save_cache_with_mtime(temp_dir.path(), &cache, current_mtime).unwrap();

    // Load it back immediately before the directory mtime can change
    let loaded = load_cache(temp_dir.path(), 604800);
    assert_eq!(loaded.len(), 10000);

    // Verify a few random entries
    let loaded_entry = loaded.get(&PathBuf::from("file_5000")).unwrap();
    assert_eq!(loaded_entry.size, 5000 * 1024);
    assert_eq!(loaded_entry.mtime, 1234567890 + 5000);
}

#[test]
fn test_memory_mapped_io_performance() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let mut cache = HashMap::new();

    // Create a moderately large cache
    for i in 0..1000 {
        let path = PathBuf::from(format!("file_{}", i));
        let entry = CacheEntry::new(
            i,
            path.clone(),
            i * 1024,
            1234567890 + i,
            i + 1, // nlink
            Some(i),
            Some(1000),
            EntryType::File,
        );
        cache.insert(path, entry);
    }

    // Save cache
    let start = std::time::Instant::now();
    save_cache(temp_dir.path(), &cache).unwrap();
    let save_duration = start.elapsed();

    // Load cache
    let start = std::time::Instant::now();
    let loaded = load_cache(temp_dir.path(), 604800);
    let load_duration = start.elapsed();

    // Verify correctness
    assert_eq!(loaded.len(), 1000);

    // These are rough performance checks - in practice, memory-mapped IO
    // should be very fast, especially for loading
    assert!(save_duration.as_millis() < 1000); // Should save in under 1 second
    assert!(load_duration.as_millis() < 100); // Should load in under 100ms
}

#[test]
fn test_cache_file_corruption_handling() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let cache_path = temp_dir.path().join(".rudu-cache.bin");

    // Create a corrupted cache file
    std::fs::write(&cache_path, b"corrupted data").unwrap();

    // Loading should return None for corrupted cache
    let result = load_cache(temp_dir.path(), 604800);
    assert!(result.is_empty());
}

#[test]
fn test_cache_directory_creation() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let nested_path = temp_dir.path().join("nested").join("deep").join("path");

    // Create cache in nested directory that doesn't exist
    let cache = HashMap::new();

    // This should create the necessary directories
    save_cache(&nested_path, &cache).unwrap();

    // Verify cache was saved
    let loaded = load_cache(&nested_path, 604800);
    assert_eq!(loaded.len(), 0);
}

#[test]
fn test_entry_validation() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let mut cache = HashMap::new();

    let entry = CacheEntry::new(
        12345,
        PathBuf::from("test.txt"),
        1024,
        1234567890,
        2, // nlink
        Some(42),
        Some(1000),
        EntryType::File,
    );

    cache.insert(PathBuf::from("test.txt"), entry);

    // Save and load
    save_cache(temp_dir.path(), &cache).unwrap();
    let loaded = load_cache(temp_dir.path(), 604800);

    let loaded_entry = loaded.get(&PathBuf::from("test.txt")).unwrap();

    // Test validation
    assert!(loaded_entry.is_valid(1234567890, 2)); // Valid mtime and nlink
    assert!(!loaded_entry.is_valid(1234567891, 2)); // Different mtime
    assert!(!loaded_entry.is_valid(1234567890, 3)); // Different nlink
}

#[test]
fn test_cache_with_complex_paths() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let mut cache = HashMap::new();

    // Test with complex path names
    let paths = vec![
        PathBuf::from("simple.txt"),
        PathBuf::from("path/with/subdirs/file.txt"),
        PathBuf::from("file with spaces.txt"),
        PathBuf::from("file-with-hyphens.txt"),
        PathBuf::from("file_with_underscores.txt"),
        PathBuf::from("file.with.dots.txt"),
        PathBuf::from("UPPERCASE.TXT"),
        PathBuf::from("123numbers.txt"),
        PathBuf::from("special!@#$%^&*().txt"),
    ];

    for (i, path) in paths.iter().enumerate() {
        let entry = CacheEntry::new(
            i as u64,
            path.clone(),
            ((i + 1) * 1024) as u64,
            1234567890 + i as u64,
            (i + 2) as u64, // nlink
            Some(i as u64),
            Some(1000),
            EntryType::File,
        );
        cache.insert(path.clone(), entry);
    }

    // Save cache
    save_cache(temp_dir.path(), &cache).unwrap();

    // Load it back
    let loaded = load_cache(temp_dir.path(), 604800);
    assert_eq!(loaded.len(), paths.len());

    // Verify all paths are preserved correctly
    for (i, path) in paths.iter().enumerate() {
        let loaded_entry = loaded.get(path).unwrap();
        assert_eq!(loaded_entry.size, ((i + 1) * 1024) as u64);
        assert_eq!(loaded_entry.mtime, 1234567890 + i as u64);
    }
}

#[test]
fn test_cache_with_unicode_paths() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let mut cache = HashMap::new();

    // Test with Unicode path names
    let unicode_paths = vec![
        PathBuf::from("файл.txt"),     // Russian
        PathBuf::from("文件.txt"),     // Chinese
        PathBuf::from("ファイル.txt"), // Japanese
        PathBuf::from("파일.txt"),     // Korean
        PathBuf::from("αρχείο.txt"),   // Greek
        PathBuf::from("archivo.txt"),  // Spanish
        PathBuf::from("𝕱𝖎𝖑𝖊.txt"),     // Mathematical symbols
    ];

    for (i, path) in unicode_paths.iter().enumerate() {
        let entry = CacheEntry::new(
            i as u64,
            path.clone(),
            ((i + 1) * 1024) as u64,
            1234567890 + i as u64,
            (i + 2) as u64, // nlink
            Some(i as u64),
            Some(1000),
            EntryType::File,
        );
        cache.insert(path.clone(), entry);
    }

    // Save cache
    save_cache(temp_dir.path(), &cache).unwrap();

    // Load it back
    let loaded = load_cache(temp_dir.path(), 604800);
    assert_eq!(loaded.len(), unicode_paths.len());

    // Verify all Unicode paths are preserved correctly
    for (i, path) in unicode_paths.iter().enumerate() {
        let loaded_entry = loaded.get(path).unwrap();
        assert_eq!(loaded_entry.size, ((i + 1) * 1024) as u64);
        assert_eq!(loaded_entry.mtime, 1234567890 + i as u64);
    }
}

#[test]
fn test_cache_with_zero_size_files() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let mut cache = HashMap::new();

    // Test with zero-size files
    let entry = CacheEntry::new(
        12345,
        PathBuf::from("empty.txt"),
        0, // Zero size
        1234567890,
        1, // nlink
        Some(0),
        Some(1000),
        EntryType::File,
    );

    cache.insert(PathBuf::from("empty.txt"), entry);

    // Save cache
    save_cache(temp_dir.path(), &cache).unwrap();

    // Load it back
    let loaded = load_cache(temp_dir.path(), 604800);
    assert_eq!(loaded.len(), 1);

    let loaded_entry = loaded.get(&PathBuf::from("empty.txt")).unwrap();
    assert_eq!(loaded_entry.size, 0);
}

#[test]
fn test_cache_concurrent_access() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();
    let temp_path = temp_dir.path().to_path_buf();

    // Create initial cache
    let mut cache = HashMap::new();
    for i in 0..100 {
        let path = PathBuf::from(format!("file_{}", i));
        let entry = CacheEntry::new(
            i,
            path.clone(),
            i * 1024,
            1234567890 + i,
            i + 1, // nlink
            Some(i),
            Some(1000),
            EntryType::File,
        );
        cache.insert(path, entry);
    }

    save_cache(&temp_path, &cache).unwrap();

    // Spawn multiple threads to read the cache concurrently
    let mut handles = vec![];
    for _ in 0..10 {
        let path = temp_path.clone();
        let handle = std::thread::spawn(move || {
            // Ensure cache is enabled in this thread
            crate::cache::set_enabled(true);
            let loaded = load_cache(&path, 604800);
            assert_eq!(loaded.len(), 100);

            // Verify a few entries
            let entry = loaded.get(&PathBuf::from("file_50")).unwrap();
            assert_eq!(entry.size, 50 * 1024);
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_cache_edge_cases() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    let temp_dir = setup_temp_cache_dir().unwrap();

    // Test with empty file name (should work)
    let mut cache = HashMap::new();
    let entry = CacheEntry::new(
        12345,
        PathBuf::from(""),
        1024,
        1234567890,
        2, // nlink
        Some(42),
        Some(1000),
        EntryType::File,
    );
    cache.insert(PathBuf::from(""), entry);

    save_cache(temp_dir.path(), &cache).unwrap();
    let loaded = load_cache(temp_dir.path(), 604800);
    assert_eq!(loaded.len(), 1);

    // Test with very long path
    let long_path = PathBuf::from("a".repeat(1000));
    let mut cache = HashMap::new();
    let entry = CacheEntry::new(
        67890,
        long_path.clone(),
        2048,
        1234567891,
        3, // nlink
        Some(100),
        Some(1001),
        EntryType::File,
    );
    cache.insert(long_path.clone(), entry);

    save_cache(temp_dir.path(), &cache).unwrap();
    let loaded = load_cache(temp_dir.path(), 604800);
    assert_eq!(loaded.len(), 1);

    let loaded_entry = loaded.get(&long_path).unwrap();
    assert_eq!(loaded_entry.size, 2048);
}

#[test]
fn test_cache_invalidation_integration() {
    let _lock = CACHE_TEST_LOCK.lock().unwrap();
    use super::load_cache;
    let temp_dir = setup_temp_cache_dir().unwrap();
    let mut cache = HashMap::new();

    // Create a cache entry
    let entry = CacheEntry::new(
        12345,
        PathBuf::from("test.txt"),
        1024,
        1234567890,
        2,
        Some(42),
        Some(1000),
        EntryType::File,
    );
    cache.insert(PathBuf::from("test.txt"), entry);

    // Save cache
    save_cache(temp_dir.path(), &cache).unwrap();

    // Test 1: Load with normal TTL should work
    let loaded = load_cache(temp_dir.path(), 604800);
    assert_eq!(loaded.len(), 1);

    // Test 2: Load with very short TTL should invalidate cache
    // Wait a small moment to ensure the cache is older than TTL
    std::thread::sleep(std::time::Duration::from_millis(10));
    let loaded = load_cache(temp_dir.path(), 0);
    assert!(loaded.is_empty());
}
