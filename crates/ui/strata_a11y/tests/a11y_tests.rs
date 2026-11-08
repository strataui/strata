use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use strata_a11y::{
    audit_tree, calculate_contrast_ratio, evaluate_normal_text_contrast,
    relative_luminance, A11yAction, A11yBridge, A11yNode, A11yNodeId, A11yRect, A11yRole,
    A11yTree, WcagCompliance,
};

#[test]
fn test_role_resolution() {
    assert_eq!(A11yRole::from_tag_or_role("button", None), A11yRole::Button);
    assert_eq!(A11yRole::from_tag_or_role("input", None), A11yRole::TextInput);
    assert_eq!(A11yRole::from_tag_or_role("div", Some("button")), A11yRole::Button);
    assert_eq!(A11yRole::from_tag_or_role("span", Some("slider")), A11yRole::Slider);
    assert_eq!(A11yRole::from_tag_or_role("div", Some("dialog")), A11yRole::Dialog);
}

#[test]
fn test_wcag_aaa_contrast_calculations() {
    // Pure black (#000000) luminance is 0.0
    let l_black = relative_luminance(0, 0, 0);
    assert_eq!(l_black, 0.0);

    // Pure white (#ffffff) luminance is 1.0
    let l_white = relative_luminance(255, 255, 255);
    assert!((l_white - 1.0).abs() < 1e-4);

    // Pure white on pure black gives maximum 21:1 contrast ratio
    let max_contrast = calculate_contrast_ratio((255, 255, 255), (0, 0, 0));
    assert!((max_contrast - 21.0).abs() < 0.1);
    assert_eq!(evaluate_normal_text_contrast(max_contrast), WcagCompliance::Aaa);

    // Low contrast grey (#777777 on #888888) fails WCAG
    let low_contrast = calculate_contrast_ratio((119, 119, 119), (136, 136, 136));
    assert!(low_contrast < 2.0);
    assert_eq!(evaluate_normal_text_contrast(low_contrast), WcagCompliance::Fail);

    // Typical high contrast dark mode (#38bdf8 cyan on #08090a void)
    let strata_theme_contrast = calculate_contrast_ratio((56, 189, 248), (8, 9, 10));
    assert!(strata_theme_contrast >= 7.0);
    assert_eq!(evaluate_normal_text_contrast(strata_theme_contrast), WcagCompliance::Aaa);
}

#[test]
fn test_differential_a11y_tree_updates() {
    let mut bridge = A11yBridge::new();

    // Initial Tree
    let root = A11yNode::new(A11yNodeId(1), A11yRole::RootWebArea);
    let mut btn = A11yNode::new(A11yNodeId(2), A11yRole::Button).with_name("Confirm Action");
    btn.bounds = Some(A11yRect::new(10.0, 20.0, 100.0, 36.0));

    let mut tree1 = A11yTree::new(A11yNodeId(1), root);
    tree1.insert_node(btn);
    tree1.get_node_mut(A11yNodeId(1)).unwrap().add_child(A11yNodeId(2));

    let update1 = bridge.update(tree1);
    assert_eq!(update1.nodes.len(), 2, "Initial push must send all nodes");
    assert!(update1.removed_nodes.is_empty());

    // Incremental Tree: Button name updated and a new checkbox added
    let mut root2 = A11yNode::new(A11yNodeId(1), A11yRole::RootWebArea);
    root2.add_child(A11yNodeId(2));
    let mut btn2 = A11yNode::new(A11yNodeId(2), A11yRole::Button).with_name("Confirmed!");
    btn2.bounds = Some(A11yRect::new(10.0, 20.0, 100.0, 36.0));
    let checkbox = A11yNode::new(A11yNodeId(3), A11yRole::Checkbox).with_name("Remember me");

    let mut tree2 = A11yTree::new(A11yNodeId(1), root2);
    tree2.insert_node(btn2);
    tree2.insert_node(checkbox);

    let update2 = bridge.update(tree2);
    // Button mutated and Checkbox added, Root node unchanged so not resent
    assert_eq!(update2.nodes.len(), 2);
    assert!(update2.nodes.iter().any(|(id, n)| id.0 == 2 && n.name.as_deref() == Some("Confirmed!")));
    assert!(update2.nodes.iter().any(|(id, n)| id.0 == 3 && n.name.as_deref() == Some("Remember me")));
}

