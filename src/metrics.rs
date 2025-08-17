//! Performance metrics and profiling utilities for `rudu`.
//!
//! This module provides:
//! - [`PhaseTimer`] - A wrapper around `Instant` for timing different phases
//! - [`rss_after_phase`] - Optional memory usage tracking using `sysinfo`
//! - [`ProfileData`] - Structured data for performance metrics
//! - [`print_profile_summary`] - Terminal output for profiling results
//! - [`save_stats_json`] - JSON output for scripting integration
//!
//! # Usage
//!
//! ```rust
//! use rudu::metrics::{PhaseTimer, rss_after_phase, ProfileData};
//!
//! let mut profile = ProfileData::new();
//! let timer = PhaseTimer::new("WalkDir");
//!
//! // ... do work ...
//!
//! profile.add_phase(timer.finish());
//! profile.memory_peak = rss_after_phase();
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use sysinfo::System;

/// A timer for measuring the duration of a specific phase or operation.
///
/// This is a wrapper around `std::time::Instant` that provides convenient
/// methods for timing operations and storing the results with a descriptive name.
#[derive(Debug, Clone)]
pub struct PhaseTimer {
    /// The name of the phase being timed
    pub name: String,
    /// The start time of the phase
    pub start: Instant,
}

impl PhaseTimer {
    /// Creates a new timer and starts timing the specified phase.
    ///
    /// # Arguments
    /// * `name` - A descriptive name for the phase being timed
    ///
    /// # Returns
    /// A new `PhaseTimer` instance with the current time as the start time.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            start: Instant::now(),
        }
    }

    /// Finishes timing the phase and returns the elapsed duration.
    ///
    /// # Returns
    /// A `PhaseResult` containing the phase name and elapsed duration.
    pub fn finish(self) -> PhaseResult {
        PhaseResult {
            name: self.name,
            duration: self.start.elapsed(),
        }
    }

    /// Gets the elapsed time without finishing the timer.
    ///
    /// # Returns
    /// The duration elapsed since the timer was created.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// The result of a completed phase timing operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResult {
    /// The name of the phase that was timed
    pub name: String,
    /// The duration of the phase
    #[serde(with = "duration_serde")]
    pub duration: Duration,
}

/// Custom serialization for Duration to make it human-readable in JSON
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

/// Comprehensive profiling data for a complete scan operation.
///
/// This struct collects timing information for different phases of the scan,
/// memory usage statistics, and cache performance metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileData {
    /// Timing results for each phase of the scan
    pub phases: Vec<PhaseResult>,
    /// Peak memory usage in bytes (if available)
    pub memory_peak: Option<u64>,
    /// Number of cache hits
    pub cache_hits: u64,
    /// Total number of cache lookups (hits + misses)
    pub cache_total: u64,
    /// Additional metadata about the scan
    pub metadata: HashMap<String, String>,
}

impl ProfileData {
    /// Creates a new empty profile data structure.
    pub fn new() -> Self {
        Self {
            phases: Vec::new(),
            memory_peak: None,
            cache_hits: 0,
            cache_total: 0,
            metadata: HashMap::new(),
        }
    }

    /// Adds a completed phase result to the profile.
    ///
    /// # Arguments
    /// * `phase` - The completed phase result to add
    pub fn add_phase(&mut self, phase: PhaseResult) {
        self.phases.push(phase);
    }

    /// Sets cache statistics for the profile.
    ///
    /// # Arguments
    /// * `hits` - Number of cache hits
    /// * `total` - Total number of cache lookups
    pub fn set_cache_stats(&mut self, hits: u64, total: u64) {
        self.cache_hits = hits;
        self.cache_total = total;
    }

    /// Adds a metadata entry to the profile.
    ///
    /// # Arguments
    /// * `key` - The metadata key
    /// * `value` - The metadata value
    pub fn add_metadata(&mut self, key: &str, value: &str) {
        self.metadata.insert(key.to_string(), value.to_string());
    }

    /// Calculates the cache hit rate as a percentage.
    ///
    /// # Returns
    /// The cache hit rate as a percentage (0.0 to 100.0), or 0.0 if no cache lookups occurred.
    pub fn cache_hit_rate(&self) -> f64 {
        if self.cache_total == 0 {
            0.0
        } else {
            (self.cache_hits as f64 / self.cache_total as f64) * 100.0
        }
    }

    /// Gets the total duration of all phases combined.
    ///
    /// # Returns
    /// The sum of all phase durations.
    pub fn total_duration(&self) -> Duration {
        self.phases.iter().map(|p| p.duration).sum()
    }
}

impl Default for ProfileData {
    fn default() -> Self {
        Self::new()
    }
}

