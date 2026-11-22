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

mod nested_ui {
    use strata_dom::{NodeBlueprint, View};
    use strata_macro::{component, rsx};

    #[component]
    pub fn Pill(label: &'static str, children: Option<Vec<NodeBlueprint>>) -> View {
        rsx! {
            <div class="pill" data-label={label}>
                {children.unwrap_or_default()}
            </div>
        }
    }

    #[component]
    pub fn Divider() -> View {
        rsx! {
            <hr class="divider" />
        }
    }

    pub fn render_status(code: &'static str) -> View {
        rsx! {
            <span class="status">{code}</span>
        }
    }

    pub struct FakeIcons;

    #[allow(non_snake_case)]
    impl FakeIcons {
        pub fn Plus(class: Option<&'static str>) -> View {
            let cls = class.unwrap_or("icon-default");
            rsx! {
                <i class={cls} />
            }
        }
    }
}

#[test]
fn test_rsx_path_qualified_components_and_portals() {
    use nested_ui::FakeIcons;

    let view = rsx! {
        <div class="host">
            <strata_dom::Portal>
                <div id="unanchored-portal-body">{"Top Layer"}</div>
            </strata_dom::Portal>
            <strata_dom::Portal anchor="menu-trigger" placement="bottom-end" offset={6.0}>
                <button id="anchored-portal-action">{"Action"}</button>
            </strata_dom::Portal>
            <strata_dom::Portal />
            <::strata_dom::Portal>
                <span id="global-path-portal">{"Global Path"}</span>
            </::strata_dom::Portal>
            <nested_ui::Pill label="beta">
                <span>{"Child"}</span>
            </nested_ui::Pill>
            <self::nested_ui::Divider />
            <nested_ui::render_status code="200" />
            <FakeIcons::Plus class={Some("w-4 h-4")} />
            <nested_ui::FakeIcons::Plus />
        </div>
    };

    match view.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(tag, Tag::Div);
            assert_eq!(children.len(), 9);

            // First 4 children are path-qualified `<strata_dom::Portal>` / `<::strata_dom::Portal>` nodes
            for (idx, portal_node) in children.iter().take(4).enumerate() {
                match portal_node {
                    NodeBlueprint::Element {
                        tag, attributes, ..
                    } => {
                        assert_eq!(*tag, Tag::Custom("portal"));
                        assert!(
                            attributes.iter().any(|attr| matches!(
                                attr,
                                Attribute::Property { key, value }
                                    if key == "data-portal" && value == "true"
                            )),
                            "Child {idx} must be a top-layer Portal element"
                        );
                    }
                    _ => panic!("Expected Portal Element at child {idx}"),
                }
            }
        }
        _ => panic!("Expected root Element"),
    }
}

#[component]
fn OptionalToastHost(max_visible: Option<usize>, position: Option<&'static str>) -> View {
    let max_v = max_visible.unwrap_or(3);
    let pos = position.unwrap_or("bottom-right");
    rsx! {
        <div data-max={max_v.to_string()} data-pos={pos} />
    }
}

#[test]
fn test_zero_prop_self_closing_component_with_optional_params() {
    let view = rsx! {
        <div>
            <OptionalToastHost />
            <OptionalToastHost max_visible={5} />
        </div>
    };
    match view.root {
        NodeBlueprint::Element { children, .. } => {
            assert_eq!(children.len(), 2);
            match &children[0] {
                NodeBlueprint::Element { attributes, .. } => {
                    assert!(attributes.iter().any(|a| matches!(
                        a,
                        Attribute::Property { key, value } if key == "data-max" && value == "3"
                    )));
                    assert!(attributes.iter().any(|a| matches!(
                        a,
                        Attribute::Property { key, value } if key == "data-pos" && value == "bottom-right"
                    )));
                }
                _ => panic!("Expected Element"),
            }
        }
        _ => panic!("Expected root Element"),
    }
}

#[component]
fn CallbackHost(
    on_change: Option<Rc<dyn Fn(&str)>>,
    on_count: Option<std::rc::Rc<dyn Fn(usize)>>,
    on_action: Rc<dyn Fn()>,
) -> View {
    if let Some(ref cb) = on_change {
        cb("hello");
    }
    if let Some(ref cb) = on_count {
        cb(42);
    }
    on_action();
    rsx! { <div /> }
}

