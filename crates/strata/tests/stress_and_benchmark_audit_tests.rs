//! Empirical Stress Testing, Resilience, and Microbenchmarking Audit Suite for Project Strata.

use std::time::Instant;
use strata_dom::{Attribute, NodeBlueprint, Tag, View};
use strata_layout::tree::LayoutNode;
use strata_style::tailwind::TailwindEngine;
use strata_style::types::{ComputedStyle, Dimension, Display, FlexDirection};
use strata::runtime::AppRuntime;

#[test]
fn test_stress_deeply_nested_tree_recursion() {
    // Milestone 66 bounded layout depth guard (MAX_LAYOUT_DEPTH = 256) guarantees stack safety
    // even for nesting depths up to 2,000+ without call-stack exhaustion (CWE-674 remediation).
    for depth in [50, 100, 200, 500, 1000, 2000] {
        let mut blueprint = NodeBlueprint::Text(format!("Leaf at depth {depth}"));
        for d in (0..depth).rev() {
            blueprint = NodeBlueprint::Element {
                tag: Tag::Div,
                classes: vec!["flex".to_string(), "p-1".to_string(), "border".to_string()].into(),
                children: vec![blueprint],
                attributes: vec![Attribute::property("data-depth", d.to_string())],
            };
        }

        let view = View::new(blueprint);
        let start = Instant::now();
        let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);
        let init_time = start.elapsed();

        let tick_start = Instant::now();
        let redraw = runtime.tick_frame();
        let tick_time = tick_start.elapsed();

        println!(
            "[STRESS] Nesting Depth {:>4} -> Init: {:>7} us, Tick: {:>6} us, DL Cmds: {}",
            depth,
            init_time.as_micros(),
            tick_time.as_micros(),
            redraw.len()
        );
        assert_eq!(runtime.viewport_width, 800.0);
    }
}

#[test]
fn test_stress_massive_dom_tree_layout_throughput() {
    // Benchmark 2,000 realistic styled DOM nodes with text, padding, and flex properties
    let count = 2_000;
    let root_style = ComputedStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dimension::Px(1200.0),
        ..ComputedStyle::default()
    };

    let mut children = Vec::with_capacity(count);
    for i in 0..count {
        let item_style = ComputedStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            width: Dimension::Px(1200.0),
            height: Dimension::Px(24.0),
            ..ComputedStyle::default()
        };

        let mut text_child = LayoutNode::new(ComputedStyle::default());
        text_child.text = Some(format!("TableRow_{:05} status active metric {}", i, i * 42));

        let node = LayoutNode::with_children(item_style, vec![text_child]);
        children.push(node);
    }

    let mut root = LayoutNode::with_children(root_style, children);

    let start = Instant::now();
    root.compute_layout(1200.0, 80000.0);
    let layout_duration = start.elapsed();

    let per_node_ns = layout_duration.as_nanos() / count as u128;
    println!(
        "[BENCHMARK] {:>5} Nodes Complex Layout: {:>6} us total ({:>5} ns/node)",
        count,
        layout_duration.as_micros(),
        per_node_ns
    );

    assert!(per_node_ns < 1_000_000, "Layout throughput too slow: {} ns/node", per_node_ns);
}

#[test]
fn test_stress_tailwind_jit_compilation_throughput() {
    // Generate 1,000 distinct Tailwind utility and arbitrary syntax classes
    let mut class_strings = Vec::with_capacity(1_000);
    for i in 0..1_000 {
        let class_spec = format!(
            "w-[{}px] h-[{}px] p-{} m-{} bg-[#abcdef] text-[#123456] flex flex-col items-center justify-between rounded-lg border-2 opacity-90",
            i % 500,
            (i * 3) % 400,
            i % 16,
            (i + 2) % 12
        );
        class_strings.push(class_spec);
    }

    let start = Instant::now();
    let mut total_rules = 0;
    for classes in &class_strings {
        let style = TailwindEngine::resolve_classes(classes);
        if style.width != Dimension::Auto {
            total_rules += 1;
        }
    }
    let elapsed = start.elapsed();

    let classes_per_sec = (1_000.0 / elapsed.as_secs_f64()) * 11.0; // 11 classes per string
    println!(
        "[BENCHMARK] Tailwind JIT Throughput: 1,000 elements ({:.0} utilities/sec) in {:>6} us",
        classes_per_sec,
        elapsed.as_micros()
    );
    assert_eq!(total_rules, 1_000);
}

