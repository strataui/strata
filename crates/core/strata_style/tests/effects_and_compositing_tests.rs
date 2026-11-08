//! Comprehensive Test Suite: W3C CSS Visual Effects, Blend Modes, Isolation, Filters, and Vector Clip Paths.

#![allow(clippy::field_reassign_with_default)]

use strata_primitives::geom::Point;
use strata_style::cascade::CascadeSolver;
use strata_style::effects::{ClipPath, FilterOp};
use strata_style::parser::{parse_clip_path, parse_filter_list, Declaration};
use strata_style::tailwind::TailwindEngine;
use strata_style::types::{BlendMode, Color, ComputedStyle, Isolation};

#[test]
fn test_w3c_blend_modes_parsing_and_cascade() {
    let blend_mode_names = [
        ("normal", BlendMode::Normal),
        ("multiply", BlendMode::Multiply),
        ("screen", BlendMode::Screen),
        ("overlay", BlendMode::Overlay),
        ("darken", BlendMode::Darken),
        ("lighten", BlendMode::Lighten),
        ("color-dodge", BlendMode::ColorDodge),
        ("color-burn", BlendMode::ColorBurn),
        ("hard-light", BlendMode::HardLight),
        ("soft-light", BlendMode::SoftLight),
        ("difference", BlendMode::Difference),
        ("exclusion", BlendMode::Exclusion),
        ("hue", BlendMode::Hue),
        ("saturation", BlendMode::Saturation),
        ("color", BlendMode::Color),
        ("luminosity", BlendMode::Luminosity),
    ];

    for (name, expected) in blend_mode_names {
        assert_eq!(BlendMode::parse(name), Some(expected), "Failed parsing blend mode: {}", name);
        assert_eq!(expected.as_str(), name);

        let mut style = ComputedStyle::default();
        let decl = Declaration {
            property: "mix-blend-mode".to_string(),
            value: name.to_string(),
            important: false,
        };
        CascadeSolver::apply_declaration_with_parent(&mut style, &decl, None);
        assert_eq!(style.mix_blend_mode, expected, "Cascade mismatch for mix-blend-mode: {}", name);
    }
}

#[test]
fn test_w3c_isolation_parsing_and_cascade() {
    assert_eq!(Isolation::parse("auto"), Some(Isolation::Auto));
    assert_eq!(Isolation::parse("isolate"), Some(Isolation::Isolate));
    assert_eq!(Isolation::Auto.as_str(), "auto");
    assert_eq!(Isolation::Isolate.as_str(), "isolate");

    let mut style_isolated = ComputedStyle::default();
    let decl_iso = Declaration {
        property: "isolation".to_string(),
        value: "isolate".to_string(),
        important: false,
    };
    CascadeSolver::apply_declaration_with_parent(&mut style_isolated, &decl_iso, None);
    assert_eq!(style_isolated.isolation, Isolation::Isolate);

    let mut style_auto = ComputedStyle::default();
    style_auto.isolation = Isolation::Isolate;
    let decl_auto = Declaration {
        property: "isolation".to_string(),
        value: "auto".to_string(),
        important: false,
    };
    CascadeSolver::apply_declaration_with_parent(&mut style_auto, &decl_auto, None);
    assert_eq!(style_auto.isolation, Isolation::Auto);
}

