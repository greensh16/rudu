//! Data structures for representing file system entries.
//!
//! This module defines the core data structures used throughout the `rudu` application
//! for representing files and directories discovered during file system traversal.

use std::path::PathBuf;

/// Represents a file or directory entry discovered during scanning.
///
/// # Fields
/// * `path` - The full path to the file or directory
/// * `size` - Size in bytes
/// * `owner` - Optional owner (username) of the file/directory
/// * `inodes` - Optional number of inodes (files/subdirectories) for directories
/// * `entry_type` - Type of entry (file or directory)
/// * `atime` - Optional last-access time (Unix seconds); for directories this
///   is the *oldest* access time in the subtree, see [`crate::atime`]
/// * `at_risk_bytes` - Directories only: bytes under this directory belonging
///   to files older than the access-age policy threshold
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    pub owner: Option<String>,
    pub inodes: Option<u64>,
    pub entry_type: EntryType,
    /// Last access time in seconds since the Unix epoch.
    ///
    /// For a file this is its own `st_atime`; `None` if the stat failed. For a
    /// directory, [`crate::atime::apply_rollup`] replaces the scan's raw value
    /// with the *minimum* atime over every leaf in its subtree — the entry that
    /// will be purged first — or `None` when the subtree holds no leaves at all.
    ///
    /// Always populated by the scan (it costs nothing: the same `lstat` already
    /// supplies size and mtime). Whether it is *displayed* is `--show-atime`'s
    /// job.
    pub atime: Option<u64>,
    /// Bytes beneath this directory belonging to leaves at or past the
    /// access-age threshold (`--purge-days`). `None` for files, and for
    /// directories until [`crate::atime::apply_rollup`] has run.
    pub at_risk_bytes: Option<u64>,
}

/// Represents the type of file system entry.
///
/// # Variants
/// * `File` - A regular file
/// * `Dir` - A directory
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EntryType {
    File,
    Dir,
}

impl EntryType {
    /// Returns a string representation of the entry type.
    ///
    /// # Returns
    /// * `"FILE"` for `EntryType::File`
    /// * `"DIR"` for `EntryType::Dir`
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryType::File => "FILE",
            EntryType::Dir => "DIR",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_file_entry_creation() {
        let entry = FileEntry {
            path: PathBuf::from("/test/file.txt"),
            size: 1024,
            owner: Some("user".to_string()),
            inodes: None,
            entry_type: EntryType::File,
            atime: None,
            at_risk_bytes: None,
        };

        assert_eq!(entry.size, 1024);
        assert_eq!(entry.owner, Some("user".to_string()));
        assert_eq!(entry.entry_type, EntryType::File);
        assert_eq!(entry.entry_type.as_str(), "FILE");
    }

    #[test]
    fn test_entry_type_as_str() {
        assert_eq!(EntryType::File.as_str(), "FILE");
        assert_eq!(EntryType::Dir.as_str(), "DIR");
    }
}
