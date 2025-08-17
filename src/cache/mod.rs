//! Cache module for rudu
//!
//! This module provides disk-based caching functionality for rudu to improve
//! performance on subsequent runs by storing metadata about scanned directories.
//!
//! The cache uses bincode for efficient serialization and stores cache files
//! either in the scanned directory (as `.rudu-cache.bin`) or in the system
//! cache directory as a fallback.

pub mod model;

#[cfg(test)]
mod tests;

use anyhow::{anyhow, Context, Result};
use memmap2::{Mmap, MmapMut};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

// Thread-safe file lock for atomic cache operations
static FILE_LOCK: Lazy<parking_lot::Mutex<()>> = Lazy::new(|| parking_lot::Mutex::new(()));

// Global cache enabled flag - can be disabled dynamically when nearing memory limits
static CACHE_ENABLED: AtomicBool = AtomicBool::new(true);

pub use model::{CacheEntry, CacheHeader};

/// Enable or disable caching dynamically
///
/// This function can be called to enable or disable cache operations at runtime,
/// typically used when memory usage is nearing limits to reduce memory consumption.
///
/// # Arguments
/// * `enabled` - True to enable caching, false to disable
pub fn set_enabled(enabled: bool) {
    CACHE_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Check if caching is currently enabled
///
/// Returns the current state of the cache enable flag.
///
/// # Returns
/// * `bool` - True if caching is enabled, false otherwise
pub fn is_enabled() -> bool {
    CACHE_ENABLED.load(Ordering::Relaxed)
}

/// Get the cache root directory
///
/// This function provides a centralized way to determine the cache root directory:
/// 1. If `RUDU_CACHE_DIR` environment variable is set, use that
/// 2. Otherwise, fall back to XDG cache directory logic
///
/// # Returns
/// * `PathBuf` - The cache root directory path
pub fn cache_root() -> PathBuf {
    if let Ok(cache_dir) = std::env::var("RUDU_CACHE_DIR") {
        PathBuf::from(cache_dir)
    } else {
        // Fall back to XDG cache directory logic
        model::get_xdg_cache_dir().unwrap_or_else(|_| {
            // Final fallback if even HOME is not set
            std::env::temp_dir().join("rudu-cache")
        })
    }
}

/// Load cache from disk using memory-mapped IO for O(1) access time
///
/// This function uses memory-mapped files to efficiently load large caches
/// without reading the entire file into memory at once. Returns an empty cache
/// if the cache file doesn't exist or is invalid.
///
/// # Arguments
/// * `root` - The root path to determine the cache file location
/// * `ttl_seconds` - Time to live in seconds for cache invalidation
///
/// # Returns
/// * `HashMap<PathBuf, CacheEntry>` - The loaded cache entries, or empty cache if not found
pub fn load_cache(root: &Path, ttl_seconds: u64) -> HashMap<PathBuf, CacheEntry> {
    // Check if caching is disabled dynamically
    if !is_enabled() {
        return HashMap::new();
    }

    let cache_path = match model::Cache::get_cache_path_without_write_test(root) {
        Ok(path) => path,
        Err(_) => {
            return HashMap::new();
        }
    };

    // Check if cache file exists
    if !cache_path.exists() {
        return HashMap::new();
    }

    match load_cache_from_file(&cache_path) {
        Ok(cache) => {
            // Check if cache should be invalidated
            if cache.header.should_invalidate(root, ttl_seconds) {
                println!(
                    "🗑️  Cache invalidated (version mismatch, TTL expired, or root mtime changed)"
                );
                // Optionally remove the invalidated cache file
                let _ = std::fs::remove_file(&cache_path);
                return HashMap::new();
            }
            // Convert from hash-based entries back to path-based entries
            let path_entries: HashMap<PathBuf, CacheEntry> = cache
                .entries
                .into_values()
                .map(|entry| (entry.path.clone(), entry))
                .collect();
            path_entries
        }
        Err(_e) => {
            HashMap::new() // If loading fails, return an empty cache (cache will be regenerated)
        }
    }
}

/// Save cache to disk using efficient serialization
///
/// This function saves the cache entries to disk in a format that can be
/// efficiently loaded using memory-mapped IO.
///
/// # Arguments
/// * `root` - The root path to determine the cache file location
/// * `cache` - The cache entries to save
///
/// # Returns
/// * `Result<()>` - Success or error information
pub fn save_cache(root: &Path, cache: &HashMap<PathBuf, CacheEntry>) -> Result<()> {
    // Capture root mtime before any directory modifications
    let root_mtime = model::get_root_mtime(root);
    save_cache_with_mtime(root, cache, root_mtime)
}

/// Invalidate (remove) cache files for a given root directory
///
/// This function removes the cache file from disk, effectively invalidating
/// the cache for the specified root directory.
///
/// # Arguments
/// * `root` - The root path for which to invalidate the cache
///
/// # Returns
/// * `Result<bool>` - True if a cache file was removed, false if none existed
pub fn invalidate_cache(root: &Path) -> Result<bool> {
    let cache_path = model::Cache::get_cache_path_without_write_test(root)
        .context("Failed to determine cache file path")?;

    if cache_path.exists() {
        std::fs::remove_file(&cache_path)
            .with_context(|| format!("Failed to remove cache file: {}", cache_path.display()))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Save cache to disk using efficient serialization with a specific root mtime
///
/// This function saves the cache entries to disk in a format that can be
/// efficiently loaded using memory-mapped IO.
///
/// # Arguments
/// * `root` - The root path to determine the cache file location
/// * `cache` - The cache entries to save
/// * `root_mtime` - The root directory's mtime to use for the cache header
///
/// # Returns
/// * `Result<()>` - Success or error information
pub fn save_cache_with_mtime(
    root: &Path,
    cache: &HashMap<PathBuf, CacheEntry>,
    root_mtime: Option<u64>,
) -> Result<()> {
    // Check if caching is disabled dynamically
    if !is_enabled() {
        return Ok(()); // Silently skip cache saving when disabled
    }

    let cache_path = model::Cache::get_cache_path_without_write_test(root)
        .context("Failed to determine cache file path")?;

    // Ensure parent directory exists
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create cache directory: {}", parent.display()))?;
    }

    // Create new cache structure with header using pre-captured root mtime
    let header = model::CacheHeader::new_with_mtime(root.to_path_buf(), root_mtime);
    let entries: HashMap<u64, CacheEntry> = cache
        .iter()
        .map(|(path, entry)| {
            let mut new_entry = entry.clone();
            // Ensure path is set in the entry
            new_entry.path = path.clone();
            (crate::utils::path_hash(path), new_entry)
        })
        .collect();

    let full_cache = model::Cache { header, entries };

    save_cache_to_file(&cache_path, &full_cache)
        .with_context(|| format!("Failed to save cache to: {}", cache_path.display()))
}

/// Load cache from a specific file using memory-mapped IO
fn load_cache_from_file(path: &Path) -> Result<model::Cache> {
    // Lock file access to prevent concurrent reads/writes
    let _g = FILE_LOCK.lock();

    let file = File::open(path)
        .with_context(|| format!("Failed to open cache file: {}", path.display()))?;

    let file_len = file
        .metadata()
        .with_context(|| format!("Failed to get file metadata: {}", path.display()))?
        .len();

    if file_len == 0 {
        return Err(anyhow!("Cache file is empty"));
    }

    // Create memory-mapped file for efficient access
    let mmap = unsafe {
        Mmap::map(&file)
            .with_context(|| format!("Failed to memory-map cache file: {}", path.display()))?
    };

    // Try to deserialize as new Cache format first
    match bincode::deserialize::<model::Cache>(&mmap) {
        Ok(cache) => Ok(cache),
        Err(_) => {
            // Try to deserialize as old format (HashMap<PathBuf, CacheEntry>)
            let legacy_cache: HashMap<PathBuf, CacheEntry> = bincode::deserialize(&mmap)
                .with_context(|| format!("Failed to deserialize cache from: {}", path.display()))?;

            // Convert legacy format to new format
            let header =
                model::CacheHeader::new(path.parent().unwrap_or(Path::new("/")).to_path_buf());
            let entries: HashMap<u64, CacheEntry> = legacy_cache
                .into_iter()
                .map(|(path, mut entry)| {
                    // Add path field to legacy entry if missing
                    entry.path = path.clone();
                    (crate::utils::path_hash(&path), entry)
                })
                .collect();

            Ok(model::Cache { header, entries })
        }
    }
}

/// Save cache to a specific file using efficient serialization with atomic writes
fn save_cache_to_file(path: &Path, cache: &model::Cache) -> Result<()> {
    // Lock file access to prevent concurrent reads/writes
    let _g = FILE_LOCK.lock();

    // First serialize to get the size
    let serialized_data = bincode::serialize(cache).context("Failed to serialize cache data")?;

    // Create temporary file path
    let temp_path = path.with_extension("tmp");

    // Try memory-mapped IO first, fall back to regular file IO if it fails
    if try_save_with_mmap(&temp_path, &serialized_data).is_err() {
        // Fallback to regular file IO
        save_with_regular_io(&temp_path, &serialized_data).with_context(|| {
            format!(
                "Failed to save cache to temporary file: {}",
                temp_path.display()
            )
        })?;
    }

    // Atomically move the temporary file to the final location
    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Failed to atomically move cache file from {} to {}",
            temp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

/// Try to save using memory-mapped IO
fn try_save_with_mmap(path: &Path, data: &[u8]) -> Result<()> {
    let file_size = data.len() as u64;

    // Create or truncate the file
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("Failed to create cache file: {}", path.display()))?;

    // Set the file size
    file.set_len(file_size)
        .with_context(|| format!("Failed to set file size: {}", path.display()))?;

    // Ensure we're at the beginning of the file
    file.seek(SeekFrom::Start(0))
        .with_context(|| format!("Failed to seek to beginning of file: {}", path.display()))?;

    // Create memory-mapped file for writing
    let mut mmap = unsafe {
        MmapMut::map_mut(&file).with_context(|| {
            format!(
                "Failed to memory-map cache file for writing: {}",
                path.display()
            )
        })?
    };

    // Copy the serialized data to the memory-mapped region
    if mmap.len() >= data.len() {
        mmap[..data.len()].copy_from_slice(data);
    } else {
        return Err(anyhow!(
            "Memory-mapped region too small: {} < {}",
            mmap.len(),
            data.len()
        ));
    }

    // Flush the memory-mapped data to disk
    mmap.flush()
        .with_context(|| format!("Failed to flush cache data to disk: {}", path.display()))?;

    Ok(())
}

/// Fallback to regular file IO
fn save_with_regular_io(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("Failed to create cache file: {}", path.display()))?;

    file.write_all(data)
        .with_context(|| format!("Failed to write cache data: {}", path.display()))?;

    file.flush()
        .with_context(|| format!("Failed to flush cache data: {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod cache_root_tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_cache_root_with_rudu_cache_dir() {
        let _lock = crate::cache::tests::safe_lock(&crate::cache::tests::CACHE_TEST_LOCK);
        let temp_dir = TempDir::new().unwrap();
        let custom_cache_path = temp_dir.path().to_string_lossy().to_string();

        // Set RUDU_CACHE_DIR environment variable
        env::set_var("RUDU_CACHE_DIR", &custom_cache_path);

        // Test that cache_root() uses the custom directory
        let cache_root_result = cache_root();
        assert_eq!(cache_root_result, PathBuf::from(&custom_cache_path));

        // Clean up
        env::remove_var("RUDU_CACHE_DIR");
    }

    #[test]
    fn test_cache_root_fallback_to_xdg() {
        let _lock = crate::cache::tests::safe_lock(&crate::cache::tests::CACHE_TEST_LOCK);
        // Ensure RUDU_CACHE_DIR is not set
        env::remove_var("RUDU_CACHE_DIR");

        // Test that cache_root() falls back to XDG logic
        let cache_root_result = cache_root();

        // Should not be empty and should contain some sensible path
        assert!(!cache_root_result.to_string_lossy().is_empty());

        // Should be different from a custom path
        assert_ne!(cache_root_result, PathBuf::from("/tmp/custom-rudu-cache"));
    }

    #[test]
    fn test_cache_operations_use_configurable_directory() {
        let _lock = crate::cache::tests::safe_lock(&crate::cache::tests::CACHE_TEST_LOCK);
        let temp_dir = TempDir::new().unwrap();
        let custom_cache_path = temp_dir.path().to_string_lossy().to_string();

        // Set RUDU_CACHE_DIR environment variable
        env::set_var("RUDU_CACHE_DIR", &custom_cache_path);

        // Ensure caching is enabled for the test
        set_enabled(true);

        // Create test cache
        let root_path = PathBuf::from(".");
        let mut cache = HashMap::new();
        let entry = CacheEntry::new(
            12345,
            PathBuf::from("test.txt"),
            1024,
            1234567890,
            1,
            Some(1),
            Some(1000),
            crate::data::EntryType::File,
        );
        cache.insert(PathBuf::from("test.txt"), entry);

        // Save cache (should use custom directory)
        let save_result = save_cache(&root_path, &cache);
        assert!(save_result.is_ok());

        // Load cache (should load from custom directory)
        let loaded_cache = load_cache(&root_path, 604800);
        assert_eq!(loaded_cache.len(), 1);
        assert!(loaded_cache.contains_key(&PathBuf::from("test.txt")));

        // Test invalidation
        let was_invalidated = invalidate_cache(&root_path);
        assert!(was_invalidated.is_ok());
        assert!(was_invalidated.unwrap());

        // Clean up
        env::remove_var("RUDU_CACHE_DIR");
    }

    #[test]
    fn test_dynamic_cache_enabling_disabling() {
        // Store initial state to restore it at the end
        let initial_state = is_enabled();

        // Test initial state (should normally be enabled)
        set_enabled(true); // Ensure we start enabled
        assert!(is_enabled());

        // Test disabling
        set_enabled(false);
        assert!(!is_enabled());

        // Test that load_cache returns empty when disabled
        let root_path = PathBuf::from(".");
        let cache = load_cache(&root_path, 604800);
        assert!(cache.is_empty());

        // Test that save_cache succeeds silently when disabled
        let mut test_cache = HashMap::new();
        let entry = CacheEntry::new(
            12345,
            PathBuf::from("test.txt"),
            1024,
            1234567890,
            1,
            Some(1),
            Some(1000),
            crate::data::EntryType::File,
        );
        test_cache.insert(PathBuf::from("test.txt"), entry);

        let save_result = save_cache_with_mtime(&root_path, &test_cache, None);
        assert!(save_result.is_ok());

        // Re-enable caching
        set_enabled(true);
        assert!(is_enabled());

        // Restore original state for other tests
        set_enabled(initial_state);
    }
}
