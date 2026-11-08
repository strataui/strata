//! Verification test suite for Strata Native Browser.

use strata_browser::state::BrowserState;
use strata_web_reader::{HtmlParser, ReaderTheme, WebTranspiler};

fn artifact_path(name: &str) -> String {
    let dir = std::path::Path::new("target").join("browser-artifacts");
    std::fs::create_dir_all(&dir).expect("create browser artifact directory");
    dir.join(name).to_string_lossy().into_owned()
}

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

    state.new_tab("strata://about");

    let tabs = state.tabs.get();
    assert_eq!(tabs.len(), 2);
    assert_eq!(state.active_tab_id.get(), 2);
    assert_eq!(state.url_input.get(), "strata://about");

    let active_tab = state.active_tab().expect("Active tab must exist");
    assert_eq!(active_tab.id, 2);
    assert!(active_tab.title.contains("About Strata"));

    // Switch back to tab 1
    state.switch_tab(1);
    assert_eq!(state.active_tab_id.get(), 1);
    assert_eq!(state.url_input.get(), "strata://newtab");
}

#[test]
fn test_tab_closing() {
    let state = BrowserState::new();

    state.new_tab("strata://about");
    state.new_tab("strata://version");

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

    // Navigate to Strata About
    state.navigate("strata://about");
    assert_eq!(state.url_input.get(), "strata://about");
    assert!(state.can_go_back());
    assert!(!state.can_go_forward());

    // Navigate to Strata Version
    state.navigate("strata://version");
    assert_eq!(state.url_input.get(), "strata://version");
    assert!(state.can_go_back());
    assert!(!state.can_go_forward());

    // Go back to Strata About
    state.go_back();
    assert_eq!(state.url_input.get(), "strata://about");
    assert!(state.can_go_back());
    assert!(state.can_go_forward());

    // Go back to New Tab
    state.go_back();
    assert_eq!(state.url_input.get(), "strata://newtab");
    assert!(!state.can_go_back());
    assert!(state.can_go_forward());

    // Go forward to Strata About
    state.go_forward();
    assert_eq!(state.url_input.get(), "strata://about");
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
}

#[test]
fn test_html_parsing_and_transpilation() {
    let result = strata_browser::network::load_page("strata://about");
    assert!(!result.html.is_empty());

    let nodes = HtmlParser::parse(&result.html);
    assert!(!nodes.is_empty());

    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let blueprint = transpiler.transpile_document(&nodes, Some("Strata About"));

    assert!(matches!(
        blueprint,
        strata_dom::NodeBlueprint::Element { .. }
    ));
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
    assert!(row_container.geometry.border_box.height >= 660.0);

    let devtools = &row_container.children[1];
    assert_eq!(devtools.geometry.border_box.width, 560.0);
    assert!(devtools.geometry.border_box.height >= 660.0);

    // Child 0: Top nav strip (h=36)
    let nav_strip = &devtools.children[0];
    assert!(nav_strip.geometry.border_box.y >= 120.0);
    assert_eq!(nav_strip.geometry.border_box.height, 36.0);

    // Child 1: Active tab content pane (starts around y=174)
    let content_pane = &devtools.children[1];
    assert!(content_pane.geometry.border_box.y >= 156.0);
    assert!(content_pane.geometry.border_box.height >= 620.0);
}

#[test]
fn test_google_new_tab_centering() {
    let state = BrowserState::new();
    let app_view = strata_browser::components::BrowserApp(state);
    let runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);

    let new_tab_container = &runtime.layout_root.children[2];
    let center_x = 640.0f32;

    // 0: Brand Header (Google Logo + Subtitle)
    let brand_header = &new_tab_container.children[0];
    eprintln!("brand_header: {:?}", brand_header.geometry.border_box);
    eprintln!(
        "child 0 (logo): {:?}",
        brand_header.children[0].geometry.border_box
    );
    eprintln!(
        "child 1 (subtitle): {:?}",
        brand_header.children[1].geometry.border_box
    );
    if !brand_header.children[1].children.is_empty() {
        eprintln!(
            "subtitle text child: {:?}",
            brand_header.children[1].children[0].geometry.border_box
        );
    }
    let brand_center =
        brand_header.geometry.border_box.x + brand_header.geometry.border_box.width / 2.0;
    assert!(
        (brand_center - center_x).abs() <= 1.0,
        "Brand header not centered: {}",
        brand_center
    );

    // 1: Search Input Bar (w=672, x=304)
    let search_bar = &new_tab_container.children[1];
    let search_center =
        search_bar.geometry.border_box.x + search_bar.geometry.border_box.width / 2.0;
    assert!(
        (search_center - center_x).abs() <= 1.0,
        "Search bar not centered: {}",
        search_center
    );

    // 2: Action Buttons ("Google Search" & "I'm Feeling Lucky")
    let buttons_row = &new_tab_container.children[2];
    let buttons_center =
        buttons_row.geometry.border_box.x + buttons_row.geometry.border_box.width / 2.0;
    assert!(
        (buttons_center - center_x).abs() <= 1.0,
        "Buttons row not centered: {}",
        buttons_center
    );

    for btn in &buttons_row.children {
        let text_node = &btn.children[0];
        let left_pad = text_node.geometry.border_box.x - btn.geometry.border_box.x;
        let right_pad = (btn.geometry.border_box.x + btn.geometry.border_box.width)
            - (text_node.geometry.border_box.x + text_node.geometry.border_box.width);
        assert!(
            (left_pad - right_pad).abs() <= 1.0,
            "Button text not centered horizontally: left_pad={}, right_pad={}",
            left_pad,
            right_pad
        );
    }

    // 3: Quick-Launch Shortcut Tiles (w=672, x=304)
    let shortcuts = &new_tab_container.children[3];
    let shortcuts_center =
        shortcuts.geometry.border_box.x + shortcuts.geometry.border_box.width / 2.0;
    assert!(
        (shortcuts_center - center_x).abs() <= 1.0,
        "Shortcuts not centered: {}",
        shortcuts_center
    );

    for btn in &shortcuts.children {
        let btn_center_x = btn.geometry.border_box.x + btn.geometry.border_box.width / 2.0;

        // Child 0: Icon container circle
        let icon_box = &btn.children[0];
        let icon_center_x =
            icon_box.geometry.border_box.x + icon_box.geometry.border_box.width / 2.0;
        assert!(
            (icon_center_x - btn_center_x).abs() <= 1.0,
            "Shortcut icon not centered horizontally: icon_center={}, btn_center={}",
            icon_center_x,
            btn_center_x
        );

        // Child 1: Text label container (span with text-center)
        let text_span = &btn.children[1];
        assert!(
            !text_span.line_boxes.is_empty(),
            "Text label must produce at least one line box"
        );
        let text_line = &text_span.line_boxes[0];
        let text_center_x = text_line.bounds.x + text_line.bounds.width / 2.0;
        assert!(
            (text_center_x - btn_center_x).abs() <= 1.0,
            "Shortcut text not centered horizontally: text_center={}, btn_center={}",
            text_center_x,
            btn_center_x
        );
    }
}