#[test]
fn test_component_rc_closure_and_inline_closure_coercion() {
    let seen_str = Rc::new(std::cell::RefCell::new(String::new()));
    let seen_num = Rc::new(Cell::new(0usize));
    let seen_act = Rc::new(Cell::new(0usize));

    let s1 = Rc::clone(&seen_str);
    let n1 = Rc::clone(&seen_num);
    let a1 = Rc::clone(&seen_act);

    // 1. Passing unannotated `Rc::new(|s| ...)` AND bare inline closures `|n| ...` directly inside `rsx!`
    //    with method calls on `s` (`s.to_uppercase()`) and `n` (`n.saturating_add(1)`) to prove
    //    expected-type inference works without parameter type annotations!
    let _v1 = rsx! {
        <CallbackHost
            on_change={Rc::new(move |s| *s1.borrow_mut() = s.to_uppercase())}
            on_count={move |n| n1.set(n.saturating_add(1))}
            on_action={Rc::new(move || a1.set(a1.get() + 1))}
        />
    };
    assert_eq!(*seen_str.borrow(), "HELLO");
    assert_eq!(seen_num.get(), 43);
    assert_eq!(seen_act.get(), 1);

    // 2. Passing bare `None`, typed `Rc<dyn Fn(...)>`, and explicit `.on_change_fn(|s| ...)` builder overload
    let s2 = Rc::clone(&seen_str);
    let a2 = Rc::clone(&seen_act);
    let typed_cb: Rc<dyn Fn(&str)> = Rc::new(move |s| *s2.borrow_mut() = format!("{s}-world"));
    let _v2 = rsx! {
        <CallbackHost
            on_change={typed_cb}
            on_count={None}
            on_action={move || a2.set(a2.get() + 10)}
        />
    };
    assert_eq!(*seen_str.borrow(), "hello-world");
    assert_eq!(seen_act.get(), 11);

    // 3. Direct builder `.on_change_fn(|s| ...)` and `.on_count_fn(|n| ...)` overloads with inferred arg types
    let s3 = Rc::clone(&seen_str);
    let n3 = Rc::clone(&seen_num);
    let a3 = Rc::clone(&seen_act);
    let _v3 = CallbackHost::builder()
        .on_change_fn(move |s| *s3.borrow_mut() = s.repeat(2))
        .on_count_fn(move |n| n3.set(n.wrapping_mul(2)))
        .on_action_fn(move || a3.set(a3.get() + 100))
        .build();
    assert_eq!(*seen_str.borrow(), "hellohello");
    assert_eq!(seen_num.get(), 84);
    assert_eq!(seen_act.get(), 111);

    // 4. Block-wrapped closures (`{ let ...; move |s| ... }`), `Some(|n| ...)`, and `Some(Rc::new(|s| ...))`
    //    preserve expected-type inference on unannotated closure arguments (`s.to_uppercase()`, `n.saturating_add(...)`)
    let _v4 = rsx! {
        <CallbackHost
            on_change={{
                let s4 = Rc::clone(&seen_str);
                let prefix = "block";
                move |s| *s4.borrow_mut() = format!("{prefix}:{}", s.to_uppercase())
            }}
            on_count={Some({
                let n4 = Rc::clone(&seen_num);
                move |n| n4.set(n.saturating_add(8))
            })}
            on_action={{
                let a4 = Rc::clone(&seen_act);
                move || a4.set(a4.get() + 1000)
            }}
        />
    };
    assert_eq!(*seen_str.borrow(), "block:HELLO");
    assert_eq!(seen_num.get(), 50);
    assert_eq!(seen_act.get(), 1111);

    // 5. `Some(Rc::new(move |s| ...))` and block ending in `Some(Rc::new(move |n| ...))`
    let s5 = Rc::clone(&seen_str);
    let _v5 = rsx! {
        <CallbackHost
            on_change={Some(Rc::new(move |s| *s5.borrow_mut() = s.to_ascii_uppercase()))}
            on_count={{
                let n5 = Rc::clone(&seen_num);
                Some(Rc::new(move |n| n5.set(n.saturating_sub(2))))
            }}
            on_action={|| {}}
        />
    };
    assert_eq!(*seen_str.borrow(), "HELLO");
    assert_eq!(seen_num.get(), 40);
}

#[derive(Clone, Debug, PartialEq)]
struct NonDefaultRow {
    id: u32,
    label: String,
}

