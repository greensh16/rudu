use rudu::cli::{Args, SortKey};
use rudu::scan::scan_files_and_dirs;
use rudu::utils::{build_exclude_matcher, expand_exclude_patterns};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_inode_counting_with_tempdir() {
    // Create a temporary directory structure for testing
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root_path = temp_dir.path();

    // Create test directory structure:
    // temp/
    // ├── dir1/
    // │   ├── file1.txt
    // │   └── file2.txt
    // ├── dir2/
    // │   ├── subdir/
    // │   │   └── file3.txt
    // │   └── file4.txt
    // └── file5.txt

    let dir1 = root_path.join("dir1");
    let dir2 = root_path.join("dir2");
    let subdir = dir2.join("subdir");

    fs::create_dir(&dir1).expect("Failed to create dir1");
    fs::create_dir(&dir2).expect("Failed to create dir2");
    fs::create_dir(&subdir).expect("Failed to create subdir");

    // Create files
    fs::write(dir1.join("file1.txt"), "content1").expect("Failed to write file1");
    fs::write(dir1.join("file2.txt"), "content2").expect("Failed to write file2");
    fs::write(subdir.join("file3.txt"), "content3").expect("Failed to write file3");
    fs::write(dir2.join("file4.txt"), "content4").expect("Failed to write file4");
    fs::write(root_path.join("file5.txt"), "content5").expect("Failed to write file5");

    // Set up args for scanning
    let args = Args {
        path: root_path.to_path_buf(),
        depth: None,
        sort: SortKey::Name,
        show_files: true,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher =
        build_exclude_matcher(&exclude_patterns).expect("Failed to build exclude matcher");

    // Scan the directory
    let entries = scan_files_and_dirs(root_path, &args, &exclude_matcher, args.sort)
        .expect("Failed to scan directory");

    // Verify the results
    assert!(!entries.is_empty());

    // Find directory entries and verify inode counts
    let dir1_entry = entries
        .iter()
        .find(|e| e.path == dir1)
        .expect("dir1 not found");
    let dir2_entry = entries
        .iter()
        .find(|e| e.path == dir2)
        .expect("dir2 not found");
    let subdir_entry = entries
        .iter()
        .find(|e| e.path == subdir)
        .expect("subdir not found");

    // dir1 should have 2 files (inodes = 2)
    assert_eq!(dir1_entry.inodes, Some(2));

    // dir2 should have 2 entries: subdir and file4.txt (inodes = 2)
    assert_eq!(dir2_entry.inodes, Some(2));

    // subdir should have 1 file (inodes = 1)
    assert_eq!(subdir_entry.inodes, Some(1));

    // Verify all files are present
    let file_paths: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == rudu::data::EntryType::File)
        .map(|e| e.path.file_name().unwrap().to_str().unwrap())
        .collect();

    assert!(file_paths.contains(&"file1.txt"));
    assert!(file_paths.contains(&"file2.txt"));
    assert!(file_paths.contains(&"file3.txt"));
    assert!(file_paths.contains(&"file4.txt"));
    assert!(file_paths.contains(&"file5.txt"));
}

#[test]
fn test_exclude_patterns_with_tempdir() {
    // Create a temporary directory structure for testing exclusion
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root_path = temp_dir.path();

    // Create test directory structure:
    // temp/
    // ├── node_modules/
    // │   └── package.json
    // ├── src/
    // │   └── main.rs
    // └── target/
    //     └── debug/
    //         └── app

    let node_modules = root_path.join("node_modules");
    let src = root_path.join("src");
    let target = root_path.join("target");
    let debug = target.join("debug");

    fs::create_dir(&node_modules).expect("Failed to create node_modules");
    fs::create_dir(&src).expect("Failed to create src");
    fs::create_dir(&target).expect("Failed to create target");
    fs::create_dir(&debug).expect("Failed to create debug");

    fs::write(node_modules.join("package.json"), "{}").expect("Failed to write package.json");
    fs::write(src.join("main.rs"), "fn main() {}").expect("Failed to write main.rs");
    fs::write(debug.join("app"), "binary").expect("Failed to write app");

    // Set up args with exclusions
    let args = Args {
        path: root_path.to_path_buf(),
        depth: None,
        sort: SortKey::Name,
        show_files: true,
        exclude: vec!["node_modules".to_string(), "target".to_string()],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher =
        build_exclude_matcher(&exclude_patterns).expect("Failed to build exclude matcher");

    // Scan the directory
    let entries = scan_files_and_dirs(root_path, &args, &exclude_matcher, args.sort)
        .expect("Failed to scan directory");

    // Verify that excluded directories are not present
    let paths: Vec<_> = entries.iter().map(|e| &e.path).collect();

    assert!(!paths.contains(&&node_modules));
    assert!(!paths.contains(&&target));
    assert!(!paths.contains(&&debug));

    // Verify that src directory is present
    assert!(paths.contains(&&src));

    // Verify that files in excluded directories are not present
    let file_names: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == rudu::data::EntryType::File)
        .map(|e| e.path.file_name().unwrap().to_str().unwrap())
        .collect();

    assert!(!file_names.contains(&"package.json"));
    assert!(!file_names.contains(&"app"));
    assert!(file_names.contains(&"main.rs"));
}