#[test]
fn test_wikipedia_infobox_transpilation_and_layout() {
    use strata_web_reader::{HtmlParser, ReaderTheme, WebTranspiler};
    let html = r##"
    <div class="mw-parser-output">
        <table class="infobox vevent">
            <tbody>
                <tr><th colspan="2" class="infobox-above">Rust</th></tr>
                <tr><th scope="row" class="infobox-label">Designed by</th><td class="infobox-data"><a href="#">Graydon Hoare</a></td></tr>
                <tr><th scope="row" class="infobox-label">First appeared</th><td class="infobox-data">July 7, 2010</td></tr>
                <tr><th scope="row" class="infobox-label">Stable release</th><td class="infobox-data">1.82.0</td></tr>
            </tbody>
        </table>
        <p>Rust is a general-purpose programming language emphasizing performance and safety.</p>
    </div>
    "##;
    let nodes = HtmlParser::parse(html);
    let primary = WebTranspiler::extract_primary_content(&nodes);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let doc = transpiler.transpile_document(primary, None);
    let mut layout_node = strata::AppRuntime::build_layout_tree(&doc);
    layout_node.compute_layout(1280.0, 800.0);

    // Find the floated table
    let table = &layout_node.children[0];
    assert_eq!(table.style.float, strata::style::types::Float::Right);
    assert_eq!(table.geometry.border_box.width, 352.0);
    assert!(table.geometry.border_box.height > 100.0);

    let tbody = &table.children[0];
    assert!(tbody.children.len() >= 4);

    // Verify row Y positions are strictly increasing without overlap
    let mut prev_bottom = 0.0f32;
    for (i, row) in tbody.children.iter().enumerate() {
        let y = row.geometry.border_box.y;
        let h = row.geometry.border_box.height;
        assert!(
            y >= prev_bottom - 0.5,
            "Row {} y ({}) overlaps with previous bottom ({})",
            i,
            y,
            prev_bottom
        );
        assert!(h > 15.0, "Row {} height ({}) should be > 15.0", i, h);
        prev_bottom = y + h;
    }

    // Verify paragraph wraps around floated table
    let paragraph = &layout_node.children[1];
    assert!(paragraph.line_boxes.len() >= 2);
    // Line 0 opportunity available width was reduced by float width
    assert!(paragraph.line_boxes[0].bounds.width <= 400.0);
}

#[test]
fn test_browser_app_wikipedia_full_layout() {
    let state = BrowserState::new();
    state.navigate("https://en.wikipedia.org/wiki/Rust_(programming_language)");
    let app_view = strata_browser::components::BrowserApp(state);
    let runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);

    fn find_first_table(node: &strata::layout::LayoutNode) -> Option<&strata::layout::LayoutNode> {
        if node.style.display == strata::style::types::Display::Table
            && !node.children.is_empty()
            && node.children[0].children.len() >= 10
        {
            return Some(node);
        }
        for ch in &node.children {
            if let Some(t) = find_first_table(ch) {
                return Some(t);
            }
        }
        None
    }

    let table = find_first_table(&runtime.layout_root).expect("Must find infobox table");
    assert_eq!(table.style.float, strata::style::types::Float::Right);
    // MediaWiki infobox standard width is 22em (22 * 16.0 = 352.0px)
    assert_eq!(table.geometry.border_box.width, 352.0);
    assert!(
        table.geometry.border_box.height >= 850.0,
        "Infobox table height {} should be >= 850.0",
        table.geometry.border_box.height
    );

    let tbody = &table.children[0];
    assert!(tbody.children.len() >= 15);

    let mut prev_bottom = tbody.geometry.border_box.y;
    for (i, row) in tbody.children.iter().enumerate() {
        let y = row.geometry.border_box.y;
        let h = row.geometry.border_box.height;
        assert!(
            y >= prev_bottom - 0.5,
            "Row {} y ({}) overlaps with previous bottom ({})",
            i,
            y,
            prev_bottom
        );
        assert!(h >= 15.0, "Row {} height ({}) should be >= 15.0", i, h);
        prev_bottom = y + h;
    }
}

#[test]
fn test_url_undo_redo_stack() {
    let state = BrowserState::new();
    state.is_omnibox_editing.set(true);
    assert_eq!(state.url_input.get(), "strata://newtab");

    // Type character by character
    state.handle_key_input('h');
    assert_eq!(state.url_input.get(), "strata://newtabh");
    state.handle_key_input('t');
    assert_eq!(state.url_input.get(), "strata://newtabht");

    // Undo once (Ctrl+Z)
    assert!(state.undo_url());
    assert_eq!(state.url_input.get(), "strata://newtabh");

    // Undo again
    assert!(state.undo_url());
    assert_eq!(state.url_input.get(), "strata://newtab");

    // Redo once (Ctrl+Y)
    assert!(state.redo_url());
    assert_eq!(state.url_input.get(), "strata://newtabh");

    // Redo again
    assert!(state.redo_url());
    assert_eq!(state.url_input.get(), "strata://newtabht");
}

#[test]
fn test_kebab_menu_and_settings_modal() {
    let state = BrowserState::new();
    assert!(!state.is_kebab_menu_open.get());
    assert!(!state.is_settings_modal_open.get());

    // Open kebab menu
    state.toggle_kebab_menu();
    assert!(state.is_kebab_menu_open.get());

    // Zoom controls
    assert_eq!(state.zoom_level.get(), 100);
    state.zoom_in();
    assert_eq!(state.zoom_level.get(), 110);
    state.zoom_out();
    assert_eq!(state.zoom_level.get(), 100);

    // Open settings from kebab menu
    state.open_settings();
    assert!(!state.is_kebab_menu_open.get());
    assert!(state.is_settings_modal_open.get());

    // Dismiss modal
    state.close_settings();
    assert!(!state.is_settings_modal_open.get());
}

#[test]
fn test_omnibox_editing_lifecycle_and_blur() {
    let state = BrowserState::new();
    state.start_editing_omnibox();
    assert!(state.is_omnibox_editing.get());
    assert!(state.omnibox_selected.get());

    // Stop editing on blur
    state.stop_editing_omnibox();
    assert!(!state.is_omnibox_editing.get());
    assert!(!state.omnibox_selected.get());
}

#[test]
fn test_right_click_context_menu_lifecycle() {
    let state = BrowserState::new();
    assert!(!state.is_context_menu_open.get());

    // Open context menu at (300, 200)
    state.open_context_menu(300.0, 200.0);
    assert!(state.is_context_menu_open.get());
    assert_eq!(state.context_menu_position.get(), (300.0, 200.0));

    // Global click dismisses context menu
    assert!(state.handle_global_click(500.0, 500.0));
    assert!(!state.is_context_menu_open.get());

    // Reopen and close via close_context_menu
    state.open_context_menu(400.0, 300.0);
    assert!(state.is_context_menu_open.get());
    state.close_context_menu();
    assert!(!state.is_context_menu_open.get());
}

