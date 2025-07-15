//! Cache data structures and serialization logic
//!
//! This module defines the core data structures used for caching filesystem
//! metadata to improve performance on subsequent directory scans.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::data::EntryType;

/// Cache header containing metadata about the cache file
///
/// This structure stores global information about the cache including
/// the root path that was scanned, when the cache was created, and
/// the version of rudu that created it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheHeader {
    /// The root path that was scanned to create this cache
    pub root_path: PathBuf,
    /// Unix timestamp when the cache was created
    pub creation_time: u64,
    /// Version of rudu that created this cache
    pub rudu_version: String,
    /// Root directory's modification time when cache was created
    pub root_mtime: Option<u64>,
}

/// Individual cache entry for a file or directory
///
/// This structure stores the essential metadata needed to determine
/// if a file system entry has changed since the last scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Hash of the file path for efficient lookups
    pub path_hash: u64,
    /// The actual file path (for reconstruction)
    pub path: PathBuf,
    /// Size of the file/directory in bytes
    pub size: u64,
    /// Last modification time (Unix timestamp)
    pub mtime: u64,
    /// Number of hard links (for directories, indicates potential new children)
    pub nlink: u64,
    /// Number of inodes (for directories)
    pub inode_cnt: Option<u64>,
    /// Owner user ID
    pub owner: Option<u32>,
    /// Type of entry (file or directory)
    pub entry_type: EntryType,
}

/// Complete cache structure containing header and entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cache {
    /// Cache metadata
    pub header: CacheHeader,
    /// Map of path hashes to cache entries
    pub entries: HashMap<u64, CacheEntry>,
}

impl CacheHeader {
    /// Create a new cache header for the given root path
    pub fn new(root_path: PathBuf) -> Self {
        let creation_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let root_mtime = get_root_mtime(&root_path);

        Self {
            root_path,
            creation_time,
            rudu_version: env!("CARGO_PKG_VERSION").to_string(),
            root_mtime,
        }
    }
    
    /// Create a new cache header with a specific root mtime
    pub fn new_with_mtime(root_path: PathBuf, root_mtime: Option<u64>) -> Self {
        let creation_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            root_path,
            creation_time,
            rudu_version: env!("CARGO_PKG_VERSION").to_string(),
            root_mtime,
        }
    }

    /// Check if the cache should be invalidated based on version, TTL, and root mtime
    ///
    /// # Arguments
    /// * `root_path` - The root path being scanned
    /// * `ttl_seconds` - Time to live in seconds (default 7 days = 604800)
    ///
    /// # Returns
    /// * `bool` - true if cache should be invalidated, false if still valid
    pub fn should_invalidate(&self, root_path: &Path, ttl_seconds: u64) -> bool {
        let current_version = env!("CARGO_PKG_VERSION");
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Check version mismatch
        if self.rudu_version != current_version {
            return true;
        }

        // Check TTL
        if current_time.saturating_sub(self.creation_time) >= ttl_seconds {
            return true;
        }

        // Check root path mismatch
        if self.root_path != root_path {
            return true;
        }

        // Check root's own mtime
        if let Some(current_root_mtime) = get_root_mtime(root_path) {
            if let Some(cached_root_mtime) = self.root_mtime {
                if current_root_mtime != cached_root_mtime {
                    return true;
                }
            } else {
                // If we don't have cached root mtime, invalidate to be safe
                return true;
            }
        }

        false
    }
}

impl CacheEntry {
    /// Create a new cache entry from file metadata
    pub fn new(
        path_hash: u64,
        path: PathBuf,
        size: u64,
        mtime: u64,
        nlink: u64,
        inode_cnt: Option<u64>,
        owner: Option<u32>,
        entry_type: EntryType,
    ) -> Self {
        Self {
            path_hash,
            path,
            size,
            mtime,
            nlink,
            inode_cnt,
            owner,
            entry_type,
        }
    }

    /// Check if this cache entry is still valid compared to current metadata
    pub fn is_valid(&self, current_mtime: u64, current_nlink: u64) -> bool {
        self.mtime == current_mtime && self.nlink == current_nlink
    }
}

