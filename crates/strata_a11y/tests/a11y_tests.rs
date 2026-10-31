use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
