use std::collections::HashMap;
use std::path::PathBuf;
use strata_hot_reload::{
    FileChangeTracker, HotReloadMessage, HotReloadSnapshot, HotReloader,
};

fn compile_test_dylib(name: &str, version: u32, entry_ret: i32) -> PathBuf {
    let temp_dir = std::env::temp_dir();
    let src_path = temp_dir.join(format!("{name}_test_module.rs"));
    let dylib_path = temp_dir.join(format!("lib{name}_test_module.so"));

    let code = format!(
        r#"
        static mut HYDRATED_SIGNALS: usize = 0;
        static mut LAST_EPOCH: u64 = 0;

        #[no_mangle]
        pub extern "C" fn strata_hot_reload_version() -> u32 {{
            {version}
        }}

        #[no_mangle]
        pub extern "C" fn strata_hot_reload_hydrate(ptr: *const u8, len: usize) -> i32 {{
            if ptr.is_null() && len > 0 {{
                return -1;
            }}
            if !ptr.is_null() && len >= 8 {{
                unsafe {{
                    let slice = std::slice::from_raw_parts(ptr, len);
                    let mut epoch_bytes = [0u8; 8];
                    epoch_bytes.copy_from_slice(&slice[0..8]);
                    LAST_EPOCH = u64::from_le_bytes(epoch_bytes);
                    HYDRATED_SIGNALS = 1;
                }}
            }}
            0
        }}

        #[no_mangle]
        pub extern "C" fn strata_hot_reload_entry() -> i32 {{
            {entry_ret}
        }}

        #[no_mangle]
        pub extern "C" fn test_get_hydrated_epoch() -> u64 {{
            unsafe {{ LAST_EPOCH }}
        }}
        "#
    );

    std::fs::write(&src_path, code).expect("Failed to write test dylib source");
    let status = std::process::Command::new("rustc")
        .args([
            "--crate-type",
            "cdylib",
            src_path.to_str().unwrap(),
            "-o",
            dylib_path.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to invoke rustc to build test cdylib");
    assert!(status.success(), "rustc compilation failed");
    dylib_path
}

#[test]
fn test_hot_reload_snapshot_encoding_roundtrip() {
    let mut snapshot = HotReloadSnapshot::new(42);

    // Insert signals
    snapshot.insert_signal(1, "Signal<i32>", 100i32.to_le_bytes().to_vec());
    snapshot.insert_signal(2, "Signal<String>", "Hello Strata".as_bytes().to_vec());

    // Insert component
    let mut comp_state = HashMap::new();
    comp_state.insert("slider_val".to_string(), "75.5".to_string());
    comp_state.insert("theme".to_string(), "dark".to_string());
    snapshot.insert_component("TelemetryPanel#1", 3, comp_state);

    let encoded = snapshot.encode();
    assert!(!encoded.is_empty());

    let decoded = HotReloadSnapshot::decode(&encoded).expect("Failed to decode snapshot");
    assert_eq!(decoded.epoch, 42);
    assert_eq!(decoded.signals.len(), 2);
    assert_eq!(decoded.components.len(), 1);

    let sig1 = decoded.signals.get(&1).unwrap();
    assert_eq!(sig1.type_name, "Signal<i32>");
    assert_eq!(i32::from_le_bytes(sig1.value_bytes.clone().try_into().unwrap()), 100);

    let comp = decoded.components.get("TelemetryPanel#1").unwrap();
    assert_eq!(comp.memo_index, 3);
    assert_eq!(comp.state_data.get("slider_val").map(|s| s.as_str()), Some("75.5"));
}

#[test]
fn test_file_change_tracker() {
    let mut tracker = FileChangeTracker::new();

    // Initial registration does not trigger change
    assert!(!tracker.record_file("src/main.rs", 1000));
    assert_eq!(tracker.tracked_count(), 1);

    // Same timestamp does not trigger change
    assert!(!tracker.record_file("src/main.rs", 1000));

    // Newer timestamp triggers change
    assert!(tracker.record_file("src/main.rs", 1005));
    assert_eq!(tracker.get_mtime("src/main.rs"), Some(1005));
}

#[test]
fn test_hot_reloader_lifecycle_with_real_dylib() {
    let mut reloader = HotReloader::new();
    assert_eq!(reloader.current_epoch(), 0);
    assert!(!reloader.is_loaded());

    let mut snapshot = HotReloadSnapshot::new(42);
    snapshot.insert_signal(10, "Signal<f32>", 42.0f32.to_le_bytes().to_vec());
    reloader.capture_snapshot(snapshot);

    assert!(reloader.last_snapshot().is_some());

    // Compile and apply real dylib patch
    let dylib_path = compile_test_dylib("lifecycle", HotReloader::CURRENT_ABI_VERSION, 0);
    let msg = reloader.apply_patch(dylib_path.to_str().unwrap());
    assert!(msg.is_success());
    assert_eq!(reloader.current_epoch(), 1);
    assert!(reloader.is_loaded());

    if let HotReloadMessage::PatchSuccess { latency_us } = msg {
        // Acceptance criteria: editing a component updates in < 50ms (50,000 us)
        assert!(latency_us < 50_000, "Hot reload latency must be sub-50ms, was {latency_us} us");
    } else {
        panic!("Expected PatchSuccess");
    }

    // Verify entry symbol invocation
    let entry_res = reloader.call_entry();
    assert_eq!(entry_res, Ok(0));

    // Verify dylib hydrated the snapshot's epoch 42
    let lib = reloader.active_library().expect("Library should be active");
    let get_epoch_fn: libloading::Symbol<unsafe extern "C" fn() -> u64> = unsafe {
        lib.get(b"test_get_hydrated_epoch\0").expect("get_hydrated_epoch symbol")
    };
    let hydrated_epoch = unsafe { get_epoch_fn() };
    assert_eq!(hydrated_epoch, 42, "Dylib state was preserved and hydrated into new epoch");
}

#[test]
fn test_hot_reloader_error_conditions() {
    let mut reloader = HotReloader::new();

    // 1. Empty dylib path
    let empty_res = reloader.apply_patch("");
    assert_eq!(
        empty_res,
        HotReloadMessage::PatchFailed {
            error: "Dylib path is empty".to_string(),
        }
    );

    // 2. Non-existent file
    let missing_res = reloader.apply_patch("/tmp/definitely_nonexistent_strata_module.so");
    assert!(
        matches!(missing_res, HotReloadMessage::PatchFailed { ref error } if error.contains("not found"))
    );

    // 3. Corrupt file
    let temp_corrupt = std::env::temp_dir().join("corrupt_module.so");
    std::fs::write(&temp_corrupt, b"not an elf or dylib binary").unwrap();
    let corrupt_res = reloader.apply_patch(temp_corrupt.to_str().unwrap());
    assert!(
        matches!(corrupt_res, HotReloadMessage::PatchFailed { ref error } if error.contains("Failed to load dynamic library"))
    );

    // 4. ABI version mismatch
    let bad_ver_dylib = compile_test_dylib("bad_version", 9999, 0);
    let ver_res = reloader.apply_patch(bad_ver_dylib.to_str().unwrap());
    assert!(
        matches!(ver_res, HotReloadMessage::PatchFailed { ref error } if error.contains("version mismatch"))
    );
}

#[test]
fn test_generational_signal_serialization_and_migration() {
    use strata_hot_reload::{GenerationalFieldValue, SignalMigration};

    let mut snapshot = HotReloadSnapshot::new(101);

    // Create a generation 1 signal with structured fields
    let mut fields_gen1 = HashMap::new();
    fields_gen1.insert(
        "counter".to_string(),
        GenerationalFieldValue::Int(42),
    );
    fields_gen1.insert(
        "username".to_string(),
        GenerationalFieldValue::Text("alice".to_string()),
    );
    fields_gen1.insert(
        "legacy_token".to_string(),
        GenerationalFieldValue::Bytes(vec![0xAA, 0xBB]),
    );

    snapshot.insert_generational_signal(
        1001,
        "UserState",
        1,
        fields_gen1,
    );

    // Verify roundtrip encoding & decoding of generational fields
    let encoded = snapshot.encode();
    let mut decoded = HotReloadSnapshot::decode(&encoded).expect("Failed to decode generational snapshot");
    assert_eq!(decoded.epoch, 101);
    let sig = decoded.signals.get(&1001).unwrap();
    assert_eq!(sig.generation, 1);
    assert_eq!(sig.get_field("counter"), Some(&GenerationalFieldValue::Int(42)));
    assert_eq!(
        sig.get_field("username"),
        Some(&GenerationalFieldValue::Text("alice".to_string()))
    );

    // Define migration 1 -> 2:
    // - rename 'counter' to 'total_count'
    // - convert 'total_count' from Int(42) to Int(420)
    // - add default 'is_active' = Bool(true)
    // - remove 'legacy_token'
    let mig_1_to_2 = SignalMigration::new("UserState", 1, 2)
        .rename("counter", "total_count")
        .convert("total_count", |val| match val {
            GenerationalFieldValue::Int(x) => GenerationalFieldValue::Int(x * 10),
            other => other,
        })
        .add_default("is_active", GenerationalFieldValue::Bool(true))
        .remove("legacy_token");

    // Define migration 2 -> 3:
    // - add default 'rating' = Float(4.8)
    // - rename type to 'UserStateV3'
    let mig_2_to_3 = SignalMigration::new("UserState", 2, 3)
        .add_default("rating", GenerationalFieldValue::Float(4.8))
        .with_target_type("UserStateV3");

    let count = decoded.migrate_signals(&[mig_1_to_2, mig_2_to_3]);
    assert_eq!(count, 2, "Both generational migrations applied sequentially");

    let migrated_sig = decoded.signals.get(&1001).unwrap();
    assert_eq!(migrated_sig.generation, 3);
    assert_eq!(migrated_sig.type_name, "UserStateV3");
    assert_eq!(
        migrated_sig.get_field("total_count"),
        Some(&GenerationalFieldValue::Int(420))
    );
    assert_eq!(
        migrated_sig.get_field("username"),
        Some(&GenerationalFieldValue::Text("alice".to_string()))
    );
    assert_eq!(
        migrated_sig.get_field("is_active"),
        Some(&GenerationalFieldValue::Bool(true))
    );
    assert_eq!(
        migrated_sig.get_field("rating"),
        Some(&GenerationalFieldValue::Float(4.8))
    );
    assert_eq!(migrated_sig.get_field("legacy_token"), None);
    assert_eq!(migrated_sig.get_field("counter"), None);
}

#[test]
fn test_hot_reloader_registered_migrations() {
    use strata_hot_reload::{GenerationalFieldValue, SignalMigration};

    let mut reloader = HotReloader::new();
    let mut snapshot = HotReloadSnapshot::new(1);

    let mut fields = HashMap::new();
    fields.insert("speed".to_string(), GenerationalFieldValue::Float(10.0));
    snapshot.insert_generational_signal(55, "PhysicsConfig", 1, fields);
    reloader.capture_snapshot(snapshot);

    // Register migration: 1 -> 2, adds max_speed
    reloader.register_migration(
        SignalMigration::new("PhysicsConfig", 1, 2)
            .add_default("max_speed", GenerationalFieldValue::Float(120.0)),
    );

    let applied = reloader.apply_registered_migrations();
    assert_eq!(applied, 1);

    let snap = reloader.last_snapshot().unwrap();
    let sig = snap.signals.get(&55).unwrap();
    assert_eq!(sig.generation, 2);
    assert_eq!(
        sig.get_field("max_speed"),
        Some(&GenerationalFieldValue::Float(120.0))
    );
    assert_eq!(
        sig.get_field("speed"),
        Some(&GenerationalFieldValue::Float(10.0))
    );
}

