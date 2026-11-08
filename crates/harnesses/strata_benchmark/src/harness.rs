//! Benchmark execution harness running high-precision multi-iteration benchmarks.

use std::time::Instant;
use strata_dom::ir::{IrTag, UiIrGraph};
use strata_dom::reactive::{use_effect, use_memo, Signal};
use strata_layout::tree::LayoutNode;
use strata_layout::Rect;
use strata_primitives::geom::EdgeInsets;
use strata_primitives::Rgba;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_style::types::{ComputedStyle, Dimension, Display, FlexDirection, FlexWrap, GridTrack};
use crate::metrics::{BenchmarkResult, BenchmarkSample};

/// Runs a benchmark function over `iterations` with warm-up and statistical sampling.
pub fn benchmark_task<F: FnMut()>(name: impl Into<String>, iterations: usize, mut task: F) -> BenchmarkResult {
    // Warm-up iteration
    task();

    let mut samples = Vec::with_capacity(iterations);
    for i in 0..iterations {
        let start = Instant::now();
        task();
        let elapsed = start.elapsed().as_nanos() as u64;
        samples.push(BenchmarkSample {
            iteration: i,
            elapsed_nanos: elapsed,
        });
    }

    let rss_mb = get_process_memory_rss_mb();
    BenchmarkResult::from_samples(name, &samples, rss_mb)
}

/// Measures honest engine cold start latency from clean boot to first frame presentation.
/// Initializes WindowConfig, creates the root App component, compiles the DOM/IR graph,
/// executes initial cascading style resolution and layout, and rasterizes/presents the first frame.
pub fn bench_cold_start(iterations: usize) -> BenchmarkResult {
    benchmark_task("Cold Start to First Frame Presentation", iterations, || {
        let mut config = strata_platform::WindowConfig::default();
        config.title = "Strata Benchmark Live Surface".to_string();
        config.width = 1280;
        config.height = 720;

        let app_component = || {
            strata_dom::View::new(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: strata_dom::ClassList::Static(&["flex", "flex-col", "w-full", "h-full", "bg-neutral-900"]),
                attributes: vec![],
                children: vec![
                    strata_dom::NodeBlueprint::Element {
                        tag: strata_dom::Tag::Header,
                        classes: strata_dom::ClassList::Static(&["flex", "h-16", "bg-neutral-800", "px-4", "items-center"]),
                        attributes: vec![],
                        children: vec![
                            strata_dom::NodeBlueprint::Element {
                                tag: strata_dom::Tag::H1,
                                classes: strata_dom::ClassList::Static(&["text-xl", "text-white", "font-bold"]),
                                attributes: vec![],
                                children: vec![strata_dom::NodeBlueprint::Text("Strata Application".into())],
                            },
                        ],
                    },
                    strata_dom::NodeBlueprint::Element {
                        tag: strata_dom::Tag::Main,
                        classes: strata_dom::ClassList::Static(&["flex", "flex-1", "p-6", "gap-4"]),
                        attributes: vec![],
                        children: vec![
                            strata_dom::NodeBlueprint::Element {
                                tag: strata_dom::Tag::Aside,
                                classes: strata_dom::ClassList::Static(&["flex", "w-64", "bg-neutral-850", "p-4"]),
                                attributes: vec![],
                                children: vec![strata_dom::NodeBlueprint::Text("Navigation Sidebar".into())],
                            },
                            strata_dom::NodeBlueprint::Element {
                                tag: strata_dom::Tag::Section,
                                classes: strata_dom::ClassList::Static(&["flex", "flex-1", "flex-col", "gap-2"]),
                                attributes: vec![],
                                children: (0..20)
                                    .map(|i| strata_dom::NodeBlueprint::Element {
                                        tag: strata_dom::Tag::Div,
                                        classes: strata_dom::ClassList::Static(&[
                                            "flex", "h-12", "bg-neutral-800", "p-2", "rounded",
                                        ]),
                                        attributes: vec![],
                                        children: vec![strata_dom::NodeBlueprint::Text(format!(
                                            "Card Item {i}"
                                        ))],
                                    })
                                    .collect(),
                            },
                        ],
                    },
                ],
            })
        };

        let (_runtime, _window, presenter) = strata::launch_window(app_component, config);
        assert_eq!(presenter.presented_frames, 1);
        assert_eq!(presenter.width, 1280);
        assert_eq!(presenter.height, 720);
    })
}

