//! W3C CSS 2D and 3D Transforms Level 1 & 2 comprehensive verification test suite.

#![allow(clippy::field_reassign_with_default)]

use strata_style::cascade::CascadeSolver;
use strata_style::parser::{parse_declarations, parse_transform_list, parse_transform_origin};
use strata_style::tailwind::TailwindEngine;
use strata_style::transition::{interpolate_transform_ops, ActiveTransition, AnimatableValue};
use strata_style::types::{
    BackfaceVisibility, ComputedStyle, Dimension, TransformOp, TransformOrigin,
    TransformStyle, TransitionBehavior, TransitionEasing, TransitionProperty, TransitionSpec,
};

#[test]
fn test_parse_transform_2d_and_3d() {
    // 1. Translations
    let ops = parse_transform_list("translate(10px, 20px)");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0], TransformOp::Translate(Dimension::Px(10.0), Dimension::Px(20.0)));

    let ops = parse_transform_list("translateX(50%) translateY(2rem)");
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0], TransformOp::Translate(Dimension::Percent(50.0), Dimension::Px(0.0)));
    assert_eq!(ops[1], TransformOp::Translate(Dimension::Px(0.0), Dimension::Rem(2.0)));

    let ops = parse_transform_list("translate3d(10px, 20px, 30px)");
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0],
        TransformOp::Translate3d(Dimension::Px(10.0), Dimension::Px(20.0), Dimension::Px(30.0))
    );

    // 2. Scales
    let ops = parse_transform_list("scale(1.5, 2.0)");
    assert_eq!(ops[0], TransformOp::Scale(1.5, 2.0));

    let ops = parse_transform_list("scaleX(0.5) scaleY(3.0)");
    assert_eq!(ops[0], TransformOp::Scale(0.5, 1.0));
    assert_eq!(ops[1], TransformOp::Scale(1.0, 3.0));

    let ops = parse_transform_list("scale3d(2.0, 3.0, 4.0)");
    assert_eq!(ops[0], TransformOp::Scale3d(2.0, 3.0, 4.0));

    // 3. Rotations & Angles (deg, rad, turn, grad)
    let ops = parse_transform_list("rotate(180deg)");
    if let TransformOp::Rotate(rad) = ops[0] {
        assert!((rad - std::f32::consts::PI).abs() < 1e-4);
    } else {
        panic!("Expected Rotate operation");
    }

    let ops = parse_transform_list("rotate(0.5turn)");
    if let TransformOp::Rotate(rad) = ops[0] {
        assert!((rad - std::f32::consts::PI).abs() < 1e-4);
    } else {
        panic!("Expected Rotate operation");
    }

    let ops = parse_transform_list("rotateX(90deg) rotateY(45deg) rotateZ(30deg)");
    assert_eq!(ops.len(), 3);
    assert!(matches!(ops[0], TransformOp::RotateX(_)));
    assert!(matches!(ops[1], TransformOp::RotateY(_)));
    assert!(matches!(ops[2], TransformOp::RotateZ(_)));

    let ops = parse_transform_list("rotate3d(0, 1, 0, 90deg)");
    if let TransformOp::Rotate3d(x, y, z, rad) = ops[0] {
        assert_eq!((x, y, z), (0.0, 1.0, 0.0));
        assert!((rad - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    } else {
        panic!("Expected Rotate3d operation");
    }

    // 4. Skew & Perspective
    let ops = parse_transform_list("skew(10deg, 20deg) perspective(400px)");
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], TransformOp::Skew(_, _)));
    assert_eq!(ops[1], TransformOp::Perspective(400.0));

    // 5. Matrix & Matrix3d
    let ops = parse_transform_list("matrix(1, 0, 0, 1, 50, 100)");
    assert_eq!(ops[0], TransformOp::Matrix([1.0, 0.0, 0.0, 1.0, 50.0, 100.0]));

    // 6. none keyword
    let ops = parse_transform_list("none");
    assert!(ops.is_empty());
}

#[test]
fn test_transform_origin_and_style_parsing() {
    let origin = parse_transform_origin("50% 50%").unwrap();
    assert_eq!(origin.x, Dimension::Percent(50.0));
    assert_eq!(origin.y, Dimension::Percent(50.0));
    assert_eq!(origin.z, 0.0);

    let origin = parse_transform_origin("top left").unwrap();
    assert_eq!(origin.x, Dimension::Percent(0.0));
    assert_eq!(origin.y, Dimension::Percent(0.0));

    let origin = parse_transform_origin("bottom right").unwrap();
    assert_eq!(origin.x, Dimension::Percent(100.0));
    assert_eq!(origin.y, Dimension::Percent(100.0));

    let origin = parse_transform_origin("center").unwrap();
    assert_eq!(origin.x, Dimension::Percent(50.0));
    assert_eq!(origin.y, Dimension::Percent(50.0));

    let origin = parse_transform_origin("20px 40px 100px").unwrap();
    assert_eq!(origin.x, Dimension::Px(20.0));
    assert_eq!(origin.y, Dimension::Px(40.0));
    assert_eq!(origin.z, 100.0);
}

