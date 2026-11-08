#![allow(clippy::field_reassign_with_default)]

use strata_codegen_js::prelude::*;
use strata_dom::{Attribute, EventType, NodeBlueprint, Tag};

#[test]
fn test_reactive_runtime_emission() {
    let esm_runtime = JsRuntimeEmitter::emit_full_runtime(true);
    assert!(esm_runtime.contains("export function createSignal("));
    assert!(esm_runtime.contains("export function createEffect("));
    assert!(esm_runtime.contains("export function createMemo("));
    assert!(esm_runtime.contains("export function batch("));
    assert!(esm_runtime.contains("export function el("));
    assert!(esm_runtime.contains("export function txt("));
    assert!(esm_runtime.contains("export function bindText("));

    let iife_runtime = JsRuntimeEmitter::emit_full_runtime(false);
    assert!(iife_runtime.contains("global.Strata = {"));
    assert!(iife_runtime.contains("createSignal, createEffect"));
}

#[test]
fn test_transpiler_nested_blueprints() {
    let blueprint = NodeBlueprint::Element {
        tag: Tag::Div,
        classes: vec!["flex".to_string(), "items-center".to_string()].into(),
        attributes: vec![
            Attribute::property("id", "container"),
            Attribute::event_listener(EventType::Click, || {}),
        ],
        children: vec![
            NodeBlueprint::Element {
                tag: Tag::H1,
                classes: vec!["font-bold".to_string()].into(),
                attributes: vec![],
                children: vec![NodeBlueprint::text("Welcome to Strata")],
            },
            NodeBlueprint::text("Subtitle text"),
        ],
    };

    let component_js = JsTranspiler::emit_esm_component("Dashboard", &blueprint, "./runtime.js");
    assert!(component_js.contains("import { el, txt, frag, on, bindText, bindAttr, bindClass, mount } from './runtime.js';"));
    assert!(component_js.contains("export function Dashboard() {"));
    assert!(component_js.contains("el('div', 'flex items-center', {'id': 'container', 'onclick': (e) => { /* event handler */ }}, ["));
    assert!(component_js.contains("el('h1', 'font-bold', {}, ["));
    assert!(component_js.contains("txt('Welcome to Strata')"));
    assert!(component_js.contains("txt('Subtitle text')"));
}

#[test]
fn test_css_extractor_classes_and_rules() {
    let blueprint = NodeBlueprint::Element {
        tag: Tag::Section,
        classes: vec!["flex flex-col p-4 w-[400px]".to_string()].into(),
        attributes: vec![],
        children: vec![NodeBlueprint::Element {
            tag: Tag::Button,
            classes: vec!["hover:font-bold rounded-md".to_string()].into(),
            attributes: vec![],
            children: vec![],
        }],
    };

    let classes = CssExtractor::collect_classes(&blueprint);
    assert!(classes.contains("flex"));
    assert!(classes.contains("flex-col"));
    assert!(classes.contains("p-4"));
    assert!(classes.contains("w-[400px]"));
    assert!(classes.contains("hover:font-bold"));
    assert!(classes.contains("rounded-md"));

    let css = CssExtractor::generate_stylesheet(&classes);
    assert!(css.contains(".flex { display: flex; }"));
    assert!(css.contains(".flex-col { flex-direction: column; }"));
    assert!(css.contains(".p-4 { padding: 1rem; }"));
    assert!(css.contains(r#".w-\[400px\] { width: 400px; }"#));
    assert!(css.contains(r#".hover\:font-bold:hover { font-weight: 700; }"#));
    assert!(css.contains(".rounded-md { border-radius: 0.375rem; }"));
}

#[test]
fn test_wasm_canvas_harness_generation() {
    let html = WasmHarnessGenerator::generate_index_html("Strata Studio", "strata-canvas", "bundle.js");
    assert!(html.contains("<title>Strata Studio</title>"));
    assert!(html.contains("<canvas id=\"strata-canvas\""));
    assert!(html.contains("<script type=\"module\" src=\"bundle.js\"></script>"));

    let loader = WasmHarnessGenerator::generate_wasm_loader_js("strata-canvas", "app.wasm");
    assert!(loader.contains("document.getElementById('strata-canvas')"));
    assert!(loader.contains("window.devicePixelRatio"));
    assert!(loader.contains("WebAssembly.instantiateStreaming"));
    assert!(loader.contains("requestAnimationFrame(renderFrame)"));
}

#[test]
fn test_unified_web_target_builder_pure_js_mode() {
    let mut config = WebBuildConfig::default();
    config.app_title = "Apex Storefront".to_string();
    config.root_element_id = "apex-root".to_string();

    let blueprint = NodeBlueprint::Element {
        tag: Tag::Main,
        classes: vec!["w-full h-full flex items-center justify-center".to_string()].into(),
        attributes: vec![],
        children: vec![NodeBlueprint::text("Apex Athletics Storefront")],
    };

    let artifacts = WebTargetBuilder::build(&config, &blueprint);
    assert_eq!(artifacts.mode, WebTargetMode::PureJsDom);
    assert!(artifacts.html.contains("<title>Apex Storefront</title>"));
    assert!(artifacts.html.contains("<div id=\"apex-root\"></div>"));
    assert!(artifacts.html.contains("<link rel=\"stylesheet\" href=\"./styles.css\">"));
    assert!(artifacts.css.is_some());
    assert!(artifacts.runtime_js.is_some());
    assert!(artifacts.wasm_loader.is_none());
    assert!(artifacts.js.contains("export function App()"));

    // Test inlined runtime
    config.inline_runtime = true;
    let inlined = WebTargetBuilder::build(&config, &blueprint);
    assert!(inlined.runtime_js.is_none());
    assert!(inlined.html.contains("<script type=\"module\">"));
    assert!(inlined.html.contains("export function createSignal("));
}

#[test]
fn test_unified_web_target_builder_wasm_mode() {
    let mut config = WebBuildConfig::default();
    config.mode = WebTargetMode::WasmCanvas;
    config.app_title = "Strata 3D Viewport".to_string();

    let blueprint = NodeBlueprint::empty();
    let artifacts = WebTargetBuilder::build(&config, &blueprint);

    assert_eq!(artifacts.mode, WebTargetMode::WasmCanvas);
    assert!(artifacts.html.contains("<title>Strata 3D Viewport</title>"));
    assert!(artifacts.html.contains("<canvas id=\"strata-surface\""));
    assert!(artifacts.wasm_loader.is_some());
    assert!(artifacts.css.is_none());
}
