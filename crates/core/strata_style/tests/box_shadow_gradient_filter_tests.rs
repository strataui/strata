//! Integration Tests: Multi-Layer CSS Box-Shadow, Gradients, and Filter Shaders.

use strata_style::cascade::CascadeSolver;
use strata_style::effects::FilterOp;
use strata_style::parser::{parse_box_shadow, parse_filter_list, parse_gradient};
use strata_style::prelude::*;
use strata_style::types::{Color, Gradient};

#[test]
fn test_box_shadow_parsing_outer_and_inset() {
    // 1. Single outer shadow: 2px 4px 6px #000000
    let s1 = parse_box_shadow("2px 4px 6px #000000");
    assert_eq!(s1.len(), 1);
    assert_eq!(s1[0].offset_x, 2.0);
    assert_eq!(s1[0].offset_y, 4.0);
    assert_eq!(s1[0].blur_radius, 6.0);
    assert_eq!(s1[0].spread_radius, 0.0);
    assert!(!s1[0].inset);

    // 2. Inset shadow with spread and rgba: inset 0px 2px 4px 1px rgba(0, 0, 0, 0.25)
    let s2 = parse_box_shadow("inset 0px 2px 4px 1px rgba(0, 0, 0, 0.25)");
    assert_eq!(s2.len(), 1);
    assert_eq!(s2[0].offset_x, 0.0);
    assert_eq!(s2[0].offset_y, 2.0);
    assert_eq!(s2[0].blur_radius, 4.0);
    assert_eq!(s2[0].spread_radius, 1.0);
    assert!(s2[0].inset);

    // 3. Multi-layer comma-separated shadows
    let s3 = parse_box_shadow("0 1px 3px rgba(0,0,0,0.1), 0 1px 2px rgba(0,0,0,0.06)");
    assert_eq!(s3.len(), 2);
    assert_eq!(s3[0].offset_y, 1.0);
    assert_eq!(s3[0].blur_radius, 3.0);
    assert_eq!(s3[1].offset_y, 1.0);
    assert_eq!(s3[1].blur_radius, 2.0);
}

#[test]
fn test_gradient_parsing_linear_radial_conic() {
    // 1. Linear gradient to right: linear-gradient(to right, #ff0000, #00ff00)
    let g1 = parse_gradient("linear-gradient(to right, #ff0000, #00ff00)").expect("Linear gradient must parse");
    match g1 {
        Gradient::Linear { angle_deg, stops } => {
            assert_eq!(angle_deg, 90.0);
            assert_eq!(stops.len(), 2);
            assert_eq!(stops[0].color, Color::rgb(255, 0, 0));
            assert_eq!(stops[1].color, Color::rgb(0, 255, 0));
        }
        _ => panic!("Expected Linear gradient"),
    }

    // 2. Linear gradient with degrees: linear-gradient(135deg, rgba(255,255,255,0.1) 0%, rgba(255,255,255,0) 100%)
    let g2 = parse_gradient("linear-gradient(135deg, rgba(255,255,255,0.1) 0%, rgba(255,255,255,0) 100%)").expect("Angle linear gradient must parse");
    match g2 {
        Gradient::Linear { angle_deg, stops } => {
            assert_eq!(angle_deg, 135.0);
            assert_eq!(stops.len(), 2);
            assert_eq!(stops[0].position, Some(0.0));
            assert_eq!(stops[1].position, Some(1.0));
        }
        _ => panic!("Expected Linear gradient"),
    }

    // 3. Radial gradient: radial-gradient(circle at center, #ffffff, #000000)
    let g3 = parse_gradient("radial-gradient(circle at center, #ffffff, #000000)").expect("Radial gradient must parse");
    match g3 {
        Gradient::Radial { center, stops } => {
            assert_eq!(center, (0.5, 0.5));
            assert_eq!(stops.len(), 2);
        }
        _ => panic!("Expected Radial gradient"),
    }
}

#[test]
fn test_filter_list_parsing() {
    // 1. Dual filter: blur(8px) brightness(1.2)
    let f1 = parse_filter_list("blur(8px) brightness(1.2)").expect("Filter list must parse");
    assert_eq!(f1.operations.len(), 2);
    assert_eq!(f1.operations[0], FilterOp::Blur(8.0));
    assert_eq!(f1.operations[1], FilterOp::Brightness(1.2));

    // 2. Grayscale and contrast: grayscale(100%) contrast(150%)
    let f2 = parse_filter_list("grayscale(100%) contrast(150%)").expect("Percentage filters must parse");
    assert_eq!(f2.operations.len(), 2);
    assert_eq!(f2.operations[0], FilterOp::Grayscale(1.0));
    assert_eq!(f2.operations[1], FilterOp::Contrast(1.5));
}

#[test]
fn test_cascade_inherit_and_initial_for_visual_effects() {
    let mut parent = ComputedStyle::default();
    let decl_shadow = Declaration {
        property: "box-shadow".to_string(),
        value: "0 4px 6px rgba(0,0,0,0.1)".to_string(),
        important: false,
    };
    let decl_filter = Declaration {
        property: "backdrop-filter".to_string(),
        value: "blur(12px)".to_string(),
        important: false,
    };
    CascadeSolver::apply_declaration_with_parent(&mut parent, &decl_shadow, None);
    CascadeSolver::apply_declaration_with_parent(&mut parent, &decl_filter, None);

    assert_eq!(parent.box_shadow.len(), 1);
    assert!(parent.backdrop_filter.is_some());

    // Child inheriting parent properties
    let mut child = ComputedStyle::default();
    let decl_inherit_shadow = Declaration {
        property: "box-shadow".to_string(),
        value: "inherit".to_string(),
        important: false,
    };
    let decl_inherit_filter = Declaration {
        property: "backdrop-filter".to_string(),
        value: "inherit".to_string(),
        important: false,
    };
    CascadeSolver::apply_declaration_with_parent(&mut child, &decl_inherit_shadow, Some(&parent));
    CascadeSolver::apply_declaration_with_parent(&mut child, &decl_inherit_filter, Some(&parent));

    assert_eq!(child.box_shadow, parent.box_shadow);
    assert_eq!(child.backdrop_filter, parent.backdrop_filter);

    // Resetting via initial
    let decl_initial_shadow = Declaration {
        property: "box-shadow".to_string(),
        value: "initial".to_string(),
        important: false,
    };
    CascadeSolver::apply_declaration_with_parent(&mut child, &decl_initial_shadow, Some(&parent));
    assert!(child.box_shadow.is_empty());
}
