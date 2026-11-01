//! Comprehensive integration tests for strata_vfs.

use strata_vfs::prelude::*;
use strata_vfs::{clear_test_overrides, set_test_override};

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_vfs_path_normalization_and_traversal_prevention() {
    let base = app_data();
    assert_eq!(base.root(), VfsRoot::AppData);
    assert_eq!(base.to_uri(), "vfs://app_data/");

    // Attempt directory traversal
    let escape = base.join("../../etc/passwd");
    // Traversal components (`..`) must be neutralized
    assert!(!escape.subpath().to_string_lossy().contains(".."));
    assert_eq!(escape.to_uri(), "vfs://app_data/etc/passwd");

    // Joining nested paths
    let nested = base.join("projects").join("strata").join("build.json");
    assert_eq!(nested.to_uri(), "vfs://app_data/projects/strata/build.json");

    // URI round-trip
    let uri = "vfs://cache/shaders/metal/pbr.metallib";
    let parsed = VfsPath::from_uri(uri).expect("Failed to parse valid URI");
    assert_eq!(parsed.root(), VfsRoot::Cache);
    assert_eq!(parsed.to_uri(), uri);
}

#[test]
fn test_vfs_read_write_file_operations() {
    let _guard = TEST_LOCK.lock().unwrap();
    let test_dir = std::env::temp_dir().join("strata_vfs_test_rw");
    let _ = std::fs::remove_dir_all(&test_dir);
    set_test_override(VfsRoot::AppData, test_dir.clone());

    let file_path = app_data().join("config/app_settings.json");

    // Initial state
    let _ = remove_file(&file_path);
    assert!(!exists(&file_path));

    // Write file
    let test_data = "{\"theme\":\"system\",\"high_dpi\":true}";
    write_all(&file_path, test_data.as_bytes()).expect("Write failed");
    assert!(exists(&file_path));

    // Metadata
    let meta = metadata(&file_path).expect("Metadata query failed");
    assert!(meta.is_file);
    assert!(!meta.is_dir);
    assert_eq!(meta.len, test_data.len() as u64);

    // Read string
    let read_back = read_to_string(&file_path).expect("Read string failed");
    assert_eq!(read_back, test_data);

    // Read bytes
    let read_bytes = read_to_vec(&file_path).expect("Read bytes failed");
    assert_eq!(read_bytes, test_data.as_bytes());

    // Remove file
    remove_file(&file_path).expect("Remove file failed");
    assert!(!exists(&file_path));

    // Cleanup
    let _ = std::fs::remove_dir_all(&test_dir);
    clear_test_overrides();
}

#[test]
fn test_vfs_read_dir_and_nested_directories() {
    let _guard = TEST_LOCK.lock().unwrap();
    let test_dir = std::env::temp_dir().join("strata_vfs_test_dirs");
    let _ = std::fs::remove_dir_all(&test_dir);
    set_test_override(VfsRoot::Cache, test_dir.clone());

    let f1 = cache().join("assets/fonts/inter.ttf");
    let f2 = cache().join("assets/fonts/jetbrains_mono.ttf");
    let f3 = cache().join("assets/images/logo.png");

    write_all(&f1, b"FONT1").expect("Write f1 failed");
    write_all(&f2, b"FONT2").expect("Write f2 failed");
    write_all(&f3, b"PNG").expect("Write f3 failed");

    let fonts_dir = cache().join("assets/fonts");
    let entries = read_dir(&fonts_dir).expect("Read dir failed");
    assert_eq!(entries.len(), 2);

    let assets_dir = cache().join("assets");
    let asset_entries = read_dir(&assets_dir).expect("Read assets dir failed");
    assert_eq!(asset_entries.len(), 2); // fonts dir and images dir

    // Cleanup
    remove_dir_all(&cache().join("assets")).expect("Remove dir all failed");
    assert!(!exists(&f1));
    assert!(!exists(&f3));

    let _ = std::fs::remove_dir_all(&test_dir);
    clear_test_overrides();
}
