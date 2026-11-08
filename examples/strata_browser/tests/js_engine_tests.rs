//! Integration test verifying GOAL-27: Real JavaScript execution in Strata Browser tab workers
//! and reactive display list updates.

use std::time::Duration;
use strata_browser::tab_worker::{TabWorkerJob, TabWorkerPool};
use strata_render::compositor::DrawCommand;

#[test]
fn test_js_engine_execution_updates_display_list() {
    let pool = TabWorkerPool::new(1);

    // Acceptance criteria vector: document.body.innerHTML = "<h1>Hello from JS</h1>"
    let url = "data:text/html,<!DOCTYPE html><html><body><div id=\"app\">Original Body</div><script>document.body.innerHTML = \"<h1>Hello from JS</h1>\";</script></body></html>".to_string();

    let job = TabWorkerJob::new(0, 1, url);
    assert!(pool.submit_job(job), "Failed to submit job to tab worker");

    let resp = pool
        .wait_timeout(Duration::from_secs(3))
        .expect("Tab worker timed out processing JavaScript page");

    assert_eq!(resp.status, 200);
    assert!(!resp.is_error);
    assert!(!resp.is_crashed);

    // Verify DOM nodes reflect JavaScript mutation
    assert!(
        !resp.dom_nodes.is_empty(),
        "Mutated DOM nodes must be present"
    );
    let all_text = resp
        .dom_nodes
        .iter()
        .map(|n| n.text_content())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        all_text.contains("Hello from JS"),
        "DOM text must reflect JS mutation: {all_text}"
    );

    // Verify zero-copy compositor display list was committed with updated text
    let ring_buffer = pool
        .get_ring_buffer(0)
        .expect("Failed to get ring buffer for worker 0");

    let (_seq, frame) = ring_buffer
        .acquire_latest(0)
        .expect("Expected committed frame in ShmRingBuffer");

    let mut found_js_text = false;
    for cmd in &frame.display_list.commands {
        if let DrawCommand::DrawText { text, .. } = cmd {
            if text.contains("Hello from JS") {
                found_js_text = true;
                break;
            }
        }
    }

    assert!(
        found_js_text,
        "Compositor DisplayList must contain 'Hello from JS' from executed script: {:?}",
        frame.display_list.commands
    );
}

#[test]
fn test_js_engine_dom_mutation_via_get_element_by_id() {
    let pool = TabWorkerPool::new(1);

    let url = "data:text/html,<html><body><h1 id=\"title\">Initial</h1><p id=\"status\">idle</p><script>document.getElementById(\"status\").textContent = \"active\";</script></body></html>".to_string();

    let job = TabWorkerJob::new(1, 2, url);
    assert!(pool.submit_job(job));

    let resp = pool
        .wait_timeout(Duration::from_secs(3))
        .expect("Tab worker timed out");

    assert_eq!(resp.status, 200);
    let all_text = resp
        .dom_nodes
        .iter()
        .map(|n| n.text_content())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(all_text.contains("active"), "DOM must have text 'active' from JS: {all_text}");
}
