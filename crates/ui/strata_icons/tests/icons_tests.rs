//! Comprehensive integration tests for strata_icons.

use strata_dom::{NodeBlueprint, Tag, Attribute};
use strata_icons::prelude::*;

#[test]
fn test_path_command_and_bounds() {
    let mut path = VectorPath::new();
    path.move_to(0.0, 0.0)
        .line_to(20.0, 0.0)
        .line_to(20.0, 10.0)
        .line_to(0.0, 10.0)
        .close();

    let bounds = path.compute_bounds();
    assert_eq!(bounds.origin.x, 0.0);
    assert_eq!(bounds.origin.y, 0.0);
    assert_eq!(bounds.size.width, 20.0);
    assert_eq!(bounds.size.height, 10.0);

    let svg_d = path.to_svg_d();
    assert!(svg_d.contains("M 0 0"));
    assert!(svg_d.contains("L 20 0"));
    assert!(svg_d.contains('Z'));
}

#[test]
fn test_icon_registry_all_icons_valid() {
    let list = all_icons();
    assert!(list.len() >= 1000, "Expected at least 1,000 icons, found {}", list.len());

    for name in list {
        let data = lookup_icon(*name);
        assert_eq!(data.name, *name);
        assert!(!data.svg_path.is_empty(), "Icon {:?} has empty SVG path", name);
        assert_eq!(data.view_box, (0.0, 0.0, 24.0, 24.0));
    }
}

#[test]
fn test_vector_path_binary_serialization_roundtrip() {
    let mut path = VectorPath::new();
    path.move_to(2.0, 4.0)
        .line_to(10.0, 12.0)
        .quad_to(14.0, 16.0, 18.0, 20.0)
        .cubic_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)
        .arc_to(8.0, 8.0, 0.0, false, true, 22.0, 22.0)
        .close();

    let binary = path.to_binary();
    assert!(!binary.is_empty());
    // Verify compressed size is compact
    assert!(binary.len() < 120);

    let decoded = VectorPath::from_binary(&binary).expect("Failed to decode binary path");
    assert_eq!(decoded.commands.len(), path.commands.len());
    assert_eq!(decoded.commands, path.commands);
}

#[test]
fn test_icon_component_rendering() {
    let view = Icon(IconName::Search, Some("w-6 h-6 text-indigo-500"));
    match &view.root {
        NodeBlueprint::Element { tag, classes, children, .. } => {
            assert_eq!(*tag, Tag::Svg);
            assert!(classes.iter().any(|c| c.contains("w-6")));
            assert!(classes.iter().any(|c| c.contains("h-6")));
            assert_eq!(children.len(), 1);
            match &children[0] {
                NodeBlueprint::Element { tag: child_tag, .. } => {
                    assert_eq!(*child_tag, Tag::Path);
                }
                _ => panic!("Expected child element to be Path"),
            }
        }
        _ => panic!("Expected Element root"),
    }
}

#[test]
fn test_icons_typed_constructors() {
    let check_view = Icons::Check(Some("text-green-500"));
    match &check_view.root {
        NodeBlueprint::Element { tag, classes, .. } => {
            assert_eq!(*tag, Tag::Svg);
            assert!(classes.iter().any(|c| c.contains("text-green-500")));
        }
        _ => panic!("Expected Element root"),
    }

    let server_view = Icons::Server(None);
    match &server_view.root {
        NodeBlueprint::Element { tag, .. } => {
            assert_eq!(*tag, Tag::Svg);
        }
        _ => panic!("Expected Element root"),
    }

    let sparkles_view = Icons::Sparkles(Some("animate-pulse"));
    match &sparkles_view.root {
        NodeBlueprint::Element { classes, .. } => {
            assert!(classes.iter().any(|c| c.contains("animate-pulse")));
        }
        _ => panic!("Expected Element root"),
    }
}

#[test]
fn test_icon_config_with_custom_size_and_stroke() {
    let view = IconConfig(IconName::Bell, Some(32.0), Some(1.5), Some("text-yellow-400"));
    match &view.root {
        NodeBlueprint::Element { tag, attributes, .. } => {
            assert_eq!(*tag, Tag::Svg);
            let style_attr = attributes.iter().find(|a| match a {
                Attribute::Property { key, value } => key == "style" && value.contains("32px"),
                _ => false,
            });
            assert!(style_attr.is_some(), "Expected style with 32px size");
        }
        _ => panic!("Expected Element root"),
    }
}
