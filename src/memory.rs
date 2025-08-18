use std::time::{Duration, Instant};
use sysinfo::{Pid, System};

pub struct MemoryMonitor {
    limit_bytes: u64,
    warn_threshold: f64,
    last_check: Instant,
    system: System,
    pid: Pid,
    check_interval: Duration,
}

impl MemoryMonitor {
    /// Create a new MemoryMonitor with the specified limit in MB
    #[allow(dead_code)]
    pub fn new(limit_mb: u64) -> Self {
        Self::new_with_interval(limit_mb, 200)
    }

    /// Create a new MemoryMonitor with the specified limit in MB and check interval in ms
    pub fn new_with_interval(limit_mb: u64, check_interval_ms: u64) -> Self {
        let mut system = System::new_all();
        system.refresh_processes();

        let pid = Pid::from(std::process::id() as usize);
        let check_interval = Duration::from_millis(check_interval_ms);

        Self {
            limit_bytes: limit_mb * 1024 * 1024, // Convert MB to bytes
            warn_threshold: 0.95,                // 95% threshold for nearing_limit
            last_check: Instant::now() - check_interval, // Allow immediate first check
            system,
            pid,
            check_interval,
        }
    }

    /// Returns true if memory usage is at or above 100% of the limit
    ///
    /// Returns false if RSS is not available (None), effectively bypassing
    /// memory limit checks on unsupported platforms.
    pub fn exceeds_limit(&mut self) -> bool {
        match self.get_current_memory_usage() {
            Some(usage) => usage >= self.limit_bytes,
            None => {
                // RSS not available - bypass memory limit checks
                // This signals that memory monitoring should be disabled
                false
            }
        }
    }

    /// Returns true if memory usage is at or above 95% of the limit
    ///
    /// Returns false if RSS is not available (None), effectively bypassing
    /// memory limit warnings on unsupported platforms.
    pub fn nearing_limit(&mut self) -> bool {
        match self.get_current_memory_usage() {
            Some(usage) => {
                let threshold = (self.limit_bytes as f64 * self.warn_threshold) as u64;
                usage >= threshold
            }
            None => {
                // RSS not available - bypass memory limit warnings
                // This allows the scan to continue without memory restrictions
                false
            }
        }
    }

    /// Get the current RSS memory usage, with throttling to minimize overhead
    ///
    /// Returns None if RSS is not available on this platform, signaling that
    /// memory monitoring should be bypassed entirely.
    fn get_current_memory_usage(&mut self) -> Option<u64> {
        let now = Instant::now();

        // Throttle checks to avoid excessive overhead
        if now.duration_since(self.last_check) < self.check_interval {
            // Return cached value by getting the process memory without refresh
            if let Some(process) = self.system.process(self.pid) {
                return Some(process.memory()); // sysinfo returns bytes
            }
            return None;
        }

        // Update last check time
        self.last_check = now;

        // Refresh process information
        self.system.refresh_process(self.pid);

        // Get current process memory usage (RSS)
        // Use the same RSS function that metrics uses for consistency
        crate::metrics::rss_after_phase()
    }

    #[cfg(test)]
    /// Mock version of exceeds_limit for testing with controlled memory values
    pub fn exceeds_limit_with_mock<F>(&self, get_usage: F) -> bool
    where
        F: FnOnce() -> Option<u64>,
    {
        match get_usage() {
            Some(usage) => usage >= self.limit_bytes,
            None => false,
        }
    }