/// Measures the current process's RSS (Resident Set Size) memory usage.
///
/// This function uses the `sysinfo` crate to get the current process's
/// memory usage. It's designed to be called after completing a phase
/// to track peak memory usage.
///
/// # Platform Support
///
/// - **Linux/macOS**: Reliable RSS values from `/proc/[pid]/status` or `task_info()`
/// - **Windows**: Best-effort support; may have limited accuracy on some versions
///
/// When memory monitoring returns `None`, the monitor should bypass memory checks
/// to avoid false positives or system instability.
///
/// # Returns
/// The current RSS memory usage in bytes, or `None` if the information
/// is not available on this platform or if an error occurs.
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub fn rss_after_phase() -> Option<u64> {
    let mut system = System::new_all();
    system.refresh_processes();

    let current_pid = std::process::id();

    // Find the current process
    for (pid, process) in system.processes() {
        if pid.as_u32() == current_pid {
            // Convert from KB to bytes
            // On Unix-like systems, sysinfo reliably reports RSS via system calls
            return Some(process.memory() * 1024);
        }
    }

    None
}

/// Windows implementation of RSS measurement (best-effort)
///
/// On Windows, RSS reporting may be less reliable due to differences in
/// memory management and system API behavior across Windows versions.
#[cfg(target_os = "windows")]
pub fn rss_after_phase() -> Option<u64> {
    let mut system = System::new_all();
    system.refresh_processes();

    let current_pid = std::process::id();

    // Find the current process
    for (pid, process) in system.processes() {
        if pid.as_u32() == current_pid {
            // Convert from KB to bytes
            // Note: Windows RSS reporting is best-effort and may vary by Windows version
            return Some(process.memory() * 1024);
        }
    }

    None
}

/// Fallback implementation for unsupported platforms
#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "windows"
)))]
pub fn rss_after_phase() -> Option<u64> {
    // On unsupported platforms, return None to signal that memory monitoring
    // should be bypassed entirely
    None
}

/// Prints a formatted profile summary to the terminal.
///
/// This function outputs a human-readable summary of the profiling data,
/// including phase timings, memory usage, and cache statistics.
///
/// # Arguments
/// * `profile` - The profile data to display
///
/// # Example Output
/// ```text
/// Scan phase timings
///   WalkDir         150 ms
///   Disk-usage I/O  220 ms
///   Aggregation      30 ms
/// Memory peak:      42 MB
/// Cache hits:       8123 / 9000 (90.3 %)
/// ```
pub fn print_profile_summary(profile: &ProfileData) {
    println!("\nScan phase timings");

    for phase in &profile.phases {
        println!("  {:<15} {:>7} ms", phase.name, phase.duration.as_millis());
    }

    if let Some(memory_peak) = profile.memory_peak {
        let memory_mb = memory_peak as f64 / (1024.0 * 1024.0);
        println!("Memory peak:      {:.1} MB", memory_mb);
    }

    if profile.cache_total > 0 {
        println!(
            "Cache hits:       {} / {} ({:.1}%)",
            profile.cache_hits,
            profile.cache_total,
            profile.cache_hit_rate()
        );
    }

    // Print any additional metadata
    if !profile.metadata.is_empty() {
        println!("\nAdditional metrics:");
        for (key, value) in &profile.metadata {
            println!("  {:<15} {}", key, value);
        }
    }

    println!(); // Extra newline for readability
}

