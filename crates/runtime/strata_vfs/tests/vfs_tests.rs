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

#[test]
#[cfg(unix)]
fn test_vfs_symlink_traversal_prevention_exploit_3a() {
    let _guard = TEST_LOCK.lock().unwrap();
    let test_dir = std::env::temp_dir().join("strata_vfs_test_symlinks");
    let outside_dir = std::env::temp_dir().join("strata_vfs_outside_secret");
    let _ = std::fs::remove_dir_all(&test_dir);
    let _ = std::fs::remove_dir_all(&outside_dir);

    std::fs::create_dir_all(&test_dir).unwrap();
    std::fs::create_dir_all(&outside_dir).unwrap();

    let secret_file = outside_dir.join("root_secret.txt");
    std::fs::write(&secret_file, b"CONFIDENTIAL_PRIVATE_KEY").unwrap();

    set_test_override(VfsRoot::AppData, test_dir.clone());

    // 1. Create a symlink inside the sandbox pointing outside
    let symlink_path = test_dir.join("symlink_to_secret.txt");
    std::os::unix::fs::symlink(&secret_file, &symlink_path).unwrap();

    let vfs_symlink = app_data().join("symlink_to_secret.txt");

    // Reading through symlink must return SecurityViolation, not the file contents!
    let read_res = read_to_string(&vfs_symlink);
    match read_res {
        Err(FsError::SecurityViolation(msg)) => {
            assert!(msg.contains("Symlink or directory traversal outside sandbox"));
        }
        other => panic!("Expected SecurityViolation for symlink escape, got: {other:?}"),
    }

    // Checking exists() must not expose the outside target
    assert!(!exists(&vfs_symlink));

    // Writing through the symlink to overwrite outside file must be blocked
    let write_res = write_all(&vfs_symlink, b"OVERWRITTEN");
    match write_res {
        Err(FsError::SecurityViolation(_)) => {}
        other => panic!("Expected SecurityViolation when writing to escaping symlink, got: {other:?}"),
    }
    // Verify outside file was NOT modified
    let secret_content = std::fs::read_to_string(&secret_file).unwrap();
    assert_eq!(secret_content, "CONFIDENTIAL_PRIVATE_KEY");

    // Removing through symlink must be blocked
    let remove_res = remove_file(&vfs_symlink);
    match remove_res {
        Err(FsError::SecurityViolation(_)) => {}
        other => panic!("Expected SecurityViolation when removing escaping symlink, got: {other:?}"),
    }

    // 2. Directory symlink pointing outside
    let symlink_dir = test_dir.join("symlink_dir");
    std::os::unix::fs::symlink(&outside_dir, &symlink_dir).unwrap();

    let vfs_dir_escape = app_data().join("symlink_dir/root_secret.txt");
    let dir_read_res = read_to_vec(&vfs_dir_escape);
    match dir_read_res {
        Err(FsError::SecurityViolation(_)) => {}
        other => panic!("Expected SecurityViolation for directory symlink escape, got: {other:?}"),
    }

    // 3. Permitted intra-sandbox symlink
    let internal_target = test_dir.join("safe_internal.txt");
    std::fs::write(&internal_target, b"SAFE_DATA").unwrap();
    let safe_symlink = test_dir.join("safe_link.txt");
    std::os::unix::fs::symlink(&internal_target, &safe_symlink).unwrap();

    let vfs_safe = app_data().join("safe_link.txt");
    let safe_read = read_to_string(&vfs_safe).expect("Internal symlink should be permitted");
    assert_eq!(safe_read, "SAFE_DATA");

    // Cleanup
    let _ = std::fs::remove_dir_all(&test_dir);
    let _ = std::fs::remove_dir_all(&outside_dir);
    clear_test_overrides();
}

#[test]
fn test_to_physical_path_never_panics_on_security_violation() {
    let _guard = TEST_LOCK.lock().unwrap();
    let test_dir = std::env::temp_dir().join("strata_vfs_test_panic_free");
    let outside_dir = std::env::temp_dir().join("strata_vfs_outside_panic_free");
    let _ = std::fs::remove_dir_all(&test_dir);
    let _ = std::fs::remove_dir_all(&outside_dir);

    std::fs::create_dir_all(&test_dir).unwrap();
    std::fs::create_dir_all(&outside_dir).unwrap();

    set_test_override(VfsRoot::AppData, test_dir.clone());

    // 1. Valid non-existent file in sandbox returns Ok(path)
    let valid_path = app_data().join("valid_subfile.txt");
    let resolved = strata_vfs::driver::to_physical_path(&valid_path);
    assert!(resolved.is_ok());
    assert_eq!(resolved.unwrap(), test_dir.join("valid_subfile.txt"));

    #[cfg(unix)]
    {
        // 2. Symlink escaping sandbox must return Err(SecurityViolation), never panic
        let secret = outside_dir.join("secret.key");
        std::fs::write(&secret, b"SECRET").unwrap();

        let symlink = test_dir.join("escape_symlink");
        std::os::unix::fs::symlink(&secret, &symlink).unwrap();

        let escaping_vfs = app_data().join("escape_symlink");
        let result = strata_vfs::driver::to_physical_path(&escaping_vfs);
        match result {
            Err(FsError::SecurityViolation(msg)) => {
                assert!(msg.contains("Symlink or directory traversal outside sandbox"));
            }
            other => panic!("Expected Err(SecurityViolation), got {other:?}"),
        }
    }

    let _ = std::fs::remove_dir_all(&test_dir);
    let _ = std::fs::remove_dir_all(&outside_dir);
    clear_test_overrides();
}

#[test]
fn test_vfs_windows_drive_letters_ads_and_dos_devices_rejected() {
    let _guard = TEST_LOCK.lock().unwrap();
    let test_dir = std::env::temp_dir().join("strata_vfs_test_win_hardening");
    let _ = std::fs::remove_dir_all(&test_dir);
    std::fs::create_dir_all(&test_dir).unwrap();
    set_test_override(VfsRoot::AppData, test_dir.clone());

    // 1. Windows drive prefix stripping in sanitize_subpath
    let drive_path = app_data().join("C:/Windows/System32");
    assert_eq!(drive_path.subpath(), std::path::Path::new("Windows/System32"));

    // 2. DOS reserved devices rejected
    for device in &["CON", "PRN", "AUX", "NUL", "COM1", "LPT1", "con.txt", "nul.json"] {
        let dev_path = VfsPath::new(VfsRoot::AppData, std::path::PathBuf::from(device));
        let res = strata_vfs::driver::to_physical_path(&dev_path);
        assert!(
            matches!(res, Err(FsError::SecurityViolation(_))),
            "Expected DOS reserved device {device} to be rejected with SecurityViolation, got {res:?}"
        );
    }

    // 3. Alternate Data Streams (ADS) containing colon rejected
    let ads_path = VfsPath::new(VfsRoot::AppData, std::path::PathBuf::from("payload.exe::$DATA"));
    let res = strata_vfs::driver::to_physical_path(&ads_path);
    assert!(
        matches!(res, Err(FsError::SecurityViolation(_))),
        "Expected ADS colon path to be rejected with SecurityViolation, got {res:?}"
    );

    let _ = std::fs::remove_dir_all(&test_dir);
    clear_test_overrides();
}