#[component]
pub fn GenericList<T>(
    items: Vec<T>,
    title: Option<String>,
    #[prop(default = "empty")] empty_label: String,
    #[prop(default)] compact: bool,
    render_item: Option<Rc<dyn Fn(&T) -> View>>,
) -> View
where
    T: Clone + std::fmt::Debug + 'static,
{
    let heading = title.unwrap_or_else(|| "List".to_string());
    let rendered: Vec<NodeBlueprint> = if items.is_empty() {
        vec![NodeBlueprint::text(empty_label)]
    } else if let Some(ref render) = render_item {
        items.iter().map(|it| render(it).root).collect()
    } else {
        items
            .iter()
            .map(|it| NodeBlueprint::text(format!("{:?}", it)))
            .collect()
    };
    let cls = if compact { "list compact" } else { "list" };
    rsx! {
        <div class={cls} data-heading={heading}>
            {rendered}
        </div>
    }
}

#[component]
pub fn SpreadCard(
    title: Option<String>,
    subtitle: Option<String>,
    #[prop(default = 1)] level: u8,
) -> View {
    rsx! {
        <div
            data-title={title.unwrap_or_default()}
            data-subtitle={subtitle.unwrap_or_default()}
            data-level={level}
        />
    }
}

#[test]
fn test_generic_component_option_string_defaults_and_spread() {
    // 1. Generic component with non-Default T, Option<String> literal, #[prop(default)], and inferred closure
    let rows = vec![
        NonDefaultRow {
            id: 1,
            label: "Alpha".to_string(),
        },
        NonDefaultRow {
            id: 2,
            label: "Beta".to_string(),
        },
    ];
    let v = rsx! {
        <GenericList
            items={rows}
            title="Inventory"
            render_item={move |row| rsx! { <span>{format!("{}:{}", row.id, row.label)}</span> }}
        />
    };
    let debug = format!("{:?}", v.root);
    assert!(debug.contains("Inventory"));
    assert!(debug.contains("1:Alpha"));
    assert!(debug.contains("2:Beta"));
    assert!(!debug.contains("compact"));

    // 2. Explicit generic turbofish `<GenericList<String> ... />` with defaults triggered
    let v_empty = rsx! {
        <GenericList<String> items={Vec::new()} title={None} compact />
    };
    let debug_empty = format!("{:?}", v_empty.root);
    assert!(debug_empty.contains("empty"));
    assert!(debug_empty.contains("compact"));
    assert!(debug_empty.contains("List"));

    // 3. JSX `{..props}` spread on component (later props override earlier spread, and spread overrides earlier defaults)
    let base = SpreadCard::builder().title("Base Title").subtitle("Base Sub");
    let v_override = rsx! {
        <SpreadCard {..base} title="Override Title" />
    };
    let d1 = format!("{:?}", v_override.root);
    assert!(d1.contains("Override Title"), "d1={d1}");
    assert!(d1.contains("Base Sub"), "d1={d1}");

    let overrides = SpreadCard::builder().subtitle("Override Sub").level(3u8);
    let v_after = rsx! {
        <SpreadCard title="Kept Title" subtitle="Default Sub" {..overrides} />
    };
    let d2 = format!("{:?}", v_after.root);
    assert!(d2.contains("Kept Title"), "d2={d2}");
    assert!(d2.contains("Override Sub"), "d2={d2}");
    assert!(d2.contains("\"3\""), "d2={d2}");

    // 4. JSX `{..attrs}` spread on native HTML element
    let extra = vec![
        Attribute::property("data-a", "1"),
        Attribute::property("data-b", "2"),
    ];
    let v_el = rsx! {
        <div id="root" {..extra} data-c="3" />
    };
    if let NodeBlueprint::Element { attributes, .. } = v_el.root {
        let pairs: Vec<(&str, &str)> = attributes
            .iter()
            .filter_map(|a| match a {
                Attribute::Property { key, value } => Some((key.as_str(), value.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            pairs,
            vec![("id", "root"), ("data-a", "1"), ("data-b", "2"), ("data-c", "3")]
        );
    } else {
        panic!("Expected Element");
    }
}

#[test]
fn test_component_scoped_hooks_context_and_error_boundary() {
    use std::cell::{Cell, RefCell};
    use strata_dom::{
        create_context, provide_context, reset_component_hooks, use_callback, use_context,
        use_effect, use_effect_with_deps, use_id, use_memo, use_reducer, use_ref, use_state,
        Context, ErrorBoundary, ReadSignal, WriteSignal,
    };

    reset_component_hooks();

    // Shared test handles
    let mount_count = Rc::new(Cell::new(0usize));
    let cleanup_count = Rc::new(Cell::new(0usize));
    let effect_runs = Rc::new(Cell::new(0usize));
    let deps_runs = Rc::new(Cell::new(0usize));
    let deps_cleanups = Rc::new(Cell::new(0usize));
    let captured_setters: Rc<RefCell<Vec<WriteSignal<i32>>>> = Rc::new(RefCell::new(Vec::new()));
    let captured_ids: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let captured_cb_ptrs: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));

    let theme_ctx: Context<String> = create_context("light".to_string());

    #[component]
    fn StatefulLeaf(
        dep_ver: i32,
        theme_ctx: Context<String>,
        mount_count: Rc<Cell<usize>>,
        cleanup_count: Rc<Cell<usize>>,
        effect_runs: Rc<Cell<usize>>,
        deps_runs: Rc<Cell<usize>>,
        deps_cleanups: Rc<Cell<usize>>,
        captured_setters: Rc<RefCell<Vec<WriteSignal<i32>>>>,
        captured_ids: Rc<RefCell<Vec<String>>>,
        captured_cb_ptrs: Rc<RefCell<Vec<usize>>>,
    ) -> View {
        let (count, set_count): (ReadSignal<i32>, WriteSignal<i32>) = use_state(10);
        let doubled = use_memo({
            let c = count.clone();
            move || c.get() * 2
        });
        let render_ref = use_ref(0usize);
        render_ref.with_mut(|n| *n += 1);
        let stable_id = use_id();
        let cb = use_callback(dep_ver, move |x: i32| x + dep_ver);
        let theme = use_context(&theme_ctx);

        let (reduced, dispatch) = use_reducer(|state: &i32, delta: i32| *state + delta, 100);
        if render_ref.get() == 1 {
            mount_count.set(mount_count.get() + 1);
            dispatch(5);
        }

        let eff_runs = effect_runs.clone();
        let eff_clean = cleanup_count.clone();
        let c_for_eff = count.clone();
        use_effect(move || {
            let _ = c_for_eff.get();
            eff_runs.set(eff_runs.get() + 1);
            let cl = eff_clean.clone();
            Some(Box::new(move || cl.set(cl.get() + 1)) as Box<dyn FnOnce()>)
        });

        let dr = deps_runs.clone();
        let dc = deps_cleanups.clone();
        use_effect_with_deps(dep_ver, move |_| {
            dr.set(dr.get() + 1);
            let cl = dc.clone();
            Some(Box::new(move || cl.set(cl.get() + 1)) as Box<dyn FnOnce()>)
        });

        captured_setters.borrow_mut().push(set_count);
        captured_ids.borrow_mut().push(stable_id.clone());
        captured_cb_ptrs
            .borrow_mut()
            .push(Rc::as_ptr(&cb) as *const () as usize);

        rsx! {
            <div
                id={stable_id}
                data-count={count.get()}
                data-doubled={doubled.get()}
                data-renders={render_ref.get()}
                data-reduced={reduced.get()}
                data-theme={theme}
            />
        }
    }

    #[component]
    fn HostApp(
        show_second: bool,
        dep_ver: i32,
        theme_ctx: Context<String>,
        mount_count: Rc<Cell<usize>>,
        cleanup_count: Rc<Cell<usize>>,
        effect_runs: Rc<Cell<usize>>,
        deps_runs: Rc<Cell<usize>>,
        deps_cleanups: Rc<Cell<usize>>,
        captured_setters: Rc<RefCell<Vec<WriteSignal<i32>>>>,
        captured_ids: Rc<RefCell<Vec<String>>>,
        captured_cb_ptrs: Rc<RefCell<Vec<usize>>>,
    ) -> View {
        provide_context(&theme_ctx, "dark".to_string());
        let second = if show_second {
            rsx! {
                <StatefulLeaf
                    key="second"
                    dep_ver={dep_ver}
                    theme_ctx={theme_ctx.clone()}
                    mount_count={mount_count.clone()}
                    cleanup_count={cleanup_count.clone()}
                    effect_runs={effect_runs.clone()}
                    deps_runs={deps_runs.clone()}
                    deps_cleanups={deps_cleanups.clone()}
                    captured_setters={captured_setters.clone()}
                    captured_ids={captured_ids.clone()}
                    captured_cb_ptrs={captured_cb_ptrs.clone()}
                />
            }
        } else {
            rsx! { <></> }
        };
        rsx! {
            <div>
                <StatefulLeaf
                    key="first"
                    dep_ver={dep_ver}
                    theme_ctx={theme_ctx}
                    mount_count={mount_count}
                    cleanup_count={cleanup_count}
                    effect_runs={effect_runs}
                    deps_runs={deps_runs}
                    deps_cleanups={deps_cleanups}
                    captured_setters={captured_setters}
                    captured_ids={captured_ids}
                    captured_cb_ptrs={captured_cb_ptrs}
                />
                {second}
            </div>
        }
    }

    // Render 1: both siblings mount with independent state, stable IDs, and inherited "dark" context
    let v1 = HostApp(
        true,
        1,
        theme_ctx.clone(),
        mount_count.clone(),
        cleanup_count.clone(),
        effect_runs.clone(),
        deps_runs.clone(),
        deps_cleanups.clone(),
        captured_setters.clone(),
        captured_ids.clone(),
        captured_cb_ptrs.clone(),
    );
    let d1 = format!("{:?}", v1.root);
    assert!(d1.contains("dark"));
    assert!(d1.contains("\"105\"")); // reducer initial 100 + 5
    assert_eq!(mount_count.get(), 2);
    assert_eq!(effect_runs.get(), 2);
    assert_eq!(deps_runs.get(), 2);
    assert_ne!(captured_ids.borrow()[0], captured_ids.borrow()[1]);

    // Mutate only the first sibling's state via WriteSignal
    captured_setters.borrow()[0].set(42);
    assert_eq!(effect_runs.get(), 3, "tracked effect must re-run on signal write");
    assert_eq!(cleanup_count.get(), 1, "prior effect cleanup must run before re-execution");

    // Render 2 (same dep_ver=1): state persists (42 vs 10), memo updates (84 vs 20),
    // use_id & use_callback pointer identity stay identical, no duplicate effects!
    let first_id_r1 = captured_ids.borrow()[0].clone();
    let first_cb_r1 = captured_cb_ptrs.borrow()[0];
    captured_setters.borrow_mut().clear();
    captured_ids.borrow_mut().clear();
    captured_cb_ptrs.borrow_mut().clear();

    let v2 = HostApp(
        true,
        1,
        theme_ctx.clone(),
        mount_count.clone(),
        cleanup_count.clone(),
        effect_runs.clone(),
        deps_runs.clone(),
        deps_cleanups.clone(),
        captured_setters.clone(),
        captured_ids.clone(),
        captured_cb_ptrs.clone(),
    );
    let d2 = format!("{:?}", v2.root);
    assert!(d2.contains("\"42\"") && d2.contains("\"84\""), "first persisted: {d2}");
    assert!(d2.contains("\"10\"") && d2.contains("\"20\""), "second independent: {d2}");
    assert_eq!(mount_count.get(), 2, "must not remount");
    assert_eq!(effect_runs.get(), 3, "must not register duplicate effects on re-render");
    assert_eq!(deps_runs.get(), 2, "unchanged deps must not re-run use_effect_with_deps");
    assert_eq!(captured_ids.borrow()[0], first_id_r1);
    assert_eq!(captured_cb_ptrs.borrow()[0], first_cb_r1);

    // Render 3: change dep_ver=2 AND unmount second sibling ->
    // second's effect + deps cleanups run, first's use_effect_with_deps cleans up and re-runs,
    // first's use_callback allocates a new Rc.
    captured_cb_ptrs.borrow_mut().clear();
    let _v3 = HostApp(
        false,
        2,
        theme_ctx.clone(),
        mount_count.clone(),
        cleanup_count.clone(),
        effect_runs.clone(),
        deps_runs.clone(),
        deps_cleanups.clone(),
        captured_setters.clone(),
        captured_ids.clone(),
        captured_cb_ptrs.clone(),
    );
    assert_eq!(cleanup_count.get(), 2, "unmounted sibling effect cleanup must run");
    assert_eq!(deps_runs.get(), 3, "first sibling deps effect re-runs on dep change");
    assert_eq!(deps_cleanups.get(), 2, "both first (dep change) and second (unmount) deps cleanups run");
    assert_ne!(captured_cb_ptrs.borrow()[0], first_cb_r1);

    // Context default value outside provider
    assert_eq!(use_context(&theme_ctx), "light");

    // ErrorBoundary: catches render panic, latches fallback, and recovers on reset()
    let should_panic = Rc::new(Cell::new(true));
    let reset_handle: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

    let render_boundary = |sp: &Rc<Cell<bool>>, rh: &Rc<RefCell<Option<Rc<dyn Fn()>>>>| {
        let sp = sp.clone();
        let rh = rh.clone();
        rsx! {
            <ErrorBoundary
                key="eb-test"
                render={move || {
                    if sp.get() {
                        panic!("boom during render");
                    }
                    rsx! { <span>"Recovered OK"</span> }
                }}
                fallback={move |err| {
                    *rh.borrow_mut() = Some(err.reset.clone());
                    rsx! { <div class="fallback">{format!("Caught: {}", err.message)}</div> }
                }}
            />
        }
    };

    let eb1 = render_boundary(&should_panic, &reset_handle);
    assert!(format!("{:?}", eb1.root).contains("Caught: boom during render"));

    // Flip flag and call reset() -> ErrorBoundary retries and renders recovered UI
    should_panic.set(false);
    let reset_fn = reset_handle.borrow().clone().expect("reset callback");
    reset_fn();
    let eb2 = render_boundary(&should_panic, &reset_handle);
    assert!(format!("{:?}", eb2.root).contains("Recovered OK"));

    reset_component_hooks();
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
struct CustomPanelOptions {
    retries: u32,
}

#[component]
fn NonOptionDefaultsPanel(
    title: String,
    subtitle: &'static str,
    collapsed: bool,
    columns: usize,
    offset: i32,
    opacity: f32,
    items: Vec<String>,
    class: strata_dom::ClassList,
    #[prop(default = "p-4")] default_class: strata_dom::ClassList,
    #[prop(default = "N/A")] fallback_note: String,
    #[prop(default = 8)] gap: usize,
    #[prop(default)] bordered: bool,
    #[prop(default)] opts: CustomPanelOptions,
    children: Vec<NodeBlueprint>,
) -> View {
    rsx! {
        <div
            data-cls-len={class.len().to_string()}
            data-has-p2={class.contains("p-2").to_string()}
            data-default-p4={default_class.contains("p-4").to_string()}
            data-title={title}
            data-subtitle={subtitle}
            data-collapsed={collapsed.to_string()}
            data-columns={columns.to_string()}
            data-offset={offset.to_string()}
            data-opacity={opacity.to_string()}
            data-items-len={items.len().to_string()}
            data-note={fallback_note}
            data-gap={gap.to_string()}
            data-bordered={bordered.to_string()}
            data-retries={opts.retries.to_string()}
            data-children-len={children.len().to_string()}
        >
            {children}
        </div>
    }
}

#[test]
fn test_component_non_option_default_empty_values_and_prop_default_attrs() {
    // Omitting all String, &str, bool, numeric, Vec, ClassList, and #[prop(default*)] props must
    // NOT panic at runtime and must populate their empty/configured defaults cleanly.
    let v_empty = rsx! { <NonOptionDefaultsPanel /> };
    if let NodeBlueprint::Element { attributes, .. } = v_empty.root {
        let get_attr = |k: &str| -> Option<String> {
            attributes.iter().find_map(|a| match a {
                Attribute::Property { key, value } if key == k => Some(value.clone()),
                _ => None,
            })
        };
        assert_eq!(get_attr("data-cls-len").as_deref(), Some("0"));
        assert_eq!(get_attr("data-has-p2").as_deref(), Some("false"));
        assert_eq!(get_attr("data-default-p4").as_deref(), Some("true"));
        assert_eq!(get_attr("data-title").as_deref(), Some(""));
        assert_eq!(get_attr("data-subtitle").as_deref(), Some(""));
        assert_eq!(get_attr("data-collapsed").as_deref(), Some("false"));
        assert_eq!(get_attr("data-columns").as_deref(), Some("0"));
        assert_eq!(get_attr("data-offset").as_deref(), Some("0"));
        assert_eq!(get_attr("data-opacity").as_deref(), Some("0"));
        assert_eq!(get_attr("data-items-len").as_deref(), Some("0"));
        assert_eq!(get_attr("data-note").as_deref(), Some("N/A"));
        assert_eq!(get_attr("data-gap").as_deref(), Some("8"));
        assert_eq!(get_attr("data-bordered").as_deref(), Some("false"));
        assert_eq!(get_attr("data-retries").as_deref(), Some("0"));
        assert_eq!(get_attr("data-children-len").as_deref(), Some("0"));
    } else {
        panic!("Expected Element");
    }

    // Explicit overrides still apply cleanly.
    let v_set = rsx! {
        <NonOptionDefaultsPanel
            title="Thermal"
            collapsed
            columns={3}
            offset={-4}
            opacity={0.75}
            items={vec!["a".to_string(), "b".to_string()]}
            class="p-2"
            bordered
            opts={CustomPanelOptions { retries: 5 }}
        >
            <span>{"Child"}</span>
        </NonOptionDefaultsPanel>
    };
    if let NodeBlueprint::Element { attributes, .. } = v_set.root {
        let get_attr = |k: &str| -> Option<String> {
            attributes.iter().find_map(|a| match a {
                Attribute::Property { key, value } if key == k => Some(value.clone()),
                _ => None,
            })
        };
        assert_eq!(get_attr("data-cls-len").as_deref(), Some("1"));
        assert_eq!(get_attr("data-has-p2").as_deref(), Some("true"));
        assert_eq!(get_attr("data-default-p4").as_deref(), Some("true"));
        assert_eq!(get_attr("data-title").as_deref(), Some("Thermal"));
        assert_eq!(get_attr("data-collapsed").as_deref(), Some("true"));
        assert_eq!(get_attr("data-columns").as_deref(), Some("3"));
        assert_eq!(get_attr("data-offset").as_deref(), Some("-4"));
        assert_eq!(get_attr("data-opacity").as_deref(), Some("0.75"));
        assert_eq!(get_attr("data-items-len").as_deref(), Some("2"));
        assert_eq!(get_attr("data-note").as_deref(), Some("N/A"));
        assert_eq!(get_attr("data-gap").as_deref(), Some("8"));
        assert_eq!(get_attr("data-bordered").as_deref(), Some("true"));
        assert_eq!(get_attr("data-retries").as_deref(), Some("5"));
        assert_eq!(get_attr("data-children-len").as_deref(), Some("1"));
    } else {
        panic!("Expected Element");
    }
}

#[component]
fn TypedEventButton(
    label: String,
    onclick: Option<Rc<dyn Fn(strata_dom::Event)>>,
) -> View {
    if let Some(cb) = onclick {
        rsx! { <button onclick={cb}>{label}</button> }
    } else {
        rsx! { <button>{label}</button> }
    }
}

#[component]
fn IndexedActionButton(index: usize, onclick: Rc<dyn Fn(usize)>) -> View {
    rsx! {
        <button data-idx={index.to_string()} onclick={move || onclick(index)}>
            {"Act"}
        </button>
    }
}

#[component]
fn UnitActionButtonOptional(onclick: Option<Rc<dyn Fn()>>) -> View {
    if let Some(cb) = onclick {
        rsx! { <button onclick={cb}>{"OptUnit"}</button> }
    } else {
        rsx! { <button>{"OptUnitNone"}</button> }
    }
}

#[component]
fn UnitActionButtonRequired(onclick: Rc<dyn Fn()>) -> View {
    rsx! { <button onclick={onclick}>{"ReqUnit"}</button> }
}

#[test]
fn test_component_typed_and_unit_onclick_callbacks() {
    let clicked_x = Rc::new(Cell::new(0.0_f64));
    let clicked_x_cb = Rc::clone(&clicked_x);
    let view_typed = rsx! {
        <TypedEventButton
            label="Fire"
            onclick={move |ev: strata_dom::Event| clicked_x_cb.set(ev.client_x)}
        />
    };
    if let NodeBlueprint::Element { attributes, .. } = view_typed.root {
        let handler = attributes
            .iter()
            .find_map(|a| match a {
                Attribute::EventListener {
                    event: EventType::Click,
                    handler,
                } => Some(Rc::clone(handler)),
                _ => None,
            })
            .expect("Expected EventType::Click handler");
        let mut ev = strata_dom::Event::default();
        ev.client_x = 128.5;
        handler(ev);
        assert!((clicked_x.get() - 128.5).abs() < 1e-6);
    } else {
        panic!("Expected Element");
    }

    // Omitted Option<Rc<dyn Fn(Event)>> compiles and defaults to None
    let view_omitted = rsx! { <TypedEventButton label="Idle" /> };
    assert!(matches!(view_omitted.root, NodeBlueprint::Element { .. }));

    // Typed non-Option Rc<dyn Fn(usize)> onclick
    let seen_idx = Rc::new(Cell::new(0usize));
    let seen_idx_cb = Rc::clone(&seen_idx);
    let view_indexed = rsx! {
        <IndexedActionButton
            index={42}
            onclick={move |idx: usize| seen_idx_cb.set(idx)}
        />
    };
    if let NodeBlueprint::Element { attributes, .. } = view_indexed.root {
        let handler = attributes
            .iter()
            .find_map(|a| match a {
                Attribute::EventListener {
                    event: EventType::Click,
                    handler,
                } => Some(Rc::clone(handler)),
                _ => None,
            })
            .expect("Expected EventType::Click handler");
        handler(strata_dom::Event::default());
        assert_eq!(seen_idx.get(), 42);
    } else {
        panic!("Expected Element");
    }

    // Both optional and required unit onclick callbacks continue to work with || closures
    let opt_count = Rc::new(Cell::new(0usize));
    let opt_cb = Rc::clone(&opt_count);
    let req_count = Rc::new(Cell::new(0usize));
    let req_cb = Rc::clone(&req_count);
    let v_units = rsx! {
        <div>
            <UnitActionButtonOptional onclick={move || opt_cb.set(opt_cb.get() + 1)} />
            <UnitActionButtonRequired onclick={move || req_cb.set(req_cb.get() + 1)} />
        </div>
    };
    if let NodeBlueprint::Element { children, .. } = v_units.root {
        for child in children {
            if let NodeBlueprint::Element { attributes, .. } = child {
                for a in attributes {
                    if let Attribute::EventListener {
                        event: EventType::Click,
                        handler,
                    } = a
                    {
                        handler(strata_dom::Event::default());
                    }
                }
            }
        }
        assert_eq!(opt_count.get(), 1);
        assert_eq!(req_count.get(), 1);
    } else {
        panic!("Expected Element");
    }
}

#[component]
fn ViewSlotCard(title: String, children: View) -> View {
    rsx! {
        <div class="card">
            <h3>{title}</h3>
            <div class="body">{children}</div>
        </div>
    }
}

#[component]
fn OptViewSlotCard(children: Option<View>) -> View {
    rsx! {
        <div class="opt-card">
            {children.unwrap_or_else(|| rsx! { <span>{"Fallback"}</span> })}
        </div>
    }
}

#[component]
fn SingleNodeSlotCard(children: NodeBlueprint) -> View {
    rsx! {
        <div class="node-card">{children}</div>
    }
}

#[test]
fn test_component_children_view_and_node_blueprint_support() {
    // Multi-child passed to children: View -> wraps into Fragment in .body
    let v_multi = rsx! {
        <ViewSlotCard title="Multi">
            <span>{"One"}</span>
            <span>{"Two"}</span>
        </ViewSlotCard>
    };
    if let NodeBlueprint::Element { children, .. } = v_multi.root {
        assert_eq!(children.len(), 2);
        if let NodeBlueprint::Element { children: body_children, .. } = &children[1] {
            assert_eq!(body_children.len(), 1);
            if let NodeBlueprint::Fragment(inner) = &body_children[0] {
                assert_eq!(inner.len(), 2);
            } else {
                panic!("Expected Fragment for 2 children in View, got {:?}", body_children[0]);
            }
        } else {
            panic!("Expected .body Element");
        }
    } else {
        panic!("Expected Element");
    }

    // Omitted children on children: View defaults cleanly to NodeBlueprint::Empty
    let v_empty = rsx! { <ViewSlotCard title="Empty" /> };
    if let NodeBlueprint::Element { children, .. } = v_empty.root {
        if let NodeBlueprint::Element { children: body_children, .. } = &children[1] {
            assert!(matches!(body_children.as_slice(), [NodeBlueprint::Empty]));
        } else {
            panic!("Expected .body Element");
        }
    } else {
        panic!("Expected Element");
    }

    // children: Option<View> omitted vs provided
    let v_opt_none = rsx! { <OptViewSlotCard /> };
    if let NodeBlueprint::Element { children, .. } = v_opt_none.root {
        assert!(matches!(
            &children[0],
            NodeBlueprint::Element { tag: Tag::Span, .. }
        ));
    } else {
        panic!("Expected Element");
    }

    let v_opt_some = rsx! {
        <OptViewSlotCard>
            <p>{"Provided"}</p>
        </OptViewSlotCard>
    };
    if let NodeBlueprint::Element { children, .. } = v_opt_some.root {
        assert!(matches!(
            &children[0],
            NodeBlueprint::Element { tag: Tag::P, .. }
        ));
    } else {
        panic!("Expected Element");
    }

    // children: NodeBlueprint single child & omitted
    let v_node_single = rsx! {
        <SingleNodeSlotCard>
            <span>{"Leaf"}</span>
        </SingleNodeSlotCard>
    };
    if let NodeBlueprint::Element { children, .. } = v_node_single.root {
        assert!(matches!(
            &children[0],
            NodeBlueprint::Element { tag: Tag::Span, .. }
        ));
    } else {
        panic!("Expected Element");
    }

    let v_node_empty = rsx! { <SingleNodeSlotCard /> };
    if let NodeBlueprint::Element { children, .. } = v_node_empty.root {
        assert!(matches!(&children[0], NodeBlueprint::Empty));
    } else {
        panic!("Expected Element");
    }
}