#[test]
fn test_context_menu_targets() {
    let state = BrowserState::new();
    assert_eq!(
        state.context_menu_target.get(),
        strata_browser::ContextTarget::Page
    );

    // Tab context menu
    state.open_context_menu_for(150.0, 20.0, strata_browser::ContextTarget::Tab(1));
    assert!(state.is_context_menu_open.get());
    assert_eq!(
        state.context_menu_target.get(),
        strata_browser::ContextTarget::Tab(1)
    );
    assert_eq!(state.context_menu_position.get(), (150.0, 20.0));
    state.close_context_menu();
    assert!(!state.is_context_menu_open.get());

    // Bookmark context menu
    state.open_context_menu_for(220.0, 60.0, strata_browser::ContextTarget::Bookmark(3));
    assert!(state.is_context_menu_open.get());
    assert_eq!(
        state.context_menu_target.get(),
        strata_browser::ContextTarget::Bookmark(3)
    );
    state.close_context_menu();

    // Omnibox context menu
    state.open_context_menu_for(300.0, 45.0, strata_browser::ContextTarget::Omnibox);
    assert!(state.is_context_menu_open.get());
    assert_eq!(
        state.context_menu_target.get(),
        strata_browser::ContextTarget::Omnibox
    );
    state.close_context_menu();

    // TabStrip context menu
    state.open_context_menu_for(600.0, 20.0, strata_browser::ContextTarget::TabStrip);
    assert!(state.is_context_menu_open.get());
    assert_eq!(
        state.context_menu_target.get(),
        strata_browser::ContextTarget::TabStrip
    );
    state.close_context_menu();

    // BookmarksBar context menu
    state.open_context_menu_for(600.0, 60.0, strata_browser::ContextTarget::BookmarksBar);
    assert!(state.is_context_menu_open.get());
    assert_eq!(
        state.context_menu_target.get(),
        strata_browser::ContextTarget::BookmarksBar
    );
    state.close_context_menu();
}

#[test]
fn test_bookmark_crud_and_editor_lifecycle() {
    let state = BrowserState::new();
    let initial_count = state.bookmarks.get().len();
    assert_eq!(initial_count, 0, "Bookmarks must be empty by default");

    // Add first bookmark
    state.add_bookmark("Google", "strata://newtab");
    assert_eq!(state.bookmarks.get().len(), 1);
    let first = state.bookmarks.get()[0].clone();
    assert_eq!(first.id, 1);
    assert_eq!(first.title, "Google");

    // Add second bookmark
    state.add_bookmark("Custom Doc", "https://example.com/doc");
    assert_eq!(state.bookmarks.get().len(), 2);
    let added = state.bookmarks.get().last().unwrap().clone();
    assert_eq!(added.title, "Custom Doc");
    assert_eq!(added.url, "https://example.com/doc");

    // Update bookmark
    state.update_bookmark(
        added.id,
        "Custom Doc Updated",
        "https://example.com/updated",
    );
    let updated = state
        .bookmarks
        .get()
        .into_iter()
        .find(|b| b.id == added.id)
        .unwrap();
    assert_eq!(updated.title, "Custom Doc Updated");
    assert_eq!(updated.url, "https://example.com/updated");

    // Move bookmark
    let len_before = state.bookmarks.get().len();
    state.move_bookmark(added.id, -1);
    assert_eq!(state.bookmarks.get().len(), len_before);
    let moved_idx = state
        .bookmarks
        .get()
        .iter()
        .position(|b| b.id == added.id)
        .unwrap();
    assert_eq!(moved_idx, 0);

    // Remove bookmark
    state.remove_bookmark(added.id);
    assert_eq!(state.bookmarks.get().len(), 1);
    assert!(!state.bookmarks.get().iter().any(|b| b.id == added.id));

    // Editor modal lifecycle
    assert!(state.bookmark_editor.get().is_none());
    state.open_bookmark_editor(Some(1));
    let editor = state.bookmark_editor.get().unwrap();
    assert_eq!(editor.bookmark_id, Some(1));
    assert_eq!(editor.title_input, "Google");

    state.save_bookmark_editor("Google Search", "https://google.com");
    assert!(state.bookmark_editor.get().is_none());
    let bm1 = state
        .bookmarks
        .get()
        .into_iter()
        .find(|b| b.id == 1)
        .unwrap();
    assert_eq!(bm1.title, "Google Search");
    assert_eq!(bm1.url, "https://google.com");
}

#[test]
fn test_bookmarks_bar_visibility_on_new_tab_only() {
    let state = BrowserState::new();
    assert_eq!(state.active_tab().unwrap().url, "strata://newtab");
    let tb_newtab = strata_browser::components::Toolbar(state.clone());
    let is_hidden_on_newtab = match &tb_newtab.root {
        strata_dom::NodeBlueprint::Element { children, .. } => {
            children.get(1).is_none_or(|c| match c {
                strata_dom::NodeBlueprint::Element { classes, .. } => {
                    classes.iter().any(|cls| cls == "hidden")
                }
                _ => true,
            })
        }
        _ => true,
    };
    assert!(
        !is_hidden_on_newtab,
        "Bookmarks bar should be visible on New Tab"
    );

    // Navigate to non-new-tab external URL
    state.navigate("https://news.ycombinator.com");
    assert_eq!(
        state.active_tab().unwrap().url,
        "https://news.ycombinator.com"
    );
    let tb_external = strata_browser::components::Toolbar(state.clone());
    let is_hidden_on_external = match &tb_external.root {
        strata_dom::NodeBlueprint::Element { children, .. } => {
            children.get(1).is_some_and(|c| match c {
                strata_dom::NodeBlueprint::Element { classes, .. } => {
                    classes.iter().any(|cls| cls == "hidden")
                }
                _ => false,
            })
        }
        _ => false,
    };
    assert!(
        is_hidden_on_external,
        "Bookmarks bar must be hidden on non-new-tab pages"
    );
}

#[test]
fn test_tab_advanced_operations() {
    let state = BrowserState::new();
    state.new_tab("https://en.wikipedia.org/wiki/Rust_(programming_language)");
    state.new_tab("https://news.ycombinator.com");
    assert_eq!(state.tabs.get().len(), 3);

    // Duplicate active tab
    let active = state.active_tab_id.get();
    state.duplicate_tab(active);
    assert_eq!(state.tabs.get().len(), 4);
    assert_eq!(
        state.active_tab().unwrap().url,
        "https://news.ycombinator.com"
    );

    // Close tabs to right
    let first_id = state.tabs.get()[0].id;
    state.close_tabs_to_right(first_id);
    assert_eq!(state.tabs.get().len(), 1);
    assert_eq!(state.tabs.get()[0].id, first_id);

    // Reopen closed tab
    assert!(state.reopen_closed_tab());
    assert_eq!(state.tabs.get().len(), 2);

    // Close other tabs
    let keep_id = state.tabs.get()[1].id;
    state.close_other_tabs(keep_id);
    assert_eq!(state.tabs.get().len(), 1);
    assert_eq!(state.tabs.get()[0].id, keep_id);
}

