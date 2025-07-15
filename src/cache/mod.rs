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
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub use model::{CacheEntry, CacheHeader};

/// Load cache from disk using memory-mapped IO for O(1) access time
///
/// This function uses memory-mapped files to efficiently load large caches
/// without reading the entire file into memory at once. Returns None if the
/// cache file doesn't exist or is invalid.
///
/// # Arguments
/// * `root` - The root path to determine the cache file location
/// * `ttl_seconds` - Time to live in seconds for cache invalidation
///
/// # Returns
/// * `Option<HashMap<PathBuf, CacheEntry>>` - The loaded cache entries, or None if not found
pub fn load_cache(root: &Path, ttl_seconds: u64) -> Option<HashMap<PathBuf, CacheEntry>> {
    let cache_path = match model::Cache::get_cache_path_without_write_test(root) {
        Ok(path) => path,
        Err(_) => return None,
    };

    // Check if cache file exists
    if !cache_path.exists() {
        return None;
    }

    match load_cache_from_file(&cache_path) {
        Ok(cache) => {
            // Check if cache should be invalidated
            if cache.header.should_invalidate(root, ttl_seconds) {
                println!("🗑️  Cache invalidated (version mismatch, TTL expired, or root mtime changed)");
                // Optionally remove the invalidated cache file
                let _ = std::fs::remove_file(&cache_path);
                return None;
            }
            // Convert from hash-based entries back to path-based entries
            let path_entries: HashMap<PathBuf, CacheEntry> = cache.entries
                .into_iter()
                .map(|(_, entry)| (entry.path.clone(), entry))
                .collect();
            Some(path_entries)
        },
        Err(_) => None, // If loading fails, return None (cache will be regenerated)
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
pub fn save_cache_with_mtime(root: &Path, cache: &HashMap<PathBuf, CacheEntry>, root_mtime: Option<u64>) -> Result<()> {
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
    let file = File::open(path)
        .with_context(|| format!("Failed to open cache file: {}", path.display()))?;

    let file_len = file.metadata()
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
            let header = model::CacheHeader::new(path.parent().unwrap_or(Path::new("/")).to_path_buf());
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

/// Save cache to a specific file using efficient serialization
fn save_cache_to_file(path: &Path, cache: &model::Cache) -> Result<()> {
    // First serialize to get the size
    let serialized_data = bincode::serialize(cache)
        .context("Failed to serialize cache data")?;

    // Try memory-mapped IO first, fall back to regular file IO if it fails
    if let Err(_) = try_save_with_mmap(path, &serialized_data) {
        // Fallback to regular file IO
        save_with_regular_io(path, &serialized_data)
            .with_context(|| format!("Failed to save cache to: {}", path.display()))?;
    }

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
        MmapMut::map_mut(&file)
            .with_context(|| format!("Failed to memory-map cache file for writing: {}", path.display()))?
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
