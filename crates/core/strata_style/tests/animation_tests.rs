#![allow(clippy::field_reassign_with_default)]

use strata_style::animation::{AnimationLifecycleEvent, AnimationManager, KeyframeRegistry};
use strata_style::cascade::CascadeSolver;
use strata_style::parser::{parse_animations, StyleSheet};
use strata_style::selector::PseudoState;
use strata_style::tailwind::TailwindEngine;
use strata_style::transition::AnimatableValue;
use strata_style::types::{
    AnimationDirection, AnimationFillMode, AnimationIterationCount, AnimationPlayState,
    AnimationSpec, ComputedStyle, TransformOp, TransitionEasing,
    TransitionProperty,
};

#[test]
fn test_parse_animation_shorthand_and_longhands() {
    let anims = parse_animations("spin 1s linear infinite, pulse 2000ms ease-in-out 500ms alternate forwards");
    assert_eq!(anims.len(), 2);

    assert_eq!(anims[0].name, "spin");
    assert_eq!(anims[0].duration_ms, 1000.0);
    assert_eq!(anims[0].timing_function, TransitionEasing::Linear);
    assert_eq!(anims[0].iteration_count, AnimationIterationCount::Infinite);
    assert_eq!(anims[0].direction, AnimationDirection::Normal);
    assert_eq!(anims[0].fill_mode, AnimationFillMode::None);
    assert_eq!(anims[0].play_state, AnimationPlayState::Running);

    assert_eq!(anims[1].name, "pulse");
    assert_eq!(anims[1].duration_ms, 2000.0);
    assert_eq!(anims[1].timing_function, TransitionEasing::EaseInOut);
    assert_eq!(anims[1].delay_ms, 500.0);
    assert_eq!(anims[1].direction, AnimationDirection::Alternate);
    assert_eq!(anims[1].fill_mode, AnimationFillMode::Forwards);

    // Test cascade resolution of animation longhands
    let css = r#"
        .hero {
            animation-name: custom-fade, custom-slide;
            animation-duration: 400ms, 800ms;
            animation-timing-function: ease-out, linear;
            animation-iteration-count: 3, infinite;
            animation-direction: alternate, reverse;
            animation-fill-mode: both, forwards;
            animation-play-state: paused, running;
        }
    "#;
    let sheet = StyleSheet::parse(css).expect("Failed to parse CSS with animation longhands");
    let style = CascadeSolver::resolve(
        "div",
        &["hero"],
        None,
        None,
        &[&sheet],
        None,
        &PseudoState::default(),
    );

    assert_eq!(style.animations.len(), 2);
    assert_eq!(style.animations[0].name, "custom-fade");
    assert_eq!(style.animations[0].duration_ms, 400.0);
    assert_eq!(style.animations[0].iteration_count, AnimationIterationCount::Count(3.0));
    assert_eq!(style.animations[0].direction, AnimationDirection::Alternate);
    assert_eq!(style.animations[0].fill_mode, AnimationFillMode::Both);
    assert_eq!(style.animations[0].play_state, AnimationPlayState::Paused);

    assert_eq!(style.animations[1].name, "custom-slide");
    assert_eq!(style.animations[1].duration_ms, 800.0);
    assert_eq!(style.animations[1].iteration_count, AnimationIterationCount::Infinite);
    assert_eq!(style.animations[1].direction, AnimationDirection::Reverse);
    assert_eq!(style.animations[1].fill_mode, AnimationFillMode::Forwards);
    assert_eq!(style.animations[1].play_state, AnimationPlayState::Running);
}

