//! Runtime Integration Test Suite for Milestone 48: Next-Gen CSS Standards & Modern Transitions Suite
//!
//! Validates:
//! - Entry transitions with `@starting-style` seeding `old_style` from unmounted/display:none state.
//! - Scroll-driven animations with zero CPU consumption when stationary.
//! - View Transitions Module Level 1 & 2 element snapshotting and geometry interpolation.

#![allow(clippy::field_reassign_with_default)]

use strata::prelude::*;
use strata::runtime::AppRuntime;
use strata::view_transition::ViewTransitionManager;
use strata_primitives::geom::Rect;
use strata_style::{
    AnimationDirection, AnimationFillMode, AnimationIterationCount, AnimationPlayState,
    AnimationRange, AnimationSpec, AnimationTimeline, ComputedStyle, ScrollAxis,
    ScrollTimelineSource, TransitionBehavior, TransitionEasing, TransitionProperty, TransitionSpec,
};

#[test]
fn test_runtime_starting_style_entrance_transition() {
    let mut runtime = AppRuntime::new(
        rsx! { <div></div> },
        800.0,
        600.0,
        1.0,
    );

    // Initial tick
    runtime.tick_frame();

    // Now mount an element with @starting-style opacity: 0 and target opacity: 1
    let mut starting_style = ComputedStyle::default();
    starting_style.opacity = 0.0;

    let mut target_style = ComputedStyle::default();
    target_style.opacity = 1.0;
    target_style.starting_style = Some(Box::new(starting_style));
    target_style.transitions = vec![TransitionSpec {
        property: TransitionProperty::Opacity,
        duration_ms: 300.0,
        timing_function: TransitionEasing::Linear,
        delay_ms: 0.0,
        behavior: TransitionBehavior::Normal,
    }];

    // 1. Initial state: child is display: None
    let mut initial_child_style = ComputedStyle::default();
    initial_child_style.display = strata_style::Display::None;

    runtime.update_view(rsx! {
        <div>
            <div></div>
        </div>
    });
    runtime.layout_root.children[0].style = initial_child_style;
    runtime.sync_tree_animations(0.0);

    // 2. Element transitions from display: None to display: Block with @starting-style
    runtime.layout_root.children[0].style = target_style;

    // Run sync_tree_animations at t = 0.0
    runtime.sync_tree_animations(0.0);

    // Verify active transition was spawned from starting_style opacity: 0.0
    assert!(
        runtime.has_active_animations(),
        "Runtime must have an active transition initiated from @starting-style"
    );

    // Advance to t = 150.0ms (midway through 300ms duration)
    let _ = runtime.transition_manager.advance(150.0);
    runtime.apply_animations_to_layout_tree();
    let child_node = &runtime.layout_root.children[0];
    assert!(
        (child_node.style.opacity - 0.5).abs() < 0.05,
        "Midway opacity should be approximately 0.5, got {}",
        child_node.style.opacity
    );

    // Advance to t = 300.0ms (completion)
    let _ = runtime.transition_manager.advance(300.0);
    runtime.apply_animations_to_layout_tree();
    let child_node = &runtime.layout_root.children[0];
    assert!(
        (child_node.style.opacity - 1.0).abs() < 0.01,
        "Completed opacity should be 1.0, got {}",
        child_node.style.opacity
    );
}

#[test]
fn test_scroll_driven_animation_zero_cpu_auto_sleep() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div>
                <div></div>
            </div>
        },
        800.0,
        600.0,
        1.0,
    );

    // Attach a scroll-driven animation to child
    let mut style = ComputedStyle::default();
    style.animations = vec![AnimationSpec {
        name: "pulse".to_string(),
        duration_ms: 1000.0,
        timing_function: TransitionEasing::Linear,
        delay_ms: 0.0,
        iteration_count: AnimationIterationCount::Count(1.0),
        direction: AnimationDirection::Normal,
        fill_mode: AnimationFillMode::Both,
        play_state: AnimationPlayState::Running,
        timeline: AnimationTimeline::Scroll(ScrollTimelineSource::Root, ScrollAxis::Block),
        range: AnimationRange::default(),
    }];

    runtime.layout_root.children[0].style = style;
    runtime.sync_tree_animations(0.0);

    // Because the animation is scroll-driven (not time-based), has_active_animations()
    // must return FALSE when stationary so the engine enters auto-sleep with 0% CPU!
    assert!(
        !runtime.has_active_animations(),
        "Scroll-driven animation must NOT keep timer awake when stationary (preserving zero-CPU sleep)"
    );

    // Now test synchronous scroll progression:
    // Update scroll progress and trigger sync_scroll_animations
    runtime.sync_scroll_animations();

    // Still does not wake up timer
    assert!(!runtime.has_active_animations());
}

#[test]
fn test_view_transitions_manager_lifecycle() {
    let mut vt = ViewTransitionManager::new();

    // 1. Capture old state before DOM mutation
    let old_elements = vec![
        ("hero-card".to_string(), Rect::from_xywh(10.0, 10.0, 100.0, 100.0)),
        ("none".to_string(), Rect::from_xywh(0.0, 0.0, 50.0, 50.0)),
    ];
    vt.capture_old_state(&old_elements);
    assert_eq!(vt.pending_old_snapshots.len(), 1, "none must be ignored");

    // 2. Start transition after layout change
    let new_elements = vec![
        ("hero-card".to_string(), Rect::from_xywh(50.0, 50.0, 200.0, 200.0)),
    ];
    vt.start_transition(&new_elements, 0.0, 200.0);
    assert!(vt.has_active_transitions());
    assert_eq!(vt.active_transitions.len(), 1);

    // 3. Sample at midpoint t = 100ms
    let (mid_rect, progress) = vt.sample("hero-card", 100.0).expect("Active transition must sample");
    assert!((progress - 0.5).abs() < 0.01);
    assert_eq!(mid_rect.origin.x, 30.0); // 10 + (50 - 10) * 0.5
    assert_eq!(mid_rect.origin.y, 30.0); // 10 + (50 - 10) * 0.5
    assert_eq!(mid_rect.size.width, 150.0); // 100 + (200 - 100) * 0.5
    assert_eq!(mid_rect.size.height, 150.0);

    // 4. Sample and step past duration t = 250ms
    let active = vt.step(250.0);
    assert!(!active, "Transition must complete after duration has elapsed");
    assert!(!vt.has_active_transitions(), "No active transitions remaining after completion");
}
