//! Comprehensive Integration Tests for W3C ARIA Live Regions, Accessibility Tree & AccessKit Synchronization.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use strata::prelude::*;
use strata::{
    a11y::{
        A11yAction, A11yLivePoliteness, A11yRole, AccessKitActionRequest, AccessKitAdapter,
    },
    launch_headless,
};
use strata_aria::prelude::*;

#[test]
fn test_w3c_aria_live_regions_queue_and_announcer() {
    let announcer = LiveRegionAnnouncer::global();
    announcer.clear();

    // 1. Direct speech queueing
    let id1 = announce_polite("File top_module.v saved successfully");
    let id2 = announce_assertive("CRITICAL: Timing closure failed at 250 MHz");
    assert!(id1 > 0);
    assert!(id2 > id1);
    assert_eq!(announcer.pending_count(), 2);

    let announcements = announcer.drain_announcements();
    assert_eq!(announcements.len(), 2);
    assert_eq!(announcements[0].politeness, LiveRegionPoliteness::Polite);
    assert_eq!(announcements[0].message, "File top_module.v saved successfully");
    assert_eq!(announcements[1].politeness, LiveRegionPoliteness::Assertive);
    assert_eq!(announcements[1].message, "CRITICAL: Timing closure failed at 250 MHz");
    assert_eq!(announcer.pending_count(), 0);

    // 2. Reactive live region state & attributes
    let telemetry_region = use_live_region(LiveRegionConfig {
        id: "telemetry-stream".to_string(),
        role: Some("status".to_string()),
        politeness: LiveRegionPoliteness::Polite,
        atomic: true,
        relevant: LiveRegionRelevant::AdditionsText,
    });

    let attrs = telemetry_region.attrs();
    assert!(attrs.iter().any(|(k, v)| *k == "role" && v == "status"));
    assert!(attrs.iter().any(|(k, v)| *k == "aria-live" && v == "polite"));
    assert!(attrs.iter().any(|(k, v)| *k == "aria-atomic" && v == "true"));
    assert!(attrs.iter().any(|(k, v)| *k == "aria-busy" && v == "false"));

    telemetry_region.set_content("Power draw: 14.2W");
    assert_eq!(announcer.pending_count(), 1);
    let drained = announcer.drain_announcements();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].message, "Power draw: 14.2W");
}

#[test]
fn test_runtime_layout_tick_accessibility_tree_synchronization() {
    let clicked = Arc::new(AtomicBool::new(false));
    let clicked_clone = Arc::clone(&clicked);

    let on_click = move |_| {
        clicked_clone.store(true, Ordering::SeqCst);
    };

    let mut rt = launch_headless(
        move || {
            rsx! {
                <div role="main" aria-label="Synthesizer Control Panel" class="flex flex-col gap-4 p-6">
                    <h1 class="text-xl font-bold">FPGA Compiler</h1>
                    <button
                        aria-label="Start Synthesis"
                        onclick={on_click}
                        class="w-48 h-12 bg-blue-600"
                    >
                        Start Synthesis
                    </button>
                    <input
                        value="clk_250mhz"
                        aria-label="Clock Domain"
                        class="w-48 h-10 border"
                    />
                    <div
                        role="alert"
                        aria-live="assertive"
                        aria-atomic="true"
                        class="p-4 bg-red-900 text-white"
                    >
                        Thermal throttle active
                    </div>
                </div>
            }
        },
        800.0,
        600.0,
    );

    // 1. Accessibility tree must be built immediately on runtime creation
    assert!(rt.a11y_tree.is_some(), "Accessibility tree must be constructed on launch");
    assert!(rt.a11y_updates_count >= 1, "At least 1 accessibility update must have occurred");

    let tree = rt.a11y_tree.as_ref().unwrap();

    // Verify root and landmark
    let main_node = tree
        .nodes
        .values()
        .find(|n| n.role == A11yRole::Main)
        .expect("Main landmark node missing");
    assert_eq!(main_node.name.as_deref(), Some("Synthesizer Control Panel"));

    // Verify button node and physical layout bounds
    let btn_node = tree
        .nodes
        .values()
        .find(|n| n.role == A11yRole::Button)
        .expect("Button node missing");
    assert_eq!(btn_node.name.as_deref(), Some("Start Synthesis"));
    assert!(btn_node.focusable);
    assert!(btn_node.supported_actions.contains(&A11yAction::Click));
    assert!(btn_node.bounds.is_some(), "Button must have bound spatial layout coordinates");
    let btn_bounds = btn_node.bounds.unwrap();
    eprintln!("btn_bounds: {:?}", btn_bounds);
    assert!(btn_bounds.width > 100.0, "Button width must be computed by layout engine");
    assert!(btn_bounds.height > 20.0, "Button height must be computed by layout engine");



    // Verify input node
    let input_node = tree
        .nodes
        .values()
        .find(|n| n.role == A11yRole::TextInput)
        .expect("TextInput node missing");
    assert_eq!(input_node.name.as_deref(), Some("Clock Domain"));
    assert_eq!(input_node.value.as_deref(), Some("clk_250mhz"));

    // Verify alert live region
    let alert_node = tree
        .nodes
        .values()
        .find(|n| n.role == A11yRole::Alert)
        .expect("Alert node missing");
    assert_eq!(alert_node.live, Some(A11yLivePoliteness::Assertive));
    assert!(alert_node.live_atomic);

    // 2. Test assistive technology action dispatching (Click)
    let btn_id = btn_node.id;
    assert!(!clicked.load(Ordering::SeqCst));
    let handled = rt.dispatch_accessibility_action(btn_id, A11yAction::Click);
    assert!(handled, "Dispatching Click action on button must succeed");
    assert!(clicked.load(Ordering::SeqCst), "Button click listener must be executed");

    // 3. Test frame tick re-evaluation & update count increment
    let prior_updates = rt.a11y_updates_count;
    rt.tick_frame();
    assert!(
        rt.a11y_updates_count > prior_updates,
        "Accessibility tree must update on every tick_frame"
    );

    // 4. AccessKit serialization protocol verification
    let mut adapter = AccessKitAdapter::new("Strata EDA Studio");
    let initial_tree = rt.a11y_tree.as_ref().expect("Accessibility tree must exist");
    let ak_update = adapter.process_tree(initial_tree);

    let json = ak_update.to_json();
    assert!(json.contains("\"type\":\"accesskit_tree_update\""));
    assert!(json.contains("\"role\":\"button\""));
    assert!(json.contains("\"name\":\"Start Synthesis\""));
    assert!(json.contains("\"role\":\"alert\""));
    assert!(json.contains("\"live\":\"assertive\""));

    // Differential update verification
    let diff = rt.update_accessibility_tree();
    let ak_diff = adapter.process_update(&diff, None);
    assert_eq!(ak_diff.nodes.len(), 0, "No differential changes should be reported without DOM mutations");

    // 5. Test AccessKit action request handling
    let req = AccessKitActionRequest {
        target: btn_id,
        action: A11yAction::Click,
        data: None,
    };
    adapter.handle_action_request(&req, &rt.a11y_bridge);
}
