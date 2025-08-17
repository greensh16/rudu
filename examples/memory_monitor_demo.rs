use rudu::cli::{Args, SortKey};
use rudu::memory::MemoryMonitor;
use rudu::scan::{MemoryLimitStatus, scan_files_and_dirs_with_memory_monitor};
use std::path::Path;
use std::sync::{Arc, Mutex};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a memory monitor with a 100MB limit
    let memory_monitor = Arc::new(Mutex::new(MemoryMonitor::new(100)));

    // Set up basic scanning args (this would typically come from CLI)
    let args = Args {
        path: Path::new(".").to_path_buf(),
        depth: Some(3),
        exclude: vec![],
        show_files: false,
        show_owner: false,
        show_inodes: false,
        sort: SortKey::Size,
        output: None,
        no_cache: false,
        cache_ttl: 24,
        profile: false,
        threads: None,
        threads_strategy: rudu::thread_pool::ThreadPoolStrategy::Default,
        memory_limit: Some(100),
        memory_check_interval_ms: 200,
    };

    // Create a simple exclude matcher (empty in this demo)
    let exclude_matcher = globset::GlobSetBuilder::new().build()?;

    // Run the scan with memory monitoring
    println!("🔍 Starting scan with memory monitoring...");
    let result = scan_files_and_dirs_with_memory_monitor(
        &args.path,
        &args,
        &exclude_matcher,
        args.sort,
        Some(memory_monitor.clone()),
    )?;

    // Check memory status
    match result.memory_status {
        MemoryLimitStatus::Normal => {
            println!("✅ Scan completed normally");
        }
        MemoryLimitStatus::NearingLimit => {
            println!("⚠️  Scan completed but memory was nearing limit (some features disabled)");
        }
        MemoryLimitStatus::MemoryLimitHit => {
            println!("🚨 Scan terminated early due to memory limit");
        }
    }

    println!("📊 Found {} entries", result.entries.len());
    println!(
        "📊 Cache: {}/{} hits",
        result.cache_hits, result.cache_total
    );

    // Show current memory usage
    if let Ok(mut monitor) = memory_monitor.lock() {
        let nearing = monitor.nearing_limit();
        let exceeds = monitor.exceeds_limit();
        println!("💾 Memory status: nearing={}, exceeds={}", nearing, exceeds);
    }

    // Show a few sample entries
    println!("\n📁 Sample entries:");
    for (i, entry) in result.entries.iter().take(5).enumerate() {
        println!(
            "  {}: {} ({} bytes) - {:?}",
            i + 1,
            entry.path.display(),
            entry.size,
            entry.entry_type
        );
    }

    Ok(())
}
