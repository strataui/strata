use std::collections::HashMap;
use strata_agent_ui::prelude::*;

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
    // Evasion vectors that must be rejected
    assert!(!is_attribute_safe("href", " javascript:alert(1)"));
    assert!(!is_attribute_safe("href", "\tjavascript:alert(1)"));
    assert!(!is_attribute_safe("href", "\njavascript:evil()"));
    assert!(!is_attribute_safe("href", "vbscript:msgbox(1)"));
    assert!(!is_attribute_safe("src", "data:image/svg+xml,<svg onload=alert(1)>"));
    assert!(!is_attribute_safe("onclick", "alert(1)"));
    assert!(!is_attribute_safe("oninput", "eval(1)"));
    assert!(!is_attribute_safe("onchange", "evil()"));
    assert!(!is_attribute_safe("onload", "evil()"));

    // HTML entity evasion vectors
    assert!(!is_attribute_safe("href", "java&#x09;script:alert(1)"));
    assert!(!is_attribute_safe("href", "java&#9;script:alert(1)"));
    assert!(!is_attribute_safe("href", "&#106;avascript:alert(1)"));
    assert!(!is_attribute_safe("href", "&#x6a;avascript:alert(1)"));
    assert!(!is_attribute_safe("href", "&Tab;javascript:alert(1)"));

    // Arbitrary data: URIs must be blocked except safe images
    assert!(!is_attribute_safe("src", "data:text/javascript,alert(1)"));
    assert!(!is_attribute_safe("src", "data:application/xhtml+xml,<script>alert(1)</script>"));
    assert!(!is_attribute_safe("src", "data:;base64,alert(1)"));
    assert!(!is_attribute_safe("src", "data:image/png;base64,<script>alert(1)</script>"));
    assert!(is_attribute_safe("src", "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="));
    assert!(is_attribute_safe("src", "data:image/webp;base64,UklGRiQAAABXRUJQVlA4IBgAAAAwAQCdASoBAAEAAwA0JaQAA3AA/vuUAAA="));

    // Style attribute script injection
    assert!(!is_attribute_safe("style", "width: expression(alert(1))"));
    assert!(!is_attribute_safe("style", "background: url('javascript:alert(1)')"));
    assert!(is_attribute_safe("style", "color: #ffffff; padding: 12px; margin-top: 4px;"));

    let validator = AgentSchemaValidator::new();
    assert!(validator.validate_tag("button").is_ok());
    assert!(validator.validate_tag("script").is_err());
}

#[test]
fn test_agent_patch_json_roundtrip() {
    let mut props = HashMap::new();
    props.insert("class".to_string(), "p-4 bg-[#0f1012]".to_string());
    props.insert("action".to_string(), "scale_cluster".to_string());

    let patch = AgentPatch::InsertNode {
        node_id: 101,
        parent_id: Some(1),
        tag: "button".to_string(),
        props,
        text: Some("Deploy Cluster".to_string()),
    };

    let json = patch.to_json();
    let parsed = AgentPatch::parse_json(&json).expect("Failed to parse json patch");

    if let AgentPatch::InsertNode { node_id, parent_id, tag, props: parsed_props, text } = parsed {
        assert_eq!(node_id, 101);
        assert_eq!(parent_id, Some(1));
        assert_eq!(tag, "button");
        assert_eq!(text.as_deref(), Some("Deploy Cluster"));
        assert_eq!(parsed_props.get("action").map(|s| s.as_str()), Some("scale_cluster"));
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
        text: Some("evil()".to_string()),
    };
    assert!(sandbox.apply_patch(bad_patch).is_err());
}