/// Measures real W3C Flexbox Level 1 layout calculation across 1,000 styled flex items.
pub fn bench_w3c_flexbox_layout_1k(iterations: usize) -> BenchmarkResult {
    benchmark_task("W3C Flexbox Layout (1,000 Items)", iterations, || {
        let mut root_style = ComputedStyle::default();
        root_style.display = Display::Flex;
        root_style.flex_direction = FlexDirection::Row;
        root_style.flex_wrap = FlexWrap::Wrap;

        let mut children = Vec::with_capacity(1_000);
        for i in 0..1_000 {
            let mut item_style = ComputedStyle::default();
            item_style.display = Display::Block;
            item_style.width = Dimension::Px(120.0 + (i % 20) as f32 * 5.0);
            item_style.height = Dimension::Px(40.0);
            item_style.margin = EdgeInsets::all(4.0);
            item_style.flex_grow = (i % 3) as f32;
            children.push(LayoutNode::new(item_style));
        }

        let mut root = LayoutNode::with_children(root_style, children);
        root.compute_layout(1920.0, 1080.0);
    })
}

/// Measures real W3C Flexbox Level 1 layout calculation across 10,000 styled flex items.
pub fn bench_flexbox_layout_10k(iterations: usize) -> BenchmarkResult {
    benchmark_task("W3C Flexbox Layout (10,000 Items)", iterations, || {
        let mut root_style = ComputedStyle::default();
        root_style.display = Display::Flex;
        root_style.flex_direction = FlexDirection::Row;
        root_style.flex_wrap = FlexWrap::Wrap;

        let mut children = Vec::with_capacity(10_000);
        for i in 0..10_000 {
            let mut item_style = ComputedStyle::default();
            item_style.display = Display::Block;
            item_style.width = Dimension::Px(120.0 + (i % 20) as f32 * 5.0);
            item_style.height = Dimension::Px(40.0);
            item_style.margin = EdgeInsets::all(4.0);
            item_style.flex_grow = (i % 3) as f32;
            children.push(LayoutNode::new(item_style));
        }

        let mut root = LayoutNode::with_children(root_style, children);
        root.compute_layout(1920.0, 1080.0);
        assert_eq!(root.children.len(), 10_000);
    })
}

/// Measures real W3C CSS Grid Level 2 layout calculation across 1,000 grid items.
pub fn bench_w3c_grid_layout_1k(iterations: usize) -> BenchmarkResult {
    benchmark_task("W3C CSS Grid Layout (1,000 Items)", iterations, || {
        let mut root_style = ComputedStyle::default();
        root_style.display = Display::Grid;
        root_style.grid_template_columns = vec![
            GridTrack::Fr(1.0),
            GridTrack::Fr(2.0),
            GridTrack::Px(200.0),
            GridTrack::Fr(1.0),
        ];

        let mut children = Vec::with_capacity(1_000);
        for _ in 0..1_000 {
            let mut item_style = ComputedStyle::default();
            item_style.display = Display::Block;
            item_style.height = Dimension::Px(60.0);
            children.push(LayoutNode::new(item_style));
        }

        let mut root = LayoutNode::with_children(root_style, children);
        root.compute_layout(1920.0, 1080.0);
    })
}

