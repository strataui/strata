use strata::runtime::AppRuntime;
use strata_dom::{Attribute, EventType, IrNodeFlags, IrTag, NodeBlueprint, Tag, View};

#[test]
fn test_ui_ir_pipeline_runtime_unification() {
    let blueprint = NodeBlueprint::element(
        Tag::Div,
        vec!["w-[800px]".to_string(), "h-[600px]".to_string(), "flex".to_string(), "overflow-hidden".to_string()],
        vec![Attribute::event_listener(EventType::Click, || {})],
        vec![
            NodeBlueprint::element_simple(
                Tag::Button,
                vec!["w-[200px]".to_string(), "h-[50px]".to_string()],
                vec![NodeBlueprint::text("Submit")],
            ),
            NodeBlueprint::element_simple(
                Tag::Span,
                vec!["w-[300px]".to_string(), "h-[50px]".to_string()],
                vec![NodeBlueprint::text("Status Text")],
            ),
        ],
    );

    let mut runtime = AppRuntime::new(View::new(blueprint), 800.0, 600.0, 1.0);

    // 1. Verify IR graph was compiled
    assert_eq!(runtime.ir_graph.len(), 5); // root(div), button, text, span, text
    let root_id = runtime.ir_graph.root_id.expect("Root ID set");
    let root_ir = runtime.ir_graph.get(root_id).expect("Root node exists");

    assert_eq!(root_ir.tag, IrTag::FlexContainer);
    assert_ne!(root_ir.flags & IrNodeFlags::HAS_EVENT_LISTENERS, 0);
    assert_ne!(root_ir.flags & IrNodeFlags::IS_CONTAINMENT_BOUNDARY, 0);
    assert_eq!(root_ir.child_count, 2);
    assert_eq!(root_ir.subtree_span, 5);

    // 2. Verify layout sync to IR FixedRect
    let (rx, ry, rw, rh) = root_ir.layout_rect.to_f32();
    assert_eq!(rw, 800.0);
    assert_eq!(rh, 600.0);
    assert_eq!(rx, 0.0);
    assert_eq!(ry, 0.0);

    // 3. Verify child geometry in IR matches LayoutNode
    let children: Vec<_> = runtime.ir_graph.children(root_id).collect();
    assert_eq!(children.len(), 2);
    let button_ir = children[0];
    let (bx, by, bw, bh) = button_ir.layout_rect.to_f32();
    assert_eq!(bw, 200.0);
    assert_eq!(bh, 50.0);
    assert_eq!(bx, 0.0);
    assert_eq!(by, 0.0);

    let span_ir = children[1];
    let (sx, sy, sw, sh) = span_ir.layout_rect.to_f32();
    assert_eq!(sw, 300.0);
    assert_eq!(sh, 50.0);
    assert_eq!(sx, 200.0); // right next to button in flex row
    assert_eq!(sy, 0.0);

    // 4. Test reconstruction of LayoutNode from UiIrGraph
    let reconstructed_root = AppRuntime::build_layout_from_ir(&runtime.ir_graph, root_id);
    assert_eq!(reconstructed_root.children.len(), 2);
    assert_eq!(reconstructed_root.geometry.border_box.width, 800.0);
    assert_eq!(reconstructed_root.geometry.border_box.height, 600.0);

    // 5. Test update_view synchronizes IR graph
    let updated_blueprint = NodeBlueprint::element_simple(
        Tag::Div,
        vec!["w-[400px]".to_string(), "h-[200px]".to_string()],
        vec![NodeBlueprint::text("Single Child")],
    );
    runtime.update_view(View::new(updated_blueprint));

    assert_eq!(runtime.ir_graph.len(), 2);
    let new_root_id = runtime.ir_graph.root_id.unwrap();
    let new_root_ir = runtime.ir_graph.get(new_root_id).unwrap();
    let (_, _, nw, nh) = new_root_ir.layout_rect.to_f32();
    assert_eq!(nw, 400.0);
    assert_eq!(nh, 200.0);
}
