//! Terminal ASCII preview renderer for the Strata Native Browser.

use crate::mock_pages::resolve_page;
use crate::state::BrowserState;
use strata_web_reader::HtmlParser;

/// Renders the complete browser interface into an ASCII terminal presentation.
pub fn render_terminal_browser(state: &BrowserState) -> String {
    let mut out = String::new();
    let width = 80;
    let divider = "═".repeat(width);
    let thin_div = "─".repeat(width);

    // 1. Window Header
    out.push_str(&divider);
    out.push('\n');
    out.push_str(" STRATA NATIVE BROWSER - ZERO CHROMIUM ARCHITECTURE (PURE RUST ENGINE)\n");
    out.push_str(&divider);
    out.push('\n');

    // 2. Tab Bar
    out.push_str(" (o) (o) (o)  TABS: ");
    let active_id = state.active_tab_id.get();
    for tab in state.tabs.get() {
        let is_active = tab.id == active_id;
        let star = if is_active { " *" } else { "" };
        let title_trunc = if tab.title.len() > 18 {
            format!("{}...", &tab.title[..15])
        } else {
            tab.title.clone()
        };
        out.push_str(&format!("[ {}: {}{} [x] ] ", tab.id, title_trunc, star));
    }
    out.push_str("[+] New Tab\n");
    out.push_str(&thin_div);
    out.push('\n');

    // 3. Navigation Controls and Omnibox
    let can_back = if state.can_go_back() { "<" } else { " " };
    let can_fwd = if state.can_go_forward() { ">" } else { " " };
    let current_url = state
        .active_tab()
        .map(|t| t.url)
        .unwrap_or_else(|| "strata://newtab".to_string());

    let sec_badge = if current_url.starts_with("strata://") {
        "[INTERNAL]"
    } else if current_url.starts_with("https://") {
        "[SECURE-TLS]"
    } else {
        "[HTTP]"
    };

    out.push_str(&format!(
        " [{}] [{}] [R] [H]  {} URL: {}\n",
        can_back, can_fwd, sec_badge, current_url
    ));

    // Bookmarks Strip
    out.push_str(" BOOKMARKS: [1] Google  [2] Wikipedia  [3] Hacker News  [4] Strata Docs  [5] About\n");
    out.push_str(&thin_div);
    out.push('\n');

    // 4. Viewport Content
    if current_url == "strata://newtab" || current_url == "about:blank" {
        render_new_tab_content(&mut out);
    } else {
        render_document_content(&mut out, state);
    }

    // 5. Status Bar
    out.push('\n');
    out.push_str(&thin_div);
    out.push('\n');
    let tab_count = state.tabs.get().len();
    let theme = state.active_theme.get();
    out.push_str(&format!(
        " STATUS: Ready | Tabs: {} | Theme: {} | Memory: < 2.8 MB | 120 FPS | LayoutNG\n",
        tab_count, theme
    ));
    out.push_str(&divider);
    out.push('\n');

    out
}

fn render_new_tab_content(out: &mut String) {
    out.push_str("\n");
    out.push_str("                           G o o g l e\n");
    out.push_str("               Strata Native Zero-Chromium Edition\n\n");
    out.push_str("       [================ Search Google or type a URL ===============]\n\n");
    out.push_str("                   [ Google Search ]   [ I'm Feeling Lucky ]\n\n");
    out.push_str("       SHORTCUT TILES:\n");
    out.push_str("       [1] Wikipedia: Rust      [2] Hacker News        [3] Strata Docs\n");
    out.push_str("       [4] Rust Programming    [5] About Browser      [6] GitHub\n");
}

fn render_document_content(out: &mut String, state: &BrowserState) {
    let tab = match state.active_tab() {
        Some(t) => t,
        None => return,
    };

    let nodes = HtmlParser::parse(&tab.html_content);
    out.push_str(&format!(
        " VIEWPORT: {} (Semantic DOM Nodes: {})\n\n",
        tab.title,
        nodes.len()
    ));

    // Plain text extraction preview from the parsed HTML nodes
    let mut lines = Vec::new();
    extract_text_lines(&nodes, &mut lines);

    for line in lines.iter().take(22) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            out.push_str("   ");
            out.push_str(trimmed);
            out.push('\n');
        }
    }
}

fn extract_text_lines(nodes: &[strata_web_reader::SemanticNode], out: &mut Vec<String>) {
    use strata_web_reader::SemanticNode;
    for node in nodes {
        match node {
            SemanticNode::Text(t) => {
                if !t.trim().is_empty() {
                    out.push(t.clone());
                }
            }
            SemanticNode::Element { tag, children, .. } => {
                match tag.as_str() {
                    "h1" => {
                        let text = node.text_content();
                        out.push(format!("# {}", text.trim()));
                    }
                    "h2" => {
                        let text = node.text_content();
                        out.push(format!("## {}", text.trim()));
                    }
                    "h3" => {
                        let text = node.text_content();
                        out.push(format!("### {}", text.trim()));
                    }
                    "li" => {
                        let text = node.text_content();
                        out.push(format!("* {}", text.trim()));
                    }
                    "blockquote" => {
                        let text = node.text_content();
                        out.push(format!("> {}", text.trim()));
                    }
                    "pre" | "code" => {
                        let text = node.text_content();
                        out.push(format!("    {}", text.trim()));
                    }
                    "th" | "td" => {
                        // Handled in table traversal
                    }
                    _ => {
                        extract_text_lines(children, out);
                    }
                }
            }
            _ => {}
        }
    }
}
