use std::env;
use std::sync::Mutex;
use tempfile::TempDir;

/// Serialises tests that read/write `RUDU_CACHE_DIR` to prevent races.
static CACHE_DIR_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard that points `RUDU_CACHE_DIR` at a fresh temporary directory
/// for the lifetime of the guard, then restores the previous value on drop.
///
/// # Example
/// ```
/// let _guard = TempCacheDir::new().unwrap();
/// // RUDU_CACHE_DIR now points to a unique temp dir.
/// // On drop it is restored to whatever it was before.
/// ```
pub struct TempCacheDir {
    _dir: TempDir,
    previous: Option<String>,
}

impl TempCacheDir {
    pub fn new() -> std::io::Result<Self> {
        let dir = tempfile::tempdir()?;
        let previous = env::var("RUDU_CACHE_DIR").ok();
        unsafe { env::set_var("RUDU_CACHE_DIR", dir.path()) };
        Ok(TempCacheDir { _dir: dir, previous })
    }
}

impl Drop for TempCacheDir {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => unsafe { env::set_var("RUDU_CACHE_DIR", v) },
            None => unsafe { env::remove_var("RUDU_CACHE_DIR") },
        }
    }
}

/// Convenience wrapper kept for compatibility with older call sites.
pub fn setup_temp_cache_dir() -> std::io::Result<TempCacheDir> {
    TempCacheDir::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_temp_cache_dir_sets_env_var() {
        let _lock = CACHE_DIR_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _guard = setup_temp_cache_dir().unwrap();

        let cache_dir = env::var("RUDU_CACHE_DIR").expect("RUDU_CACHE_DIR not set");
        assert!(!cache_dir.is_empty());
        assert!(std::path::Path::new(&cache_dir).exists());
    }

    #[test]
    fn test_env_var_is_restored_after_drop() {
        let _lock = CACHE_DIR_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        // Record the state before
        let before = env::var("RUDU_CACHE_DIR").ok();

        {
            let _guard = TempCacheDir::new().unwrap();
            // Inside the guard the var points somewhere temporary
            let during = env::var("RUDU_CACHE_DIR").ok();
            assert_ne!(during, before, "RUDU_CACHE_DIR should have changed");
        }

        // After drop it should be restored
        let after = env::var("RUDU_CACHE_DIR").ok();
        assert_eq!(after, before, "RUDU_CACHE_DIR should be restored after drop");
    }

    #[test]
    fn test_multiple_guards_create_different_dirs() {
        let _lock = CACHE_DIR_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        let guard1 = TempCacheDir::new().unwrap();
        let dir1 = env::var("RUDU_CACHE_DIR").unwrap();
        drop(guard1);

        let guard2 = TempCacheDir::new().unwrap();
        let dir2 = env::var("RUDU_CACHE_DIR").unwrap();
        drop(guard2);

        assert_ne!(dir1, dir2, "Each guard should use a distinct temp directory");
    }
}
