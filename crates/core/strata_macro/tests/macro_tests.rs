use std::cell::Cell;
use std::rc::Rc;
use strata_dom::{Attribute, Effect, EventType, NodeBlueprint, Tag, View};
use strata_js::JsValue;
use strata_macro::{component, dynamic, rsx, Store};

#[component]
fn Header(title: &'static str, subtitle: Option<&'static str>) -> View {
    rsx! {
        <header class="header">
            <h1>{title}</h1>
            {subtitle.map(|sub| rsx! { <h2>{sub}</h2> })}
        </header>
    }
}

#[component]
fn Button(label: &'static str, disabled: bool) -> View {
    rsx! {
        <button class="btn" disabled={disabled}>
            {label}
        </button>
    }
}

#[test]
fn test_rsx_simple_elements() {
    let node = rsx! {
        <div id="main" class="container">
            <span class="text">{"Hello, Strata!"}</span>
        </div>
    };

    match node.root {
        NodeBlueprint::Element {
            tag,
            classes,
            attributes,
            children,
        } => {
            assert_eq!(tag, Tag::Div);
            assert_eq!(classes, vec!["container".to_string()]);
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "id" && value == "main")));
            assert_eq!(children.len(), 1);
            match &children[0] {
                NodeBlueprint::Element {
                    tag: child_tag,
                    classes: child_classes,
                    ..
                } => {
                    assert_eq!(*child_tag, Tag::Span);
                    assert_eq!(*child_classes, vec!["text".to_string()]);
                }
                _ => panic!("Expected child element"),
            }
        }
        _ => panic!("Expected element node"),
    }
}

#[test]
fn test_rsx_void_tags_and_boolean_attributes() {
    let node = rsx! {
        <form>
            <input type="text" placeholder="Enter name..." autofocus />
            <br />
            <img src="logo.png" alt="Logo" />
        </form>
    };

    match node.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(tag, Tag::Form);
            assert_eq!(children.len(), 3);
        }
        _ => panic!("Expected element node"),
    }
}

#[test]
fn test_rsx_expression_interpolation() {
    let count = 42;
    let maybe_badge: Option<&'static str> = Some("NEW");
    let none_badge: Option<&'static str> = None;
    let items = vec!["Alpha", "Beta", "Gamma"];

    let node = rsx! {
        <div class="metrics">
            <p>{"Count: "}{count}</p>
            {maybe_badge.map(|b| rsx! { <span class="badge">{b}</span> })}
            {none_badge.map(|b| rsx! { <span class="badge">{b}</span> })}
            <ul>
                {items.into_iter().map(|item| rsx! { <li>{item}</li> }).collect::<Vec<_>>()}
            </ul>
        </div>
    };

    match node.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(tag, Tag::Div);
            assert!(children.len() >= 3);
        }
        _ => panic!("Expected element node"),
    }
}

#[test]
fn test_rsx_event_listeners() {
    let clicked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let clicked_clone = clicked.clone();

    let node = rsx! {
        <button onclick={move |_evt| { clicked_clone.store(true, std::sync::atomic::Ordering::SeqCst); }}>
            {"Click me"}
        </button>
    };

    match node.root {
        NodeBlueprint::Element {
            tag, attributes, ..
        } => {
            assert_eq!(tag, Tag::Button);
            let has_listener = attributes
                .iter()
                .any(|a| matches!(a, Attribute::EventListener { .. }));
            assert!(has_listener);
        }
        _ => panic!("Expected element node"),
    }
}

#[test]
fn test_rsx_snake_case_event_names_normalize() {
    let node = rsx! {
        <button
            on_click={|_| {}}
            on_pointer_down={|_| {}}
            on_input={|_value: String| {}}
        >
            {"Click me"}
        </button>
    };

    match node.root {
        NodeBlueprint::Element { attributes, .. } => {
            let events: Vec<EventType> = attributes
                .iter()
                .filter_map(|a| match a {
                    Attribute::EventListener { event, .. } => Some(*event),
                    _ => None,
                })
                .collect();
            assert_eq!(
                events,
                vec![EventType::Click, EventType::PointerDown, EventType::Input]
            );
        }
        _ => panic!("Expected element node"),
    }
}

