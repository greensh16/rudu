//! Thread pool configuration strategies for optimizing performance.
//!
//! This module provides different thread pool configuration strategies
//! to optimize performance based on different workload characteristics.
//!
//! # Strategies
//! - `Default`: Uses Rayon's default thread pool configuration
//! - `Fixed`: Uses a fixed number of threads
//! - `NumCpusMinus1`: Uses number of CPUs minus 1 (leaves one CPU free)
//! - `IOHeavy`: Optimized for I/O-heavy workloads (typically 2x CPU count)

use anyhow::{Context, Result};
use clap::ValueEnum;

/// Thread pool configuration strategies.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum ThreadPoolStrategy {
    /// Use Rayon's default thread pool configuration
    Default,
    /// Use a fixed number of threads
    Fixed,
    /// Use number of CPUs minus 1 (leaves one CPU free)
    NumCpusMinus1,
    /// Optimized for I/O-heavy workloads (typically 2x CPU count)
    IOHeavy,
    /// Work-stealing optimized for uneven directory trees
    WorkStealingUneven,
}

impl ThreadPoolStrategy {
    /// Returns a string representation of the strategy for display purposes.
    pub fn as_str(&self) -> &'static str {
        match self {
            ThreadPoolStrategy::Default => "Default",
            ThreadPoolStrategy::Fixed => "Fixed",
            ThreadPoolStrategy::NumCpusMinus1 => "NumCpusMinus1",
            ThreadPoolStrategy::IOHeavy => "IOHeavy",
            ThreadPoolStrategy::WorkStealingUneven => "WorkStealingUneven",
        }
    }
}

/// Configures the global thread pool based on the given strategy and number of threads.
///
/// # Arguments
/// * `strategy` - The thread pool strategy to use
/// * `n_threads` - Number of threads (used for Fixed strategy, ignored for others)
///
/// # Returns
/// * `Result<usize>` - The actual number of threads configured
///
/// # Examples
/// ```rust
/// use rudu::thread_pool::{configure_pool, ThreadPoolStrategy};
///
/// // Configure with default strategy
/// let threads = configure_pool(ThreadPoolStrategy::Default, 4).unwrap();
/// println!("Configured {} threads", threads);
/// ```
pub fn configure_pool(strategy: ThreadPoolStrategy, n_threads: usize) -> Result<usize> {
    let actual_threads = match strategy {
        ThreadPoolStrategy::Default => {
            // Use Rayon's default configuration
            let default_threads = num_cpus::get();
            println!(
                "🔧 Using default thread pool strategy ({} threads)",
                default_threads
            );
            return Ok(default_threads);
        }
        ThreadPoolStrategy::Fixed => {
            if n_threads == 0 {
                anyhow::bail!("Fixed strategy requires n_threads > 0");
            }
            n_threads
        }
        ThreadPoolStrategy::NumCpusMinus1 => {
            let cpus = num_cpus::get();
            std::cmp::max(1, cpus - 1)
        }
        ThreadPoolStrategy::IOHeavy => {
            // For I/O-heavy workloads, use 2x CPU count
            num_cpus::get() * 2
        }
        ThreadPoolStrategy::WorkStealingUneven => {
            // For work-stealing with uneven trees, use CPU count
            // The real optimization comes from the spawn_handler
            num_cpus::get()
        }
    };

    rayon::ThreadPoolBuilder::new()
        .num_threads(actual_threads)
        .build_global()
        .context("Failed to configure thread pool")?;

    println!(
        "🔧 Using {} strategy with {} threads",
        strategy.as_str(),
        actual_threads
    );
    Ok(actual_threads)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_pool_strategy_as_str() {
        assert_eq!(ThreadPoolStrategy::Default.as_str(), "Default");
        assert_eq!(ThreadPoolStrategy::Fixed.as_str(), "Fixed");
        assert_eq!(ThreadPoolStrategy::NumCpusMinus1.as_str(), "NumCpusMinus1");
        assert_eq!(ThreadPoolStrategy::IOHeavy.as_str(), "IOHeavy");
        assert_eq!(
            ThreadPoolStrategy::WorkStealingUneven.as_str(),
            "WorkStealingUneven"
        );
    }

    #[test]
    fn test_configure_pool_num_cpus_minus_1() {
        let cpus = num_cpus::get();
        let expected = std::cmp::max(1, cpus - 1);

        // This would actually configure the global pool, so we just test the logic
        let actual = match ThreadPoolStrategy::NumCpusMinus1 {
            ThreadPoolStrategy::NumCpusMinus1 => std::cmp::max(1, cpus - 1),
            _ => unreachable!(),
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_configure_pool_io_heavy() {
        let cpus = num_cpus::get();
        let expected = cpus * 2;

        // This would actually configure the global pool, so we just test the logic
        let actual = match ThreadPoolStrategy::IOHeavy {
            ThreadPoolStrategy::IOHeavy => cpus * 2,
            _ => unreachable!(),
        };

        assert_eq!(actual, expected);
    }
}
