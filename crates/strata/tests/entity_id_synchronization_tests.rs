//! Integration tests verifying bidirectional blueprint and layout synchronization with Entity IDs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use strata::runtime::AppRuntime;
use strata_dom::{Attribute, Event, EventType, NodeBlueprint, Tag, View};
use strata_primitives::EntityId;

#[test]
fn test_entity_id_preservation_across_fragment_flattening() {
    let mut tree = NodeBlueprint::element_simple(
        Tag::Div,
        "flex flex-col w-[500px]",
        vec![NodeBlueprint::fragment(vec![
            NodeBlueprint::element_simple(
                Tag::Button,
                "px-4 py-2 bg-blue-500",
                vec![NodeBlueprint::text("Button 1")],
            ),
            NodeBlueprint::fragment(vec![
                NodeBlueprint::element_simple(
                    Tag::Button,
                    "px-4 py-2 bg-green-500",
                    vec![NodeBlueprint::text("Button 2")],
                ),
                NodeBlueprint::element_simple(
                    Tag::Span,
                    "text-sm",
                    vec![NodeBlueprint::text("Label")],
                ),
            ]),
        ])],
    );

    tree.ensure_entity_ids();
    let root_id = tree.entity_id().expect("Root must have entity ID");
    assert!(root_id.is_valid());

    // Collect IDs before flattening
    let mut before_ids = Vec::new();
    if let NodeBlueprint::Element { children, .. } = &tree {
        if let NodeBlueprint::Fragment(f1) = &children[0] {
            before_ids.push(f1[0].entity_id().unwrap());
            if let NodeBlueprint::Fragment(f2) = &f1[1] {
                before_ids.push(f2[0].entity_id().unwrap());
                before_ids.push(f2[1].entity_id().unwrap());
            }
        }
    }
    assert_eq!(before_ids.len(), 3);

    // Flatten fragments
    let flat_tree = tree.flatten_fragments();
    assert_eq!(flat_tree.entity_id(), Some(root_id));

    if let NodeBlueprint::Element { children, .. } = &flat_tree {
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].entity_id(), Some(before_ids[0]));
        assert_eq!(children[1].entity_id(), Some(before_ids[1]));
        assert_eq!(children[2].entity_id(), Some(before_ids[2]));
    } else {
        panic!("Flattened tree root must be an Element");
    }

    // Bidirectional query on flat tree
    for &id in &before_ids {
        let node = flat_tree.find_by_entity_id(id);
        assert!(node.is_some(), "Node with ID {} must be found", id);
        assert_eq!(node.unwrap().entity_id(), Some(id));
    }
}

#[test]
fn test_layout_node_entity_id_propagation() {
    let mut tree = NodeBlueprint::element_simple(
        Tag::Div,
        "w-[400px] h-[300px]",
        vec![
            NodeBlueprint::element_simple(Tag::Button, "w-[100px] h-[40px]", vec![]),
            NodeBlueprint::element_simple(Tag::Input, "w-[200px] h-[30px]", vec![]),
        ],
    );
    tree.ensure_entity_ids();

    let root_id = tree.entity_id().unwrap();
    let btn_id = match &tree {
        NodeBlueprint::Element { children, .. } => children[0].entity_id().unwrap(),
        _ => unreachable!(),
    };
    let input_id = match &tree {
        NodeBlueprint::Element { children, .. } => children[1].entity_id().unwrap(),
        _ => unreachable!(),
    };

    let layout = AppRuntime::build_layout_tree(&tree);
    assert_eq!(layout.entity_id, root_id);
    assert_eq!(layout.children.len(), 2);
    assert_eq!(layout.children[0].entity_id, btn_id);
    assert_eq!(layout.children[1].entity_id, input_id);

    // Verify layout tree lookup by entity ID
    let found_btn = layout.find_by_entity_id(btn_id);
    assert!(found_btn.is_some());
    assert_eq!(found_btn.unwrap().entity_id, btn_id);

    let found_input = layout.find_by_entity_id(input_id);
    assert!(found_input.is_some());
    assert_eq!(found_input.unwrap().entity_id, input_id);
}

#[test]
fn test_spatial_index_coordinate_and_event_queries() {
    let clicked = Arc::new(AtomicBool::new(false));
    let clicked_clone = Arc::clone(&clicked);

    let view = View::new(NodeBlueprint::element(
        Tag::Div,
        "w-[800px] h-[600px] bg-white",
        vec![],
        vec![NodeBlueprint::element(
            Tag::Button,
            "w-[200px] h-[100px] bg-blue-500",
            vec![Attribute::event_listener(
                EventType::Click,
                move |_e: Event| {
                    clicked_clone.store(true, Ordering::SeqCst);
                },
            )],
            vec![NodeBlueprint::text("Click Me")],
        )],
    ));

    let runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);

    let root_id = runtime.root_blueprint.entity_id().unwrap();
    let btn_id = match &runtime.root_blueprint {
        NodeBlueprint::Element { children, .. } => children[0].entity_id().unwrap(),
        _ => unreachable!(),
    };

    assert!(runtime.spatial_index.len() >= 2);

    // Point query inside button (e.g. 50, 50)
    let hit_entity = runtime.query_entity_at_point(50.0, 50.0);
    assert_eq!(hit_entity, Some(btn_id));

    // Point query outside button, inside root (e.g. 500, 400)
    let hit_root = runtime.query_entity_at_point(500.0, 400.0);
    assert_eq!(hit_root, Some(root_id));

    // Point query outside all elements (e.g. 900, 900)
    let hit_outside = runtime.query_entity_at_point(900.0, 900.0);
    assert_eq!(hit_outside, None);

    // Query click event at (50, 50)
    let ev_query = runtime.query_event_at_point(50.0, 50.0, EventType::Click);
    assert!(ev_query.is_some());
    let (ev_entity, ev_handler, bounds) = ev_query.unwrap();
    assert_eq!(ev_entity, btn_id);
    assert!(bounds.contains_point(50.0, 50.0));

    // Invoke callback
    assert!(!clicked.load(Ordering::SeqCst));
    ev_handler(Event::new(EventType::Click));
    assert!(clicked.load(Ordering::SeqCst));
}