#[test]
fn test_filter_functions_parsing() {
    // 1. Gaussian blur
    let f_blur = parse_filter_list("blur(8px)").expect("blur must parse");
    assert_eq!(f_blur.operations.len(), 1);
    assert_eq!(f_blur.operations[0], FilterOp::Blur(8.0));

    // 2. Brightness with float and percentage
    let f_bright1 = parse_filter_list("brightness(1.5)").expect("brightness float must parse");
    assert_eq!(f_bright1.operations[0], FilterOp::Brightness(1.5));
    let f_bright2 = parse_filter_list("brightness(150%)").expect("brightness % must parse");
    assert_eq!(f_bright2.operations[0], FilterOp::Brightness(1.5));

    // 3. Contrast
    let f_contrast = parse_filter_list("contrast(200%)").expect("contrast must parse");
    assert_eq!(f_contrast.operations[0], FilterOp::Contrast(2.0));

    // 4. Grayscale
    let f_gray = parse_filter_list("grayscale(100%)").expect("grayscale must parse");
    assert_eq!(f_gray.operations[0], FilterOp::Grayscale(1.0));

    // 5. Invert and Sepia
    let f_invert = parse_filter_list("invert(0.5)").expect("invert must parse");
    assert_eq!(f_invert.operations[0], FilterOp::Invert(0.5));
    let f_sepia = parse_filter_list("sepia(0.8)").expect("sepia must parse");
    assert_eq!(f_sepia.operations[0], FilterOp::Sepia(0.8));

    // 6. Hue-rotate with deg
    let f_hue = parse_filter_list("hue-rotate(180deg)").expect("hue-rotate must parse");
    assert_eq!(f_hue.operations[0], FilterOp::HueRotate(180.0));

    // 7. Drop-shadow filter
    let f_shadow = parse_filter_list("drop-shadow(2px 4px 6px #333333)").expect("drop-shadow must parse");
    assert_eq!(f_shadow.operations.len(), 1);
    match f_shadow.operations[0] {
        FilterOp::DropShadow { offset_x, offset_y, blur_radius, color } => {
            assert_eq!(offset_x, 2.0);
            assert_eq!(offset_y, 4.0);
            assert_eq!(blur_radius, 6.0);
            assert_eq!(color, Color::rgb(0x33, 0x33, 0x33));
        }
        _ => panic!("Expected DropShadow filter operation"),
    }

    // 8. Chained multi-filter
    let chained = parse_filter_list("blur(4px) brightness(1.2) contrast(1.1)").expect("chained filters must parse");
    assert_eq!(chained.operations.len(), 3);
    assert_eq!(chained.operations[0], FilterOp::Blur(4.0));
    assert_eq!(chained.operations[1], FilterOp::Brightness(1.2));
    assert_eq!(chained.operations[2], FilterOp::Contrast(1.1));
}

#[test]
fn test_clip_path_basic_shapes_and_containment() {
    // 1. Inset rectangular clip path
    let cp_inset = parse_clip_path("inset(10px 20px 30px 40px round 5px)").expect("inset must parse");
    match cp_inset {
        ClipPath::Inset { top, right, bottom, left, border_radius } => {
            assert_eq!(top, 10.0);
            assert_eq!(right, 20.0);
            assert_eq!(bottom, 30.0);
            assert_eq!(left, 40.0);
            assert_eq!(border_radius, 5.0);
        }
        _ => panic!("Expected Inset clip path"),
    }
    // Containment test: container is 200x200. Allowed x in [40, 180], y in [10, 170]
    assert!(cp_inset.contains_point(50.0, 50.0, 200.0, 200.0));
    assert!(!cp_inset.contains_point(20.0, 50.0, 200.0, 200.0)); // x < 40
    assert!(!cp_inset.contains_point(190.0, 50.0, 200.0, 200.0)); // x > 180
    assert!(!cp_inset.contains_point(50.0, 5.0, 200.0, 200.0)); // y < 10
    assert!(!cp_inset.contains_point(50.0, 180.0, 200.0, 200.0)); // y > 170

    // 2. Circular clip path: circle(50px at 100px 100px)
    let cp_circle = parse_clip_path("circle(50px at 100px 100px)").expect("circle must parse");
    match cp_circle {
        ClipPath::Circle { radius, center_x, center_y } => {
            assert_eq!(radius, 50.0);
            assert_eq!(center_x, 100.0);
            assert_eq!(center_y, 100.0);
        }
        _ => panic!("Expected Circle clip path"),
    }
    assert!(cp_circle.contains_point(100.0, 100.0, 200.0, 200.0)); // center
    assert!(cp_circle.contains_point(130.0, 100.0, 200.0, 200.0)); // dist = 30 < 50
    assert!(!cp_circle.contains_point(160.0, 100.0, 200.0, 200.0)); // dist = 60 > 50

    // 3. Elliptical clip path: ellipse(60px 30px at 100px 100px)
    let cp_ellipse = parse_clip_path("ellipse(60px 30px at 100px 100px)").expect("ellipse must parse");
    match cp_ellipse {
        ClipPath::Ellipse { radius_x, radius_y, center_x, center_y } => {
            assert_eq!(radius_x, 60.0);
            assert_eq!(radius_y, 30.0);
            assert_eq!(center_x, 100.0);
            assert_eq!(center_y, 100.0);
        }
        _ => panic!("Expected Ellipse clip path"),
    }
    assert!(cp_ellipse.contains_point(140.0, 100.0, 200.0, 200.0)); // dx=40 <= 60, dy=0
    assert!(!cp_ellipse.contains_point(100.0, 140.0, 200.0, 200.0)); // dy=40 > 30

    // 4. Polygon: polygon(0px 0px, 100px 0px, 100px 100px, 0px 100px)
    let cp_poly = parse_clip_path("polygon(0px 0px, 100px 0px, 100px 100px, 0px 100px)").expect("polygon must parse");
    match &cp_poly {
        ClipPath::Polygon(pts) => {
            assert_eq!(pts.len(), 4);
            assert_eq!(pts[0], Point::new(0.0, 0.0));
            assert_eq!(pts[1], Point::new(100.0, 0.0));
            assert_eq!(pts[2], Point::new(100.0, 100.0));
            assert_eq!(pts[3], Point::new(0.0, 100.0));
        }
        _ => panic!("Expected Polygon clip path"),
    }
    assert!(cp_poly.contains_point(50.0, 50.0, 200.0, 200.0));
    assert!(!cp_poly.contains_point(150.0, 50.0, 200.0, 200.0));

    // 5. SVG Path: path("M 0 0 L 100 0 L 50 100 Z")
    let cp_path = parse_clip_path("path(\"M 0 0 L 100 0 L 50 100 Z\")").expect("path must parse");
    match cp_path {
        ClipPath::Path(ref d) => assert_eq!(d, "M 0 0 L 100 0 L 50 100 Z"),
        _ => panic!("Expected Path clip path"),
    }
}

