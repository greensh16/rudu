use std::env;
use tempfile::TempDir;

/// Sets up a unique temporary cache directory for each test to prevent cross-test interference.
/// This function creates a temporary directory and sets the RUDU_CACHE_DIR environment variable
/// to point to it, ensuring each test gets its own isolated cache space.
///
/// # Returns
/// A `TempDir` handle that should be kept alive for the duration of the test.
/// The temporary directory will be automatically cleaned up when the returned handle is dropped.
///
/// # Example
/// ```
/// use tests::util::setup_temp_cache_dir;
/// 
/// #[test]
/// fn my_test() {
///     let _temp_dir = setup_temp_cache_dir();
///     // Test code here - each test gets its own cache directory
/// }
/// ```
pub fn setup_temp_cache_dir() -> std::io::Result<TempDir> {
    let dir = tempfile::tempdir()?;
    env::set_var("RUDU_CACHE_DIR", dir.path());
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_setup_temp_cache_dir() {
        let _temp_dir = setup_temp_cache_dir().unwrap();
        
        // Check that the environment variable was set
        let cache_dir = env::var("RUDU_CACHE_DIR").unwrap();
        assert!(!cache_dir.is_empty());
        
        // Check that the directory exists
        assert!(std::path::Path::new(&cache_dir).exists());
    }
    
    #[test]
    fn test_multiple_calls_create_different_dirs() {
        let _temp_dir1 = setup_temp_cache_dir().unwrap();
        let cache_dir1 = env::var("RUDU_CACHE_DIR").unwrap();
        
        let _temp_dir2 = setup_temp_cache_dir().unwrap();
        let cache_dir2 = env::var("RUDU_CACHE_DIR").unwrap();
        
        // Each call should create a different directory
        assert_ne!(cache_dir1, cache_dir2);
    }
}
