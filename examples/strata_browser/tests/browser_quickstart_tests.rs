//! Integration Tests for Strata Browser Quickstart Example.

use strata::prelude::*;
use strata_browser::{BrowserApp, BrowserState};
use strata_web_reader::{HtmlParser, ReaderTheme, WebTranspiler};

#[test]
fn test_browser_state_lifecycle_and_navigation() {
    let state = BrowserState::new();

    // Verify initial state preloaded with welcome and about tabs
    let tabs = state.tabs.get();
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs[0].url, "strata://welcome");
    assert_eq!(tabs[1].url, "strata://about");
    assert_eq!(state.active_index.get(), 0);

    // Navigate to benchmarks page
    state.navigate("strata://benchmarks");
    let active = state.active_tab().expect("Active tab should exist");
    assert_eq!(active.url, "strata://benchmarks");
    assert_eq!(active.title, "Empirical Benchmarks");
    assert!(active.html.contains("Empirical Performance Metrics"));

    // Open a new tab
    state.new_tab("Rust Lang", "https://www.rust-lang.org", "<h1>Empowering Everyone</h1>");
    assert_eq!(state.tabs.get().len(), 3);
    assert_eq!(state.active_index.get(), 2);
    assert_eq!(state.url_input.get(), "https://www.rust-lang.org");

    // Close tab
    state.close_tab(2);
    assert_eq!(state.tabs.get().len(), 2);
    assert_eq!(state.active_index.get(), 1);

    // Test history navigation
    state.navigate("strata://welcome");
    state.navigate("strata://about");
    state.back();
    assert_eq!(state.url_input.get(), "strata://welcome");
    state.forward();
    assert_eq!(state.url_input.get(), "strata://about");
}

#[test]
fn test_browser_html_transpilation_pipeline() {
    let raw_html = r#"
        <article class="test-article">
            <h1>Zero-Chromium Architecture</h1>
            <p>Direct pure-Rust HTML5 to Strata DOM transpilation.</p>
            <ul>
                <li>Item A</li>
                <li>Item B</li>
            </ul>
        </article>
    "#;

    let nodes = HtmlParser::parse(raw_html);
    assert!(!nodes.is_empty(), "Parsed semantic nodes must not be empty");

    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let blueprint = transpiler.transpile_document(&nodes, Some("Test Article"));

    match blueprint {
        NodeBlueprint::Element { children, .. } => {
            assert!(!children.is_empty(), "Transpiled element must contain child nodes");
        }
        NodeBlueprint::Fragment(children) => {
            assert!(!children.is_empty(), "Transpiled fragment must contain child nodes");
        }
        _ => panic!("Expected Element or Fragment blueprint"),
    }
}

#[test]
fn test_browser_headless_presentation_tick() {
    let state = BrowserState::new();
    let render_state = state.clone();

    let runtime = strata::launch_headless(
        move || BrowserApp(render_state),
        1280.0,
        800.0,
    );

    assert_eq!(runtime.viewport_width, 1280.0);
    assert_eq!(runtime.viewport_height, 800.0);
    assert!(
        !runtime.display_list.commands.is_empty(),
        "Display list must contain rendered commands"
    );
}