#[test]
fn test_automated_accessibility_auditor() {
    let root = A11yNode::new(A11yNodeId(1), A11yRole::RootWebArea);
    // Button with missing name
    let unnamed_btn = A11yNode::new(A11yNodeId(2), A11yRole::Button);
    // Image missing alt text
    let altless_img = A11yNode::new(A11yNodeId(3), A11yRole::Image);
    // Slider missing bounds (has name so rule 1 passes, but bounds rule 4 triggers)
    let broken_slider = A11yNode::new(A11yNodeId(4), A11yRole::Slider).with_name("Volume");

    let mut tree = A11yTree::new(A11yNodeId(1), root);
    tree.insert_node(unnamed_btn);
    tree.insert_node(altless_img);
    tree.insert_node(broken_slider);

    let violations = audit_tree(&tree);
    assert_eq!(violations.len(), 3);
    assert!(violations.iter().any(|v| v.rule == "A11Y_RULE_001_MISSING_ACCESSIBLE_NAME" && v.node_id == 2));
    assert!(violations.iter().any(|v| v.rule == "A11Y_RULE_002_IMAGE_MISSING_ALT" && v.node_id == 3));
    assert!(violations.iter().any(|v| v.rule == "A11Y_RULE_004_SLIDER_MISSING_BOUNDS" && v.node_id == 4));
}

#[test]
fn test_screen_reader_action_dispatch() {
    let mut bridge = A11yBridge::new();
    let action_triggered = Arc::new(AtomicBool::new(false));

    let flag_clone = Arc::clone(&action_triggered);
    bridge.set_action_handler(move |id, action| {
        if id.0 == 42 && action == A11yAction::Click {
            flag_clone.store(true, Ordering::SeqCst);
        }
    });

    bridge.dispatch_action(A11yNodeId(42), A11yAction::Click);
    assert!(action_triggered.load(Ordering::SeqCst));
}

#[test]
fn test_accesskit_protocol_serialization_and_adapter() {
    use strata_a11y::{
        AccessKitActionRequest, AccessKitAdapter, A11yLivePoliteness,
    };

    let mut bridge = A11yBridge::new();
    let clicked = Arc::new(AtomicBool::new(false));
    let clicked_clone = Arc::clone(&clicked);
    bridge.set_action_handler(move |id, action| {
        if id.0 == 10 && action == A11yAction::Click {
            clicked_clone.store(true, Ordering::SeqCst);
        }
    });

    let mut adapter = AccessKitAdapter::new("Strata EDA Studio");

    // Construct a tree with interactive nodes and a live region
    let root = A11yNode::new(A11yNodeId(1), A11yRole::RootWebArea);
    let mut btn = A11yNode::new(A11yNodeId(10), A11yRole::Button)
        .with_name("Compile FPGA Bitstream")
        .with_bounds(A11yRect::new(20.0, 30.0, 180.0, 44.0))
        .with_actions(vec![A11yAction::Click, A11yAction::Focus]);
    btn.focused = true;

    let live_status = A11yNode::new(A11yNodeId(20), A11yRole::Status)
        .with_name("Synthesis: 45% complete")
        .with_live(A11yLivePoliteness::Polite, true);

    let mut tree = A11yTree::new(A11yNodeId(1), root);
    tree.insert_node(btn);
    tree.insert_node(live_status);
    tree.get_node_mut(A11yNodeId(1)).unwrap().add_child(A11yNodeId(10));
    tree.get_node_mut(A11yNodeId(1)).unwrap().add_child(A11yNodeId(20));

    let diff = bridge.update(tree);
    let ak_update = adapter.process_update(&diff, Some(A11yNodeId(1)));

    assert_eq!(ak_update.nodes.len(), 3);
    assert_eq!(ak_update.focus, Some(10));
    assert!(ak_update.tree.is_some());
    assert_eq!(ak_update.tree.as_ref().unwrap().app_name, "Strata EDA Studio");

    let json = ak_update.to_json();
    assert!(json.contains("\"type\":\"accesskit_tree_update\""));
    assert!(json.contains("\"role\":\"button\""));
    assert!(json.contains("\"name\":\"Compile FPGA Bitstream\""));
    assert!(json.contains("\"bounds\":[20,30,180,44]"));
    assert!(json.contains("\"live\":\"polite\""));
    assert!(json.contains("\"focus\":10"));

    // Test OS action request execution via adapter
    let req = AccessKitActionRequest {
        target: A11yNodeId(10),
        action: A11yAction::Click,
        data: None,
    };
    adapter.handle_action_request(&req, &bridge);
    assert!(clicked.load(Ordering::SeqCst), "AccessKit action request must trigger bridge callback");
}

