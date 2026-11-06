//! Contract tests for the declarative `ScrollArea`, `TitleBar`, and `WindowDragHandle`
//! component blueprints.

use strata_dom::{Attribute, NodeBlueprint, Tag};

fn property<'a>(node: &'a NodeBlueprint, key: &str) -> Option<&'a str> {
    if let NodeBlueprint::Element { attributes, .. } = node {
        for attr in attributes {
            if let Attribute::Property { key: k, value } = attr {
                if k == key {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn collect_elements<'a>(node: &'a NodeBlueprint, out: &mut Vec<&'a NodeBlueprint>) {
    match node {
        NodeBlueprint::Element { children, .. } => {
            out.push(node);
            for child in children {
                collect_elements(child, out);
            }
        }
        NodeBlueprint::Fragment(children) => {
            for child in children {
                collect_elements(child, out);
            }
        }
        _ => {}
    }
}

#[test]
fn test_scroll_area_contract() {
    let view = strata_ui::scroll_area::ScrollArea(Some("scroll-contract"), None, None, Some(true));

    assert!(
        matches!(&view.root, NodeBlueprint::Element { tag: Tag::Div, .. }),
        "ScrollArea root must be a div"
    );
    if let NodeBlueprint::Element { classes, .. } = &view.root {
        let joined = classes.join(" ");
        assert!(
            joined
                .split_whitespace()
                .any(|token| token == "overflow-y-auto"),
            "ScrollArea root must opt into vertical overflow scrolling, got {joined:?}"
        );
    }

    assert_eq!(
        property(&view.root, "id"),
        Some("scroll-contract"),
        "ScrollArea must preserve its stable id for scroll offset persistence"
    );
}

#[test]
fn test_title_bar_drag_and_no_drag_contract() {
    let view = strata_ui::title_bar::TitleBar(
        Some("Contract Window".to_string()),
        Some(true),
        Some(true),
        None,
        None,
        None,
        None,
        None,
    );

    assert!(
        matches!(
            &view.root,
            NodeBlueprint::Element {
                tag: Tag::Header,
                ..
            }
        ),
        "TitleBar root must be a header"
    );
    assert_eq!(
        property(&view.root, "data-app-region"),
        Some("drag"),
        "TitleBar root must expose a native drag region"
    );

    let mut elements = Vec::new();
    collect_elements(&view.root, &mut elements);

    let mut actions = Vec::new();
    for element in &elements {
        if let Some(action) = property(element, "data-window-action") {
            assert_eq!(
                property(element, "data-app-region"),
                Some("no-drag"),
                "window control {action} must opt out of the drag region"
            );
            actions.push(action.to_string());
        }
    }
    actions.sort();
    assert_eq!(
        actions,
        vec![
            "close".to_string(),
            "maximize".to_string(),
            "minimize".to_string()
        ]
    );

    let plain = strata_ui::title_bar::TitleBar(
        Some("No Drag".to_string()),
        Some(false),
        Some(false),
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(property(&plain.root, "data-app-region"), Some("no-drag"));
}

#[test]
fn test_window_drag_handle_contract() {
    let view = strata_ui::title_bar::WindowDragHandle(None, None);
    assert_eq!(property(&view.root, "data-app-region"), Some("drag"));
}