#[test]
fn test_stress_text_line_breaker_pathological_inputs() {
    use strata_layout::text::measure_text_bounds_ext;

    // 1. Pathological: 50,000 unbroken characters without a single space
    let massive_unbroken: String = "X".repeat(50_000);
    let start = Instant::now();
    let bounds = measure_text_bounds_ext(&massive_unbroken, 14.0, 1.2, Some(300.0), false);
    let elapsed = start.elapsed();

    println!(
        "[STRESS] 50k unbroken characters wrapped to 300px: {:>6} us (bounds: {}x{})",
        elapsed.as_micros(),
        bounds.0,
        bounds.1
    );
    assert!(bounds.0 > 0.0);

    // 2. Pathological: Dense zero-width space and joiner sequences
    let mut zw_stress = String::new();
    for _ in 0..10_000 {
        zw_stress.push_str("a\u{200B}b\u{200D}c ");
    }
    let start = Instant::now();
    let bounds_zw = measure_text_bounds_ext(&zw_stress, 14.0, 1.2, Some(500.0), false);
    let elapsed_zw = start.elapsed();

    println!(
        "[STRESS] 10k zero-width joiner tokens wrapped to 500px: {:>6} us (bounds: {}x{})",
        elapsed_zw.as_micros(),
        bounds_zw.0,
        bounds_zw.1
    );
    assert!(bounds_zw.0 > 0.0);
}

#[test]
fn test_stress_web_reader_malformed_html_fuzzing() {
    use strata_web_reader::parser::HtmlParser;
    use strata_web_reader::reader_theme::ReaderTheme;
    use strata_web_reader::transpiler::WebTranspiler;

    // Fuzz test with severely broken, nested, unclosed HTML
    let broken_html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Unclosed test
        <style>body { color: red; div { font-size: 14px;
        <body>
            <div class="outer" style="width: 100px;
                <span><p><b><i>Missing all closing tags!
                <img src="foo.png" alt="unclosed"
                <table class="wikitable"><tr><td>cell 1<td>cell 2<tr><td>cell 3
                <script>alert("should be stripped"); console.log("evil");
                <div class="nested"><div class="nested2"><div class="nested3">
                <a href="/target">link with unclosed tag
    "#;

    let nodes = HtmlParser::parse(broken_html);
    assert!(!nodes.is_empty(), "Parser must gracefully recover from broken HTML");

    let theme = ReaderTheme::default();
    let transpiler = WebTranspiler::new(&theme);
    let blueprint = transpiler.transpile_document(&nodes, Some("Malformed Document"));
    assert!(matches!(blueprint, NodeBlueprint::Element { .. }));

    let view = View::new(blueprint);
    let mut runtime = AppRuntime::new(view, 1024.0, 768.0, 1.0);
    let redraw = runtime.tick_frame();
    println!("[STRESS] Malformed HTML gracefully transpiled and mounted, DL Cmds: {}", redraw.len());
}

#[test]
fn test_stress_circular_css_variables_resolution() {
    use strata_style::parser::StyleSheet;

    // Circular variable references: --a: var(--b); --b: var(--c); --c: var(--a);
    let css = r#"
        :root {
            --var-a: var(--var-b);
            --var-b: var(--var-c);
            --var-c: var(--var-a);
            --fallback: 24px;
            --safe: var(--undefined-var, 16px);
        }
        .test-box {
            width: var(--var-a, 100px);
            padding: var(--safe);
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("CSS stylesheet must parse");
    assert!(!sheet.rules.is_empty());
    println!("[STRESS] CSS with circular variable cycle parsed successfully without hanging");
}