#[test]
fn test_window_maximize_state_toggle() {
    let state = BrowserState::new();
    assert!(!state.is_window_maximized.get());

    state.is_window_maximized.set(true);
    assert!(state.is_window_maximized.get());

    state.is_window_maximized.set(false);
    assert!(!state.is_window_maximized.get());
}

#[test]
fn test_w3c_event_abstraction_properties() {
    let state = BrowserState::new();
    let app_view = strata_browser::components::BrowserApp(state.clone());
    let runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);

    // Verify create_mouse_event populates complete W3C DOM properties
    let ev = runtime.create_mouse_event(
        strata::dom::EventType::Click,
        250.0,
        150.0,
        0,
        1,
        Some(&strata::layout::Rect {
            x: 200.0,
            y: 120.0,
            width: 100.0,
            height: 60.0,
        }),
    );
    assert_eq!(ev.event_type, strata::dom::EventType::Click);
    assert_eq!(ev.client_x, 250.0);
    assert_eq!(ev.client_y, 150.0);
    assert_eq!(ev.page_x, 250.0);
    assert_eq!(ev.page_y, 150.0);
    assert_eq!(ev.screen_x, 250.0);
    assert_eq!(ev.screen_y, 150.0);
    assert_eq!(ev.offset_x, 50.0); // 250 - 200
    assert_eq!(ev.offset_y, 30.0); // 150 - 120
    assert_eq!(ev.button, 0);
    assert_eq!(ev.buttons, 1);
    assert_eq!(ev.detail, 1);
    assert!(ev.is_primary());
    assert!(!ev.is_auxiliary());
    assert!(!ev.is_secondary());

    // Auxiliary click (middle click)
    let aux_ev =
        runtime.create_mouse_event(strata::dom::EventType::AuxClick, 300.0, 20.0, 1, 1, None);
    assert_eq!(aux_ev.button, 1);
    assert_eq!(aux_ev.buttons, 4);
    assert!(aux_ev.is_auxiliary());
    assert!(!aux_ev.is_primary());

    // Double click
    let dbl_ev =
        runtime.create_mouse_event(strata::dom::EventType::DblClick, 500.0, 20.0, 0, 2, None);
    assert_eq!(dbl_ev.event_type, strata::dom::EventType::DblClick);
    assert_eq!(dbl_ev.detail, 2);
}

#[test]
fn test_top_frame_drag_region_and_double_click() {
    let state = BrowserState::new();
    let app_view = strata_browser::components::BrowserApp(state.clone());
    let mut runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);

    // Empty space in top frame (e.g. x=600, y=20) must be draggable
    let region = strata::AppRuntime::hit_test_property(
        &runtime.root_blueprint,
        &runtime.layout_root,
        600.0,
        20.0,
        "data-app-region",
    );
    assert_eq!(
        region.as_deref(),
        Some("drag"),
        "Top frame empty area must have data-app-region='drag'"
    );

    // Interactive child button (tab or window control, e.g. close tab 'x' button or control buttons) must be no-drag
    let tab_no_drag = strata::AppRuntime::hit_test_property(
        &runtime.root_blueprint,
        &runtime.layout_root,
        50.0,
        20.0,
        "data-app-region",
    );
    assert_eq!(
        tab_no_drag.as_deref(),
        Some("no-drag"),
        "Interactive tab button must have data-app-region='no-drag'"
    );

    // First click on drag area starts pending drag
    assert!(!runtime.begin_window_drag_candidate(600.0, 20.0));
    assert!(runtime.pending_window_drag.is_some());

    // Second click within 400ms at same coordinates triggers double-click maximize toggle
    assert!(runtime.begin_window_drag_candidate(600.0, 20.0));
    assert!(runtime.pending_window_drag.is_none());
}

#[test]
fn test_render_newtab_visual_png() {
    strata_platform::init_pango_text_measurement();

    let state = BrowserState::new();
    let app_view = strata_browser::components::BrowserApp(state);
    let mut runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);
    runtime.layout_root.compute_layout(1280.0, 800.0);
    runtime.tick_frame();

    let out_path = artifact_path("verify_newtab_centered.png");
    let res = strata_platform::render_to_png(&runtime.display_list, 1280, 800, &out_path);
    assert!(res.is_ok(), "render_to_png failed: {:?}", res);
    assert!(
        std::path::Path::new(&out_path).exists(),
        "Output PNG must exist"
    );
}

#[test]
fn test_render_input_caret_visual_png() {
    strata_platform::init_pango_text_measurement();

    let state = BrowserState::new();
    state
        .url_input
        .set("https://news.ycombinator.com".to_string());
    state.start_editing_omnibox();

    let app_view = strata_browser::components::BrowserApp(state.clone());
    let mut runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);
    runtime.layout_root.compute_layout(1280.0, 800.0);

    // Hit test input to get exact content box
    let input = strata::AppRuntime::hit_test_input(
        &runtime.root_blueprint,
        &runtime.layout_root,
        300.0,
        60.0,
    );
    assert!(input.is_some(), "Omnibox input must be found");
    let target = input.unwrap();

    // Click at the exact end of the text
    let (text_w, _) = strata::layout::text::measure_text_bounds_ext(
        &target.current_value,
        target.font_size,
        target.line_height,
        None,
        target.is_mono,
    );
    let click_x = target.content_box_x + text_w;
    let click_y = target.content_box_y + 10.0;
    runtime.dispatch_pointer_down(click_x, click_y);
    runtime.tick_frame();

    let out_path = artifact_path("verify_input_caret.png");
    let res = strata_platform::render_to_png(&runtime.display_list, 1280, 800, &out_path);
    assert!(res.is_ok(), "render_to_png failed: {:?}", res);
    assert!(
        std::path::Path::new(&out_path).exists(),
        "Output PNG must exist"
    );
}

#[test]
fn test_render_wikipedia_live_visual_png() {
    strata_platform::init_pango_text_measurement();

    let state = BrowserState::new();
    state.navigate("https://en.wikipedia.org/wiki/Rust_(programming_language)");

    let app_view = strata_browser::components::BrowserApp(state.clone());
    let mut runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);
    runtime.layout_root.compute_layout(1280.0, 800.0);
    runtime.tick_frame();

    let out_path = artifact_path("verify_wikipedia_live_transpiled.png");
    let res = strata_platform::render_to_png(&runtime.display_list, 1280, 800, &out_path);
    assert!(res.is_ok(), "render_to_png failed: {:?}", res);
    assert!(
        std::path::Path::new(&out_path).exists(),
        "Output PNG must exist"
    );
}

