use strata::prelude::*;
use strata_dom::{NodeBlueprint, Tag, View};

#[component]
fn TwoGridItems() -> View {
    rsx! {
        <div class="h-[40px] bg-red-500">
            "Item 1"
        </div>
        <div class="h-[40px] bg-blue-500">
            "Item 2"
        </div>
    }
}

#[component]
fn GridContainerApp() -> View {
    rsx! {
        <div class="grid grid-cols-2 w-[200px] h-[100px] bg-black">
            <TwoGridItems />
        </div>
    }
}

#[test]
fn test_node_blueprint_fragment_flattening() {
    let item1 = NodeBlueprint::element_simple(Tag::Div, "item-1", vec![]);
    let item2 = NodeBlueprint::element_simple(Tag::Div, "item-2", vec![]);
    let item3 = NodeBlueprint::element_simple(Tag::Div, "item-3", vec![]);

    // Deeply nested fragment structure
    let nested_fragment = NodeBlueprint::fragment(vec![
        item1.clone(),
        NodeBlueprint::fragment(vec![
            item2.clone(),
            NodeBlueprint::fragment(vec![item3.clone(), NodeBlueprint::empty()]),
        ]),
    ]);

    let parent_element = NodeBlueprint::element_simple(Tag::Div, "parent", vec![nested_fragment]);

    let flattened = parent_element.flatten_fragments();

    if let NodeBlueprint::Element { children, .. } = &flattened {
        assert_eq!(
            children.len(),
            3,
            "All nested fragments must flatten into 3 direct children"
        );
        assert!(
            matches!(&children[0], NodeBlueprint::Element { classes, .. } if classes.iter().any(|c| c == "item-1"))
        );
        assert!(
            matches!(&children[1], NodeBlueprint::Element { classes, .. } if classes.iter().any(|c| c == "item-2"))
        );
        assert!(
            matches!(&children[2], NodeBlueprint::Element { classes, .. } if classes.iter().any(|c| c == "item-3"))
        );
    } else {
        panic!("Flattened blueprint must remain an Element");
    }
}

#[test]
fn test_compile_to_ir_splicing_without_component_root_wrapper() {
    let btn1 = NodeBlueprint::element_simple(Tag::Button, "btn-1", vec![]);
    let btn2 = NodeBlueprint::element_simple(Tag::Button, "btn-2", vec![]);
    let fragment = NodeBlueprint::fragment(vec![btn1, btn2]);

    let container = NodeBlueprint::element_simple(Tag::Div, "container", vec![fragment]);

    let mut graph = strata_dom::UiIrGraph::new();
    let root_id = container.compile_to_ir(&mut graph);
    assert!(!root_id.is_null());

    let root_node = graph.get(root_id).unwrap();
    assert_eq!(
        root_node.child_count, 2,
        "Parent IR node must have 2 direct children"
    );

    let first_child_id = root_node.first_child;
    assert!(!first_child_id.is_null());
    let first_child = graph.get(first_child_id).unwrap();
    assert_eq!(first_child.tag, strata_dom::IrTag::Button);

    let second_child_id = first_child.next_sibling;
    assert!(!second_child_id.is_null());
    let second_child = graph.get(second_child_id).unwrap();
    assert_eq!(second_child.tag, strata_dom::IrTag::Button);
    assert!(second_child.next_sibling.is_null());
}

#[test]
fn test_app_runtime_grid_fragment_flattening() {
    let view = GridContainerApp();
    let runtime = AppRuntime::new(view, 200.0, 100.0, 1.0);

    // Verify root layout node is a Grid container with exactly 2 direct children
    assert_eq!(
        runtime.layout_root.style.display,
        strata_style::types::Display::Grid,
        "Root container must be Display::Grid"
    );
    assert_eq!(
        runtime.layout_root.children.len(),
        2,
        "Component fragment must splice children directly into Grid without wrapper Flex container"
    );

    // Item 1 placed in column 1 (x = 0, stretched across 1fr column = 100px)
    let item1 = &runtime.layout_root.children[0];
    assert_eq!(item1.geometry.border_box.x, 0.0);
    assert_eq!(item1.geometry.border_box.width, 100.0);

    // Item 2 placed in column 2 (x = 100, stretched across 1fr column = 100px)
    let item2 = &runtime.layout_root.children[1];
    assert_eq!(item2.geometry.border_box.x, 100.0);
    assert_eq!(item2.geometry.border_box.width, 100.0);

    // Verify IR graph synchronization
    let ir_root_id = runtime.ir_graph.root_id.expect("IR graph must have root");
    let ir_root = runtime.ir_graph.get(ir_root_id).unwrap();
    assert_eq!(
        ir_root.child_count, 2,
        "IR graph must have 2 direct children matching layout"
    );

    let ir_item1 = runtime.ir_graph.get(ir_root.first_child).unwrap();
    let (x1, _y1, w1, _h1) = ir_item1.layout_rect.to_f32();
    assert_eq!(x1, 0.0);
    assert_eq!(w1, 100.0);

    let ir_item2 = runtime.ir_graph.get(ir_item1.next_sibling).unwrap();
    let (x2, _y2, w2, _h2) = ir_item2.layout_rect.to_f32();
    assert_eq!(x2, 100.0);
    assert_eq!(w2, 100.0);
}