#[test]
fn test_component_macro_and_builder_syntax() {
    // 1. Invoking via RSX custom component syntax
    let rsx_view = rsx! {
        <div>
            <Header title="Strata Engine" subtitle={Some("Next-Gen UI")} />
            <Button label="Submit" disabled=false />
        </div>
    };

    match rsx_view.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(tag, Tag::Div);
            assert_eq!(children.len(), 2);
        }
        _ => panic!("Expected element node"),
    }

    // 2. Invoking via generated companion builder in type namespace
    let built_header = Header::builder()
        .title("Direct Builder Title")
        .subtitle(None)
        .build();

    match built_header.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(tag, Tag::Header),
        _ => panic!("Expected header element"),
    }

    // 3. Invoking original function in value namespace
    let fn_header = Header("Direct Fn Title", Some("Direct Subtitle"));
    match fn_header.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(tag, Tag::Header),
        _ => panic!("Expected header element"),
    }
}

#[test]
fn test_dynamic_macro() {
    let elapsed = std::f32::consts::PI;
    let val = dynamic!({
        "status": "ready",
        "code": 200,
        "is_prod": true,
        "time": elapsed,
        "meta": {
            "author": "Strata",
            "tags": ["frontend", "rust", "wasm"]
        },
        "flags": [1, 2, 3],
        "nullable": null,
        "void_val": undefined
    });

    assert!(matches!(val, JsValue::Object(_)));
    assert_eq!(val["status"].as_str(), Some("ready"));
    assert_eq!(val["code"].as_f64(), Some(200.0));
    assert_eq!(val["is_prod"].as_bool(), Some(true));
    assert!((val["time"].as_f64().unwrap() - std::f64::consts::PI).abs() < 1e-4);

    let meta = &val["meta"];
    assert_eq!(meta["author"].as_str(), Some("Strata"));
    assert_eq!(meta["tags"][0].as_str(), Some("frontend"));
    assert_eq!(meta["tags"][1].as_str(), Some("rust"));

    let flags = &val["flags"];
    assert_eq!(flags[0].as_f64(), Some(1.0));
    assert_eq!(flags[1].as_f64(), Some(2.0));
    assert_eq!(flags[2].as_f64(), Some(3.0));

    assert!(val["nullable"].is_null());
    assert!(val["void_val"].is_undefined());
}

#[derive(Store, Clone, Debug, Default, PartialEq)]
pub struct ClusterTelemetry {
    pub node_id: String,
    pub cpu_percent: f32,
    pub connections: u32,
    pub is_healthy: bool,
}