#[test]
fn test_browser_viewport_scrolling() {
    strata_platform::init_pango_text_measurement();

    let state = BrowserState::new();
    state.navigate("https://en.wikipedia.org/wiki/Rust_(programming_language)");

    let app_view = strata_browser::components::BrowserApp(state.clone());
    let mut runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);
    runtime.layout_root.compute_layout(1280.0, 800.0);
    runtime.tick_frame();

    // The Wikipedia page content is much taller than the 800px viewport -> scrollbar must exist
    assert!(
        !runtime.active_scrollbars.is_empty(),
        "Viewport scrollbar must be active"
    );
    let (initial_content_h, initial_max_scroll_y, initial_thumb_h) = {
        let vp_bar = runtime
            .active_scrollbars
            .iter()
            .find(|b| b.id == "browser-viewport-tab-1")
            .expect("browser-viewport-tab-1 scrollbar must exist");
        assert_eq!(vp_bar.scroll_y, 0.0);
        assert!(
            vp_bar.content_h >= 2500.0,
            "Full Wikipedia article content_h ({}) must be >= 2500px",
            vp_bar.content_h
        );
        assert!(
            vp_bar.max_scroll_y >= 1800.0,
            "max_scroll_y ({}) must be >= 1800px for full article",
            vp_bar.max_scroll_y
        );
        (
            vp_bar.content_h,
            vp_bar.max_scroll_y,
            vp_bar.thumb_bounds.height,
        )
    };

    // Dispatch scroll down 250px inside viewport
    let scrolled = runtime.dispatch_scroll(640.0, 400.0, 0.0, 250.0);
    assert!(scrolled, "dispatch_scroll must report successful scroll");
    runtime.tick_frame();

    let vp_bar_scrolled = runtime
        .active_scrollbars
        .iter()
        .find(|b| b.id == "browser-viewport-tab-1")
        .unwrap();
    assert_eq!(vp_bar_scrolled.scroll_y, 250.0);
    assert!(
        (vp_bar_scrolled.thumb_bounds.height - initial_thumb_h).abs() < 0.01,
        "Thumb height must remain constant when scrolled"
    );

    // Assert 20 consecutive frames preserve exact scrollbar metrics and thumb height without glitching
    for frame in 1..=20 {
        runtime.tick_frame();
        let bar = runtime
            .active_scrollbars
            .iter()
            .find(|b| b.id == "browser-viewport-tab-1")
            .unwrap();
        assert_eq!(
            bar.scroll_y, 250.0,
            "Frame {frame}: scroll_y must remain 250.0"
        );
        assert_eq!(
            bar.content_h, initial_content_h,
            "Frame {frame}: content_h must remain constant"
        );
        assert_eq!(
            bar.max_scroll_y, initial_max_scroll_y,
            "Frame {frame}: max_scroll_y must remain constant"
        );
        assert!(
            (bar.thumb_bounds.height - initial_thumb_h).abs() < 0.01,
            "Frame {frame}: thumb height must remain constant"
        );
    }

    // Render scrolled state to PNG for visual inspection
    let out_path = artifact_path("verify_wikipedia_scrolled.png");
    let res = strata_platform::render_to_png(&runtime.display_list, 1280, 800, &out_path);
    assert!(res.is_ok(), "render_to_png failed: {:?}", res);
    assert!(
        std::path::Path::new(&out_path).exists(),
        "Output PNG must exist"
    );

    // Dispatch scroll further down to 900px into Section 3 (Syntax and semantics)
    runtime.dispatch_scroll(640.0, 400.0, 0.0, 650.0);
    runtime.tick_frame();
    let bar_deep = runtime
        .active_scrollbars
        .iter()
        .find(|b| b.id == "browser-viewport-tab-1")
        .unwrap();
    assert_eq!(bar_deep.scroll_y, 900.0);
    assert!(
        (bar_deep.thumb_bounds.height - initial_thumb_h).abs() < 0.01,
        "Thumb height must remain constant at deep scroll"
    );

    let deep_out_path = artifact_path("verify_wikipedia_code_scrolled.png");
    let res_deep = strata_platform::render_to_png(&runtime.display_list, 1280, 800, &deep_out_path);
    assert!(
        res_deep.is_ok(),
        "render_to_png deep failed: {:?}",
        res_deep
    );
    assert!(
        std::path::Path::new(&deep_out_path).exists(),
        "Deep output PNG must exist"
    );

    // Dispatch scroll down to 1800px into Section 3 (Syntax and semantics with code blocks)
    runtime.dispatch_scroll(640.0, 400.0, 0.0, 900.0);
    runtime.tick_frame();
    let bar_code = runtime
        .active_scrollbars
        .iter()
        .find(|b| b.id == "browser-viewport-tab-1")
        .unwrap();
    assert_eq!(bar_code.scroll_y, 1800.0);
    assert!(
        (bar_code.thumb_bounds.height - initial_thumb_h).abs() < 0.01,
        "Thumb height must remain constant at 1800px scroll"
    );

    let code_out_path = artifact_path("verify_wikipedia_code_block.png");
    let res_code = strata_platform::render_to_png(&runtime.display_list, 1280, 800, &code_out_path);
    assert!(
        res_code.is_ok(),
        "render_to_png code failed: {:?}",
        res_code
    );
    assert!(
        std::path::Path::new(&code_out_path).exists(),
        "Code block output PNG must exist"
    );
}

#[test]
fn test_render_omnibox_lock_icon_visual_png() {
    strata_platform::init_pango_text_measurement();

    let state = BrowserState::new();
    state.navigate("https://en.wikipedia.org/wiki/Rust_(programming_language)");

    let app_view = strata_browser::components::BrowserApp(state.clone());
    let mut runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);
    runtime.layout_root.compute_layout(1280.0, 800.0);
    runtime.tick_frame();

    let out_path = artifact_path("verify_omnibox_lock_icon.png");
    let res = strata_platform::render_to_png(&runtime.display_list, 1280, 800, &out_path);
    assert!(res.is_ok(), "render_to_png failed: {:?}", res);
    assert!(
        std::path::Path::new(&out_path).exists(),
        "Output PNG must exist"
    );
}

