//! Comprehensive unit tests for W3C CSS Transitions Level 1 and Level 2 Engine.

#![allow(clippy::field_reassign_with_default)]

use strata_primitives::geom::EdgeInsets;
use strata_style::cascade::CascadeSolver;
use strata_style::parser::{parse_declarations, parse_single_transition, parse_transitions};
use strata_style::tailwind::TailwindEngine;
use strata_style::transition::{
    interpolate_border_radius, interpolate_box_shadow, interpolate_color, interpolate_dimension,
    interpolate_edge_insets, TransitionLifecycleEvent, TransitionManager,
};
use strata_style::types::{
    BorderRadius, BoxShadow, Color, ComputedStyle, Dimension, StepPosition, TransitionBehavior,
    TransitionEasing, TransitionProperty, TransitionSpec,
};

#[test]
fn test_parse_transition_shorthand() {
    let css = "opacity 200ms ease, background-color 300ms linear 50ms, all 400ms cubic-bezier(0.4, 0.0, 0.2, 1.0) allow-discrete";
    let transitions = parse_transitions(css);

    assert_eq!(transitions.len(), 3);

    // First transition
    assert_eq!(transitions[0].property, TransitionProperty::Opacity);
    assert_eq!(transitions[0].duration_ms, 200.0);
    assert_eq!(transitions[0].timing_function, TransitionEasing::Ease);
    assert_eq!(transitions[0].delay_ms, 0.0);
    assert_eq!(transitions[0].behavior, TransitionBehavior::Normal);

    // Second transition
    assert_eq!(transitions[1].property, TransitionProperty::BackgroundColor);
    assert_eq!(transitions[1].duration_ms, 300.0);
    assert_eq!(transitions[1].timing_function, TransitionEasing::Linear);
    assert_eq!(transitions[1].delay_ms, 50.0);
    assert_eq!(transitions[1].behavior, TransitionBehavior::Normal);

    // Third transition
    assert_eq!(transitions[2].property, TransitionProperty::All);
    assert_eq!(transitions[2].duration_ms, 400.0);
    assert_eq!(
        transitions[2].timing_function,
        TransitionEasing::CubicBezier(0.4, 0.0, 0.2, 1.0)
    );
    assert_eq!(transitions[2].delay_ms, 0.0);
    assert_eq!(transitions[2].behavior, TransitionBehavior::AllowDiscrete);
}

#[test]
fn test_parse_steps_timing_function() {
    let t1 = parse_single_transition("opacity 100ms steps(4)").unwrap();
    assert_eq!(t1.timing_function, TransitionEasing::Steps(4, StepPosition::End));

    let t2 = parse_single_transition("transform 250ms steps(5, jump-start)").unwrap();
    assert_eq!(
        t2.timing_function,
        TransitionEasing::Steps(5, StepPosition::JumpStart)
    );

    let t3 = parse_single_transition("width 150ms step-start").unwrap();
    assert_eq!(t3.timing_function, TransitionEasing::StepStart);
}

#[test]
fn test_cascade_transition_longhands() {
    let mut style = ComputedStyle::default();

    let decls = parse_declarations(
        "transition-property: opacity, width; transition-duration: 250ms; transition-timing-function: ease-in-out; transition-delay: 100ms; transition-behavior: allow-discrete;"
    );

    for decl in &decls {
        CascadeSolver::apply_declaration(&mut style, decl);
    }

    assert_eq!(style.transitions.len(), 2);
    assert_eq!(style.transitions[0].property, TransitionProperty::Opacity);
    assert_eq!(style.transitions[0].duration_ms, 250.0);
    assert_eq!(style.transitions[0].timing_function, TransitionEasing::EaseInOut);
    assert_eq!(style.transitions[0].delay_ms, 100.0);
    assert_eq!(style.transitions[0].behavior, TransitionBehavior::AllowDiscrete);

    assert_eq!(style.transitions[1].property, TransitionProperty::Width);
    assert_eq!(style.transitions[1].duration_ms, 250.0);
    assert_eq!(style.transitions[1].timing_function, TransitionEasing::EaseInOut);
    assert_eq!(style.transitions[1].delay_ms, 100.0);
    assert_eq!(style.transitions[1].behavior, TransitionBehavior::AllowDiscrete);
}

#[test]
fn test_tailwind_transition_utilities() {
    let style = TailwindEngine::resolve_classes("transition-all duration-300 ease-in-out delay-150");

    assert_eq!(style.transitions.len(), 1);
    let spec = &style.transitions[0];
    assert_eq!(spec.property, TransitionProperty::All);
    assert_eq!(spec.duration_ms, 300.0);
    assert_eq!(
        spec.timing_function,
        TransitionEasing::CubicBezier(0.4, 0.0, 0.2, 1.0)
    );
    assert_eq!(spec.delay_ms, 150.0);

    let style_colors = TailwindEngine::resolve_classes("transition-colors duration-200");
    assert_eq!(style_colors.transitions.len(), 3);
    assert_eq!(style_colors.transitions[0].property, TransitionProperty::Color);
    assert_eq!(style_colors.transitions[0].duration_ms, 200.0);
    assert_eq!(
        style_colors.transitions[1].property,
        TransitionProperty::BackgroundColor
    );
    assert_eq!(style_colors.transitions[1].duration_ms, 200.0);
    assert_eq!(
        style_colors.transitions[2].property,
        TransitionProperty::BorderColor
    );
    assert_eq!(style_colors.transitions[2].duration_ms, 200.0);
}

