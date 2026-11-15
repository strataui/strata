//! Integration test suite verifying live client-side DOM mutation and dynamic hydration in AppRuntime.

use strata::runtime::{dom_node_to_blueprint, AppRuntime};
use strata_dom::View;
use strata_js::{Document, JsEngine};

#[test]
fn test_client_side_append_child_hydration() {
    let html = r#"<div id="container" class="flex flex-col w-[600px] p-4 bg-gray-100"><p id="first-p" class="text-sm text-gray-700">Initial paragraph in container</p></div>"#;
    let doc = Document::from_html(html);
    let mut engine = JsEngine::with_document(doc.clone());

    let root_bp = dom_node_to_blueprint(&doc.body);
    let mut runtime = AppRuntime::new(View::new(root_bp), 1024.0, 768.0, 1.0);

    let container_path = runtime
        .root_blueprint
        .find_path_by_dom_uid(doc.get_element_by_id("container").unwrap().uid())
        .expect("Container must have path in blueprint");

    let initial_child_count = {
        let node =
            AppRuntime::find_layout_node_mut(&mut runtime.layout_root, &container_path).unwrap();
        node.children.len()
    };
    assert_eq!(initial_child_count, 1);

    // Client-side script execution appending a new button
    let script = r#"
        const btn = document.createElement("button");
        btn.setAttribute("id", "dynamic-btn");
        btn.setAttribute("class", "px-4 py-2 bg-blue-600 text-white font-semibold");
        btn.textContent = "Enterprise Action";
        document.getElementById("container").appendChild(btn);
    "#;
    let res = engine.eval(script);
    assert!(res.is_ok(), "Script execution failed: {:?}", res);

    let mutations = engine.document.drain_mutations();
    assert_eq!(
        mutations.len(),
        4,
        "Must emit 4 mutations (2 setAttribute, 1 textContent, 1 appendChild)"
    );

    let hydrated = runtime.hydrate_dom_mutations(&mutations, Some(&engine.document));
    assert!(hydrated, "Hydration must report changes");

    // Verify layout tree now has the new button child
    let container_layout =
        AppRuntime::find_layout_node_mut(&mut runtime.layout_root, &container_path).unwrap();
    assert_eq!(
        container_layout.children.len(),
        2,
        "Container must have 2 children after appendChild"
    );

    let btn_layout = &container_layout.children[1];
    assert!(
        btn_layout.geometry.border_box.width > 0.0,
        "New button must have positive layout width"
    );
    assert!(
        btn_layout.geometry.border_box.height > 0.0,
        "New button must have positive layout height"
    );
    assert_eq!(
        btn_layout.children[0].text.as_deref(),
        Some("Enterprise Action")
    );
}

#[test]
fn test_client_side_text_content_mutation() {
    let html = r#"<div class="flex flex-col w-[800px] p-4"><p id="status-label" class="text-sm font-normal">Idle</p></div>"#;
    let doc = Document::from_html(html);
    let mut engine = JsEngine::with_document(doc.clone());

    let root_bp = dom_node_to_blueprint(&doc.body);
    let mut runtime = AppRuntime::new(View::new(root_bp), 1024.0, 768.0, 1.0);

    let label_uid = doc.get_element_by_id("status-label").unwrap().uid();
    let label_path = runtime
        .root_blueprint
        .find_path_by_dom_uid(label_uid)
        .expect("Label path must exist");

    let initial_width = {
        let node = AppRuntime::find_layout_node_mut(&mut runtime.layout_root, &label_path).unwrap();
        node.geometry.content_box.width
    };

    // Client-side JS mutates textContent to a much longer text
    let script = r#"
        document.getElementById("status-label").textContent = "Processing batch job with 50,000 items in cluster partition 4";
    "#;
    let res = engine.eval(script);
    assert!(res.is_ok());

    let mutations = engine.document.drain_mutations();
    assert_eq!(
        mutations.len(),
        1,
        "Must emit 1 mutation for textContent update"
    );

    let hydrated = runtime.hydrate_dom_mutations(&mutations, Some(&engine.document));
    assert!(hydrated);

    let updated_node =
        AppRuntime::find_layout_node_mut(&mut runtime.layout_root, &label_path).unwrap();
    assert_eq!(
        updated_node.text.as_deref(),
        Some("Processing batch job with 50,000 items in cluster partition 4")
    );
    assert!(
        updated_node.geometry.content_box.width > initial_width,
        "Text bounds must re-measure and expand with longer text"
    );
}

#[test]
fn test_client_side_inner_html_mutation() {
    let html = r#"<div id="wrapper" class="flex flex-col w-[700px] p-4"><div id="dynamic-content"><span>Stale placeholder</span></div></div>"#;
    let doc = Document::from_html(html);
    let mut engine = JsEngine::with_document(doc.clone());

    let root_bp = dom_node_to_blueprint(&doc.body);
    let mut runtime = AppRuntime::new(View::new(root_bp), 1024.0, 768.0, 1.0);

    let content_uid = doc.get_element_by_id("dynamic-content").unwrap().uid();
    let content_path = runtime
        .root_blueprint
        .find_path_by_dom_uid(content_uid)
        .expect("Content path must exist");

    // Client-side JS replaces innerHTML
    let script = r#"
        document.getElementById("dynamic-content").innerHTML = "<h2 class=\"text-xl font-bold\">New Operational Dashboard</h2><p class=\"text-xs text-zinc-500\">Live telemetry stream active</p>";
    "#;
    let res = engine.eval(script);
    assert!(res.is_ok());

    let mutations = engine.document.drain_mutations();
    assert_eq!(mutations.len(), 1, "Must emit 1 InnerHtmlChanged mutation");

    let hydrated = runtime.hydrate_dom_mutations(&mutations, Some(&engine.document));
    assert!(hydrated);

    let content_layout =
        AppRuntime::find_layout_node_mut(&mut runtime.layout_root, &content_path).unwrap();
    assert_eq!(
        content_layout.children.len(),
        2,
        "Inner HTML replacement must yield 2 child nodes"
    );

    let h2_child = &content_layout.children[0];
    let p_child = &content_layout.children[1];
    assert!(h2_child.geometry.border_box.width > 0.0);
    assert!(p_child.geometry.border_box.width > 0.0);
}