#[test]
fn test_derive_store_macro_atomic_reactivity_and_transactions() {
    let initial = ClusterTelemetry {
        node_id: "node-us-east-1a".to_string(),
        cpu_percent: 24.5,
        connections: 1200,
        is_healthy: true,
    };

    let store = ClusterTelemetryStore::new(initial.clone());

    // 1. Initial field signals
    assert_eq!(store.node_id.get(), "node-us-east-1a");
    assert_eq!(store.cpu_percent.get(), 24.5);
    assert_eq!(store.connections.get(), 1200);
    assert!(store.is_healthy.get());

    // 2. Track single field reactivity
    let cpu_runs = Rc::new(Cell::new(0));
    let cpu_obs = Rc::new(Cell::new(0.0f32));

    let cr = cpu_runs.clone();
    let co = cpu_obs.clone();
    let cpu_sig = store.cpu_percent.clone();

    let _effect = Effect::new(move || {
        cr.set(cr.get() + 1);
        co.set(cpu_sig.get());
        None
    });

    assert_eq!(cpu_runs.get(), 1);
    assert_eq!(cpu_obs.get(), 24.5);

    // Mutate only connections (cpu subscriber MUST NOT run)
    store.connections.set(1350);
    assert_eq!(
        cpu_runs.get(),
        1,
        "Unrelated field mutation must not trigger cpu effect"
    );
    assert_eq!(store.connections.get(), 1350);

    // Mutate cpu
    store.cpu_percent.set(78.2);
    assert_eq!(cpu_runs.get(), 2);
    assert_eq!(cpu_obs.get(), 78.2);

    // 3. Atomic Batch Transaction
    let batch_runs = Rc::new(Cell::new(0));
    let br = batch_runs.clone();
    let s_node = store.node_id.clone();
    let s_conn = store.connections.clone();

    let _batch_effect = Effect::new(move || {
        br.set(br.get() + 1);
        let _ = s_node.get();
        let _ = s_conn.get();
        None
    });

    assert_eq!(batch_runs.get(), 1);

    // Perform batch update across multiple fields
    store.batch(|s| {
        s.node_id.set("node-eu-west-1b".to_string());
        s.connections.set(4500);
    });

    // Despite 2 field writes, the effect must run only ONCE after the batch completes!
    assert_eq!(
        batch_runs.get(),
        2,
        "Batch must coalesce notifications to exactly one flush"
    );
    assert_eq!(store.node_id.get(), "node-eu-west-1b");
    assert_eq!(store.connections.get(), 4500);

    // 4. Snapshot and Transaction Rollback
    let snapshot = store.snapshot();
    assert_eq!(snapshot.node_id, "node-eu-west-1b");
    assert_eq!(snapshot.connections, 4500);

    // Speculative mutation
    store.cpu_percent.set(99.9);
    store.is_healthy.set(false);
    assert_eq!(store.cpu_percent.get(), 99.9);
    assert!(!store.is_healthy.get());

    // Rollback to snapshot
    store.restore(&snapshot);
    assert_eq!(store.cpu_percent.get(), 78.2);
    assert!(store.is_healthy.get());
}

#[test]
fn test_rsx_class_deduplication_and_compile_time_bitmasking() {
    // 10 standard utility classes
    let view = rsx! {
        <div class="p-4 flex bg-neutral-900 text-white rounded-lg shadow-md border border-neutral-800 m-2 cursor-pointer" />
    };

    match view.root {
        NodeBlueprint::Element { classes, .. } => {
            // Verify that it is a Masked variant with 10 classes encoded in the bitmask
            assert_eq!(classes.len(), 10);
            assert!(classes.contains("p-4"));
            assert!(classes.contains("flex"));
            assert!(classes.contains("bg-neutral-900"));
            assert!(classes.contains("text-white"));
            assert!(classes.contains("rounded-lg"));
            assert!(classes.contains("shadow-md"));
            assert!(classes.contains("border"));
            assert!(classes.contains("border-neutral-800"));
            assert!(classes.contains("m-2"));
            assert!(classes.contains("cursor-pointer"));

            // Verify variant is zero-allocation Masked with empty tail
            match classes {
                strata_dom::ClassList::Masked { mask, tail } => {
                    assert_eq!(mask.count_ones(), 10, "All 10 classes mapped to bitmask");
                    assert_eq!(tail.len(), 0, "Zero dynamic/tail classes");
                }
                _ => panic!("Expected Masked class list"),
            }
        }
        _ => panic!("Expected Element"),
    }
}

#[test]
fn test_rsx_class_deduplication_removes_redundant_tokens() {
    // Repeated tokens at compile time are deduplicated into single bits
    let view = rsx! {
        <div class="flex p-4 flex p-4 flex text-white text-white" />
    };

    match view.root {
        NodeBlueprint::Element { classes, .. } => {
            assert_eq!(classes.len(), 3);
            assert!(classes.contains("flex"));
            assert!(classes.contains("p-4"));
            assert!(classes.contains("text-white"));

            if let strata_dom::ClassList::Masked { mask, tail } = classes {
                assert_eq!(mask.count_ones(), 3);
                assert_eq!(tail.len(), 0);
            } else {
                panic!("Expected Masked class list");
            }
        }
        _ => panic!("Expected Element"),
    }
}
