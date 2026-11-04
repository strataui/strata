//! Comprehensive integration and verification suite for Multi-Process Tab Sandboxing,
//! Fault Containment, Heartbeat Watchdog, and Zero-Copy Compositor Ring Buffers.

use std::time::Duration;

use strata_browser::sandboxing::{ProcessStatus, SandboxPolicy, SandboxedTabWorker};
use strata_browser::state::BrowserState;
use strata_browser::tab_worker::{TabWorkerJob, TabWorkerPool};
use strata_render::compositor::DisplayList;
use strata_render::shm_compositor::ZeroCopyCompositor;

#[test]
fn test_sandboxed_tab_worker_lifecycle_and_heartbeat() {
    let worker = SandboxedTabWorker::new(0, SandboxPolicy::ThreadSandbox);
    assert_eq!(worker.worker_id, 0);
    assert!(worker.is_running());
    assert!(!worker.is_crashed());

    match worker.status() {
        ProcessStatus::Running { pid, .. } => {
            assert_eq!(pid, 50000);
        }
        other => panic!("Expected Running status, got {:?}", other),
    }

    // Initial heartbeat is immediate
    assert!(worker.check_heartbeat(Duration::from_millis(100)));

    // Manual ping updates heartbeat
    worker.ping_heartbeat();
    assert!(worker.check_heartbeat(Duration::from_millis(100)));

    // Trigger contained crash
    worker.trigger_crash("Fatal signal SIGSEGV");
    assert!(worker.is_crashed());
    assert!(!worker.is_running());

    match worker.status() {
        ProcessStatus::Crashed { exit_code, reason } => {
            assert_eq!(exit_code, Some(139));
            assert_eq!(reason, "Fatal signal SIGSEGV");
        }
        other => panic!("Expected Crashed status, got {:?}", other),
    }

    // Restart worker
    worker.restart();
    assert!(worker.is_running());
    assert!(!worker.is_crashed());
}

#[test]
fn test_fault_isolated_crash_containment() {
    let state = BrowserState::new();

    // Tab 1: Normal active tab
    let tab1_id = state.active_tab_id.get();

    // Tab 2: Create a second tab that will experience a crash
    state.new_tab("https://example.com/crashing-page");
    let tab2_id = state.active_tab_id.get();
    assert_ne!(tab1_id, tab2_id);

    // Dispatch simulated crash job to Tab 2
    state.worker_pool.submit_job(TabWorkerJob::with_crash(
        tab2_id,
        1,
        "https://example.com/crashing-page".to_string(),
    ));

    // Wait for worker response
    let mut received = false;
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if state.poll_worker_responses() {
            received = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(received, "Worker crash response must be received and processed");

    // Verify Tab 2 is contained in crashed state without panicking browser
    let tabs = state.tabs.get();
    let tab2 = tabs.iter().find(|t| t.id == tab2_id).expect("Tab 2 must exist");
    assert!(tab2.is_crashed, "Tab 2 must be marked as crashed");
    assert!(tab2.crash_reason.is_some(), "Tab 2 must have crash reason recorded");
    assert!(tab2.title.starts_with("Aw, Snap!"), "Tab 2 title must display Aw, Snap!");

    // Verify Tab 1 is completely healthy and untouched
    let tab1 = tabs.iter().find(|t| t.id == tab1_id).expect("Tab 1 must exist");
    assert!(!tab1.is_crashed, "Tab 1 must NOT be affected by Tab 2 crash");

    // Verify browser UI thread is fully responsive: omnibox editing works
    state.start_editing_omnibox();
    assert!(state.is_omnibox_editing.get());
}

#[test]
fn test_tab_recovery_after_crash() {
    let state = BrowserState::new();
    let tab_id = state.active_tab_id.get();

    // Simulate crash on active tab
    state.worker_pool.submit_job(TabWorkerJob::with_crash(
        tab_id,
        0,
        "strata://about".to_string(),
    ));

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if state.poll_worker_responses() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let tab_crashed = state.active_tab().unwrap();
    assert!(tab_crashed.is_crashed);

    // Perform one-click recovery
    state.recover_tab(tab_id);

    // Wait for reload
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if state.poll_worker_responses() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let tab_recovered = state.active_tab().unwrap();
    assert!(!tab_recovered.is_crashed, "Tab must be recovered from crash");
    assert!(tab_recovered.crash_reason.is_none());
    assert!(!tab_recovered.title.is_empty());
}

#[test]
fn test_unresponsive_tab_watchdog_timeout() {
    let pool = TabWorkerPool::new(2);

    // Dispatch a job with simulated infinite hang
    pool.submit_job(TabWorkerJob::with_hang(
        42,
        1,
        "https://example.com/infinite-loop".to_string(),
    ));

    let response = pool
        .wait_timeout(Duration::from_secs(2))
        .expect("Watchdog response must arrive within timeout");

    assert_eq!(response.tab_id, 42);
    assert!(response.is_crashed);
    assert_eq!(response.status, 504);
    assert!(
        response
            .crash_reason
            .as_deref()
            .unwrap_or("")
            .contains("timeout")
            || response
                .crash_reason
                .as_deref()
                .unwrap_or("")
                .contains("unresponsive")
    );
}

#[test]
fn test_zero_copy_shm_ring_buffer_frame_flow() {
    let pool = TabWorkerPool::new(1);

    // Submit a normal load job
    pool.submit_job(TabWorkerJob::new(
        1,
        1,
        "strata://about".to_string(),
    ));

    let resp = pool
        .wait_timeout(Duration::from_secs(2))
        .expect("Response must arrive");

    assert!(!resp.is_crashed);
    assert!(resp.frame_sequence > 0);

    // Check worker ring buffer
    let ring = pool
        .get_ring_buffer(0)
        .expect("Worker 0 ring buffer must exist");

    // Ingest into ZeroCopyCompositor
    let mut compositor = ZeroCopyCompositor::new();
    compositor.attach_surface(1, ring);
    compositor.set_surface_layout(1, (0.0, 60.0), true);

    let mut master_dl = DisplayList::default();
    let ingested = compositor.composite_into(&mut master_dl);
    assert!(ingested > 0, "Frames must be ingested from shared ring buffer");
    assert!(!master_dl.is_empty(), "Master display list must contain composited commands");
}