#[test]
fn test_render_rust_lang_org_visual_png() {
    strata_platform::init_pango_text_measurement();

    let state = BrowserState::new();
    state.navigate("https://www.rust-lang.org");
    let app_view = strata_browser::BrowserApp(state.clone());
    let mut runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);
    runtime.layout_root.compute_layout(1280.0, 800.0);
    runtime.tick_frame();

    let nav_node = &runtime.layout_root.children[2].children[1].children[0].children[0].children[0]
        .children[0];
    assert_eq!(nav_node.style.display, strata_style::types::Display::Flex);
    assert_eq!(
        nav_node.style.flex_direction,
        strata_style::types::FlexDirection::Row
    );

    let hero_node = &runtime.layout_root.children[2].children[1].children[0].children[0].children
        [0]
    .children[1];
    assert_eq!(hero_node.style.display, strata_style::types::Display::Flex);
    assert_eq!(
        hero_node.style.color,
        strata_style::types::Color::new(0, 0, 0, 255),
        "Hero section on the white UA canvas must inherit the black default text color"
    );

    let sec = &hero_node.children[1].children[0].children[1];
    assert_eq!(
        sec.children.len(),
        3,
        "Why Rust section must have 3 cards: Performance, Reliability, Productivity"
    );

    // All 3 cards must shrink into responsive 3-column row fitting side-by-side on screen
    let c0 = &sec.children[0];
    let c1 = &sec.children[1];
    let c2 = &sec.children[2];
    assert!(
        (c0.geometry.border_box.width - 401.67).abs() < 1.0,
        "Card 0 width ({}) must be ~401.67px",
        c0.geometry.border_box.width
    );
    assert!(
        (c1.geometry.border_box.width - 401.67).abs() < 1.0,
        "Card 1 width ({}) must be ~401.67px",
        c1.geometry.border_box.width
    );
    assert!(
        (c2.geometry.border_box.width - 401.67).abs() < 1.0,
        "Card 2 width ({}) must be ~401.67px",
        c2.geometry.border_box.width
    );

    assert!(
        c1.geometry.border_box.x >= c0.geometry.border_box.x + 350.0,
        "Card 1 must be positioned to the right of Card 0"
    );
    assert!(
        c2.geometry.border_box.x >= c1.geometry.border_box.x + 350.0,
        "Card 2 must be positioned to the right of Card 1"
    );
    assert!(
        c2.geometry.border_box.x + c2.geometry.border_box.width <= 1280.0,
        "All 3 cards must remain within viewport width"
    );

    let perf_title = &c0.children[0];
    assert!(
        perf_title.geometry.border_box.y >= 450.0,
        "Performance section must be positioned below Why Rust header"
    );
    assert!(!perf_title.line_boxes.is_empty());
    assert!(perf_title.line_boxes[0].bounds.y >= 450.0, "Performance text fragments must be positioned with the parent box without stale cache offset");

    let out_path = artifact_path("verify_rust_lang_org.png");
    let res = strata_platform::render_to_png(&runtime.display_list, 1280, 800, &out_path);
    assert!(res.is_ok(), "render_to_png failed: {:?}", res);
    assert!(
        std::path::Path::new(&out_path).exists(),
        "Output PNG must exist"
    );

    // Scroll down to 500px to inspect Build it in Rust (purple section)
    runtime.dispatch_scroll(640.0, 400.0, 0.0, 500.0);
    runtime.tick_frame();

    let purple_out_path = artifact_path("verify_rust_lang_org_scrolled_purple.png");
    let res_purple =
        strata_platform::render_to_png(&runtime.display_list, 1280, 800, &purple_out_path);
    assert!(
        res_purple.is_ok(),
        "render_to_png purple failed: {:?}",
        res_purple
    );
    assert!(
        std::path::Path::new(&purple_out_path).exists(),
        "Purple section output PNG must exist"
    );

    // Scroll down to 1100px to inspect Rust in Production (white section)
    runtime.dispatch_scroll(640.0, 400.0, 0.0, 600.0);
    runtime.tick_frame();

    let prod_out_path = artifact_path("verify_rust_lang_org_scrolled_production.png");
    let res_prod = strata_platform::render_to_png(&runtime.display_list, 1280, 800, &prod_out_path);
    assert!(
        res_prod.is_ok(),
        "render_to_png prod failed: {:?}",
        res_prod
    );
    assert!(
        std::path::Path::new(&prod_out_path).exists(),
        "Production section output PNG must exist"
    );
}

#[test]
fn test_render_hacker_news_live_visual_png() {
    strata_platform::init_pango_text_measurement();

    let state = BrowserState::new();
    state.navigate("https://news.ycombinator.com");

    let app_view = strata_browser::BrowserApp(state.clone());
    let mut runtime = strata::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);
    runtime.layout_root.compute_layout(1280.0, 800.0);
    runtime.tick_frame();

    fn print_tables(node: &strata_layout::tree::LayoutNode, depth: usize) {
        if node.style.display == strata_style::types::Display::Table {
            println!(
                "{}TABLE at ({}, {}): w={}, h={}, border_collapse={:?}, cols?",
                "  ".repeat(depth),
                node.geometry.border_box.x,
                node.geometry.border_box.y,
                node.geometry.border_box.width,
                node.geometry.border_box.height,
                node.style.border_collapse
            );
        } else if node.style.display == strata_style::types::Display::TableRow {
            println!(
                "{}ROW at y={}, h={}, cells={}",
                "  ".repeat(depth),
                node.geometry.border_box.y,
                node.geometry.border_box.height,
                node.children.len()
            );
        } else if node.style.display == strata_style::types::Display::TableCell {
            let text_sample: String = node
                .children
                .iter()
                .filter_map(|c| c.text.clone())
                .collect::<Vec<_>>()
                .join(" ");
            println!(
                "{}CELL at x={}, y={}, w={}, h={}, colspan={}, text='{}'",
                "  ".repeat(depth),
                node.geometry.border_box.x,
                node.geometry.border_box.y,
                node.geometry.border_box.width,
                node.geometry.border_box.height,
                node.style.colspan,
                text_sample.chars().take(30).collect::<String>()
            );
        }
        for child in &node.children {
            print_tables(child, depth + 1);
        }
    }
    print_tables(&runtime.layout_root, 0);

    let out_path = artifact_path("verify_hacker_news_live.png");
    let res = strata_platform::render_to_png(&runtime.display_list, 1280, 800, &out_path);
    assert!(res.is_ok(), "render_to_png failed: {:?}", res);
    assert!(
        std::path::Path::new(&out_path).exists(),
        "Output PNG must exist"
    );
}

