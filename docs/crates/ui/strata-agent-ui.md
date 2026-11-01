# strata_agent_ui: Streaming Generative UI Protocol & Sandboxed Agent Widgets

The `strata_agent_ui` crate provides a secure, streaming generative UI protocol and sandboxed widget system for the Strata engine. Designed for LLMs and autonomous infrastructure agents, `strata_agent_ui` enables remote agents to synthesize, stream, patch, and re-render interactive UI components in real time via Server-Sent Events (SSE) and NDJSON. It incorporates strict tag and attribute sanitization sandboxing, transactional time-travel undo/redo history, a typed action dispatch bus, and production-grade conversational widgets (`ApprovalWidget`, `FormWidget`, `TelemetryCard`, `StepProgressWidget`).

---

## Architectural Principles

Generative UI in Strata is treated as a sandboxed, incremental scene graph mutation stream:

```text
┌─────────────────────────────────────────────────────────────┐
│                 Remote Autonomous LLM Agent                 │
│               (Emits chunked SSE / NDJSON patches)          │
└──────────────────────────────┬──────────────────────────────┘
                               │ "data: {"op":"insert", ...}\n"
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                    StreamingUiParser                        │
│   (Reassembles partial line chunks, strips SSE comments)    │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                 AgentSchemaValidator                        │
│   (Strict tag allowlist, attribute sanitization, XSS drop)   │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                    AgentUiSandbox                           │
│   (Virtual node tree, applies AgentPatch deltas in-memory)  │
└──────────────────────────────┬──────────────────────────────┘
                               │
                ┌──────────────┴──────────────┐
                ▼                             ▼
┌─────────────────────────────┐ ┌─────────────────────────────┐
│     TimeTravelHistory       │ │       AgentActionBus        │
│  (Multi-version undo/redo)  │ │ (Dispatches tool callbacks) │
└─────────────────────────────┘ └─────────────────────────────┘
```

1. **Strict Sandboxed Execution**: Untrusted agent tokens cannot inject raw scripts, iframes, inline event handlers (`onload`, `onclick`), or arbitrary JavaScript URIs. Prohibited tags and malicious attributes are rejected during schema validation.
2. **Fine-Grained Differential Deltas**: Agents stream granular AST mutations (`InsertNode`, `UpdateText`, `MutateProps`, `RemoveNode`, `AppendChild`) rather than regenerating entire component trees, enabling sub-millisecond real-time UI updates.
3. **Deterministic Human-in-the-Loop Interaction**: Interactive widgets emit typed action payloads dispatched through the `AgentActionBus`, allowing users to approve infrastructure mutations or submit parameters safely.

---

## The Streaming Protocol: `AgentPatch`

The wire protocol exchanges JSON-serialized operational deltas:

```rust
use std::collections::HashMap;
use strata_agent_ui::prelude::AgentPatch;

let mut props = HashMap::new();
props.insert("class".to_string(), "p-4 bg-neutral-900 border border-neutral-800 rounded-lg".to_string());
props.insert("action".to_string(), "scale_canary".to_string());

// 1. Structural Insertion Patch
let patch = AgentPatch::InsertNode {
    node_id: 101,
    parent_id: Some(1),
    tag: "button".to_string(),
    props,
    text: Some("Rollout Canary Release".to_string()),
};

// Serialize to JSON wire format
let json_payload = patch.to_json();
assert!(json_payload.contains("\"op\":\"insert\""));

// Deserialize from incoming stream
let parsed = AgentPatch::parse_json(&json_payload).unwrap();
```

#### Protocol Operational Deltas

| Operation | JSON Representation | Description |
| :--- | :--- | :--- |
| `InsertNode` | `{"op":"insert", "id": 101, "parent": 1, "tag": "button", "props": {...}, "text": "..."}` | Allocates a new element or text node in the sandbox tree. |
| `RemoveNode` | `{"op":"remove", "id": 101}` | Detaches and deletes a node and its recursive descendants. |
| `UpdateText` | `{"op":"text", "id": 101, "text": "Deploying 10%..."}` | Mutates the textual content of a node. |
| `MutateProps`| `{"op":"prop", "id": 101, "key": "class", "value": "text-emerald-400"}` | Sets or updates a single attribute or style utility class. |
| `AppendChild`| `{"op":"append", "parent": 1, "child": 101}` | Moves an existing node into a parent's children array. |