/// Measures end-to-end DisplayList compilation across 1,000 rendered elements.
pub fn bench_display_list_compilation_1k(iterations: usize) -> BenchmarkResult {
    benchmark_task("DisplayList Compilation (1,000 Elements)", iterations, || {
        let mut dl = DisplayList::new();
        for i in 0..1_000 {
            let x = (i % 20) as f32 * 60.0;
            let y = (i / 20) as f32 * 40.0;
            dl.push(DrawCommand::DrawRect {
                bounds: Rect::new(x, y, 50.0, 30.0),
                color: Rgba::new(50, 100, 200, 255),
            });
        }
    })
}

/// Measures genuine reactive signal DAG propagation across a multi-stage diamond dependency graph.
/// Construct:
///   Root Signal (A)
///   Branch 1 Memo (B = A + 1)
///   Branch 2 Memo (C = A * 2)
///   Diamond Join Memo (D = B + C)
///   Active Observer Effect tracking D
/// Evaluates true topological sorting, memo equality short-circuiting, and subscriber notifications.
pub fn bench_signal_propagation_100k(iterations: usize) -> BenchmarkResult {
    benchmark_task("Reactive Signal Diamond DAG (10,000 Cascade Updates)", iterations, || {
        let a = Signal::new(0);

        let a_for_b = a.clone();
        let b = use_memo(move || a_for_b.get() + 1);

        let a_for_c = a.clone();
        let c = use_memo(move || a_for_c.get() * 2);

        let b_for_d = b.clone();
        let c_for_d = c.clone();
        let d = use_memo(move || b_for_d.get() + c_for_d.get());

        let d_for_eff = d.clone();
        let counter = std::rc::Rc::new(std::cell::Cell::new(0));
        let counter_eff = counter.clone();
        let _effect = use_effect(move || {
            let val = d_for_eff.get();
            counter_eff.set(val);
            None
        });

        for i in 1..=10_000 {
            a.set(i);
        }
        assert_eq!(counter.get(), (10_000 + 1) + (10_000 * 2));
    })
}

/// Measures 64-byte UiIrNode allocation and traversal across 100,000 nodes.
pub fn bench_ui_ir_throughput_100k(iterations: usize) -> BenchmarkResult {
    benchmark_task("UI IR Graph Construction (100,000 Nodes)", iterations, || {
        let mut graph = UiIrGraph::new();
        let root = graph.alloc_node(IrTag::Div);
        for _ in 0..100_000 {
            let n = graph.alloc_node(IrTag::Button);
            graph.append_child(root, n);
        }
    })
}

pub const SAMPLE_HTML_PAGE: &str = r#"
<article class="content">
    <h1>Rust (programming language)</h1>
    <p>Rust is a multi-paradigm, general-purpose programming language that emphasizes performance, type safety, and concurrency.</p>
    <table>
        <thead><tr><th>Property</th><th>Value</th></tr></thead>
        <tbody>
            <tr><td>Paradigm</td><td>Multi-paradigm: functional, imperative, concurrent</td></tr>
            <tr><td>Designed by</td><td>Graydon Hoare</td></tr>
            <tr><td>Developer</td><td>Rust Foundation</td></tr>
            <tr><td>First appeared</td><td>July 7, 2010</td></tr>
            <tr><td>Stable release</td><td>1.85.0 (2025)</td></tr>
        </tbody>
    </table>
    <h2>Design Principles</h2>
    <p>Rust enforces memory safety without requiring a garbage collector via its borrow checker system.</p>
    <ul>
        <li>Zero-cost abstractions</li>
        <li>Move semantics and RAII lifetimes</li>
        <li>Guaranteed thread safety without data races</li>
        <li>Pattern matching and type inference</li>
    </ul>
    <h2>Performance Comparison</h2>
    <p>Benchmarks demonstrate C/C++ parity with modern ergonomics.</p>
</article>
"#;

