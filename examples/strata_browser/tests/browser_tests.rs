//! Verification test suite for Strata Native Browser.

use strata_browser::mock_pages::resolve_page;
use strata_browser::state::BrowserState;
use strata_web_reader::{HtmlParser, ReaderTheme, WebTranspiler};

#[test]
fn test_browser_initial_state() {
    let state = BrowserState::new();

    let tabs = state.tabs.get();
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].url, "strata://newtab");
    assert_eq!(tabs[0].title, "New Tab");
    assert_eq!(state.active_tab_id.get(), 1);
    assert_eq!(state.url_input.get(), "strata://newtab");
    assert!(!state.can_go_back());
    assert!(!state.can_go_forward());
}

#[test]
fn test_new_tab_and_switching() {
    let state = BrowserState::new();

    state.new_tab("https://en.wikipedia.org/wiki/Rust_(programming_language)");

    let tabs = state.tabs.get();
    assert_eq!(tabs.len(), 2);
    assert_eq!(state.active_tab_id.get(), 2);
    assert_eq!(
        state.url_input.get(),
        "https://en.wikipedia.org/wiki/Rust_(programming_language)"
    );

    let active_tab = state.active_tab().expect("Active tab must exist");
    assert_eq!(active_tab.id, 2);
    assert!(active_tab.title.contains("Rust"));

    // Switch back to tab 1
    state.switch_tab(1);
    assert_eq!(state.active_tab_id.get(), 1);
    assert_eq!(state.url_input.get(), "strata://newtab");
}

#[test]
fn test_tab_closing() {
    let state = BrowserState::new();

    state.new_tab("https://news.ycombinator.com");
    state.new_tab("https://strata.larvance.com");

    assert_eq!(state.tabs.get().len(), 3);
    assert_eq!(state.active_tab_id.get(), 3);

    // Close active tab (tab 3)
    state.close_tab(3);
    assert_eq!(state.tabs.get().len(), 2);
    // Active tab shifts to an adjacent tab
    let active_id = state.active_tab_id.get();
    assert!(active_id == 1 || active_id == 2);

    // Close remaining tabs
    state.close_tab(2);
    assert_eq!(state.tabs.get().len(), 1);

    // Closing the final tab resets it rather than destroying the window
    let last_id = state.tabs.get()[0].id;
    state.close_tab(last_id);
    let remaining = state.tabs.get();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].url, "strata://newtab");
}

#[test]
fn test_navigation_and_history_stack() {
    let state = BrowserState::new();

    // Navigate to Hacker News
    state.navigate("https://news.ycombinator.com");
    assert_eq!(state.url_input.get(), "https://news.ycombinator.com");
    assert!(state.can_go_back());
    assert!(!state.can_go_forward());

    // Navigate to Strata Docs
    state.navigate("https://strata.larvance.com");
    assert_eq!(state.url_input.get(), "https://strata.larvance.com");
    assert!(state.can_go_back());
    assert!(!state.can_go_forward());

    // Go back to Hacker News
    state.go_back();
    assert_eq!(state.url_input.get(), "https://news.ycombinator.com");
    assert!(state.can_go_back());
    assert!(state.can_go_forward());

    // Go back to New Tab
    state.go_back();
    assert_eq!(state.url_input.get(), "strata://newtab");
    assert!(!state.can_go_back());
    assert!(state.can_go_forward());

    // Go forward to Hacker News
    state.go_forward();
    assert_eq!(state.url_input.get(), "https://news.ycombinator.com");
    assert!(state.can_go_back());
    assert!(state.can_go_forward());
}

#[test]
fn test_query_search_normalization() {
    let state = BrowserState::new();

    // Typing a search query with spaces normalizes to Google Search
    state.navigate("rust layout engine");
    let active_tab = state.active_tab().expect("Active tab must exist");
    assert_eq!(
        active_tab.url,
        "https://www.google.com/search?q=rust+layout+engine"
    );
    assert!(active_tab.title.contains("rust layout engine"));
    assert!(active_tab.html_content.contains("Google Search Results"));
}

#[test]
fn test_html_parsing_and_transpilation() {
    let (_, html) = resolve_page("https://en.wikipedia.org/wiki/Rust_(programming_language)");
    assert!(!html.is_empty());

    let nodes = HtmlParser::parse(&html);
    assert!(!nodes.is_empty());

    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let blueprint = transpiler.transpile_document(&nodes, Some("Rust Wikipedia"));

    assert!(matches!(blueprint, strata_dom::NodeBlueprint::Element { .. }));
}

#[test]
fn test_theme_switching() {
    let state = BrowserState::new();
    assert_eq!(state.active_theme.get(), "dark");

    state.set_theme("amoled");
    assert_eq!(state.active_theme.get(), "amoled");

    state.set_theme("sepia");
    assert_eq!(state.active_theme.get(), "sepia");
}

#[test]
fn test_devtools_panel_layout() {
    let state = BrowserState::new();
    state.open_devtools(strata_browser::state::DevToolsTab::Elements);
    assert!(state.is_devtools_open.get());

    let app_view = strata_browser::components::BrowserApp(state);
    let runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);

    // Root -> Row container (Child 2) -> DevToolsPanel (Child 1)
    let row_container = &runtime.layout_root.children[2];
    assert_eq!(row_container.geometry.border_box.height, 679.0);

    let devtools = &row_container.children[1];
    assert_eq!(devtools.geometry.border_box.width, 560.0);
    assert_eq!(devtools.geometry.border_box.height, 679.0);

    // Child 0: Top nav strip (h=36)
    let nav_strip = &devtools.children[0];
    assert_eq!(nav_strip.geometry.border_box.y, 121.0);
    assert_eq!(nav_strip.geometry.border_box.height, 36.0);

    // Child 1: Active tab content pane (starts at y=157, h=643)
    let content_pane = &devtools.children[1];
    assert_eq!(content_pane.geometry.border_box.y, 157.0);
    assert_eq!(content_pane.geometry.border_box.height, 643.0);
}

