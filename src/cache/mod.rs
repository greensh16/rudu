//! Cache module for rudu
//!
//! This module provides disk-based caching functionality for rudu to improve
//! performance on subsequent runs by storing metadata about scanned
//! directories and files.
//!
//! The cache uses bincode for efficient serialization. Cache files are stored
//! in a configurable cache directory (`RUDU_CACHE_DIR`, falling back to the
//! XDG cache directory), never inside the scanned tree — writing there would
//! perturb the very mtimes the cache validates against.

pub mod model;

#[cfg(test)]
mod tests;

use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

// In-process file lock serialising cache reads/writes. (Cross-process safety
// comes from the write-to-temp-then-rename pattern in save_cache_to_file.)
static FILE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

// Global cache enabled flag - can be disabled dynamically when nearing memory limits
static CACHE_ENABLED: AtomicBool = AtomicBool::new(true);

pub use model::{CacheEntry, CacheEntryParams, CacheHeader};

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

/// Load cache from disk.
///
/// Reads and deserializes the whole cache file (bincode is not a random-access
/// format, so the file must be read in full regardless of how it is opened).
/// Returns an empty cache if the cache file doesn't exist or is invalid.
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
                eprintln!(
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

/// Save cache to disk using bincode serialization with an atomic write.
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

/// Save cache to disk using bincode serialization with a specific root mtime.
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

/// Load cache from a specific file
fn load_cache_from_file(path: &Path) -> Result<model::Cache> {
    // Lock file access to prevent concurrent reads/writes within this process
    let _g = FILE_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    // Plain buffered read: bincode deserializes the whole buffer anyway, so
    // the previous memory-mapped path added an `unsafe` block (and a SIGBUS
    // hazard if another process truncated the file) for no benefit.
    let data = std::fs::read(path)
        .with_context(|| format!("Failed to read cache file: {}", path.display()))?;

    if data.is_empty() {
        return Err(anyhow!("Cache file is empty"));
    }

    // Try to deserialize as new Cache format first
    match bincode::deserialize::<model::Cache>(&data) {
        Ok(cache) => Ok(cache),
        Err(_) => {
            // Try to deserialize as old format (HashMap<PathBuf, CacheEntry>)
            let legacy_cache: HashMap<PathBuf, CacheEntry> = bincode::deserialize(&data)
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

/// Save cache to a specific file with an atomic write.
///
/// The data is written to a sibling temporary file and then renamed into
/// place, so readers never observe a partially-written cache. (The previous
/// implementation additionally tried memory-mapped writes with a regular-IO
/// fallback — extra complexity and an `unsafe` block for no measurable gain
/// over a plain buffered write.)
fn save_cache_to_file(path: &Path, cache: &model::Cache) -> Result<()> {
    // Lock file access to prevent concurrent reads/writes within this process
    let _g = FILE_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    let serialized_data = bincode::serialize(cache).context("Failed to serialize cache data")?;

    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, &serialized_data).with_context(|| {
        format!(
            "Failed to save cache to temporary file: {}",
            temp_path.display()
        )
    })?;

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
        unsafe { env::set_var("RUDU_CACHE_DIR", &custom_cache_path) };

        // Test that cache_root() uses the custom directory
        let cache_root_result = cache_root();
        assert_eq!(cache_root_result, PathBuf::from(&custom_cache_path));

        // Clean up
        unsafe { env::remove_var("RUDU_CACHE_DIR") };
    }

    #[test]
    fn test_cache_root_fallback_to_xdg() {
        let _lock = crate::cache::tests::safe_lock(&crate::cache::tests::CACHE_TEST_LOCK);
        // Ensure RUDU_CACHE_DIR is not set
        unsafe { env::remove_var("RUDU_CACHE_DIR") };

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

        // Store original values for cleanup
        let original_rudu_cache_dir = env::var("RUDU_CACHE_DIR").ok();
        let original_cache_enabled = is_enabled();

        // Set RUDU_CACHE_DIR environment variable
        unsafe { env::set_var("RUDU_CACHE_DIR", &custom_cache_path) };

        // Ensure caching is enabled for the test
        set_enabled(true);

        // Use a temp directory as root path to avoid mtime issues with current directory
        let test_root_dir = TempDir::new().unwrap();
        let root_path = test_root_dir.path();

        let mut cache = HashMap::new();
        let entry = CacheEntry::new(CacheEntryParams {
            path: PathBuf::from("test.txt"),
            size: 1024,
            mtime: 1234567890,
            nlink: 1,
            inode_cnt: Some(1),
            owner: Some(1000),
            entry_type: crate::data::EntryType::File,
            atime: None,
        });
        cache.insert(PathBuf::from("test.txt"), entry);

        // Capture the root directory's mtime before saving to avoid cache invalidation
        let root_mtime = crate::cache::model::get_root_mtime(root_path);

        // Save cache with specific mtime (should use custom directory)
        let save_result = save_cache_with_mtime(root_path, &cache, root_mtime);
        assert!(save_result.is_ok());

        // Load cache (should load from custom directory)
        let loaded_cache = load_cache(root_path, 604800);
        assert_eq!(loaded_cache.len(), 1);
        assert!(loaded_cache.contains_key(&PathBuf::from("test.txt")));

        // Test invalidation
        let was_invalidated = invalidate_cache(root_path);
        assert!(was_invalidated.is_ok());
        assert!(was_invalidated.unwrap());

        // Clean up environment variables
        match original_rudu_cache_dir {
            Some(value) => unsafe { env::set_var("RUDU_CACHE_DIR", value) },
            None => unsafe { env::remove_var("RUDU_CACHE_DIR") },
        }

        // Restore original cache enabled state
        set_enabled(original_cache_enabled);
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
        let entry = CacheEntry::new(CacheEntryParams {
            path: PathBuf::from("test.txt"),
            size: 1024,
            mtime: 1234567890,
            nlink: 1,
            inode_cnt: Some(1),
            owner: Some(1000),
            entry_type: crate::data::EntryType::File,
            atime: None,
        });
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