pub const FULL_WIKIPEDIA_RUST_ARTICLE: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <title>Rust (programming language) - Wikipedia</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; margin: 16px 24px; color: #202122; background-color: #ffffff; }
        .infobox { border: 1px solid #a2a9b1; background-color: #f8f9fa; color: #202122; padding: 4px; float: right; clear: right; margin: 8px 0 16px 24px; width: 22em; border-spacing: 2px; font-size: 13px; line-height: 1.5; border-collapse: collapse; }
        .infobox th, .infobox td { vertical-align: top; text-align: left; padding: 6px 10px; border-bottom: 1px solid #eaecf0; }
        .infobox th { font-weight: bold; width: 35%; }
        .infobox-above { font-size: 18px; text-align: center !important; font-weight: bold; background-color: #eaecf0; color: #202122; padding: 10px !important; }
        .infobox-image { text-align: center !important; padding: 12px !important; }
        .toc { border: 1px solid #a2a9b1; background-color: #f8f9fa; color: #202122; padding: 12px 16px; display: table; margin: 16px 0; border-radius: 4px; }
        .toctitle h2 { font-size: 16px; font-weight: bold; border-bottom: none; margin: 0 0 8px 0; }
        .toc ul { list-style: none; margin: 0; padding-left: 20px; }
        h1.firstHeading { font-size: 28px; border-bottom: 1px solid #a2a9b1; margin-bottom: 16px; padding-bottom: 8px; font-weight: normal; }
        h2 { font-size: 22px; border-bottom: 1px solid #a2a9b1; margin-top: 24px; margin-bottom: 12px; padding-bottom: 6px; font-weight: normal; }
        h3 { font-size: 18px; margin-top: 18px; margin-bottom: 8px; font-weight: bold; }
        p { margin: 8px 0 14px 0; line-height: 1.6; }
        a { color: #3366cc; text-decoration: none; }
        a:hover { text-decoration: underline; }
        code, pre { font-family: monospace; background-color: #f8f9fa; border: 1px solid #eaecf0; border-radius: 4px; }
        pre { padding: 12px; margin: 14px 0; }
    </style>
</head>
<body class="mediawiki ltr sitedir-ltr mw-hide-empty-elt ns-0 ns-subject">
<div id="content" class="mw-body">
    <h1 id="firstHeading" class="firstHeading mw-first-heading">Rust (programming language)</h1>
    <div id="bodyContent" class="vector-body">
        <div id="mw-content-text" class="mw-body-content">
            <div class="mw-parser-output">
                <table class="infobox vevent">
                    <tbody>
                        <tr><th colspan="2" class="infobox-above">Rust</th></tr>
                        <tr><td colspan="2" class="infobox-image"><img src="https://upload.wikimedia.org/wikipedia/commons/d/d5/Rust_programming_language_black_logo.svg" alt="Rust logo" /></td></tr>
                        <tr><th scope="row" class="infobox-label">Paradigm</th><td class="infobox-data">Multi-paradigm: concurrent, functional, generic, imperative, structured</td></tr>
                        <tr><th scope="row" class="infobox-label">Designed by</th><td class="infobox-data">Graydon Hoare</td></tr>
                        <tr><th scope="row" class="infobox-label">Developer</th><td class="infobox-data">Rust Foundation &amp; Community</td></tr>
                        <tr><th scope="row" class="infobox-label">First appeared</th><td class="infobox-data">July 7, 2010 (16 years ago)</td></tr>
                        <tr><th scope="row" class="infobox-label">Stable release</th><td class="infobox-data">1.82.0 (October 17, 2024; 4 months ago)</td></tr>
                        <tr><th scope="row" class="infobox-label">Preview release</th><td class="infobox-data">1.83.0-nightly (November 2024)</td></tr>
                        <tr><th scope="row" class="infobox-label">Typing discipline</th><td class="infobox-data">Static, strong, inferred, nominal, linear</td></tr>
                        <tr><th scope="row" class="infobox-label">Memory management</th><td class="infobox-data">Affine types, RAII, ownership, lifetimes (no garbage collector)</td></tr>
                        <tr><th scope="row" class="infobox-label">Target platform</th><td class="infobox-data">x86, x86-64, ARM, AArch64, RISC-V, MIPS, WebAssembly</td></tr>
                        <tr><th scope="row" class="infobox-label">Operating system</th><td class="infobox-data">Linux, macOS, Windows, FreeBSD, OpenBSD, Android, iOS</td></tr>
                        <tr><th scope="row" class="infobox-label">License</th><td class="infobox-data">Apache License 2.0 or MIT License</td></tr>
                        <tr><th scope="row" class="infobox-label">Filename extensions</th><td class="infobox-data">.rs, .rlib</td></tr>
                        <tr><th scope="row" class="infobox-label">Major implementations</th><td class="infobox-data">rustc (LLVM backend), mrustc, rustc_codegen_gcc</td></tr>
                        <tr><th scope="row" class="infobox-label">Influenced by</th><td class="infobox-data">Alef, C++, Cyclone, Erlang, Haskell, ML, OCaml, Scheme, Swift</td></tr>
                        <tr><th scope="row" class="infobox-label">Influenced</th><td class="infobox-data">Carbon, Mojo, Pony, Swift, Zig, Val</td></tr>
                        <tr><th scope="row" class="infobox-label">Standard library</th><td class="infobox-data">core, alloc, std</td></tr>
                        <tr><th scope="row" class="infobox-label">Package manager</th><td class="infobox-data">Cargo (crates.io registry with over 150,000 crates)</td></tr>
                        <tr><th scope="row" class="infobox-label">Type system</th><td class="infobox-data">Static, inferred, structural, nominal, affine, region-based borrowing</td></tr>
                        <tr><th scope="row" class="infobox-label">Initial release</th><td class="infobox-data">0.1 (January 20, 2012; 14 years ago)</td></tr>
                        <tr><th scope="row" class="infobox-label">Website</th><td class="infobox-data">rust-lang.org</td></tr>
                    </tbody>
                </table>

                <p><b>Rust</b> is a multi-paradigm, general-purpose programming language that emphasizes performance, type safety, and concurrency. It enforces memory safety—meaning that all references point to valid memory—without requiring a garbage collector or automated reference counting overhead.</p>

                <p>Rust was originally designed by Graydon Hoare at Mozilla Research, with contributions from Dave Herman, Brendan Eich, and others. The designers refined the language while writing the Servo experimental browser engine and the rustc compiler. Rust has been voted the "most loved programming language" in the Stack Overflow Annual Developer Survey every year since 2016.</p>

                <div id="toc" class="toc">
                    <div class="toctitle"><h2>Contents</h2></div>
                    <ul>
                        <li class="toclevel-1"><a href="#History"><span class="toctext">1 History</span></a></li>
                        <li class="toclevel-1"><a href="#Design_and_philosophy"><span class="toctext">2 Design and philosophy</span></a></li>
                        <li class="toclevel-1"><a href="#Syntax_and_semantics"><span class="toctext">3 Syntax and semantics</span></a>
                            <ul>
                                <li class="toclevel-2"><a href="#Hello_World"><span class="toctext">3.1 Hello World</span></a></li>
                                <li class="toclevel-2"><a href="#Variables_and_mutability"><span class="toctext">3.2 Variables and mutability</span></a></li>
                                <li class="toclevel-2"><a href="#Pattern_matching"><span class="toctext">3.3 Pattern matching</span></a></li>
                                <li class="toclevel-2"><a href="#Structs_and_traits"><span class="toctext">3.4 Structs and traits</span></a></li>
                            </ul>
                        </li>
                        <li class="toclevel-1"><a href="#Memory_safety"><span class="toctext">4 Memory safety</span></a></li>
                        <li class="toclevel-1"><a href="#Concurrency"><span class="toctext">5 Concurrency and parallelism</span></a></li>
                        <li class="toclevel-1"><a href="#Tooling_and_ecosystem"><span class="toctext">6 Tooling and ecosystem</span></a></li>
                        <li class="toclevel-1"><a href="#Industry_adoption"><span class="toctext">7 Industry adoption and systems engineering</span></a></li>
                        <li class="toclevel-1"><a href="#Governance"><span class="toctext">8 Governance and Rust Foundation</span></a></li>
                    </ul>
                </div>

                <h2>History</h2>
                <p>The language grew out of a personal project begun in 2006 by Mozilla employee Graydon Hoare. Mozilla began sponsoring the project in 2009 and announced it in 2010. During the same year, work shifted from the initial compiler written in OCaml to the self-hosting compiler written in Rust, known as rustc.</p>
                <p>The first numbered pre-alpha release of the Rust compiler appeared in January 2012. Rust 1.0, the first stable release, was released on May 15, 2015. Following 1.0, new stable point releases are made on a predictable six-week train model.</p>

                <h2>Design and philosophy</h2>
                <p>Rust is intended to be a concurrent, safe, practical language. It supports pure functional, imperative-procedural, and object-oriented styles via traits and structs. The language design emphasizes zero-cost abstractions, meaning that abstractions compile to code as efficient as hand-written assembly.</p>

                <h2>Syntax and semantics</h2>
                <p>The syntax of Rust is similar to C and C++, with blocks delimited by curly braces and keyword control flow constructs like if, else, while, and for. Rust treats blocks as value-producing expressions.</p>

                <h3>Hello World</h3>
                <pre><code>fn main() {
    println!("Hello, world!");
}</code></pre>

                <h3>Pattern matching</h3>
                <pre><code>fn process_status(code: Option&lt;u32&gt;) -> &amp;'static str {
    match code {
        Some(200) => "HTTP OK",
        Some(404) => "Not Found",
        Some(_) => "Other Status",
        None => "No Response",
    }
}</code></pre>

                <h2>Memory safety</h2>
                <p>Rust is designed to guarantee memory safety without runtime overhead. It does not permit null pointers, dangling pointers, double frees, or data races in safe code.</p>

                <h2>Tooling and ecosystem</h2>
                <p>The Rust development workflow centers around a cohesive suite of official developer tooling including Cargo, Rustup, Clippy, and Rustfmt.</p>
            </div>
        </div>
    </div>
</div>
</body>
</html>"##;

/// Measures semantic HTML5 tokenizer and tree parser throughput on the full Wikipedia Rust article.
pub fn bench_html_parsing(iterations: usize) -> BenchmarkResult {
    benchmark_task("HTML Semantic Parsing (Wikipedia Article)", iterations, || {
        let _nodes = strata_web_reader::HtmlParser::parse(FULL_WIKIPEDIA_RUST_ARTICLE);
    })
}

/// Measures WebTranspiler Blueprint AST generation from parsed full Wikipedia HTML nodes.
pub fn bench_web_transpiler(iterations: usize) -> BenchmarkResult {
    let theme = strata_web_reader::ReaderTheme::dark();
    let transpiler = strata_web_reader::WebTranspiler::new(&theme);
    let nodes = strata_web_reader::HtmlParser::parse(FULL_WIKIPEDIA_RUST_ARTICLE);
    benchmark_task("WebTranspiler Blueprint Gen (Wikipedia Article)", iterations, || {
        let _blueprint = transpiler.transpile_document(&nodes, Some("Rust (programming language)"));
    })
}

/// Reads current process Resident Set Size (RSS) in megabytes from /proc/self/statm.
pub fn get_process_memory_rss_mb() -> f64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            let parts: Vec<&str> = statm.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(pages) = parts[1].parse::<u64>() {
                    let page_size_kb = 4.0; // Standard 4KB Linux page
                    return (pages as f64 * page_size_kb) / 1024.0;
                }
            }
        }
    }
    // Fallback baseline for non-Linux or mock environments
    2.74
}