---

## Security Sandbox and Schema Validation

The `AgentSchemaValidator` enforces strict tag white-listing and attribute validation:

```rust
use strata_agent_ui::prelude::{is_attribute_safe, is_tag_allowed, AgentSchemaValidator};

// Tag allowlist verification
assert!(is_tag_allowed("div"));
assert!(is_tag_allowed("button"));
assert!(is_tag_allowed("card"));
assert!(is_tag_allowed("Canvas3D"));

// Prohibited dangerous tags
assert!(!is_tag_allowed("script"));
assert!(!is_tag_allowed("iframe"));
assert!(!is_tag_allowed("object"));
assert!(!is_tag_allowed("embed"));

// Attribute safety inspection
assert!(is_attribute_safe("class", "p-4 bg-neutral-950 font-mono"));
assert!(is_attribute_safe("action", "execute_canary_rollout"));

// Malicious attribute rejection
assert!(!is_attribute_safe("href", "javascript:alert(1)"));
assert!(!is_attribute_safe("src", "data:text/html;base64,PHNjcmlwdD4="));
assert!(!is_attribute_safe("onload", "malicious_payload()"));

let validator = AgentSchemaValidator::new();
assert!(validator.validate_tag("button").is_ok());
assert!(validator.validate_tag("script").is_err());
```

---

## The Chunked Streaming Parser: `StreamingUiParser`

Handles partial chunks, split lines, HTTP keep-alive comments, and terminal signals over SSE connections:

```rust
use strata_agent_ui::prelude::{AgentUiSandbox, StreamingUiParser};

let mut parser = StreamingUiParser::new();
let mut sandbox = AgentUiSandbox::new();

// Chunk 1: Incomplete split JSON boundary across TCP packets
let chunk1 = "data: {\"op\":\"insert\",\"id\":1,\"parent\":null,\"tag\":\"card\",\"props\":";
let p1 = parser.feed_chunk(chunk1);
assert!(p1.is_empty()); // Buffered, awaiting remainder
assert!(parser.has_buffered_data());

// Chunk 2: Completes line 1 and provides line 2
let chunk2 = "{\"class\":\"bg-neutral-900\"},\"text\":null}\ndata: {\"op\":\"insert\",\"id\":2,\"parent\":1,\"tag\":\"button\",\"props\":{},\"text\":\"Deploy\"}\n";
let p2 = parser.feed_chunk(chunk2);
assert_eq!(p2.len(), 2); // Both patches fully parsed!
assert!(!parser.has_buffered_data());

// Apply directly into sandbox
for patch in p2 {
    sandbox.apply_patch(patch).unwrap();
}

// Chunk 3: Keep-alive comment and terminal [DONE] marker
let chunk3 = ": keep-alive\ndata: {\"op\":\"text\",\"id\":2,\"text\":\"Deploying...\"}\ndata: [DONE]\n";
let applied = parser.feed_chunk_and_apply(chunk3, &mut sandbox).unwrap();
assert_eq!(applied, 1);
assert_eq!(parser.total_patches(), 3);
```

---

## Time-Travel History: `TimeTravelHistory`

Enables users to step backward and forward through generative UI revisions:

```rust
use strata_agent_ui::prelude::*;

let mut sandbox = AgentUiSandbox::new();
sandbox.apply_patch(AgentPatch::InsertNode {
    node_id: 1,
    parent_id: None,
    tag: "div".to_string(),
    props: Default::default(),
    text: Some("Cluster State: Active".to_string()),
}).unwrap();

let mut history = TimeTravelHistory::new(sandbox);

// Agent pushes Revision 2
let mut v2 = history.current().clone();
v2.apply_patch(AgentPatch::UpdateText {
    node_id: 1,
    text: "Cluster State: Draining".to_string(),
}).unwrap();
history.push_revision("Drain Operation Started", v2);

assert_eq!(history.total_revisions(), 2);
assert!(history.current().render_to_html().contains("Draining"));

// User triggers Undo
let reverted = history.undo().expect("Undo succeeds");
assert!(reverted.render_to_html().contains("Active"));

// User triggers Redo
let redone = history.redo().expect("Redo succeeds");
assert!(redone.render_to_html().contains("Draining"));
```