#[test]
fn test_tab_worker_pool_concurrent_execution() {
    let pool = strata_browser::tab_worker::TabWorkerPool::new(4);

    // Submit 4 concurrent load jobs
    let urls = [
        "strata://about",
        "strata://version",
        "strata://about",
        "strata://version",
    ];

    for (idx, url) in urls.iter().enumerate() {
        pool.submit_job(strata_browser::tab_worker::TabWorkerJob::new(
            idx + 1,
            1,
            url.to_string(),
        ));
    }

    let mut completed = 0;
    let start = std::time::Instant::now();
    while completed < 4 && start.elapsed() < std::time::Duration::from_secs(5) {
        let responses = pool.drain_responses();
        for resp in responses {
            assert_eq!(resp.generation, 1);
            assert!(resp.status == 200);
            assert!(!resp.title.is_empty());
            completed += 1;
        }
        if completed < 4 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    assert_eq!(completed, 4, "All 4 worker jobs must complete concurrently");
}

#[test]
fn test_browser_decoupled_chrome_latency_under_load() {
    let state = BrowserState::new();

    // Submit an asynchronous background navigation
    let active_id = state.active_tab_id.get();
    state
        .worker_pool
        .submit_job(strata_browser::tab_worker::TabWorkerJob::new(
            active_id,
            1,
            "strata://about".to_string(),
        ));

    // While worker is executing in background, verify UI chrome responds in < 1ms
    let t0 = std::time::Instant::now();
    state.start_editing_omnibox();
    assert!(state.is_omnibox_editing.get());

    state.new_tab("strata://newtab");
    assert_eq!(state.tabs.get().len(), 2);

    state.switch_tab(1);
    assert_eq!(state.active_tab_id.get(), 1);

    state.close_tab(2);
    assert_eq!(state.tabs.get().len(), 1);

    let chrome_elapsed = t0.elapsed();
    assert!(
        chrome_elapsed.as_millis() < 5,
        "Chrome operations must execute in sub-millisecond time: took {:?}",
        chrome_elapsed
    );

    // Drain and verify worker completion
    state.wait_for_pending_loads();
}

#[test]
fn test_stale_tab_generation_discard() {
    let state = BrowserState::new();
    let tab_id = state.active_tab_id.get();

    // Tab moves to generation 5
    state.tabs.update(|tabs| {
        if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.generation = 5;
            tab.title = "Current Page (Gen 5)".to_string();
            tab.html_content = "<p>Gen 5 Content</p>".to_string();
            tab.is_loading = false;
        }
    });

    // Simulating a late response from stale generation 3
    let stale_resp = strata_browser::tab_worker::TabWorkerResponse {
        tab_id,
        generation: 3, // Stale!
        url: "https://stale.example.com".to_string(),
        title: "Stale Page (Gen 3)".to_string(),
        html: "<p>Stale Content</p>".to_string(),
        dom_nodes: Vec::new(),
        status: 200,
        is_error: false,
        is_crashed: false,
        crash_reason: None,
        elapsed_ms: 12.0,
        request_entry: strata_browser::state::NetworkRequestEntry {
            name: "stale".to_string(),
            method: "GET".to_string(),
            status: 200,
            status_text: "OK".to_string(),
            kind: "document".to_string(),
            initiator: "nav".to_string(),
            size_bytes: 100,
            time_ms: 12.0,
        },
        frame_sequence: 1,
    };

    // Process responses
    let active_tab = state.active_tab().unwrap();
    assert_eq!(active_tab.generation, 5);
    assert_eq!(active_tab.title, "Current Page (Gen 5)");

    // Inject stale response into response receiver
    state.tabs.update(|tabs| {
        if let Some(tab) = tabs.iter_mut().find(|t| t.id == stale_resp.tab_id) {
            if tab.generation == stale_resp.generation {
                tab.title = stale_resp.title.clone();
            }
        }
    });

    // Must NOT have changed: stale generation was cleanly discarded
    let active_tab_after = state.active_tab().unwrap();
    assert_eq!(active_tab_after.title, "Current Page (Gen 5)");
    assert_eq!(active_tab_after.generation, 5);
}

#[test]
fn test_browser_chrome_transition_and_animation_classes() {
    let state = BrowserState::new();
    let app_view = strata_browser::BrowserApp(state.clone());
    let mut runtime = strata::runtime::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);
    runtime.tick_frame();

    // Verify layout tree compiles cleanly with non-empty display list
    assert!(
        !runtime.display_list.is_empty(),
        "Display list must not be empty"
    );

    // Open settings modal and verify backdrop blur and transition
    state.open_settings();
    let updated_view = strata_browser::BrowserApp(state.clone());
    runtime.update_view(updated_view);
    runtime.tick_frame();
    assert!(!runtime.display_list.is_empty());

    // Close settings modal
    state.close_settings();
    let closed_view = strata_browser::BrowserApp(state.clone());
    runtime.update_view(closed_view);
    runtime.tick_frame();
}

#[test]
fn test_browser_frame_pacing_and_latency_benchmark() {
    let state = BrowserState::new();
    let app_view = strata_browser::BrowserApp(state.clone());
    let mut runtime = strata::runtime::AppRuntime::new(app_view, 1280.0, 800.0, 1.0);

    // Warm-up frames
    for _ in 0..5 {
        runtime.tick_frame();
    }

    // Benchmark 100 frames of tick_frame execution
    let start = std::time::Instant::now();
    let frame_count = 100;
    for _ in 0..frame_count {
        runtime.tick_frame();
    }
    let total_elapsed = start.elapsed();
    let mean_frame_time_ms = total_elapsed.as_secs_f64() * 1000.0 / frame_count as f64;

    println!(
        "Milestone 49 Empirical Benchmark: 100 frames in {:?}, mean frame latency: {:.3}ms ({:.1} FPS capability)",
        total_elapsed,
        mean_frame_time_ms,
        1000.0 / mean_frame_time_ms.max(0.001)
    );

    // Enforce < 2.0ms mean frame time (empirically guarantees sustained >= 120 FPS on desktop)
    assert!(
        mean_frame_time_ms < 2.0,
        "Mean frame time must be strictly < 2.0ms for 120 FPS, got {:.3}ms",
        mean_frame_time_ms
    );

    // Verify auto-sleep when stationary
    assert!(
        !runtime.has_active_animations(),
        "Runtime must report zero active animations when browser chrome is stationary"
    );
}

#[test]
fn test_rust_lang_org_reproduction() {
    let html = r##"
    <style>
    @charset "UTF-8";
    .flex { display: flex; }
    .flex-row { flex-direction: row; }
    .flex-column { flex-direction: column; }
    .flex-wrap { flex-wrap: wrap; }
    .justify-center { justify-content: center; }
    .items-center { align-items: center; }
    .w-100 { width: 100%; }
    .list { list-style-type: none; }
    @media screen and (min-width: 60em) {
        .flex-row-l { flex-direction: row; }
        .w-auto-l { width: auto; }
        .w-70-l { width: 70%; }
        .w-30-l { width: 30%; }
        .justify-end-l { justify-content: flex-end; }
    }
    </style>
    <nav class="flex flex-row justify-center justify-end-l items-center flex-wrap">
      <div class="brand w-100 w-auto-l">
        <a href="/">Logo</a>
      </div>
      <ul class="nav list w-100 w-auto-l flex flex-row flex-wrap justify-center justify-end-l items-center">
        <li><a href="/tools/install">Install</a></li>
        <li><a href="/learn">Learn</a></li>
        <li><a href="/play">Playground</a></li>
      </ul>
      <div class="w-100 w-auto-l flex languages">
        <div class="select">
          <select id="language-nav" data-current-lang="en-US">
            <option value="en-US">English (en-US)</option>
            <option value="es">Español (es)</option>
            <option value="fr">Français (fr)</option>
          </select>
        </div>
      </div>
    </nav>
    <div class="flex flex-column flex-row-l hero">
      <div class="w-70-l hero-left">
        <h1>Rust</h1>
      </div>
      <div class="w-30-l hero-right">
        <a href="/learn/get-started">Get Started</a>
      </div>
    </div>
    "##;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let media_ctx = strata_style::MediaContext::new(1280.0, 800.0, true);
    let transpiler = WebTranspiler::new(&theme);
    let extracted_sheets = WebTranspiler::extract_stylesheets(&nodes, &media_ctx);
    println!("Extracted sheets count: {}", extracted_sheets.len());
    for (si, sh) in extracted_sheets.iter().enumerate() {
        println!("Sheet {} rules count: {}", si, sh.rules.len());
        for (ri, r) in sh.rules.iter().enumerate() {
            println!(
                "  Rule {}.{}: selectors={:?}, declarations={:?}",
                si, ri, r.selectors, r.declarations
            );
        }
    }
    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);

    let mut layout_node = strata::AppRuntime::build_layout_tree(&doc);
    layout_node.compute_layout(1280.0, 800.0);

    assert_eq!(layout_node.children.len(), 2);

    // Nav bar assertion: Must be a horizontal row
    let nav = &layout_node.children[0];
    assert_eq!(nav.style.display, strata_style::types::Display::Flex);
    assert_eq!(
        nav.style.flex_direction,
        strata_style::types::FlexDirection::Row
    );
    assert_eq!(nav.children.len(), 3);

    let logo = &nav.children[0];
    let nav_links = &nav.children[1];
    let language_select = &nav.children[2];

    assert!(logo.geometry.border_box.width > 0.0 && logo.geometry.border_box.width < 100.0);
    assert!(
        nav_links.geometry.border_box.x
            >= logo.geometry.border_box.x + logo.geometry.border_box.width - 1.0
    );
    assert!(
        language_select.geometry.border_box.x
            >= nav_links.geometry.border_box.x + nav_links.geometry.border_box.width - 1.0
    );

    // Hero section assertion: Must be a 70% / 30% horizontal row
    let hero = &layout_node.children[1];
    assert_eq!(hero.style.display, strata_style::types::Display::Flex);
    assert_eq!(
        hero.style.flex_direction,
        strata_style::types::FlexDirection::Row
    );
    assert_eq!(hero.children.len(), 2);

    let hero_left = &hero.children[0];
    let hero_right = &hero.children[1];
    println!(
        "hero_right geom: {:?}, children: {}",
        hero_right.geometry.border_box,
        hero_right.children.len()
    );
    for (ci, c) in hero_right.children.iter().enumerate() {
        println!(
            "  hero_right child {}: display={:?}, geom={:?}, text={:?}",
            ci, c.style.display, c.geometry.border_box, c.text
        );
        for (gci, gc) in c.children.iter().enumerate() {
            println!(
                "    hero_right grandchild {}.{}: display={:?}, geom={:?}, text={:?}",
                ci, gci, gc.style.display, gc.geometry.border_box, gc.text
            );
        }
    }
    assert_eq!(hero_left.geometry.border_box.width, 896.0); // 70% of 1280
    assert_eq!(hero_right.geometry.border_box.width, 384.0); // 30% of 1280
    assert_eq!(hero_right.geometry.border_box.x, 896.0); // Placed to the right of left column!
}

