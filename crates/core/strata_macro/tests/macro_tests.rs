use strata_macro::{component, dynamic, rsx};
use strata_dom::{Attribute, NodeBlueprint, Tag, View};
use strata_js::JsValue;

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
        NodeBlueprint::Element { tag, classes, attributes, children } => {
            assert_eq!(tag, Tag::Div);
            assert_eq!(classes, vec!["container".to_string()]);
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "id" && value == "main")));
            assert_eq!(children.len(), 1);
            match &children[0] {
                NodeBlueprint::Element { tag: child_tag, classes: child_classes, .. } => {
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
        NodeBlueprint::Element { tag, attributes, .. } => {
            assert_eq!(tag, Tag::Button);
            let has_listener = attributes.iter().any(|a| matches!(a, Attribute::EventListener { .. }));
            assert!(has_listener);
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
    let elapsed = 3.14159f32;
    let val = dynamic!({
        "status": "ready",
        "code": 200,
        "is_prod": true,
        "time": elapsed,
        "meta": {
            "author": "Antigravity",
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
    assert!((val["time"].as_f64().unwrap() - 3.14159).abs() < 1e-4);

    let meta = &val["meta"];
    assert_eq!(meta["author"].as_str(), Some("Antigravity"));
    assert_eq!(meta["tags"][0].as_str(), Some("frontend"));
    assert_eq!(meta["tags"][1].as_str(), Some("rust"));

    let flags = &val["flags"];
    assert_eq!(flags[0].as_f64(), Some(1.0));
    assert_eq!(flags[1].as_f64(), Some(2.0));
    assert_eq!(flags[2].as_f64(), Some(3.0));

    assert!(val["nullable"].is_null());
    assert!(val["void_val"].is_undefined());
}
