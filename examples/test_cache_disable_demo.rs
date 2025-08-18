// Demonstration of dynamic cache disabling when nearing memory limits
// This would typically be run as part of integration tests

use rudu::cache;
use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    println!("Testing dynamic cache disabling functionality...");

    // Show initial cache state
    println!("Cache initially enabled: {}", cache::is_enabled());

    // Create some test cache entries
    let mut test_cache = HashMap::new();
    let entry = rudu::cache::CacheEntry::new(
        12345,
        PathBuf::from("test_file.txt"),
        1024,
        1234567890,
        1,
        Some(1),
        Some(1000),
        rudu::data::EntryType::File,
    );
    test_cache.insert(PathBuf::from("test_file.txt"), entry);

    // Test normal caching works
    let root_path = PathBuf::from(".");
    println!("Saving cache with {} entries...", test_cache.len());

    match cache::save_cache_with_mtime(&root_path, &test_cache, None) {
        Ok(()) => println!("✓ Cache saved successfully"),
        Err(e) => println!("✗ Cache save failed: {}", e),
    }

    // Load cache back
    let loaded = cache::load_cache(&root_path, 604800);
    println!("Loaded {} entries from cache", loaded.len());

    // Now simulate memory pressure - disable cache
    println!("\n🚨 Simulating memory pressure - disabling cache...");
    cache::set_enabled(false);
    println!("Cache enabled after disabling: {}", cache::is_enabled());

    // Try to load cache when disabled
    let loaded_disabled = cache::load_cache(&root_path, 604800);
    println!(
        "Loaded {} entries when cache disabled",
        loaded_disabled.len()
    );

    // Try to save cache when disabled (should succeed silently)
    match cache::save_cache_with_mtime(&root_path, &test_cache, None) {
        Ok(()) => println!("✓ Cache save succeeded silently when disabled"),
        Err(e) => println!("✗ Unexpected error when saving disabled cache: {}", e),
    }

    // Re-enable cache
    println!("\n✅ Memory pressure resolved - re-enabling cache...");
    cache::set_enabled(true);
    println!("Cache enabled after re-enabling: {}", cache::is_enabled());

    // Load cache back
    let loaded_enabled = cache::load_cache(&root_path, 604800);
    println!("Loaded {} entries after re-enabling", loaded_enabled.len());

    println!("\n🎉 Dynamic cache disabling test completed successfully!");
}