#[test]
fn test_client_side_attribute_and_class_mutation() {
    let html = r#"<div class="p-6"><span id="badge" class="bg-gray-200 text-black px-2 py-1">Standard</span></div>"#;
    let doc = Document::from_html(html);
    let mut engine = JsEngine::with_document(doc.clone());

    let root_bp = dom_node_to_blueprint(&doc.body);
    let mut runtime = AppRuntime::new(View::new(root_bp), 1024.0, 768.0, 1.0);

    let badge_uid = doc.get_element_by_id("badge").unwrap().uid();
    let badge_path = runtime
        .root_blueprint
        .find_path_by_dom_uid(badge_uid)
        .expect("Badge path must exist");

    // Client-side JS changes class attribute
    let script = r#"
        const badge = document.getElementById("badge");
        badge.setAttribute("class", "bg-emerald-500 text-white font-bold px-4 py-2");
    "#;
    let res = engine.eval(script);
    assert!(res.is_ok());

    let mutations = engine.document.drain_mutations();
    let hydrated = runtime.hydrate_dom_mutations(&mutations, Some(&engine.document));
    assert!(hydrated);

    let bp = runtime.root_blueprint.find_by_dom_uid(badge_uid).unwrap();
    if let strata_dom::NodeBlueprint::Element { classes, .. } = bp {
        assert!(classes.iter().any(|c| c == "bg-emerald-500"));
        assert!(classes.iter().any(|c| c == "text-white"));
    } else {
        panic!("Badge must be an element blueprint");
    }

    let layout_node =
        AppRuntime::find_layout_node_mut(&mut runtime.layout_root, &badge_path).unwrap();
    assert_eq!(
        layout_node.style.font_weight,
        strata_style::types::FontWeight::Bold
    );
}

#[test]
fn test_client_side_remove_child() {
    let html = r#"<div id="card-list" class="flex flex-col w-[500px]"><div id="card-1" class="h-10 bg-red-100">Item 1</div><div id="card-2" class="h-10 bg-green-100">Item 2</div><div id="card-3" class="h-10 bg-blue-100">Item 3</div></div>"#;
    let doc = Document::from_html(html);
    let mut engine = JsEngine::with_document(doc.clone());

    let root_bp = dom_node_to_blueprint(&doc.body);
    let mut runtime = AppRuntime::new(View::new(root_bp), 1024.0, 768.0, 1.0);

    let list_uid = doc.get_element_by_id("card-list").unwrap().uid();
    let list_path = runtime
        .root_blueprint
        .find_path_by_dom_uid(list_uid)
        .unwrap();

    let initial_count = {
        let node = AppRuntime::find_layout_node_mut(&mut runtime.layout_root, &list_path).unwrap();
        node.children.len()
    };
    assert_eq!(initial_count, 3);

    // Client-side JS removes card-2
    let script = r#"
        document.getElementById("card-2").remove();
    "#;
    let res = engine.eval(script);
    assert!(res.is_ok());

    let mutations = engine.document.drain_mutations();
    assert_eq!(mutations.len(), 1, "Must emit single ChildRemoved mutation");
    let hydrated = runtime.hydrate_dom_mutations(&mutations, Some(&engine.document));
    assert!(hydrated);

    let node = AppRuntime::find_layout_node_mut(&mut runtime.layout_root, &list_path).unwrap();
    assert_eq!(node.children.len(), 2, "List must now contain only 2 items");

    // Verify card-2 is no longer in JS document
    let card2_node = engine.document.get_element_by_id("card-2");
    assert!(card2_node.is_none());
}

#[test]
fn test_channel_based_dom_mutation_pumping() {
    let html = r#"<div id="notification-area" class="flex flex-col w-[400px]"><span class="text-xs text-gray-500">System Ready</span></div>"#;
    let doc = Document::from_html(html);
    let mut engine = JsEngine::with_document(doc.clone());

    let root_bp = dom_node_to_blueprint(&doc.body);
    let mut runtime = AppRuntime::new(View::new(root_bp), 1024.0, 768.0, 1.0);

    // Register channel
    let sender = runtime.register_dom_mutation_channel();
    engine.document.set_mutation_channel(sender);

    // Script mutates DOM in background
    let script = r#"
        const note = document.createElement("div");
        note.setAttribute("id", "alert-item");
        note.setAttribute("class", "p-2 bg-amber-100 text-amber-900");
        note.textContent = "Warning: High memory utilization";
        document.getElementById("notification-area").appendChild(note);
    "#;
    let res = engine.eval(script);
    assert!(res.is_ok());

    // Poll mutations via runtime channel
    let redraw_needed = runtime.poll_dom_mutations(Some(&engine.document));
    assert!(
        redraw_needed,
        "Polling channel must detect incoming mutations"
    );

    let area_path = runtime
        .root_blueprint
        .find_path_by_dom_uid(doc.get_element_by_id("notification-area").unwrap().uid())
        .unwrap();

    let area_layout =
        AppRuntime::find_layout_node_mut(&mut runtime.layout_root, &area_path).unwrap();
    assert_eq!(area_layout.children.len(), 2);
    assert_eq!(
        area_layout.children[1].children[0].text.as_deref(),
        Some("Warning: High memory utilization")
    );
}