impl Cache {
    /// Create a new empty cache for the given root path
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            header: CacheHeader::new(root_path),
            entries: HashMap::new(),
        }
    }

    /// Add an entry to the cache
    pub fn add_entry(&mut self, entry: CacheEntry) {
        self.entries.insert(entry.path_hash, entry);
    }

    /// Get an entry from the cache by path hash
    pub fn get_entry(&self, path_hash: u64) -> Option<&CacheEntry> {
        self.entries.get(&path_hash)
    }

    /// Get the number of entries in the cache
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Load cache from a file using bincode deserialization
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())
            .with_context(|| format!("Failed to open cache file: {}", path.as_ref().display()))?;
        
        let reader = BufReader::new(file);
        let cache = bincode::deserialize_from(reader)
            .with_context(|| format!("Failed to deserialize cache from: {}", path.as_ref().display()))?;
        
        Ok(cache)
    }

    /// Save cache to a file using bincode serialization
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path.as_ref())
            .with_context(|| format!("Failed to create cache file: {}", path.as_ref().display()))?;
        
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, self)
            .with_context(|| format!("Failed to serialize cache to: {}", path.as_ref().display()))?;
        
        Ok(())
    }

    /// Get the cache file path for a given root directory
    /// 
    /// First tries to use `<root>/.rudu-cache.bin`. If the root directory
    /// is not writable, falls back to `$XDG_CACHE_HOME/rudu/<hash>.bin`
    /// where `<hash>` is a hash of the root path.
    pub fn get_cache_path(root: &Path) -> Result<PathBuf> {
        // Try primary location: <root>/.rudu-cache.bin
        let primary_path = root.join(".rudu-cache.bin");
        
        // Check if we can write to the root directory
        if root.is_dir() && is_writable(root) {
            return Ok(primary_path);
        }

        // Fallback to XDG cache directory
        let cache_dir = get_xdg_cache_dir()?;
        let rudu_cache_dir = cache_dir.join("rudu");
        
        // Create the rudu cache directory if it doesn't exist
        std::fs::create_dir_all(&rudu_cache_dir)
            .with_context(|| format!("Failed to create cache directory: {}", rudu_cache_dir.display()))?;
        
        // Generate a hash of the root path for the filename
        let root_hash = calculate_path_hash(root);
        let cache_file = rudu_cache_dir.join(format!("{:x}.bin", root_hash));
        
        Ok(cache_file)
    }

    /// Get the cache file path for a given root directory without performing write test
    /// 
    /// This function always uses the XDG cache directory to avoid changing the
    /// directory's mtime during cache operations.
    pub fn get_cache_path_without_write_test(root: &Path) -> Result<PathBuf> {
        // Always use XDG cache directory to avoid mtime issues
        let cache_dir = get_xdg_cache_dir()?;
        let rudu_cache_dir = cache_dir.join("rudu");
        
        // Create the rudu cache directory if it doesn't exist
        std::fs::create_dir_all(&rudu_cache_dir)
            .with_context(|| format!("Failed to create cache directory: {}", rudu_cache_dir.display()))?;
        
        // Generate a hash of the root path for the filename
        let root_hash = calculate_path_hash(root);
        let cache_file = rudu_cache_dir.join(format!("{:x}.bin", root_hash));
        
        Ok(cache_file)
    }
}

/// Check if a directory is writable
fn is_writable(path: &Path) -> bool {
    // Try creating a temporary file to test write permissions
    let temp_file = path.join(".rudu-write-test");
    match std::fs::write(&temp_file, b"test") {
        Ok(_) => {
            // Clean up the test file
            let _ = std::fs::remove_file(&temp_file);
            true
        }
        Err(_) => false,
    }
}

/// Get the XDG cache directory, falling back to a default if not set
fn get_xdg_cache_dir() -> Result<PathBuf> {
    if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
        Ok(PathBuf::from(xdg_cache))
    } else {
        // Fallback to ~/.cache on Unix systems
        let home = std::env::var("HOME")
            .context("Neither XDG_CACHE_HOME nor HOME environment variables are set")?;
        Ok(PathBuf::from(home).join(".cache"))
    }
}

/// Calculate a hash of a path for use in cache file names
fn calculate_path_hash(path: &Path) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