#[test]
fn test_tailwind_visual_effects_utilities() {
    // 1. Mix-blend utilities
    let style = TailwindEngine::resolve_classes("mix-blend-multiply isolate");
    assert_eq!(style.mix_blend_mode, BlendMode::Multiply);
    assert_eq!(style.isolation, Isolation::Isolate);

    let style2 = TailwindEngine::resolve_classes("mix-blend-screen isolation-auto");
    assert_eq!(style2.mix_blend_mode, BlendMode::Screen);
    assert_eq!(style2.isolation, Isolation::Auto);

    // 2. Filter utilities
    let style_filters = TailwindEngine::resolve_classes("filter blur-md brightness-125 contrast-150 grayscale invert sepia");
    let filter = style_filters.filter.expect("filter should be populated");
    assert!(filter.operations.iter().any(|op| matches!(op, FilterOp::Blur(r) if (r - 12.0).abs() < 1e-4)));
    assert!(filter.operations.iter().any(|op| matches!(op, FilterOp::Brightness(b) if (b - 1.25).abs() < 1e-4)));
    assert!(filter.operations.iter().any(|op| matches!(op, FilterOp::Contrast(c) if (c - 1.5).abs() < 1e-4)));
    assert!(filter.operations.iter().any(|op| matches!(op, FilterOp::Grayscale(g) if (g - 1.0).abs() < 1e-4)));
    assert!(filter.operations.iter().any(|op| matches!(op, FilterOp::Invert(i) if (i - 1.0).abs() < 1e-4)));
    assert!(filter.operations.iter().any(|op| matches!(op, FilterOp::Sepia(s) if (s - 1.0).abs() < 1e-4)));

    // 3. Backdrop filter utilities
    let style_backdrop = TailwindEngine::resolve_classes("backdrop-filter backdrop-blur-lg");
    let bf = style_backdrop.backdrop_filter.expect("backdrop_filter should be populated");
    assert!(bf.operations.iter().any(|op| matches!(op, FilterOp::Blur(r) if (r - 16.0).abs() < 1e-4)));
}

#[test]
fn test_stacking_context_creation_triggers() {
    let mut style = ComputedStyle::default();
    assert!(!style.establishes_stacking_context());
    assert!(!style.has_visual_effects());

    // Mix-blend mode establishes stacking context
    style.mix_blend_mode = BlendMode::Multiply;
    assert!(style.has_visual_effects());
    assert!(style.establishes_stacking_context());
    style.mix_blend_mode = BlendMode::Normal;

    // Isolation: isolate establishes stacking context
    style.isolation = Isolation::Isolate;
    assert!(style.has_visual_effects());
    assert!(style.establishes_stacking_context());
    style.isolation = Isolation::Auto;

    // Filter establishes stacking context
    style.filter = Some(strata_style::effects::BackdropFilter::blur(5.0));
    assert!(style.has_visual_effects());
    assert!(style.establishes_stacking_context());
    style.filter = None;

    // Clip-path establishes stacking context
    style.clip_path = Some(ClipPath::circle(50.0, 50.0, 50.0));
    assert!(style.has_visual_effects());
    assert!(style.establishes_stacking_context());
}