#[test]
fn test_input_synchronization_with_fragment_flattening_and_entity_id() {
    let view = View::new(NodeBlueprint::element(
        Tag::Div,
        "flex flex-col w-[600px] h-[400px]",
        vec![],
        vec![NodeBlueprint::fragment(vec![
            NodeBlueprint::element_simple(Tag::P, "text-base", vec![NodeBlueprint::text("Title")]),
            NodeBlueprint::element(
                Tag::Input,
                "w-[300px] h-[40px] px-2 py-1",
                vec![
                    Attribute::property("id", "test-search-input"),
                    Attribute::property("value", "initial text"),
                ],
                vec![],
            ),
        ])],
    ));

    let mut runtime = AppRuntime::new(view, 600.0, 400.0, 1.0);

    let input_node = match &runtime.root_blueprint {
        NodeBlueprint::Element { children, .. } => &children[1],
        _ => unreachable!(),
    };
    let target_eid = input_node.entity_id().expect("Input must have entity ID");

    // Click pointer on input to focus
    let hit_input = runtime.query_entity_at_point(50.0, 60.0);
    assert_eq!(hit_input, Some(target_eid));

    // Simulate focus with target_eid
    let state = strata_dom::text_input::TextInputState::new();
    state
        .text
        .set("live updated content via entity id".to_string());
    state.cursor_index.set(12);

    runtime.active_focus = Some(strata::runtime::ActiveInput {
        input_index: 0,
        input_id: Some("test-search-input".to_string()),
        entity_id: Some(target_eid),
        is_multiline: false,
        state,
        on_input: None,
        on_change: None,
        on_submit: None,
        on_blur: None,
    });

    // Synchronize active input layout
    runtime.sync_active_input_layout();

    // Verify layout node mapped to target_eid received text update directly
    let input_layout = runtime.find_layout_by_entity(target_eid);
    assert!(input_layout.is_some());
    let input_layout = input_layout.unwrap();
    assert_eq!(
        input_layout.text.as_deref(),
        Some("live updated content via entity id")
    );

    // Verify caret bounding box is non-empty and within input bounds
    let caret_rect = runtime.active_caret_rect();
    assert!(caret_rect.is_some());
    let caret = caret_rect.unwrap();
    assert!(caret.width > 0.0);
    assert!(caret.height > 0.0);
    assert!(input_layout
        .geometry
        .border_box
        .contains_point(caret.x, caret.y));
}

#[test]
fn test_bidirectional_lookup_consistency() {
    let view = View::new(NodeBlueprint::element_simple(
        Tag::Div,
        "flex flex-row w-[800px] h-[500px]",
        vec![
            NodeBlueprint::element_simple(Tag::Aside, "w-[200px] h-[500px]", vec![]),
            NodeBlueprint::element_simple(
                Tag::Main,
                "flex-1 h-[500px]",
                vec![
                    NodeBlueprint::element_simple(
                        Tag::H1,
                        "text-2xl",
                        vec![NodeBlueprint::text("Header")],
                    ),
                    NodeBlueprint::element_simple(
                        Tag::Button,
                        "px-4 py-2",
                        vec![NodeBlueprint::text("Action")],
                    ),
                ],
            ),
        ],
    ));

    let runtime = AppRuntime::new(view, 800.0, 500.0, 1.0);

    // Traverse all entities in spatial index
    let mut entity_ids = Vec::new();
    for i in 0..runtime.spatial_index.len() {
        let eid = runtime.spatial_index.query_entity_at_point(
            // Sample point from bounds
            runtime
                .spatial_index
                .get_bounds(EntityId::new((i + 1) as u64))
                .map(|b| (b.x + b.width / 2.0, b.y + b.height / 2.0))
                .unwrap_or((0.0, 0.0))
                .0,
            runtime
                .spatial_index
                .get_bounds(EntityId::new((i + 1) as u64))
                .map(|b| (b.x + b.width / 2.0, b.y + b.height / 2.0))
                .unwrap_or((0.0, 0.0))
                .1,
        );
        if let Some(id) = eid {
            if !entity_ids.contains(&id) {
                entity_ids.push(id);
            }
        }
    }

    assert!(!entity_ids.is_empty());
    for &eid in &entity_ids {
        let bp = runtime.find_blueprint_by_entity(eid);
        assert!(bp.is_some(), "Blueprint for entity {:?} must exist", eid);
        assert_eq!(bp.unwrap().entity_id(), Some(eid));

        let layout = runtime.find_layout_by_entity(eid);
        assert!(
            layout.is_some(),
            "Layout node for entity {:?} must exist",
            eid
        );
        assert_eq!(layout.unwrap().entity_id, eid);
    }
}