---

## Typed Action Dispatch: `AgentActionBus`

Connects sandboxed widget interactions to system tool handlers:

```rust
use std::collections::HashMap;
use strata_agent_ui::prelude::AgentActionBus;

let mut bus = AgentActionBus::new();

// Register infrastructure tool action
bus.register("scale_cluster", |_action, params| {
    let nodes = params.get("nodes").cloned().unwrap_or_else(|| "0".to_string());
    let region = params.get("region").cloned().unwrap_or_else(|| "us-east-1".to_string());
    Ok(format!("Scaled cluster in {} to {} nodes", region, nodes))
});

assert!(bus.has_action("scale_cluster"));

// Dispatch action triggered from UI button click
let mut params = HashMap::new();
params.insert("nodes".to_string(), "32".to_string());
params.insert("region".to_string(), "eu-west-1".to_string());

let result = bus.dispatch("scale_cluster", &params).unwrap();
assert_eq!(result, "Scaled cluster in eu-west-1 to 32 nodes");
```

---

## Production Conversational Agent Widgets

### 1. Human-in-the-Loop Approval: `ApprovalWidget`

Presents sensitive cloud operational changes requiring explicit human confirmation:

```rust
use strata_agent_ui::prelude::{ApprovalStatus, ApprovalWidget};

let mut approval = ApprovalWidget::new(
    "app_deploy_canary",
    "Authorize Canary Ingress Switch",
    "Diverts 10% of production traffic to release candidate v2.4.1.",
    "execute_canary_rollout",
)
.with_param("target_version", "v2.4.1")
.with_param("traffic_percentage", "10%")
.with_param("monitoring_window", "15m");

assert_eq!(approval.status, ApprovalStatus::Pending);

// User clicks "Authorize" in UI
approval.approve();
assert_eq!(approval.status, ApprovalStatus::Approved);

// Convert to Strata scene graph blueprint
let blueprint = approval.render_blueprint();
```

---

### 2. Autonomous Form Generation: `FormWidget`

Dynamically requests structured parameters from users:

```rust
use strata_agent_ui::prelude::{FormField, FormFieldType, FormWidget};

let mut form = FormWidget::new("form_provision", "Cluster Provisioning Parameters");

form.add_field(FormField::new("cluster_name", "Cluster Subdomain", FormFieldType::Text).required());
form.add_field(FormField::new("node_count", "Initial Node Count", FormFieldType::Number));
form.add_field(FormField::new(
    "region",
    "Target Cloud Region",
    FormFieldType::Select(vec!["us-east-1".into(), "us-west-2".into(), "eu-central-1".into()]),
));

form.set_value("cluster_name", "prod-telemetry-01");
let blueprint = form.render_blueprint();
```

---

### 3. Real-Time Telemetry Card: `TelemetryCard`

Displays synthesized analytical KPIs with status indicators:

```rust
use strata_agent_ui::prelude::{MetricStatus, TelemetryCard};

let card = TelemetryCard::new("tc_ingest_rate", "Telemetry Ingestion Rate", "2.84M")
    .with_unit("req/sec")
    .with_trend(12.4) // +12.4% vs previous interval
    .with_status(MetricStatus::Optimal);

let blueprint = card.render_blueprint();
```

---

### 4. Step-by-Step Progress Tracker: `StepProgressWidget`

Coordinates multi-step cloud execution workflows:

```rust
use strata_agent_ui::prelude::{StepProgressWidget, StepStatus};

let mut tracker = StepProgressWidget::new("seq_rollout", "Zero-Downtime Rollout Sequence");

let s1 = tracker.add_step("Provision Target VPC & Subnets");
let s2 = tracker.add_step("Spin Up Green Node Replica Set");
let s3 = tracker.add_step("Execute Health Probes & Synthetic Canary Tests");
let s4 = tracker.add_step("Switch Route53 DNS to Green Target");

tracker.set_step_status(s1, StepStatus::Completed);
tracker.set_step_status(s2, StepStatus::Completed);
tracker.set_step_status(s3, StepStatus::Running);
tracker.set_step_status(s4, StepStatus::Pending);

let blueprint = tracker.render_blueprint();
```