#[test]
fn test_color_and_dimension_continuous_interpolation() {
    let red = Color::new(255, 0, 0, 255);
    let blue = Color::new(0, 0, 255, 255);
    let mid_color = interpolate_color(red, blue, 0.5);

    assert_eq!(mid_color.r, 128);
    assert_eq!(mid_color.g, 0);
    assert_eq!(mid_color.b, 128);
    assert_eq!(mid_color.a, 255);

    let d1 = Dimension::Px(100.0);
    let d2 = Dimension::Px(200.0);
    let mid_dim = interpolate_dimension(&d1, &d2, 0.5);
    assert_eq!(mid_dim, Dimension::Px(150.0));

    let insets1 = EdgeInsets::new(10.0, 20.0, 30.0, 40.0);
    let insets2 = EdgeInsets::new(30.0, 40.0, 50.0, 60.0);
    let mid_insets = interpolate_edge_insets(&insets1, &insets2, 0.5);
    assert_eq!(mid_insets, EdgeInsets::new(20.0, 30.0, 40.0, 50.0));

    let r1 = BorderRadius::all(10.0);
    let r2 = BorderRadius::all(30.0);
    let mid_r = interpolate_border_radius(&r1, &r2, 0.5);
    assert_eq!(mid_r, BorderRadius::all(20.0));
}

#[test]
fn test_box_shadow_interpolation() {
    let s1 = vec![BoxShadow {
        offset_x: 2.0,
        offset_y: 4.0,
        blur_radius: 6.0,
        spread_radius: 0.0,
        color: Color::new(0, 0, 0, 100),
        inset: false,
    }];
    let s2 = vec![BoxShadow {
        offset_x: 6.0,
        offset_y: 12.0,
        blur_radius: 14.0,
        spread_radius: 4.0,
        color: Color::new(0, 0, 0, 200),
        inset: false,
    }];

    let mid = interpolate_box_shadow(&s1, &s2, 0.5);
    assert_eq!(mid.len(), 1);
    assert_eq!(mid[0].offset_x, 4.0);
    assert_eq!(mid[0].offset_y, 8.0);
    assert_eq!(mid[0].blur_radius, 10.0);
    assert_eq!(mid[0].spread_radius, 2.0);
    assert_eq!(mid[0].color.a, 150);
}

#[test]
fn test_transition_manager_lifecycle_and_retargeting() {
    let mut manager = TransitionManager::new();
    let node_id = 42;

    let mut old_style = ComputedStyle::default();
    old_style.opacity = 1.0;

    let mut new_style = ComputedStyle::default();
    new_style.opacity = 0.0;
    new_style.transitions = vec![TransitionSpec {
        property: TransitionProperty::Opacity,
        duration_ms: 200.0,
        timing_function: TransitionEasing::Linear,
        delay_ms: 50.0,
        behavior: TransitionBehavior::Normal,
    }];

    // 1. Initial trigger at t = 1000.0
    let events = manager.update_styles(node_id, &old_style, &new_style, 1000.0);
    assert!(events.iter().any(|e| matches!(e, TransitionLifecycleEvent::Run { .. })));
    assert!(!events.iter().any(|e| matches!(e, TransitionLifecycleEvent::Start { .. })));
    assert!(manager.has_active_transitions());

    // 2. Advance to t = 1030.0 (during delay)
    let events = manager.advance(1030.0);
    assert!(events.is_empty());
    let mut current_style = new_style.clone();
    manager.apply_to_style(node_id, &mut current_style);
    assert_eq!(current_style.opacity, 1.0); // Delay phase retains initial value

    // 3. Advance to t = 1050.0 (delay elapsed, transition starts)
    let events = manager.advance(1050.0);
    assert!(events.iter().any(|e| matches!(e, TransitionLifecycleEvent::Start { .. })));

    // 4. Advance to t = 1150.0 (halfway through 200ms duration: active_time = 100ms)
    let _ = manager.advance(1150.0);
    let mut current_style = new_style.clone();
    manager.apply_to_style(node_id, &mut current_style);
    assert!((current_style.opacity - 0.5).abs() < 1e-4);

    // 5. Retarget / reverse mid-flight: target opacity goes back to 1.0!
    let mut retarget_style = current_style.clone();
    retarget_style.opacity = 1.0;
    retarget_style.transitions = new_style.transitions.clone();

    let events = manager.update_styles(node_id, &current_style, &retarget_style, 1150.0);
    // Cancellation of previous active transition
    assert!(events.iter().any(|e| matches!(e, TransitionLifecycleEvent::Cancel { .. })));

    // New transition starts smoothly from 0.5 without popping!
    let mut check_style = retarget_style.clone();
    manager.apply_to_style(node_id, &mut check_style);
    assert!((check_style.opacity - 0.5).abs() < 1e-4);

    // 6. Advance to completion
    let events = manager.advance(1350.0);
    assert!(events.iter().any(|e| matches!(e, TransitionLifecycleEvent::End { .. })));

    let mut final_style = retarget_style.clone();
    manager.apply_to_style(node_id, &mut final_style);
    assert_eq!(final_style.opacity, 1.0);
}
