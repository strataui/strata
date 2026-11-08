//! Dedicated verification test suite for GOAL-37:
//! CSS Transitions & Animations Real-Time Interpolation:
//! - De Casteljau's cubic bezier root-finding algorithm
//! - Perceptually uniform Oklab color space interpolation
//! - Transform matrix decomposition (translation, scale, shortest-arc rotation, skew)
//! - Clip-path shape interpolation (Inset, Circle, Ellipse, Polygon)

use std::f32::consts::PI;
use strata_primitives::geom::Point;
use strata_style::bezier::cubic_bezier_solve;
use strata_style::effects::{interpolate_clip_path, ClipPath};
use strata_style::matrix_decomp::Decomposed2d;
use strata_style::oklab::{interpolate_color_oklab, oklab_to_rgba, rgba_to_oklab};
use strata_style::transition::{
    apply_animatable_value, extract_animatable_value, interpolate_transform_ops, ActiveTransition,
    AnimatableValue,
};
use strata_style::types::{
    Color, ComputedStyle, Dimension, TransformOp, TransitionBehavior, TransitionEasing,
    TransitionProperty, TransitionSpec,
};

#[test]
fn test_de_casteljau_cubic_bezier_root_finding_curves() {
    // 1. Boundary conditions
    assert_eq!(cubic_bezier_solve(0.0, 0.25, 0.1, 0.25, 1.0), 0.0);
    assert_eq!(cubic_bezier_solve(1.0, 0.25, 0.1, 0.25, 1.0), 1.0);

    // 2. Linear diagonal
    for step in 0..=10 {
        let x = step as f32 / 10.0;
        let y = cubic_bezier_solve(x, 0.0, 0.0, 1.0, 1.0);
        assert!((y - x).abs() < 1e-5, "Linear solve failed at x={x}: y={y}");
    }

    // 3. Tailwind standard curve: cubic-bezier(0.4, 0.0, 0.2, 1.0)
    let y_quarter = cubic_bezier_solve(0.25, 0.4, 0.0, 0.2, 1.0);
    let y_half = cubic_bezier_solve(0.5, 0.4, 0.0, 0.2, 1.0);
    let y_three_quarters = cubic_bezier_solve(0.75, 0.4, 0.0, 0.2, 1.0);
    assert!(y_half > 0.5, "Tailwind curve must accelerate in midsection");
    assert!(y_quarter < y_half);
    assert!(y_half < y_three_quarters);

    // 4. Material standard decelerate curve: cubic-bezier(0.0, 0.0, 0.2, 1.0)
    let y_decel = cubic_bezier_solve(0.5, 0.0, 0.0, 0.2, 1.0);
    assert!(y_decel > 0.7, "Decelerate curve must rise quickly");

    // 5. Overshoot / bounce curve: cubic-bezier(0.68, -0.55, 0.265, 1.55)
    let y_overshoot = cubic_bezier_solve(0.9, 0.68, -0.55, 0.265, 1.55);
    assert!(y_overshoot > 0.9, "Overshoot curve solves stably with De Casteljau");
}

#[test]
fn test_perceptually_uniform_oklab_color_interpolation() {
    let red = Color::new(255, 0, 0, 255);
    let green = Color::new(0, 255, 0, 255);
    let blue = Color::new(0, 0, 255, 255);

    // Roundtrip verification: RGBA -> Oklab -> RGBA
    let (l, a, b, alpha) = rgba_to_oklab(red);
    let back = oklab_to_rgba(l, a, b, alpha);
    assert!((back.r as i32 - 255).abs() <= 1);
    assert!(back.g <= 1);
    assert!(back.b <= 1);

    // Interpolating blue to red in Oklab produces vibrant purple without gray mud
    let mid_oklab = interpolate_color_oklab(blue, red, 0.5);
    assert!(mid_oklab.r > 100);
    assert!(mid_oklab.b > 100);
    assert_eq!(mid_oklab.a, 255);

    // Interpolating red to green produces vibrant yellow/amber in Oklab
    let mid_rg = interpolate_color_oklab(red, green, 0.5);
    assert!(mid_rg.r > 150, "Oklab midpoint must have high red component");
    assert!(mid_rg.g > 120, "Oklab midpoint must have high green component");
}

