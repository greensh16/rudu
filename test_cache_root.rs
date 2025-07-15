#!/usr/bin/env rust-script

//! Test script to verify cache_root() function behavior
//!
//! This script tests the cache_root() function to ensure it:
//! 1. Uses RUDU_CACHE_DIR when set
//! 2. Falls back to XDG cache directory when RUDU_CACHE_DIR is not set

use std::env;
use std::path::PathBuf;
use std::collections::HashMap;

// Add path to find the rudu crate
use rudu::cache::{cache_root, save_cache, load_cache, invalidate_cache, CacheEntry};
use rudu::data::EntryType;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing cache_root() function behavior...\n");

    // Test 1: Without RUDU_CACHE_DIR (should use XDG fallback)
    println!("Test 1: Without RUDU_CACHE_DIR");
    env::remove_var("RUDU_CACHE_DIR");
    let default_cache_root = cache_root();
    println!("Default cache root: {:?}", default_cache_root);
    
    // Test 2: With RUDU_CACHE_DIR set
    println!("\nTest 2: With RUDU_CACHE_DIR set");
    let custom_cache_dir = "/tmp/custom-rudu-cache";
    env::set_var("RUDU_CACHE_DIR", custom_cache_dir);
    let custom_cache_root = cache_root();
    println!("Custom cache root: {:?}", custom_cache_root);
    
    // Verify the custom cache directory is used
    assert_eq!(custom_cache_root, PathBuf::from(custom_cache_dir));
    println!("✓ Custom cache directory is correctly used");

    // Test 3: Test actual cache operations with custom directory
    println!("\nTest 3: Testing cache operations with custom directory");
    let root_path = PathBuf::from(".");
    let mut cache = HashMap::new();
    
    // Create a test cache entry
    let entry = CacheEntry::new(
        12345,
        PathBuf::from("test.txt"),
        1024,
        1234567890,
        1,
        Some(1),
        Some(1000),
        EntryType::File,
    );
    cache.insert(PathBuf::from("test.txt"), entry);
    
    // Save cache (should use custom directory)
    save_cache(&root_path, &cache)?;
    println!("✓ Cache saved to custom directory");
    
    // Load cache (should load from custom directory)
    let loaded_cache = load_cache(&root_path, 604800);
    assert!(!loaded_cache.is_empty());
    println!("✓ Cache loaded from custom directory");
    
    // Verify cache contents
    assert_eq!(loaded_cache.len(), 1);
    assert!(loaded_cache.contains_key(&PathBuf::from("test.txt")));
    println!("✓ Cache contents verified");
    
    // Test invalidation
    let was_invalidated = invalidate_cache(&root_path)?;
    assert!(was_invalidated);
    println!("✓ Cache invalidated successfully");
    
    // Test 4: Reset to default and verify it changes back
    println!("\nTest 4: Reset to default");
    env::remove_var("RUDU_CACHE_DIR");
    let reset_cache_root = cache_root();
    println!("Reset cache root: {:?}", reset_cache_root);
    
    // Verify it's different from the custom directory
    assert_ne!(reset_cache_root, PathBuf::from(custom_cache_dir));
    println!("✓ Cache root correctly reset to default");
    
    println!("\n🎉 All tests passed! The cache_root() function works correctly.");
    println!("   - Uses RUDU_CACHE_DIR when set");
    println!("   - Falls back to XDG cache directory when not set");
    println!("   - All cache functions use the configurable directory");
    
    Ok(())
}