#[test]
fn test_parse_keyframes_stylesheet() {
    let css = r#"
        @keyframes spin {
            from {
                transform: rotate(0deg);
            }
            to {
                transform: rotate(360deg);
            }
        }

        @keyframes multi_step {
            0% {
                opacity: 0;
                transform: scale(0.5);
            }
            50% {
                opacity: 0.8;
                transform: scale(1.2);
            }
            100% {
                opacity: 1;
                transform: scale(1.0);
            }
        }

        @keyframes grouped_stops {
            0%, 100% {
                transform: translateY(-25%);
                animation-timing-function: cubic-bezier(0.8, 0, 1, 1);
            }
            50% {
                transform: translateY(0px);
            }
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("Failed to parse stylesheet with @keyframes");
    assert_eq!(sheet.keyframes.len(), 3);

    let spin = &sheet.keyframes[0];
    assert_eq!(spin.name, "spin");
    assert_eq!(spin.stops.len(), 2);
    assert_eq!(spin.stops[0].offset, 0.0);
    assert_eq!(spin.stops[1].offset, 1.0);

    let multi = &sheet.keyframes[1];
    assert_eq!(multi.name, "multi_step");
    assert_eq!(multi.stops.len(), 3);
    assert_eq!(multi.stops[0].offset, 0.0);
    assert_eq!(multi.stops[1].offset, 0.5);
    assert_eq!(multi.stops[2].offset, 1.0);

    let grouped = &sheet.keyframes[2];
    assert_eq!(grouped.name, "grouped_stops");
    // Grouped 0%, 100% should expand to two stops plus the 50% stop
    assert_eq!(grouped.stops.len(), 3);
    assert_eq!(grouped.stops[0].offset, 0.0);
    assert_eq!(grouped.stops[1].offset, 0.5);
    assert_eq!(grouped.stops[2].offset, 1.0);
}

#[test]
fn test_tailwind_animation_utilities() {
    let spin_style = TailwindEngine::resolve_classes("animate-spin");
    assert_eq!(spin_style.animations.len(), 1);
    assert_eq!(spin_style.animations[0].name, "spin");
    assert_eq!(spin_style.animations[0].duration_ms, 1000.0);
    assert_eq!(spin_style.animations[0].timing_function, TransitionEasing::Linear);
    assert_eq!(spin_style.animations[0].iteration_count, AnimationIterationCount::Infinite);

    let ping_style = TailwindEngine::resolve_classes("animate-ping");
    assert_eq!(ping_style.animations.len(), 1);
    assert_eq!(ping_style.animations[0].name, "ping");
    assert_eq!(ping_style.animations[0].duration_ms, 1000.0);
    assert_eq!(ping_style.animations[0].iteration_count, AnimationIterationCount::Infinite);

    let pulse_style = TailwindEngine::resolve_classes("animate-pulse");
    assert_eq!(pulse_style.animations.len(), 1);
    assert_eq!(pulse_style.animations[0].name, "pulse");
    assert_eq!(pulse_style.animations[0].duration_ms, 2000.0);
    assert_eq!(pulse_style.animations[0].iteration_count, AnimationIterationCount::Infinite);

    let bounce_style = TailwindEngine::resolve_classes("animate-bounce");
    assert_eq!(bounce_style.animations.len(), 1);
    assert_eq!(bounce_style.animations[0].name, "bounce");

    let none_style = TailwindEngine::resolve_classes("animate-none");
    assert!(none_style.animations.is_empty());

    let custom_style = TailwindEngine::resolve_classes("animate-[custom_750ms_ease-in-out_infinite]");
    assert_eq!(custom_style.animations.len(), 1);
    assert_eq!(custom_style.animations[0].name, "custom");
    assert_eq!(custom_style.animations[0].duration_ms, 750.0);
    assert_eq!(custom_style.animations[0].timing_function, TransitionEasing::EaseInOut);
    assert_eq!(custom_style.animations[0].iteration_count, AnimationIterationCount::Infinite);
}

#[test]
fn test_keyframe_track_sampling_and_sub_segment_easing() {
    let registry = KeyframeRegistry::new();
    let spin_track = registry.get("spin").expect("spin track missing from registry");

    let base_style = ComputedStyle::default();

    // 0% -> rotate(0deg)
    let sample_0 = spin_track.sample(0.0, &base_style);
    if let Some(AnimatableValue::Transform(ops)) = sample_0.get(&TransitionProperty::Transform) {
        assert_eq!(ops.len(), 1);
        if let TransformOp::Rotate(rad) = ops[0] {
            assert!(rad.abs() < 1e-5);
        } else {
            panic!("Expected Rotate operation");
        }
    } else {
        panic!("Missing transform in sample 0");
    }

    // 50% -> rotate(180deg = PI rad)
    let sample_50 = spin_track.sample(0.5, &base_style);
    if let Some(AnimatableValue::Transform(ops)) = sample_50.get(&TransitionProperty::Transform) {
        if let TransformOp::Rotate(rad) = ops[0] {
            assert!((rad - std::f32::consts::PI).abs() < 1e-4);
        } else {
            panic!("Expected Rotate operation");
        }
    } else {
        panic!("Missing transform in sample 50");
    }

    // 100% -> rotate(360deg = 2*PI rad)
    let sample_100 = spin_track.sample(1.0, &base_style);
    if let Some(AnimatableValue::Transform(ops)) = sample_100.get(&TransitionProperty::Transform) {
        if let TransformOp::Rotate(rad) = ops[0] {
            assert!((rad - 2.0 * std::f32::consts::PI).abs() < 1e-4);
        } else {
            panic!("Expected Rotate operation");
        }
    } else {
        panic!("Missing transform in sample 100");
    }
}

#[test]
fn test_animation_manager_lifecycle_iterations_and_directions() {
    let mut manager = AnimationManager::new();
    let node_id = 42;

    let mut style = ComputedStyle::default();
    style.animations = vec![AnimationSpec {
        name: "pulse".to_string(),
        duration_ms: 1000.0,
        timing_function: TransitionEasing::Linear,
        delay_ms: 200.0,
        iteration_count: AnimationIterationCount::Count(2.0),
        direction: AnimationDirection::Normal,
        fill_mode: AnimationFillMode::Forwards,
        play_state: AnimationPlayState::Running,
        ..Default::default()
    }];

    // 1. Initial attachment at t=0ms (Delay phase)
    let events = manager.update_styles(node_id, &style, 0.0);
    assert!(events.is_empty(), "No events during delay phase");
    assert!(manager.has_active_animations());

    let mut applied_style = style.clone();
    manager.apply_to_style(node_id, &mut applied_style);
    // FillMode is Forwards, not Backwards, so delay phase doesn't override opacity
    assert_eq!(applied_style.opacity, 1.0);

    // 2. Advance to t=200ms (Delay passes, Start emitted)
    let base_styles = std::collections::HashMap::from([(node_id, style.clone())]);
    let events = manager.advance(200.0, &base_styles);
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], AnimationLifecycleEvent::Start { .. }));

    // 3. Advance to t=700ms (Active: 500ms / 1000ms = 50% progress in first iteration)
    // pulse at 50% opacity is 0.5
    let _ = manager.advance(700.0, &base_styles);
    manager.apply_to_style(node_id, &mut applied_style);
    assert!((applied_style.opacity - 0.5).abs() < 1e-3);

    // 4. Advance to t=1250ms (Active: 1050ms > 1000ms, iteration 1 boundary passed)
    let events = manager.advance(1250.0, &base_styles);
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], AnimationLifecycleEvent::Iteration { .. }));

    // 5. Advance to t=2300ms (Active: 2100ms >= 2 * 1000ms, completed!)
    let events = manager.advance(2300.0, &base_styles);
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], AnimationLifecycleEvent::End { .. }));

    // Verify fill_mode: forwards retains the final pulse frame (opacity 1.0)
    manager.apply_to_style(node_id, &mut applied_style);
    assert_eq!(applied_style.opacity, 1.0);
}
