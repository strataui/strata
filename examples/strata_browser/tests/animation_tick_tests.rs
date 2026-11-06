//! Regression coverage for the browser shell: an in-flight load spinner must not
//! drive a full layout recomputation on every animation frame.

use std::time::Duration;

use strata::prelude::*;
use strata_browser::state::BrowserState;
use strata_browser::BrowserApp;
use strata_platform::PlatformEvent;

fn loading_new_tab_state() -> BrowserState {
    let state = BrowserState::new();
    state.tabs.update(|tabs| {
        if let Some(tab) = tabs.first_mut() {
            tab.is_loading = true;
            tab.html_content.clear();
        }
    });
    state
}

#[test]
fn new_tab_spinner_ticks_do_not_recompute_layout() {
    let state = loading_new_tab_state();
    let mut runtime = AppRuntime::new(BrowserApp(state), 1280.0, 800.0, 1.0);

    assert!(
        runtime.has_active_animations(),
        "the loading viewport spinner must register animate-spin/animate-pulse"
    );

    let baseline = runtime.layout_compute_count();
    for _ in 0..8 {
        std::thread::sleep(Duration::from_millis(2));
        assert!(
            runtime.step_event(&PlatformEvent::AnimationTick),
            "AnimationTick must request a redraw while the spinner is visible"
        );
    }
    assert_eq!(
        runtime.layout_compute_count(),
        baseline,
        "AnimationTick must not rebuild or recompute the new-tab layout"
    );
}

#[test]
fn idle_new_tab_ticks_are_quiescent() {
    let state = BrowserState::new();
    let mut runtime = AppRuntime::new(BrowserApp(state), 1280.0, 800.0, 1.0);

    let baseline = runtime.layout_compute_count();
    for _ in 0..8 {
        std::thread::sleep(Duration::from_millis(2));
        let _ = runtime.step_event(&PlatformEvent::AnimationTick);
    }
    assert_eq!(
        runtime.layout_compute_count(),
        baseline,
        "an idle new-tab page must not recompute layout on animation ticks"
    );
}
