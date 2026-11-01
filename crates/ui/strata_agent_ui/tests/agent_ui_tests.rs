use std::collections::HashMap;
use strata_agent_ui::{
    is_attribute_safe, is_tag_allowed, AgentActionBus, AgentPatch, AgentSchemaValidator,
    AgentUiSandbox, TimeTravelHistory,
};

#[test]
fn test_schema_tag_and_attribute_security() {
    assert!(is_tag_allowed("div"));
    assert!(is_tag_allowed("button"));
    assert!(is_tag_allowed("Card"));
    assert!(is_tag_allowed("Canvas3D"));

    // Prohibited tags
    assert!(!is_tag_allowed("script"));
    assert!(!is_tag_allowed("iframe"));
    assert!(!is_tag_allowed("object"));

    // Attribute safety
    assert!(is_attribute_safe("class", "p-4 bg-[#08090a]"));
    assert!(is_attribute_safe("action", "fetch_metrics"));
    assert!(!is_attribute_safe("href", "javascript:alert(1)"));
    assert!(!is_attribute_safe("src", "data:text/html;base64,PHNjcmlwdD4="));

    let validator = AgentSchemaValidator::new();
    assert!(validator.validate_tag("button").is_ok());
    assert!(validator.validate_tag("script").is_err());
}

#[test]
fn test_agent_patch_json_roundtrip() {
    let mut props = HashMap::new();
    props.insert("class".to_string(), "p-4 bg-[#0f1012]".to_string());
    props.insert("action".to_string(), "tune_pid".to_string());

    let patch = AgentPatch::InsertNode {
        node_id: 101,
        parent_id: Some(1),
        tag: "button".to_string(),
        props,
        text: Some("Launch Mission".to_string()),
    };

    let json = patch.to_json();
    let parsed = AgentPatch::parse_json(&json).expect("Failed to parse json patch");

    if let AgentPatch::InsertNode { node_id, parent_id, tag, props: parsed_props, text } = parsed {
        assert_eq!(node_id, 101);
        assert_eq!(parent_id, Some(1));
        assert_eq!(tag, "button");
        assert_eq!(text.as_deref(), Some("Launch Mission"));
        assert_eq!(parsed_props.get("action").map(|s| s.as_str()), Some("tune_pid"));
    } else {
        panic!("Parsed wrong patch variant");
    }
}

#[test]
fn test_sandboxed_generative_ui_streaming() {
    let mut sandbox = AgentUiSandbox::new();

    // 1. Insert root card
    let patch1 = AgentPatch::InsertNode {
        node_id: 1,
        parent_id: None,
        tag: "card".to_string(),
        props: HashMap::new(),
        text: None,
    };
    assert!(sandbox.apply_patch(patch1).is_ok());

    // 2. Insert child button
    let patch2 = AgentPatch::InsertNode {
        node_id: 2,
        parent_id: Some(1),
        tag: "button".to_string(),
        props: HashMap::new(),
        text: Some("Initial Text".to_string()),
    };
    assert!(sandbox.apply_patch(patch2).is_ok());

    // 3. Update text
    let patch3 = AgentPatch::UpdateText {
        node_id: 2,
        text: "Updated Text".to_string(),
    };
    assert!(sandbox.apply_patch(patch3).is_ok());

    // 4. Mutate prop
    let patch4 = AgentPatch::MutateProps {
        node_id: 2,
        key: "class".to_string(),
        value: "text-emerald-500".to_string(),
    };
    assert!(sandbox.apply_patch(patch4).is_ok());

    let html = sandbox.render_to_html();
    assert!(html.contains("<card><button class=\"text-emerald-500\">Updated Text</button></card>"));

    // 5. Attempt malicious script injection - MUST BE REJECTED
    let bad_patch = AgentPatch::InsertNode {
        node_id: 3,
        parent_id: Some(1),
        tag: "script".to_string(),
        props: HashMap::new(),
        text: Some("eval('evil')".to_string()),
    };
    assert!(sandbox.apply_patch(bad_patch).is_err());
}

#[test]
fn test_agent_action_bus() {
    let mut bus = AgentActionBus::new();

    bus.register("calculate_trajectory", |_action, params| {
        let waypoints = params.get("waypoints").cloned().unwrap_or_else(|| "0".to_string());
        Ok(format!("Trajectory planned for {} waypoints", waypoints))
    });

    assert!(bus.has_action("calculate_trajectory"));
    assert!(!bus.has_action("unknown_action"));

    let mut params = HashMap::new();
    params.insert("waypoints".to_string(), "12".to_string());

    let res = bus.dispatch("calculate_trajectory", &params).unwrap();
    assert_eq!(res, "Trajectory planned for 12 waypoints");
}

#[test]
fn test_time_travel_history() {
    let mut initial_sandbox = AgentUiSandbox::new();
    initial_sandbox.apply_patch(AgentPatch::InsertNode {
        node_id: 1,
        parent_id: None,
        tag: "div".to_string(),
        props: HashMap::new(),
        text: Some("Version 1".to_string()),
    }).unwrap();

    let mut history = TimeTravelHistory::new(initial_sandbox);
    assert_eq!(history.total_revisions(), 1);

    // Push revision 2
    let mut v2_sandbox = history.current().clone();
    v2_sandbox.apply_patch(AgentPatch::UpdateText {
        node_id: 1,
        text: "Version 2".to_string(),
    }).unwrap();
    history.push_revision("Update to V2", v2_sandbox);
    assert_eq!(history.total_revisions(), 2);
    assert!(history.current().render_to_html().contains("Version 2"));

    // Undo back to revision 1
    let reverted = history.undo().expect("Undo failed");
    assert!(reverted.render_to_html().contains("Version 1"));

    // Redo to revision 2
    let redone = history.redo().expect("Redo failed");
    assert!(redone.render_to_html().contains("Version 2"));
}