#[test]
fn test_dom_blueprint_to_a11y_tree_builder() {
    use strata_a11y::{build_a11y_tree, A11yLivePoliteness};
    use strata_dom::{Attribute, NodeBlueprint, Tag};

    // Construct a blueprint with buttons, inputs, and a live alert region
    let blueprint = NodeBlueprint::Element {
        tag: Tag::Div,
        classes: strata_dom::ClassList::empty(),
        attributes: vec![
            Attribute::Property {
                key: "role".to_string(),
                value: "main".to_string(),
            },
        ],
        children: vec![
            NodeBlueprint::Element {
                tag: Tag::Button,
                classes: strata_dom::ClassList::empty(),
                attributes: vec![
                    Attribute::Property {
                        key: "aria-label".to_string(),
                        value: "Execute Synthesis".to_string(),
                    },
                ],
                children: vec![NodeBlueprint::Text("Run".to_string())],
            },
            NodeBlueprint::Element {
                tag: Tag::Input,
                classes: strata_dom::ClassList::empty(),
                attributes: vec![
                    Attribute::Property {
                        key: "value".to_string(),
                        value: "top_module.v".to_string(),
                    },
                    Attribute::Property {
                        key: "aria-label".to_string(),
                        value: "Source File".to_string(),
                    },
                ],
                children: vec![],
            },
            NodeBlueprint::Element {
                tag: Tag::Div,
                classes: strata_dom::ClassList::empty(),
                attributes: vec![
                    Attribute::Property {
                        key: "role".to_string(),
                        value: "alert".to_string(),
                    },
                    Attribute::Property {
                        key: "aria-live".to_string(),
                        value: "assertive".to_string(),
                    },
                    Attribute::Property {
                        key: "aria-atomic".to_string(),
                        value: "true".to_string(),
                    },
                ],
                children: vec![NodeBlueprint::Text("Timing constraint violation: Slack -1.2ns".to_string())],
            },
        ],
    };

    let tree = build_a11y_tree(&blueprint);
    assert_eq!(tree.total_nodes(), 5); // root + main container + button + input + alert

    // Verify main landmark
    let main_node = tree.nodes.values().find(|n| n.role == A11yRole::Main);
    assert!(main_node.is_some());

    // Verify button
    let btn_node = tree.nodes.values().find(|n| n.role == A11yRole::Button).expect("Button node missing");
    assert_eq!(btn_node.name.as_deref(), Some("Execute Synthesis"));
    assert!(btn_node.focusable);
    assert!(btn_node.supported_actions.contains(&A11yAction::Click));

    // Verify input
    let input_node = tree.nodes.values().find(|n| n.role == A11yRole::TextInput).expect("TextInput node missing");
    assert_eq!(input_node.name.as_deref(), Some("Source File"));
    assert_eq!(input_node.value.as_deref(), Some("top_module.v"));
    assert!(input_node.supported_actions.contains(&A11yAction::SetValue(String::new())));

    // Verify live alert region
    let alert_node = tree.nodes.values().find(|n| n.role == A11yRole::Alert).expect("Alert node missing");
    assert_eq!(alert_node.live, Some(A11yLivePoliteness::Assertive));
    assert!(alert_node.live_atomic);
    assert_eq!(alert_node.name.as_deref(), Some("Timing constraint violation: Slack -1.2ns"));
}

#[test]
fn test_a11y_tree_layout_bounds_binding() {
    use strata_a11y::build_a11y_tree_with_layout;
    use strata_dom::{Attribute, NodeBlueprint, Tag};
    use strata_layout::geometry::Rect;
    use strata_layout::tree::LayoutNode;

    let blueprint = NodeBlueprint::Element {

        tag: Tag::Div,
        classes: strata_dom::ClassList::empty(),
        attributes: vec![],
        children: vec![
            NodeBlueprint::Element {
                tag: Tag::Button,
                classes: strata_dom::ClassList::empty(),
                attributes: vec![
                    Attribute::Property {
                        key: "aria-label".to_string(),
                        value: "Launch Probe".to_string(),
                    },
                ],
                children: vec![],
            },
        ],
    };

    let mut layout_btn = LayoutNode::new(Default::default());
    layout_btn.geometry.border_box = Rect::new(15.0, 25.0, 120.0, 40.0);

    let mut layout_root = LayoutNode::with_children(Default::default(), vec![layout_btn]);
    layout_root.geometry.border_box = Rect::new(0.0, 0.0, 800.0, 600.0);

    let tree = build_a11y_tree_with_layout(&blueprint, &layout_root, None);

    let btn_node = tree.nodes.values().find(|n| n.role == A11yRole::Button).expect("Button node missing");

    assert!(btn_node.bounds.is_some(), "Button node must have bound layout geometry");
    let bounds = btn_node.bounds.unwrap();
    assert_eq!(bounds.x, 15.0);
    assert_eq!(bounds.y, 25.0);
    assert_eq!(bounds.width, 120.0);
    assert_eq!(bounds.height, 40.0);
}