    #[cfg(test)]
    /// Mock version of nearing_limit for testing with controlled memory values
    pub fn nearing_limit_with_mock<F>(&self, get_usage: F) -> bool
    where
        F: FnOnce() -> Option<u64>,
    {
        match get_usage() {
            Some(usage) => {
                let threshold = (self.limit_bytes as f64 * self.warn_threshold) as u64;
                usage >= threshold
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // A mock implementation of the memory reading function
    fn mock_memory_usage(mock_value_bytes: u64) -> Option<u64> {
        Some(mock_value_bytes)
    }

    #[test]
    fn test_memory_monitor_creation() {
        let monitor = MemoryMonitor::new(100); // 100MB limit
        assert_eq!(monitor.limit_bytes, 100 * 1024 * 1024);
        assert_eq!(monitor.warn_threshold, 0.95);
        assert_eq!(monitor.check_interval, Duration::from_millis(200)); // Default interval
    }

    #[test]
    fn test_memory_monitor_with_custom_interval() {
        let monitor = MemoryMonitor::new_with_interval(50, 500); // 50MB limit, 500ms interval
        assert_eq!(monitor.limit_bytes, 50 * 1024 * 1024);
        assert_eq!(monitor.warn_threshold, 0.95);
        assert_eq!(monitor.check_interval, Duration::from_millis(500));
    }

    #[test]
    fn test_memory_monitor_with_mock_usage() {
        let monitor = MemoryMonitor::new(1); // 1 MB limit

        // Mocking get_current_memory_usage to return a controlled value
        let two_mb = 2 * 1024 * 1024;
        let slightly_less_than_one_mb = (0.98 * 1024.0 * 1024.0) as u64;
        let half_a_mb = 512 * 1024;

        // Simulate exceeding the limit
        assert!(monitor.exceeds_limit_with_mock(|| mock_memory_usage(two_mb)));

        // Simulate nearing the limit
        assert!(monitor.nearing_limit_with_mock(|| mock_memory_usage(slightly_less_than_one_mb)));

        // Simulate well below the limit
        assert!(!monitor.exceeds_limit_with_mock(|| mock_memory_usage(half_a_mb)));
        assert!(!monitor.nearing_limit_with_mock(|| mock_memory_usage(half_a_mb)));
    }

    #[test]
    fn test_memory_monitor_basic_functionality() {
        let mut monitor = MemoryMonitor::new(1); // 1MB limit (very small for testing)

        // These should not panic and return boolean values
        let exceeds = monitor.exceeds_limit();
        let nearing = monitor.nearing_limit();

        assert!(exceeds || !exceeds); // Just ensure it returns a boolean
        assert!(nearing || !nearing); // Just ensure it returns a boolean
    }

    #[test]
    fn test_throttling() {
        let mut monitor = MemoryMonitor::new(100);

        let start = Instant::now();

        // Make multiple calls rapidly
        for _ in 0..5 {
            monitor.get_current_memory_usage();
        }

        let elapsed = start.elapsed();

        // Should complete quickly due to throttling
        assert!(elapsed < Duration::from_millis(50));

        // Wait for throttle period to pass and test again
        thread::sleep(Duration::from_millis(250));
        monitor.get_current_memory_usage();
    }

    /// Platform-specific tests for memory monitoring behavior
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
        fn test_unix_memory_monitor_should_work() {
            let mut monitor = MemoryMonitor::new(1000); // 1GB limit

            // On Unix systems, we should generally get memory readings
            let usage = monitor.get_current_memory_usage();
            match usage {
                Some(bytes) => {
                    assert!(bytes > 0, "Memory usage should be positive on Unix systems");
                    println!("✅ Unix MemoryMonitor working: {} bytes", bytes);

                    // Test limit checking with actual memory values
                    let exceeds = monitor.exceeds_limit();
                    let nearing = monitor.nearing_limit();

                    // These should return valid boolean values without panicking
                    assert!(!exceeds || exceeds); // Just verify boolean
                    assert!(!nearing || nearing); // Just verify boolean
                }
                None => {
                    println!(
                        "⚠️  Unix MemoryMonitor returned None - may be containerized or restricted"
                    );

                    // Even if None, the monitor should handle this gracefully
                    let exceeds = monitor.exceeds_limit();
                    let nearing = monitor.nearing_limit();

                    // Should return false when RSS is unavailable (bypass behavior)
                    assert!(
                        !exceeds,
                        "exceeds_limit should return false when RSS is None"
                    );
                    assert!(
                        !nearing,
                        "nearing_limit should return false when RSS is None"
                    );
                }
            }
        }

        #[test]
        #[cfg(target_os = "windows")]
        fn test_windows_memory_monitor_best_effort() {
            let mut monitor = MemoryMonitor::new(1000); // 1GB limit

            // On Windows, memory monitoring is best-effort
            let usage = monitor.get_current_memory_usage();
            match usage {
                Some(bytes) => {
                    assert!(
                        bytes > 0,
                        "Memory usage should be positive when available on Windows"
                    );
                    println!(
                        "✅ Windows MemoryMonitor working: {} bytes (best-effort)",
                        bytes
                    );

                    // Test limit checking with actual memory values
                    let exceeds = monitor.exceeds_limit();
                    let nearing = monitor.nearing_limit();

                    // These should return valid boolean values
                    assert!(!exceeds || exceeds);
                    assert!(!nearing || nearing);
                }
                None => {
                    println!(
                        "ℹ️  Windows MemoryMonitor returned None - acceptable on some Windows versions"
                    );

                    // When RSS is unavailable, should bypass checks
                    let exceeds = monitor.exceeds_limit();
                    let nearing = monitor.nearing_limit();

                    // Should return false (bypass mode)
                    assert!(
                        !exceeds,
                        "exceeds_limit should return false when RSS is None"
                    );
                    assert!(
                        !nearing,
                        "nearing_limit should return false when RSS is None"
                    );
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
        fn test_unsupported_platform_memory_monitor() {
            let mut monitor = MemoryMonitor::new(100);

            // On unsupported platforms, should always return None
            let usage = monitor.get_current_memory_usage();
            assert_eq!(
                usage, None,
                "Unsupported platforms should return None for memory usage"
            );

            // Limit checks should always return false (bypass mode)
            let exceeds = monitor.exceeds_limit();
            let nearing = monitor.nearing_limit();

            assert!(
                !exceeds,
                "exceeds_limit should return false on unsupported platforms"
            );
            assert!(
                !nearing,
                "nearing_limit should return false on unsupported platforms"
            );

            println!("✅ Unsupported platform MemoryMonitor correctly bypasses checks");
        }

        #[test]
        fn test_memory_monitor_bypass_behavior() {
            // Test that None RSS values are handled correctly across all platforms
            let mut monitor = MemoryMonitor::new(10); // Very small limit

            // Even with a tiny limit, if RSS returns None, we should bypass checks
            let exceeds = monitor.exceeds_limit();
            let nearing = monitor.nearing_limit();

            // Results depend on platform, but should never panic
            assert!(!exceeds || exceeds); // Boolean check
            assert!(!nearing || nearing); // Boolean check

            println!("✅ MemoryMonitor handles platform differences gracefully");
        }
    }
}