#[test]
fn test_matrix_decomposition_and_shortest_arc_interpolation() {
    // 1. Pure translation matrix
    let m_trans = [1.0, 0.0, 0.0, 1.0, 50.0, 100.0];
    let decomp_t = Decomposed2d::from_matrix(m_trans).expect("Decompose translation");
    assert_eq!(decomp_t.translate, (50.0, 100.0));
    assert_eq!(decomp_t.scale, (1.0, 1.0));
    assert!(decomp_t.angle.abs() < 1e-6);

    // 2. Pure scale matrix
    let m_scale = [2.0, 0.0, 0.0, 3.0, 0.0, 0.0];
    let decomp_s = Decomposed2d::from_matrix(m_scale).expect("Decompose scale");
    assert_eq!(decomp_s.translate, (0.0, 0.0));
    assert_eq!(decomp_s.scale, (2.0, 3.0));

    // 3. Pure rotation matrix (90 degrees = PI/2)
    let theta = PI / 2.0;
    let m_rot = [theta.cos(), theta.sin(), -theta.sin(), theta.cos(), 0.0, 0.0];
    let decomp_r = Decomposed2d::from_matrix(m_rot).expect("Decompose rotation");
    assert!((decomp_r.angle - theta).abs() < 1e-5);
    assert!((decomp_r.scale.0 - 1.0).abs() < 1e-5);
    assert!((decomp_r.scale.1 - 1.0).abs() < 1e-5);

    // 4. Shortest arc rotation interpolation (e.g. from 350 deg to 10 deg)
    let d1 = Decomposed2d {
        translate: (0.0, 0.0),
        scale: (1.0, 1.0),
        angle: 350.0 * PI / 180.0,
        skew: 0.0,
    };
    let d2 = Decomposed2d {
        translate: (0.0, 0.0),
        scale: (1.0, 1.0),
        angle: 10.0 * PI / 180.0,
        skew: 0.0,
    };
    let mid_rot = d1.interpolate(&d2, 0.5);
    // Midpoint must be 0 deg (or 360 deg), taking the 20 deg shortest arc rather than spinning 340 deg
    let angle_deg = (mid_rot.angle * 180.0 / PI).rem_euclid(360.0);
    assert!(
        angle_deg < 1.0 || (angle_deg - 360.0).abs() < 1.0,
        "Shortest arc angle must be ~0 deg: {angle_deg}"
    );

    // 5. Incompatible transform lists interpolate via matrix decomposition without popping
    let from_ops = vec![TransformOp::Rotate(0.0)];
    let to_ops = vec![
        TransformOp::Translate(Dimension::Px(100.0), Dimension::Px(50.0)),
        TransformOp::Scale(1.5, 1.5),
    ];
    let interpolated = interpolate_transform_ops(&from_ops, &to_ops, 0.5);
    assert_eq!(interpolated.len(), 1);
    if let TransformOp::Matrix(m) = &interpolated[0] {
        let decomp = Decomposed2d::from_matrix(*m).expect("Decompose interpolated matrix");
        assert!((decomp.translate.0 - 50.0).abs() < 1.0);
        assert!((decomp.translate.1 - 25.0).abs() < 1.0);
        assert!((decomp.scale.0 - 1.25).abs() < 0.1);
    } else {
        panic!("Expected TransformOp::Matrix from differing length transform interpolation");
    }
}