#[test]
fn test_accesskit_native_tree_update_generation() {
    use accesskit::ActionHandler;
    use strata_a11y::accesskit::{a11y_node_to_accesskit, AccessKitAdapter};

    let mut adapter = AccessKitAdapter::new("Strata Robotics Studio");

    let root = A11yNode::new(A11yNodeId(1), A11yRole::RootWebArea);
    let mut btn = A11yNode::new(A11yNodeId(2), A11yRole::Button)
        .with_name("Emergency Stop")
        .with_bounds(A11yRect::new(10.0, 20.0, 150.0, 50.0))
        .with_actions(vec![A11yAction::Click, A11yAction::Focus]);
    btn.focused = true;

    let mut tree = A11yTree::new(A11yNodeId(1), root);
    tree.insert_node(btn);
    tree.get_node_mut(A11yNodeId(1)).unwrap().add_child(A11yNodeId(2));

    let native_ak_update = adapter.to_accesskit_tree(&tree);

    assert_eq!(native_ak_update.nodes.len(), 2);
    assert_eq!(native_ak_update.tree_id, adapter.tree_id());
    assert!(native_ak_update.tree.is_some());
    let tree_info = native_ak_update.tree.as_ref().unwrap();
    assert_eq!(tree_info.root, accesskit::NodeId(1));
    assert_eq!(tree_info.toolkit_name.as_deref(), Some("Strata"));

    // Check mapped accesskit::Node
    let ak_node = a11y_node_to_accesskit(tree.get_node(A11yNodeId(2)).unwrap());
    assert_eq!(ak_node.role(), accesskit::Role::Button);
    assert_eq!(ak_node.label(), Some("Emergency Stop"));

    // Test genuine ActionHandler bridge
    let bridge = Arc::new(Mutex::new(A11yBridge::new()));
    let e_stop_triggered = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&e_stop_triggered);

    bridge.lock().unwrap().set_action_handler(move |id, action| {
        if id.0 == 2 && action == A11yAction::Click {
            flag_clone.store(true, Ordering::SeqCst);
        }
    });

    let mut handler = adapter.create_action_handler(Arc::clone(&bridge));
    let request = accesskit::ActionRequest {
        action: accesskit::Action::Click,
        target_tree: adapter.tree_id(),
        target_node: accesskit::NodeId(2),
        data: None,
    };

    handler.do_action(request);
    assert!(e_stop_triggered.load(Ordering::SeqCst), "ActionHandler must execute registered action callback");
}

#[test]
fn test_accesskit_role_and_action_roundtrip_mapping() {
    use strata_a11y::accesskit::{
        a11y_action_to_accesskit, a11y_role_to_accesskit, accesskit_action_to_a11y,
    };

    // Role mapping verification
    assert_eq!(a11y_role_to_accesskit(A11yRole::Button), accesskit::Role::Button);
    assert_eq!(a11y_role_to_accesskit(A11yRole::Checkbox), accesskit::Role::CheckBox);
    assert_eq!(a11y_role_to_accesskit(A11yRole::Slider), accesskit::Role::Slider);
    assert_eq!(a11y_role_to_accesskit(A11yRole::Heading), accesskit::Role::Heading);
    assert_eq!(a11y_role_to_accesskit(A11yRole::Dialog), accesskit::Role::Dialog);
    assert_eq!(a11y_role_to_accesskit(A11yRole::TextInput), accesskit::Role::TextInput);
    assert_eq!(a11y_role_to_accesskit(A11yRole::ProgressBar), accesskit::Role::ProgressIndicator);

    // Action round-trip verification
    let click = A11yAction::Click;
    let ak_click = a11y_action_to_accesskit(&click);
    assert_eq!(ak_click, accesskit::Action::Click);
    assert_eq!(accesskit_action_to_a11y(ak_click, None), Some(A11yAction::Click));

    let focus = A11yAction::Focus;
    let ak_focus = a11y_action_to_accesskit(&focus);
    assert_eq!(ak_focus, accesskit::Action::Focus);
    assert_eq!(accesskit_action_to_a11y(ak_focus, None), Some(A11yAction::Focus));

    let val_data = accesskit::ActionData::Value("new text".into());
    assert_eq!(
        accesskit_action_to_a11y(accesskit::Action::SetValue, Some(&val_data)),
        Some(A11yAction::SetValue("new text".to_string()))
    );
}



