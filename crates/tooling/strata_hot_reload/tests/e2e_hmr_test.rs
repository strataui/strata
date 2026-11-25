//! Real end-to-end integration tests for Strata native Hot Module Replacement (HMR):
//! - Recursive debounced DirectoryWatcher ignoring `target/` and `.git/`
//! - Cross-platform JSON-Lines HMR IPC server <-> client round-trip
//! - Live `rustc --crate-type cdylib` compile v1 -> overwrite SAME `.so` path with v2 ->
//!   verify epoch-staged loading defeats OS `dlopen` path cache, state hydration runs,
//!   retired library is retained (UAF-safe), and rollback restores v1.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use strata_hot_reload::{
    DirectoryWatcher, HmrIpcClient, HmrIpcServer, HmrRequest, HmrResponse, HotReloadMessage,
    HotReloadSnapshot, HotReloader,
};

#[test]
fn test_directory_watcher_detects_src_and_ignores_target() {
    let base = std::env::temp_dir().join(format!(
        "strata_hmr_watch_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("src")).unwrap();
    fs::create_dir_all(base.join("target/debug")).unwrap();
    fs::create_dir_all(base.join(".git")).unwrap();

    let main_rs = base.join("src/main.rs");
    fs::write(&main_rs, "fn main() {}\n").unwrap();

    let mut watcher = DirectoryWatcher::new(&base).with_debounce(Duration::from_millis(20));
    assert!(watcher.poll_changes().is_empty());

    // Writes inside target/ and .git/ MUST be ignored
    fs::write(base.join("target/debug/libapp.so"), b"fake").unwrap();
    fs::write(base.join(".git/index"), b"fake").unwrap();
    assert!(watcher.poll_changes().is_empty());
    thread::sleep(Duration::from_millis(35));
    assert!(
        watcher.poll_changes().is_empty(),
        "DirectoryWatcher must ignore target/ and .git/"
    );

    // Modifying a real source file MUST be buffered during debounce and flushed once settled
    thread::sleep(Duration::from_millis(20));
    fs::write(&main_rs, "fn main() { println!(\"v2\"); }\n").unwrap();
    let _ = watcher.poll_changes(); // captures edit timestamp
    thread::sleep(Duration::from_millis(30));
    let changed = watcher.poll_changes();
    assert!(
        changed.iter().any(|p| p == &main_rs),
        "Expected src/main.rs change to be detected after debounce, got: {:?}",
        changed
    );

    // Debounce: immediate re-poll without new write is empty
    assert!(watcher.poll_changes().is_empty());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_hmr_ipc_server_client_roundtrip() {
    #[cfg(unix)]
    let sock = std::path::PathBuf::from("/tmp").join(format!("s_hmr_ipc_{}.sock", std::process::id()));
    #[cfg(not(unix))]
    let sock = std::env::temp_dir().join(format!("s_hmr_ipc_{}.sock", std::process::id()));
    let notified = Arc::new(AtomicBool::new(false));
    let notified_cb = notified.clone();

    let server = HmrIpcServer::bind(&sock, move || {
        notified_cb.store(true, Ordering::SeqCst);
    })
    .expect("HmrIpcServer::bind must succeed");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_bg = stop.clone();
    let handler = thread::spawn(move || {
        while !stop_bg.load(Ordering::SeqCst) {
            for cmd in server.drain_pending() {
                match cmd.request {
                    HmrRequest::Ping => {
                        let _ = cmd.responder.send(HmrResponse::Pong { epoch: 3 });
                    }
                    HmrRequest::ApplyPatch { epoch, .. } => {
                        let _ = cmd.responder.send(HmrResponse::PatchAck {
                            ok: true,
                            epoch,
                            swap_us: 420,
                            signals_restored: 2,
                            error: None,
                        });
                    }
                    HmrRequest::Shutdown => {
                        let _ = cmd.responder.send(HmrResponse::ShutdownAck);
                        stop_bg.store(true, Ordering::SeqCst);
                    }
                    HmrRequest::Rollback => {
                        let _ = cmd.responder.send(HmrResponse::PatchAck {
                            ok: true,
                            epoch: 2,
                            swap_us: 100,
                            signals_restored: 2,
                            error: None,
                        });
                    }
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
    });

    let client = HmrIpcClient::new(&sock);
    assert_eq!(client.ping().unwrap(), 3);
    assert!(notified.load(Ordering::SeqCst), "on_notify wake hook must fire");

    let ack = client
        .apply_patch(std::path::Path::new("/tmp/fake.so"), 4, 150)
        .unwrap();
    assert_eq!(
        ack,
        HmrResponse::PatchAck {
            ok: true,
            epoch: 4,
            swap_us: 420,
            signals_restored: 2,
            error: None,
        }
    );

    assert_eq!(
        client.send(&HmrRequest::Shutdown).unwrap(),
        HmrResponse::ShutdownAck
    );
    handler.join().unwrap();
}

#[test]
fn test_live_cdylib_v1_to_v2_same_path_reload_and_rollback() {
    let base = std::env::temp_dir().join(format!(
        "strata_hmr_cdylib_e2e_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let src_path = base.join("patch.rs");
    let dylib_path = base.join(format!(
        "{}strata_test_patch.{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_EXTENSION
    ));

    let compile_patch = |entry_val: i32| {
        let code = format!(
            r#"
            use std::sync::atomic::{{AtomicUsize, Ordering}};
            static HYDRATED_LEN: AtomicUsize = AtomicUsize::new(0);

            #[no_mangle]
            pub extern "C" fn strata_hot_reload_version() -> u32 {{ 1 }}

            #[no_mangle]
            pub unsafe extern "C" fn strata_hot_reload_hydrate(ptr: *const u8, len: usize) -> i32 {{
                if ptr.is_null() || len == 0 {{
                    return -1;
                }}
                HYDRATED_LEN.store(len, Ordering::SeqCst);
                0
            }}

            #[no_mangle]
            pub extern "C" fn strata_hot_reload_entry() -> i32 {{
                if HYDRATED_LEN.load(Ordering::SeqCst) > 0 {{ 0 }} else {{ -99 }}
            }}

            #[no_mangle]
            pub extern "C" fn strata_test_epoch_tag() -> i32 {{
                if HYDRATED_LEN.load(Ordering::SeqCst) > 0 {{ {entry_val} }} else {{ -99 }}
            }}
            "#
        );
        fs::write(&src_path, code).unwrap();
        let status = Command::new("rustc")
            .args([
                "--edition=2021",
                "--crate-type=cdylib",
                "-o",
                dylib_path.to_str().unwrap(),
                src_path.to_str().unwrap(),
            ])
            .status()
            .expect("rustc must be available in test environment");
        assert!(status.success(), "rustc failed compiling test HMR cdylib");
    };

    let read_tag = |r: &HotReloader| -> i32 {
        let lib = r.active_library().expect("active library must be loaded");
        unsafe {
            let sym = lib
                .get::<unsafe extern "C" fn() -> i32>(b"strata_test_epoch_tag\0")
                .expect("strata_test_epoch_tag symbol missing");
            sym()
        }
    };

    // Compile v1 at `dylib_path`
    compile_patch(101);

    let mut reloader = HotReloader::new();
    let mut snap = HotReloadSnapshot::new(0);
    snap.insert_signal(1, "hmr:counter:i32", 42i32.to_le_bytes().to_vec());

    let msg1 = reloader.apply_patch_with_snapshot(dylib_path.to_str().unwrap(), snap);
    match msg1 {
        HotReloadMessage::PatchSuccess { latency_us } => assert!(latency_us >= 1),
        other => panic!("Expected PatchSuccess for v1, got {:?}", other),
    }
    assert_eq!(reloader.current_epoch(), 1);
    assert_eq!(reloader.retired_library_count(), 0);
    assert_eq!(reloader.call_entry().unwrap(), 0);
    assert_eq!(read_tag(&reloader), 101);

    // Overwrite THE EXACT SAME dylib_path with v2 (defeats Linux dlopen path cache!)
    compile_patch(202);
    let msg2 = reloader.apply_patch(dylib_path.to_str().unwrap());
    match msg2 {
        HotReloadMessage::PatchSuccess { latency_us } => assert!(latency_us >= 1),
        other => panic!("Expected PatchSuccess for v2, got {:?}", other),
    }
    assert_eq!(reloader.current_epoch(), 2);
    assert_eq!(
        reloader.retired_library_count(),
        1,
        "Previous v1 library must remain mapped in retired_libraries to prevent UAF"
    );
    assert_eq!(
        read_tag(&reloader),
        202,
        "Epoch-staged copy must load v2 instead of cached v1"
    );

    // Roll back to v1 and verify v1 symbol runs again
    let rolled_epoch = reloader.rollback().unwrap();
    assert_eq!(rolled_epoch, 1);
    assert_eq!(read_tag(&reloader), 101);

    let _ = fs::remove_dir_all(&base);
}