#[test]
fn test_cascade_transforms_and_standalone_properties() {
    let mut style = ComputedStyle::default();
    let css = "transform: translate(100px, 50px) rotate(45deg); transform-origin: top left; transform-style: preserve-3d; backface-visibility: hidden; perspective: 800px;";
    let decls = parse_declarations(css);
    for decl in &decls {
        CascadeSolver::apply_declaration(&mut style, decl);
    }

    assert_eq!(style.transform.len(), 2);
    assert_eq!(style.transform_origin.x, Dimension::Percent(0.0));
    assert_eq!(style.transform_origin.y, Dimension::Percent(0.0));
    assert_eq!(style.transform_style, TransformStyle::Preserve3d);
    assert_eq!(style.backface_visibility, BackfaceVisibility::Hidden);
    assert_eq!(style.perspective, Some(800.0));
    assert!(style.has_transform());
    assert!(style.establishes_stacking_context());

    // Test modern standalone properties (scale, rotate, translate)
    let mut style2 = ComputedStyle::default();
    let css2 = "scale: 1.5 2.0; rotate: 90deg; translate: 30px 40px;";
    let decls2 = parse_declarations(css2);
    for decl in &decls2 {
        CascadeSolver::apply_declaration(&mut style2, decl);
    }

    assert_eq!(style2.transform.len(), 3);
    assert!(style2.has_transform());
}

#[test]
fn test_tailwind_transform_utilities() {
    let style = TailwindEngine::resolve_classes("scale-150 rotate-45 translate-x-4 -translate-y-8 origin-top-left");

    assert_eq!(style.transform.len(), 4);
    assert_eq!(style.transform[0], TransformOp::Scale(1.5, 1.5));
    if let TransformOp::Rotate(rad) = style.transform[1] {
        assert!((rad - std::f32::consts::FRAC_PI_4).abs() < 1e-4);
    } else {
        panic!("Expected rotate");
    }
    assert_eq!(style.transform[2], TransformOp::Translate(Dimension::Px(16.0), Dimension::Px(0.0)));
    assert_eq!(style.transform[3], TransformOp::Translate(Dimension::Px(0.0), Dimension::Px(-32.0)));
    assert_eq!(style.transform_origin.x, Dimension::Percent(0.0));
    assert_eq!(style.transform_origin.y, Dimension::Percent(0.0));
}

#[test]
fn test_matrix_composition_and_point_transformation() {
    let mut style = ComputedStyle::default();
    style.transform = vec![
        TransformOp::Translate(Dimension::Px(100.0), Dimension::Px(50.0)),
        TransformOp::Scale(2.0, 2.0),
    ];
    style.transform_origin = TransformOrigin {
        x: Dimension::Percent(50.0),
        y: Dimension::Percent(50.0),
        z: 0.0,
    };

    let width = 200.0;
    let height = 100.0;
    let mat = style.compute_transform_matrix(width, height);
    assert!(!mat.is_identity());

    // Center of element: (100, 50).
    // In local coords, center is origin (100, 50).
    // Scale 2 about center keeps center invariant, then translation (100, 50) moves center to (200, 100).
    let (cx, cy) = mat.transform_point(100.0, 50.0);
    assert!((cx - 200.0).abs() < 1e-4, "Expected cx=200, got {}", cx);
    assert!((cy - 100.0).abs() < 1e-4, "Expected cy=100, got {}", cy);

    // Inverse check: unproject (200, 100) must return (100, 50).
    let inv = mat.inverse().expect("Matrix must be invertible");
    let (un_x, un_y) = inv.transform_point(200.0, 100.0);
    assert!((un_x - 100.0).abs() < 1e-4);
    assert!((un_y - 50.0).abs() < 1e-4);
}

#[test]
fn test_transform_transition_interpolation() {
    let from_ops = vec![TransformOp::Translate(Dimension::Px(0.0), Dimension::Px(0.0))];
    let to_ops = vec![TransformOp::Translate(Dimension::Px(100.0), Dimension::Px(200.0))];

    // Midpoint t = 0.5
    let mid_ops = interpolate_transform_ops(&from_ops, &to_ops, 0.5);
    assert_eq!(mid_ops.len(), 1);
    assert_eq!(
        mid_ops[0],
        TransformOp::Translate(Dimension::Px(50.0), Dimension::Px(100.0))
    );

    // Identity expansion from empty transform (transform: none -> transform: scale(2))
    let empty_ops = Vec::new();
    let scale_ops = vec![TransformOp::Scale(2.0, 2.0)];
    let mid_scale = interpolate_transform_ops(&empty_ops, &scale_ops, 0.5);
    assert_eq!(mid_scale.len(), 1);
    assert_eq!(mid_scale[0], TransformOp::Scale(1.5, 1.5));

    // ActiveTransition execution
    let spec = TransitionSpec {
        property: TransitionProperty::Transform,
        duration_ms: 500.0,
        timing_function: TransitionEasing::Linear,
        delay_ms: 0.0,
        behavior: TransitionBehavior::Normal,
    };

    let mut trans = ActiveTransition::new(
        TransitionProperty::Transform,
        &spec,
        AnimatableValue::Transform(empty_ops),
        AnimatableValue::Transform(to_ops),
        1000.0,
    );

    let events = trans.step(42, 1250.0); // 50% elapsed
    assert!(!events.is_empty());
    if let AnimatableValue::Transform(curr) = &trans.current_value {
        assert_eq!(curr.len(), 1);
        assert_eq!(
            curr[0],
            TransformOp::Translate(Dimension::Px(50.0), Dimension::Px(100.0))
        );
    } else {
        panic!("Expected AnimatableValue::Transform");
    }
}