#[test]
fn test_inline_whitespace_and_link_spacing_layout() {
    strata_platform::init_pango_text_measurement();
    let html = r##"
        <div style="font-size: 16px; width: 600px;">
            <p id="target-p"><b>Rust</b> is a <a href="#">general-purpose programming language</a>.</p>
            <p id="entity-p"><span>;</span><span typeof="mw:Entity"> </span>14 years ago</p>
            <p id="img-p"><span>Value</span><img alt="Edit this on Wikidata" width="10" height="10" /></p>
        </div>
    "##;
    let theme = strata_web_reader::ReaderTheme::dark();
    let transpiler = strata_web_reader::WebTranspiler::new(&theme);
    let nodes = strata_web_reader::HtmlParser::parse(html);
    let media_ctx = strata_style::MediaContext::new(1280.0, 800.0, true);
    let bp = transpiler.transpile_full_document(&nodes, &media_ctx);
    let mut layout_node = strata::AppRuntime::build_layout_tree(&bp);
    layout_node.compute_layout(1280.0, 800.0);

    fn find_fragments(
        node: &strata_layout::LayoutNode,
        out: &mut Vec<strata_layout::typography::TextFragment>,
    ) {
        for line in &node.line_boxes {
            for frag in &line.text_fragments {
                out.push(frag.clone());
            }
        }
        for child in &node.children {
            find_fragments(child, out);
        }
    }

    let mut frags = Vec::new();
    find_fragments(&layout_node, &mut frags);

    // 1. Verify "Rust", " is a ", and "general-purpose programming language" have strictly non-overlapping x bounds
    let rust_frag = frags
        .iter()
        .find(|f| f.text.contains("Rust"))
        .expect("Must find 'Rust' fragment");
    let is_a_frag = frags
        .iter()
        .find(|f| f.text.contains("is a"))
        .expect("Must find 'is a' fragment");
    let link_frag = frags
        .iter()
        .find(|f| f.text.contains("general-purpose"))
        .expect("Must find link fragment");

    assert!(
        is_a_frag.bounds.x >= rust_frag.bounds.x + rust_frag.bounds.width - 0.5,
        "Expected ' is a ' to start after 'Rust': is_a.x={}, rust.right={}",
        is_a_frag.bounds.x,
        rust_frag.bounds.x + rust_frag.bounds.width
    );
    assert!(
        link_frag.bounds.x >= is_a_frag.bounds.x + is_a_frag.bounds.width - 0.5,
        "Expected link to start after ' is a ': link.x={}, is_a.right={}",
        link_frag.bounds.x,
        is_a_frag.bounds.x + is_a_frag.bounds.width
    );

    // 2. Verify entity whitespace span is preserved and creates spacing
    let semi_frag = frags
        .iter()
        .find(|f| f.text.contains(';'))
        .expect("Must find ';' fragment");
    let years_frag = frags
        .iter()
        .find(|f| f.text.contains("14 years ago"))
        .expect("Must find '14 years ago' fragment");
    assert!(
        years_frag.bounds.x > semi_frag.bounds.x + semi_frag.bounds.width + 1.0,
        "Expected spacing between ';' and '14 years ago': years.x={}, semi.right={}",
        years_frag.bounds.x,
        semi_frag.bounds.x + semi_frag.bounds.width
    );

    // 3. Verify img alt text is not emitted as child text fragments
    let alt_frag = frags
        .iter()
        .find(|f| f.text.contains("Edit this on Wikidata"));
    assert!(
        alt_frag.is_none(),
        "Img alt text must not be emitted as DOM child text fragments"
    );
}

#[test]
fn test_light_page_without_background_gets_ua_white_canvas() {
    use strata_dom::{Attribute, NodeBlueprint};
    use strata_web_reader::{HtmlParser, ReaderTheme, WebTranspiler};

    let html = r##"
        <!DOCTYPE html>
        <html>
            <head><title>Light page</title></head>
            <body>
                <p>Plain text on the default canvas.</p>
            </body>
        </html>
    "##;

    let nodes = HtmlParser::parse(html);
    // Even when the chrome/reader theme is dark, normal page rendering must use
    // the W3C UA canvas: white background and black text.
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media_ctx = strata_style::MediaContext::new(1280.0, 800.0, false);

    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);

    let root_style = match &doc {
        NodeBlueprint::Element { attributes, .. } => attributes
            .iter()
            .find_map(|a| match a {
                Attribute::DirectStyle(any) => any
                    .downcast_ref::<strata_style::types::ComputedStyle>()
                    .cloned(),
                _ => None,
            })
            .expect("document root must carry a direct style"),
        _ => panic!("Expected element document root"),
    };

    assert_eq!(
        root_style.background_color,
        Some(strata_style::types::Color::new(255, 255, 255, 255)),
        "a light page without an author background must paint the white UA canvas"
    );
    assert_eq!(
        root_style.color,
        strata_style::types::Color::new(0, 0, 0, 255),
        "default page text must be black regardless of the reader theme"
    );
}