#[test]
fn test_agent_action_bus() {
    let mut bus = AgentActionBus::new();

    bus.register("scale_cluster", |_action, params| {
        let nodes = params.get("nodes").cloned().unwrap_or_else(|| "0".to_string());
        Ok(format!("Cluster scaled to {} nodes", nodes))
    });

    assert!(bus.has_action("scale_cluster"));
    assert!(!bus.has_action("unknown_action"));

    let mut params = HashMap::new();
    params.insert("nodes".to_string(), "12".to_string());

    let res = bus.dispatch("scale_cluster", &params).unwrap();
    assert_eq!(res, "Cluster scaled to 12 nodes");
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

#[test]
fn test_streaming_ui_parser_chunked_sse() {
    let mut parser = StreamingUiParser::new();
    let mut sandbox = AgentUiSandbox::new();

    // Chunk 1: Incomplete line
    let chunk1 = "data: {\"op\":\"insert\",\"id\":1,\"parent\":null,\"tag\":\"card\",\"props\":";
    let p1 = parser.feed_chunk(chunk1);
    assert!(p1.is_empty());
    assert!(parser.has_buffered_data());

    // Chunk 2: Completes line 1 and sends line 2
    let chunk2 = "{\"class\":\"bg-slate-900\"},\"text\":null}\ndata: {\"op\":\"insert\",\"id\":2,\"parent\":1,\"tag\":\"button\",\"props\":{},\"text\":\"Deploy\"}\n";
    let p2 = parser.feed_chunk(chunk2);
    assert_eq!(p2.len(), 2);
    assert!(!parser.has_buffered_data());

    for patch in p2 {
        assert!(sandbox.apply_patch(patch).is_ok());
    }

    // Chunk 3: SSE with keep-alive comments and [DONE]
    let chunk3 = ": keep-alive\ndata: {\"op\":\"text\",\"id\":2,\"text\":\"Deploying...\"}\ndata: [DONE]\n";
    let applied = parser.feed_chunk_and_apply(chunk3, &mut sandbox).expect("Apply should succeed");
    assert_eq!(applied, 1);
    assert_eq!(parser.total_patches(), 3);

    let html = sandbox.render_to_html();
    assert!(html.contains("<card class=\"bg-slate-900\"><button>Deploying...</button></card>"));
}

#[test]
fn test_conversational_widgets_render_and_state() {
    // 1. Approval Widget
    let mut approval = ApprovalWidget::new(
        "app_1",
        "Rollout Production Canary",
        "Approves rolling out canary release to 10% of cluster traffic.",
        "execute_canary_rollout",
    ).with_param("traffic_pct", "10%")
     .with_param("region", "us-east-1");

    assert_eq!(approval.status, ApprovalStatus::Pending);
    let bp_pending = approval.render_blueprint();
    if let strata_dom::NodeBlueprint::Element { tag, children, .. } = bp_pending {
        assert_eq!(tag, strata_dom::Tag::Div);
        assert!(!children.is_empty());
    }

    approval.approve();
    assert_eq!(approval.status, ApprovalStatus::Approved);

    // 2. Form Widget
    let mut form = FormWidget::new("form_1", "Cluster Provisioning Parameters");
    form.add_field(FormField::new("cluster_name", "Cluster Name", FormFieldType::Text).required());
    form.add_field(FormField::new("node_count", "Initial Node Count", FormFieldType::Number));
    form.add_field(FormField::new("region", "Cloud Region", FormFieldType::Select(vec!["us-east-1".into(), "eu-central-1".into()])));
    form.set_value("cluster_name", "prod-telemetry-01");
    let bp_form = form.render_blueprint();
    if let strata_dom::NodeBlueprint::Element { tag, .. } = bp_form {
        assert_eq!(tag, strata_dom::Tag::Form);
    }

    // 3. Telemetry Card
    let card = TelemetryCard::new("tc_1", "Ingestion Throughput", "2.4M")
        .with_unit("req/s")
        .with_trend(14.2)
        .with_status(MetricStatus::Optimal);
    let bp_card = card.render_blueprint();
    if let strata_dom::NodeBlueprint::Element { tag, .. } = bp_card {
        assert_eq!(tag, strata_dom::Tag::Div);
    }

    // 4. Step Progress Widget
    let mut progress = StepProgressWidget::new("sp_1", "Blue-Green Deployment Sequence");
    let s1 = progress.add_step("Provision Target Subnet");
    let s2 = progress.add_step("Spin Up Green Replica Pool");
    let s3 = progress.add_step("Run Health Probes & Switch Route");

    progress.set_step_status(s1, StepStatus::Completed);
    progress.set_step_status(s2, StepStatus::Running);
    progress.set_step_status(s3, StepStatus::Pending);

    let bp_progress = progress.render_blueprint();
    if let strata_dom::NodeBlueprint::Element { tag, children, .. } = bp_progress {
        assert_eq!(tag, strata_dom::Tag::Div);
        // Header + 3 steps = 4 children
        assert_eq!(children.len(), 4);
    }
}