#[test]
fn test_depth_filtering_with_tempdir() {
    // Create a temporary directory structure for testing depth filtering
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root_path = temp_dir.path();

    // Create test directory structure:
    // temp/                        (depth 0)
    // ├── level1/                  (depth 1)
    // │   ├── file_at_level2.txt   (depth 2) - in level1/
    // │   └── level2/              (depth 2)
    // │       └── level3/          (depth 3)
    // │           └── deep_file.txt (depth 4)
    // └── file_at_level1.txt       (depth 1)

    let level1 = root_path.join("level1");
    let level2 = level1.join("level2");
    let level3 = level2.join("level3");

    fs::create_dir(&level1).expect("Failed to create level1");
    fs::create_dir(&level2).expect("Failed to create level2");
    fs::create_dir(&level3).expect("Failed to create level3");

    fs::write(level3.join("deep_file.txt"), "deep content").expect("Failed to write deep_file.txt");
    fs::write(level1.join("file_at_level2.txt"), "level2 content")
        .expect("Failed to write file_at_level2.txt");
    fs::write(root_path.join("file_at_level1.txt"), "level1 content")
        .expect("Failed to write file_at_level1.txt");

    // Test with depth limit of 2
    let args = Args {
        path: root_path.to_path_buf(),
        depth: Some(2),
        sort: SortKey::Name,
        show_files: true,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: true,
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher =
        build_exclude_matcher(&exclude_patterns).expect("Failed to build exclude matcher");

    // Scan the directory
    let entries = scan_files_and_dirs(root_path, &args, &exclude_matcher, args.sort)
        .expect("Failed to scan directory");

    // Filter entries using our utility function
    let filtered_entries =
        rudu::utils::filter_by_depth(&entries, root_path, args.depth, args.show_files);

    // Verify that level3 directory is not included (depth > 2)
    let paths: Vec<_> = filtered_entries.iter().map(|e| &e.path).collect();
    assert!(!paths.contains(&&level3));

    // Verify that level1 and level2 directories are included
    assert!(paths.contains(&&level1));
    assert!(paths.contains(&&level2));

    // Verify that files at the target depth are included
    let file_names: Vec<_> = filtered_entries
        .iter()
        .filter(|e| e.entry_type == rudu::data::EntryType::File)
        .map(|e| e.path.file_name().unwrap().to_str().unwrap())
        .collect();
    assert!(file_names.contains(&"file_at_level2.txt"));
    assert!(!file_names.contains(&"deep_file.txt")); // This is at depth 3
}

#[test]
fn test_size_calculation_with_tempdir() {
    // Create a temporary directory structure for testing size calculation
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root_path = temp_dir.path();

    // Create test files with different disk block usage
    // On most filesystems, blocks are 4KB, so we need files that span multiple blocks
    let file1_content = "a".repeat(4000); // ~4KB - fits in 1 block
    let file2_content = "b".repeat(8000); // ~8KB - needs 2 blocks

    fs::write(root_path.join("file1.txt"), &file1_content).expect("Failed to write file1");
    fs::write(root_path.join("file2.txt"), &file2_content).expect("Failed to write file2");

    // Set up args for scanning
    let args = Args {
        path: root_path.to_path_buf(),
        depth: None,
        sort: SortKey::Size,
        show_files: true,
        exclude: vec![],
        show_owner: false,
        output: None,
        threads: None,
        show_inodes: false,
    };

    let exclude_patterns = expand_exclude_patterns(&args.exclude);
    let exclude_matcher =
        build_exclude_matcher(&exclude_patterns).expect("Failed to build exclude matcher");

    // Scan the directory
    let entries = scan_files_and_dirs(root_path, &args, &exclude_matcher, args.sort)
        .expect("Failed to scan directory");

    // Find file entries
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == rudu::data::EntryType::File)
        .collect();

    // Verify files are sorted by size (largest first)
    assert!(file_entries.len() >= 2);

    // Due to sorting by size, file2 (~8KB disk usage) should come before file1 (~4KB disk usage)
    let file2_entry = file_entries
        .iter()
        .find(|e| e.path.file_name().unwrap() == "file2.txt")
        .unwrap();
    let file1_entry = file_entries
        .iter()
        .find(|e| e.path.file_name().unwrap() == "file1.txt")
        .unwrap();

    assert!(file2_entry.size > file1_entry.size);
}
