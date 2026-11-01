use std::collections::HashMap;
use strata_hot_reload::{
    FileChangeTracker, HotReloadMessage, HotReloadSnapshot, HotReloader,
};

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
fn test_hot_reloader_lifecycle() {
    let mut reloader = HotReloader::new();
    assert_eq!(reloader.current_epoch(), 0);

    let mut snapshot = HotReloadSnapshot::new(0);
    snapshot.insert_signal(10, "Signal<f32>", 42.0f32.to_le_bytes().to_vec());
    reloader.capture_snapshot(snapshot);

    assert!(reloader.last_snapshot().is_some());

    // Apply patch with dylib
    let msg = reloader.apply_patch("/tmp/strata_app.so");
    assert!(msg.is_success());
    assert_eq!(reloader.current_epoch(), 1);

    if let HotReloadMessage::PatchSuccess { latency_us } = msg {
        assert!(latency_us < 10000, "Hot reload latency must be sub-10ms");
    } else {
        panic!("Expected PatchSuccess");
    }
}
