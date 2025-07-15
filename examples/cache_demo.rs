//! Cache functionality demonstration
//!
//! This example shows how to use the cache module to store and retrieve
//! filesystem metadata for improved performance using memory-mapped IO.

use rudu::cache::{load_cache, save_cache, CacheEntry};
use rudu::data::EntryType;
use std::collections::HashMap;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("rudu Memory-Mapped Cache Demo");
    println!("============================");

    // Create a new cache for the current directory
    let root_path = PathBuf::from(".");
    let mut cache = HashMap::new();

    println!("Creating cache for root path: {}", root_path.display());

    // Add some example entries with PathBuf keys
    let entries = vec![
        (
            PathBuf::from("src/main.rs"),
            CacheEntry::new(
                12345,
                PathBuf::from("src/main.rs"),
                1024,
                1234567890,
                2, // nlink
                Some(1),
                Some(1000),
                EntryType::File,
            ),
        ),
        (
            PathBuf::from("src/"),
            CacheEntry::new(
                67890,
                PathBuf::from("src/"),
                2048,
                1234567891,
                3, // nlink
                Some(5),
                Some(1000),
                EntryType::Dir,
            ),
        ),
        (
            PathBuf::from("Cargo.toml"),
            CacheEntry::new(
                54321,
                PathBuf::from("Cargo.toml"),
                512,
                1234567892,
                1, // nlink
                Some(1),
                Some(1001),
                EntryType::File,
            ),
        ),
    ];

    for (path, entry) in entries {
        cache.insert(path, entry);
    }

    println!("\nAdded {} entries to cache", cache.len());

    // Demonstrate memory-mapped cache saving
    println!("\nSaving cache using memory-mapped IO...");
    let start = std::time::Instant::now();
    save_cache(&root_path, &cache)?;
    let save_duration = start.elapsed();
    println!("Cache saved in {:?}", save_duration);

    // Demonstrate memory-mapped cache loading
    println!("\nLoading cache using memory-mapped IO...");
    let start = std::time::Instant::now();
    let loaded_cache = load_cache(&root_path, 604800).ok_or("Failed to load cache")?;
    let load_duration = start.elapsed();
    println!("Cache loaded in {:?}", load_duration);
    println!("Loaded cache has {} entries", loaded_cache.len());

    // Test entry retrieval by path
    if let Some(entry) = loaded_cache.get(&PathBuf::from("src/main.rs")) {
        println!("\nFound entry for 'src/main.rs':");
        println!("  Size: {} bytes", entry.size);
        println!("  Type: {}", entry.entry_type.as_str());
        println!("  Owner: {:?}", entry.owner);
        println!("  Inodes: {:?}", entry.inode_cnt);
        println!("  Path hash: {}", entry.path_hash);
    }

    // Test cache validity
    if let Some(entry) = loaded_cache.get(&PathBuf::from("src/main.rs")) {
        let is_valid = entry.is_valid(1234567890, 2); // mtime, nlink
        println!("  Entry is valid: {}", is_valid);

        let is_invalid = entry.is_valid(1234567891, 2); // different mtime
        println!("  Entry with different mtime is valid: {}", is_invalid);
    }

    // Demonstrate performance with a larger cache
    println!("\n\nPerformance Test with Larger Cache");
    println!("==================================");

    let mut large_cache = HashMap::new();
    let num_entries = 10000;

    // Create a large cache
    for i in 0..num_entries {
        let path = PathBuf::from(format!("file_{}.txt", i));
        let entry = CacheEntry::new(
            i as u64,
            path.clone(),
            (i * 1024) as u64,
            1234567890 + i as u64,
            1, // nlink
            Some(1),
            Some(1000),
            EntryType::File,
        );
        large_cache.insert(path, entry);
    }

    println!("Created large cache with {} entries", large_cache.len());

    // Save large cache
    let start = std::time::Instant::now();
    save_cache(&root_path, &large_cache)?;
    let save_duration = start.elapsed();
    println!("Large cache saved in {:?}", save_duration);

    // Load large cache
    let start = std::time::Instant::now();
    let loaded_large_cache = load_cache(&root_path, 604800).ok_or("Failed to load large cache")?;
    let load_duration = start.elapsed();
    println!(
        "Large cache loaded in {:?} (O(1) memory-mapped access)",
        load_duration
    );
    println!(
        "Loaded large cache has {} entries",
        loaded_large_cache.len()
    );

    // Verify a few entries
    if let Some(entry) = loaded_large_cache.get(&PathBuf::from("file_5000.txt")) {
        println!("\nVerified entry 'file_5000.txt':");
        println!("  Size: {} bytes", entry.size);
        println!("  Modification time: {}", entry.mtime);
    }

    // Clean up - remove cache files
    let cache_path = rudu::cache::model::Cache::get_cache_path(&root_path)?;
    if cache_path.exists() {
        std::fs::remove_file(&cache_path)?;
        println!("\nCleaned up cache file: {}", cache_path.display());
    }

    println!("\nDemo completed successfully!");
    println!("Memory-mapped IO provides O(1) load times for large caches.");

    Ok(())
}