#[test]
fn test_clip_path_continuous_interpolation() {
    // 1. Inset clip path interpolation
    let inset1 = ClipPath::Inset {
        top: 10.0,
        right: 20.0,
        bottom: 30.0,
        left: 40.0,
        border_radius: 5.0,
    };
    let inset2 = ClipPath::Inset {
        top: 30.0,
        right: 40.0,
        bottom: 50.0,
        left: 60.0,
        border_radius: 15.0,
    };
    let mid_inset = interpolate_clip_path(&inset1, &inset2, 0.5);
    assert_eq!(
        mid_inset,
        ClipPath::Inset {
            top: 20.0,
            right: 30.0,
            bottom: 40.0,
            left: 50.0,
            border_radius: 10.0,
        }
    );

    // 2. Circle clip path interpolation
    let circle1 = ClipPath::Circle {
        radius: 20.0,
        center_x: 50.0,
        center_y: 50.0,
    };
    let circle2 = ClipPath::Circle {
        radius: 60.0,
        center_x: 150.0,
        center_y: 150.0,
    };
    let mid_circle = interpolate_clip_path(&circle1, &circle2, 0.5);
    assert_eq!(
        mid_circle,
        ClipPath::Circle {
            radius: 40.0,
            center_x: 100.0,
            center_y: 100.0,
        }
    );

    // 3. Polygon clip path interpolation
    let poly1 = ClipPath::Polygon(vec![
        Point::new(0.0, 0.0),
        Point::new(100.0, 0.0),
        Point::new(100.0, 100.0),
    ]);
    let poly2 = ClipPath::Polygon(vec![
        Point::new(20.0, 20.0),
        Point::new(200.0, 40.0),
        Point::new(160.0, 180.0),
    ]);
    let mid_poly = interpolate_clip_path(&poly1, &poly2, 0.5);
    if let ClipPath::Polygon(verts) = mid_poly {
        assert_eq!(verts.len(), 3);
        assert_eq!(verts[0], Point::new(10.0, 10.0));
        assert_eq!(verts[1], Point::new(150.0, 20.0));
        assert_eq!(verts[2], Point::new(130.0, 140.0));
    } else {
        panic!("Expected ClipPath::Polygon");
    }
}

#[test]
fn test_active_transition_real_time_interpolation_with_cubic_bezier() {
    let spec = TransitionSpec {
        property: TransitionProperty::Transform,
        duration_ms: 300.0,
        timing_function: TransitionEasing::CubicBezier(0.4, 0.0, 0.2, 1.0),
        delay_ms: 0.0,
        behavior: TransitionBehavior::Normal,
    };

    let from_val = AnimatableValue::Transform(vec![TransformOp::Scale(1.0, 1.0)]);
    let to_val = AnimatableValue::Transform(vec![TransformOp::Scale(1.02, 1.02)]);

    let mut transition = ActiveTransition::new(
        TransitionProperty::Transform,
        &spec,
        from_val,
        to_val,
        0.0, // start timestamp
    );

    // Step at 150ms (halfway through duration)
    let _events = transition.step(1, 150.0);
    if let AnimatableValue::Transform(ops) = &transition.current_value {
        assert_eq!(ops.len(), 1);
        if let TransformOp::Scale(sx, sy) = ops[0] {
            assert!(sx > 1.0 && sx < 1.02);
            assert!(sy > 1.0 && sy < 1.02);
            // Cubic bezier(0.4, 0.0, 0.2, 1.0) progress at t=0.5 is > 0.5, so scale > 1.01
            assert!(sx > 1.01, "Eased scale at halfway must exceed 1.01: {sx}");
        } else {
            panic!("Expected TransformOp::Scale");
        }
    } else {
        panic!("Expected AnimatableValue::Transform");
    }

    // Step at 300ms (completion)
    let events = transition.step(1, 300.0);
    assert!(transition.is_finished());
    assert!(!events.is_empty());
    if let AnimatableValue::Transform(ops) = &transition.current_value {
        assert_eq!(ops[0], TransformOp::Scale(1.02, 1.02));
    }

    // Apply to computed style and extract back
    let mut style = ComputedStyle::default();
    apply_animatable_value(
        &mut style,
        &TransitionProperty::Transform,
        &transition.current_value,
    );
    let extracted = extract_animatable_value(&style, &TransitionProperty::Transform);
    assert_eq!(extracted, Some(transition.current_value));
}