/// Get root directory's modification time
pub fn get_root_mtime(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    
    path.metadata()
        .ok()
        .map(|meta| meta.mtime() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_cache_header_creation() {
        let root = PathBuf::from("/test/root");
        let header = CacheHeader::new(root.clone());
        
        assert_eq!(header.root_path, root);
        assert_eq!(header.rudu_version, env!("CARGO_PKG_VERSION"));
        assert!(header.creation_time > 0);
    }

    #[test]
    fn test_cache_entry_creation() {
        let path = PathBuf::from("/test/file");
        let entry = CacheEntry::new(
            12345,
            path.clone(),
            1024,
            1234567890,
            2,
            Some(42),
            Some(1000),
            EntryType::File,
        );
        
        assert_eq!(entry.path_hash, 12345);
        assert_eq!(entry.path, path);
        assert_eq!(entry.size, 1024);
        assert_eq!(entry.mtime, 1234567890);
        assert_eq!(entry.nlink, 2);
        assert_eq!(entry.inode_cnt, Some(42));
        assert_eq!(entry.owner, Some(1000));
        assert_eq!(entry.entry_type, EntryType::File);
    }

    #[test]
    fn test_cache_entry_validity() {
        let entry = CacheEntry::new(
            12345,
            PathBuf::from("/test/file"),
            1024,
            1234567890,
            2,
            Some(42),
            Some(1000),
            EntryType::File,
        );
        
        // Valid case
        assert!(entry.is_valid(1234567890, 2));
        
        // Invalid cases
        assert!(!entry.is_valid(1234567891, 2)); // Different mtime
        assert!(!entry.is_valid(1234567890, 3)); // Different nlink
    }

    #[test]
    fn test_cache_operations() {
        let root = PathBuf::from("/test/root");
        let mut cache = Cache::new(root.clone());
        
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        
        let entry = CacheEntry::new(
            12345,
            PathBuf::from("/test/file"),
            1024,
            1234567890,
            2,
            Some(42),
            Some(1000),
            EntryType::File,
        );
        
        cache.add_entry(entry.clone());
        
        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);
        
        let retrieved = cache.get_entry(12345);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().size, 1024);
        
        let missing = cache.get_entry(54321);
        assert!(missing.is_none());
    }

    #[test]
    fn test_cache_serialization() {
        let temp_dir = tempdir().unwrap();
        let cache_file = temp_dir.path().join("test_cache.bin");
        
        // Create a cache with some data
        let root = PathBuf::from("/test/root");
        let mut cache = Cache::new(root);
        
        let entry = CacheEntry::new(
            12345,
            PathBuf::from("/test/file"),
            1024,
            1234567890,
            2,
            Some(42),
            Some(1000),
            EntryType::File,
        );
        cache.add_entry(entry);
        
        // Save to file
        cache.save_to_file(&cache_file).unwrap();
        
        // Load from file
        let loaded_cache = Cache::load_from_file(&cache_file).unwrap();
        
        // Verify the loaded cache matches the original
        assert_eq!(loaded_cache.header.root_path, cache.header.root_path);
        assert_eq!(loaded_cache.header.rudu_version, cache.header.rudu_version);
        assert_eq!(loaded_cache.len(), cache.len());
        
        let loaded_entry = loaded_cache.get_entry(12345).unwrap();
        assert_eq!(loaded_entry.size, 1024);
        assert_eq!(loaded_entry.mtime, 1234567890);
        assert_eq!(loaded_entry.entry_type, EntryType::File);
    }

    #[test]
    fn test_path_hash_calculation() {
        let path1 = PathBuf::from("/test/path1");
        let path2 = PathBuf::from("/test/path2");
        let path1_dup = PathBuf::from("/test/path1");
        
        let hash1 = calculate_path_hash(&path1);
        let hash2 = calculate_path_hash(&path2);
        let hash1_dup = calculate_path_hash(&path1_dup);
        
        assert_eq!(hash1, hash1_dup);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_cache_path_generation() {
        let temp_dir = tempdir().unwrap();
        let cache_path = Cache::get_cache_path(temp_dir.path()).unwrap();
        
        // Should prefer the primary location since temp dir is writable
        assert_eq!(cache_path, temp_dir.path().join(".rudu-cache.bin"));
    }

    #[test]
    fn test_cache_invalidation_version_mismatch() {
        let root = PathBuf::from("/test/root");
        let mut header = CacheHeader::new(root.clone());
        
        // Test version mismatch
        header.rudu_version = "0.0.0".to_string();
        assert!(header.should_invalidate(&root, 604800));
    }

    #[test]
    fn test_cache_invalidation_ttl_expired() {
        let root = PathBuf::from("/test/root");
        let mut header = CacheHeader::new(root.clone());
        
        // Set creation time to 8 days ago (TTL is 7 days = 604800 seconds)
        header.creation_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() - 8 * 24 * 60 * 60; // 8 days ago
        
        assert!(header.should_invalidate(&root, 604800));
        
        // Test exact TTL boundary (cache created exactly TTL seconds ago)
        header.creation_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() - 604800; // Exactly 7 days ago
        
        assert!(header.should_invalidate(&root, 604800)); // Should invalidate at TTL boundary
    }

    #[test]
    fn test_cache_invalidation_path_mismatch() {
        let root = PathBuf::from("/test/root");
        let header = CacheHeader::new(root.clone());
        let different_root = PathBuf::from("/different/root");
        
        assert!(header.should_invalidate(&different_root, 604800));
    }

    #[test]
    fn test_cache_invalidation_valid_cache() {
        let root = PathBuf::from("/test/root");
        let header = CacheHeader::new(root.clone());
        
        // Should be valid (same version, recent, same path)
        assert!(!header.should_invalidate(&root, 604800));
    }
}
