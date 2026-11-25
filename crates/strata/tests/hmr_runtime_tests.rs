//! Integration tests for built-in Strata HMR (`use_hmr_signal`, `HmrRuntimeController`,
//! state preservation across reloads, and `AppRuntime` scroll/input retention).

use strata::dom::text_input::TextInputState;
use strata::hot_reload::{HmrIpcClient, HmrResponse};
use strata::prelude::*;
use strata::{HmrRuntimeController, RebuildReason};

#[test]
fn test_hmr_signal_codec_and_registry_roundtrip() {
    clear_persistent_signals();
    clear_hmr_signals();

    let count = use_hmr_signal("counter", 7i32);
    let ratio = use_hmr_signal("ratio", 3.5f64);
    let flag = use_hmr_signal("dark_mode", true);
    let label = use_hmr_signal("label", "hello".to_string());
    let opt = use_hmr_signal("selected", Some(99u32));
    let tags = use_hmr_signal("tags", vec!["rust".to_string(), "strata".to_string()]);

    count.set(42);
    ratio.set(9.25);
    flag.set(false);
    label.set("updated".to_string());
    opt.set(None);
    tags.set(vec!["hmr".to_string()]);

    let exported = export_hmr_signals();
    assert!(exported.len() >= 6);

    // Simulate a fresh dynamic-library epoch with empty local registries
    clear_persistent_signals();
    clear_hmr_signals();

    let pairs: Vec<(String, Vec<u8>)> = exported.into_iter().map(|(k, _, b)| (k, b)).collect();
    let restored_count = import_hmr_signals(&pairs);
    assert!(restored_count >= 6);

    assert_eq!(use_hmr_signal("counter", 0i32).get(), 42);
    assert!((use_hmr_signal("ratio", 0.0f64).get() - 9.25).abs() < 1e-9);
    assert!(!use_hmr_signal("dark_mode", true).get());
    assert_eq!(use_hmr_signal("label", String::new()).get(), "updated");
    assert_eq!(use_hmr_signal("selected", Some(1u32)).get(), None);
    assert_eq!(
        use_hmr_signal("tags", Vec::<String>::new()).get(),
        vec!["hmr".to_string()]
    );
}

#[test]
fn test_hmr_runtime_controller_ipc_ping_and_missing_patch_preserves_runtime_state() {
    clear_persistent_signals();
    clear_hmr_signals();

    let sock = std::env::temp_dir().join(format!(
        "strata_runtime_hmr_test_{}_{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut ctrl = HmrRuntimeController::bind(&sock, || {}).unwrap();
    let mut runtime = AppRuntime::from_root(
        || {
            let n = use_hmr_signal("rt_counter", 10i32);
            rsx! {
                <div id="scroller" class="overflow-y-auto h-32">
                    <span>{format!("count={}", n.get())}</span>
                </div>
            }
        },
        400.0,
        300.0,
        1.0,
    );

    // Seed runtime scroll + text input state to verify preservation across HMR polls
    runtime
        .scroll_offsets_by_id
        .insert("scroller".to_string(), (0.0, 145.0));
    let search_state = TextInputState::new();
    search_state.text.set("strata hmr".to_string());
    runtime
        .input_states
        .insert("search".to_string(), search_state);

    // Ping from a background client thread while main thread polls controller
    let sock_client = sock.clone();
    let client_thread = std::thread::spawn(move || {
        let client = HmrIpcClient::new(&sock_client);
        let epoch = client.ping().unwrap();
        let fail_ack = client
            .apply_patch(std::path::Path::new("/nonexistent/patch.so"), 1, 50)
            .unwrap();
        (epoch, fail_ack)
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut saw_outcome = None;
    while std::time::Instant::now() < deadline && saw_outcome.is_none() {
        if let Some(out) = ctrl.poll_and_apply(&mut runtime) {
            saw_outcome = Some(out);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let (epoch, fail_ack) = client_thread.join().unwrap();
    assert_eq!(epoch, 0);
    assert!(matches!(fail_ack, HmrResponse::PatchAck { ok: false, .. }));
    assert!(saw_outcome.is_some_and(|o| !o.ok));

    // Scroll offsets and input states must remain completely untouched
    assert_eq!(
        runtime.scroll_offsets_by_id.get("scroller"),
        Some(&(0.0, 145.0))
    );
    assert_eq!(
        runtime.input_states.get("search").map(|s| s.text.get()),
        Some("strata hmr".to_string())
    );
    assert_eq!(format!("{}", RebuildReason::HmrPatch(2)), "HmrPatch(epoch=2)");
}