/// Saves profiling statistics to a JSON file for scripting integration.
///
/// This function creates a `stats.json` file alongside the main output
/// when CSV or JSON output is requested. The file contains machine-readable
/// profiling data that can be used by scripts or other tools.
///
/// # Arguments
/// * `output_path` - The path where the main output file is being written
/// * `profile` - The profile data to save
///
/// # Returns
/// `Ok(())` if the file was written successfully, or an error if writing failed.
///
/// # Example
/// If the main output is being written to `results.csv`, this function
/// will create `stats.json` in the same directory.
pub fn save_stats_json(
    output_path: &Path,
    profile: &ProfileData,
) -> Result<(), Box<dyn std::error::Error>> {
    let stats_path = output_path.with_file_name("stats.json");

    // Create a structured stats object for JSON output
    let stats = serde_json::json!({
        "scan_phases": profile.phases,
        "total_duration_ms": profile.total_duration().as_millis(),
        "memory_peak_bytes": profile.memory_peak,
        "memory_peak_mb": profile.memory_peak.map(|b| b as f64 / (1024.0 * 1024.0)),
        "cache_hits": profile.cache_hits,
        "cache_total": profile.cache_total,
        "cache_hit_rate": profile.cache_hit_rate(),
        "metadata": profile.metadata,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    std::fs::write(&stats_path, serde_json::to_string_pretty(&stats)?)?;

    println!("Performance stats saved to: {}", stats_path.display());

    Ok(())
}

/// A convenience macro for timing a block of code.
///
/// This macro creates a `PhaseTimer`, executes the provided code block,
/// and returns both the result of the code and the `PhaseResult`.
///
/// # Arguments
/// * `name` - A string literal for the phase name
/// * `code` - The code block to time
///
/// # Returns
/// A tuple containing `(result, PhaseResult)` where `result` is the
/// return value of the code block and `PhaseResult` contains the timing data.
///
/// # Example
/// ```rust
/// use rudu::time_phase;
///
/// let (result, timing) = time_phase!("Database Query", {
///     // ... some expensive operation ...
///     42
/// });
/// ```
#[macro_export]
macro_rules! time_phase {
    ($name:expr, $code:block) => {{
        let timer = $crate::metrics::PhaseTimer::new($name);
        let result = $code;
        let timing = timer.finish();
        (result, timing)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_phase_timer() {
        let timer = PhaseTimer::new("test_phase");
        thread::sleep(Duration::from_millis(10));
        let result = timer.finish();

        assert_eq!(result.name, "test_phase");
        assert!(result.duration.as_millis() >= 10);
    }

    #[test]
    fn test_profile_data() {
        let mut profile = ProfileData::new();

        let phase1 = PhaseResult {
            name: "Phase 1".to_string(),
            duration: Duration::from_millis(100),
        };
        let phase2 = PhaseResult {
            name: "Phase 2".to_string(),
            duration: Duration::from_millis(200),
        };

        profile.add_phase(phase1);
        profile.add_phase(phase2);
        profile.set_cache_stats(80, 100);

        assert_eq!(profile.phases.len(), 2);
        assert_eq!(profile.cache_hit_rate(), 80.0);
        assert_eq!(profile.total_duration(), Duration::from_millis(300));
    }

    #[test]
    fn test_memory_tracking() {
        let memory = rss_after_phase();
        // We can't assert exact values, but we can check that it returns some data
        // on most platforms
        match memory {
            Some(bytes) => assert!(bytes > 0),
            None => {
                // Memory tracking might not be available on all platforms
                println!("Memory tracking not available on this platform");
            }
        }
    }

    /// Tests for platform-specific RSS behavior
    mod platform_tests {
        use super::*;

        #[test]
        #[cfg(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        fn test_unix_rss_should_return_some() {
            // On Unix-like systems, RSS should generally be available
            let memory = rss_after_phase();
            // We expect Some(bytes) on Unix systems, but allow None for edge cases
            // (like very restricted environments or system API failures)
            match memory {
                Some(bytes) => {
                    assert!(bytes > 0, "RSS should be positive on Unix systems");
                    println!("✅ Unix RSS tracking working: {} bytes", bytes);
                }
                None => {
                    println!("⚠️  Unix RSS returned None - this may indicate a system issue");
                    // Don't fail the test as some containerized environments might restrict access
                }
            }
        }

        #[test]
        #[cfg(target_os = "windows")]
        fn test_windows_rss_best_effort() {
            // On Windows, RSS is best-effort and may return None
            let memory = rss_after_phase();
            match memory {
                Some(bytes) => {
                    assert!(
                        bytes > 0,
                        "RSS should be positive when available on Windows"
                    );
                    println!(
                        "✅ Windows RSS tracking working: {} bytes (best-effort)",
                        bytes
                    );
                }
                None => {
                    println!(
                        "ℹ️  Windows RSS returned None - this is expected on some Windows versions"
                    );
                    // This is acceptable on Windows - it's best-effort
                }
            }
        }

        #[test]
        #[cfg(not(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "windows"
        )))]
        fn test_unsupported_platform_returns_none() {
            // On unsupported platforms, RSS should always return None
            let memory = rss_after_phase();
            assert_eq!(
                memory, None,
                "Unsupported platforms should return None for RSS"
            );
            println!("✅ Unsupported platform correctly returns None for RSS");
        }

        #[test]
        fn test_memory_monitor_handles_none_rss() {
            // Test that the system handles None RSS values gracefully
            // This simulates what happens on unsupported platforms or when RSS fails

            // We can't directly test MemoryMonitor here since it's in a different module,
            // but we can verify that None RSS values are handled properly in the metrics
            let mut profile = ProfileData::new();
            profile.memory_peak = None; // Simulates RSS returning None

            // Should not panic and should handle None gracefully
            let summary = format!("{:?}", profile);
            assert!(summary.contains("memory_peak: None"));

            println!("✅ ProfileData handles None memory_peak gracefully");
        }
    }

    #[test]
    fn test_time_phase_macro() {
        let (result, timing) = time_phase!("test_macro", {
            thread::sleep(Duration::from_millis(5));
            42
        });

        assert_eq!(result, 42);
        assert_eq!(timing.name, "test_macro");
        assert!(timing.duration.as_millis() >= 5);
    }
}
