//! Viewport renderer executing native Strata RSX or WebTranspiler HTML pipelines.

use crate::components::google_new_tab::GoogleNewTab;
use crate::state::BrowserState;
use strata::prelude::*;
use strata_web_reader::{HtmlParser, ReaderTheme, WebTranspiler, WikipediaEngine};

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static BLUEPRINT_CACHE: RefCell<HashMap<(String, u64), (NodeBlueprint, usize)>> = RefCell::new(HashMap::new());
}

fn compute_html_hash(html: &str) -> u64 {
    // Stable 64-bit FNV-1a over every byte so edits in the middle of a large
    // document cannot alias a stale cached blueprint.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in html.as_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn count_blueprint_nodes(bp: &NodeBlueprint) -> usize {
    match bp {
        NodeBlueprint::Element { children, .. } => {
            1 + children.iter().map(count_blueprint_nodes).sum::<usize>()
        }
        NodeBlueprint::Fragment(children) => children.iter().map(count_blueprint_nodes).sum(),
        _ => 1,
    }
}

#[component]
pub fn Viewport(state: BrowserState) -> View {
    let active_tab = state.active_tab();

    let tab = match active_tab {
        Some(t) => t,
        None => return GoogleNewTab(state),
    };

    if tab.is_crashed {
        let tab_id = tab.id;
        let crash_reason = tab
            .crash_reason
            .clone()
            .unwrap_or_else(|| "Process terminated unexpectedly".to_string());
        let state_clone = state.clone();

        return rsx! {
            <div class="flex-1 flex flex-col items-center justify-center bg-[#1e1e2e] text-[#cdd6f4] p-8 select-none">
                <div class="max-w-md w-full bg-[#181825] border border-[#313244] rounded-xl p-8 shadow-2xl flex flex-col items-center text-center">
                    <div class="w-16 h-16 rounded-full bg-[#f38ba8]/10 border border-[#f38ba8]/30 flex items-center justify-center mb-6">
                        <span class="text-2xl font-mono font-bold text-[#f38ba8]">!</span>
                    </div>
                    <h2 class="text-xl font-bold text-[#cdd6f4] mb-2">Aw, Snap!</h2>
                    <p class="text-sm text-[#a6adc8] mb-4">Something went wrong while displaying this webpage.</p>
                    <div class="w-full bg-[#11111b] border border-[#313244] rounded-lg p-3 text-xs font-mono text-[#f38ba8] mb-6 text-left overflow-x-auto">
                        <span class="text-[#6c7086]">Error: </span>
                        {crash_reason}
                    </div>
                    <div class="flex gap-3">
                        <button
                            type="button"
                            class="px-5 py-2.5 rounded-lg bg-[#89b4fa] hover:bg-[#b4befe] text-[#11111b] text-sm font-semibold transition shadow-md cursor-pointer"
                            onclick={move |_| {
                                state_clone.recover_tab(tab_id);
                            }}
                        >
                            {"Reload Tab"}
                        </button>
                    </div>
                </div>
            </div>
        };
    }

    if tab.url == "strata://newtab" || tab.url == "about:blank" || tab.url.is_empty() {
        return GoogleNewTab(state);
    }

    // Resolve active reader theme
    let theme_mode = state.active_theme.get();
    let theme = match theme_mode.as_str() {
        "amoled" => ReaderTheme::amoled(),
        "sepia" => ReaderTheme::sepia(),
        "light" => ReaderTheme::light(),
        _ => ReaderTheme::dark(),
    };

    let html_hash = compute_html_hash(&tab.html_content);
    let cache_key = (theme_mode.clone(), html_hash);

    let (document_blueprint, _dom_nodes_count) = BLUEPRINT_CACHE.with(|c| {
        let mut map = c.borrow_mut();
        if let Some((bp, count)) = map.get(&cache_key) {
            let clone_start = std::time::Instant::now();
            let cloned = bp.clone();
            crate::perf::log_ms("blueprint_clone", clone_start, format!("{} nodes", count));
            return (cloned, *count);
        }

        // Tokenize, parse, and transpile raw HTML into native Strata NodeBlueprint with full Chromium parity
        let parse_start = std::time::Instant::now();
        let parsed_nodes;
        let nodes: &[strata_web_reader::ast::SemanticNode] = if !tab.dom_nodes.is_empty() {
            &tab.dom_nodes
        } else {
            parsed_nodes = HtmlParser::parse(&tab.html_content);
            &parsed_nodes
        };
        crate::perf::log_ms(
            "html_parse_ui",
            parse_start,
            format!("{} bytes, {} nodes", tab.html_content.len(), nodes.len()),
        );
        // Normal page rendering follows the browser's light color-scheme preference
        // so W3C UA defaults and `prefers-color-scheme: light` author rules apply.
        // The chrome and any future reader mode remain themed independently.
        let media_ctx = strata_style::MediaContext::new(1280.0, 800.0, false);
        let transpile_start = std::time::Instant::now();
        let transpiler = WebTranspiler::new(&theme);
        let document_blueprint = transpiler.transpile_full_document(&nodes, &media_ctx);
        if crate::perf::enabled() {
            crate::perf::log_ms(
                "transpile_full_document",
                transpile_start,
                format!(
                    "{} nodes -> {} blueprint nodes",
                    nodes.len(),
                    count_blueprint_nodes(&document_blueprint)
                ),
            );
        }
        let count = nodes.len();

        if map.len() > 32 {
            map.clear();
        }
        map.insert(cache_key, (document_blueprint.clone(), count));
        (document_blueprint, count)
    });

    let (bg_color, text_color) = match theme_mode.as_str() {
        "amoled" => ("bg-black", "text-zinc-200"),
        "sepia" => ("bg-[#fbf0d9]", "text-[#5f4b32]"),
        "light" => ("bg-white", "text-slate-800"),
        _ => ("bg-slate-950", "text-neutral-200"),
    };

    if tab.is_loading && tab.html_content.is_empty() {
        return rsx! {
            <div class={format!("flex-1 flex flex-col items-center justify-center min-h-0 w-full {}", bg_color)}>
                <div class="w-full h-[2.5px] bg-blue-500 opacity-90 shrink-0 animate-pulse transition-all duration-300" />
                <div class="flex-1 flex flex-col items-center justify-center p-8 space-y-4">
                    <div class="w-8 h-8 rounded-full border-2 border-blue-500 border-t-transparent animate-spin" />
                    <div class={format!("text-sm font-medium opacity-75 {}", text_color)}>
                        { format!("Connecting to {}...", tab.url) }
                    </div>
                </div>
            </div>
        };
    }

    let page_ctx_state = state.clone();

    rsx! {
        <div
            id={format!("browser-viewport-tab-{}", tab.id)}
            class={format!("flex-1 flex flex-col min-h-0 w-full overflow-y-auto {}", bg_color)}
            oncontextmenu={move |e: Event| {
                page_ctx_state.open_context_menu_for(
                    e.client_x as f32,
                    e.client_y as f32,
                    crate::state::ContextTarget::Page,
                );
            }}
        >
            // Top double-buffering load indicator
            { if tab.is_loading {
                rsx! { <div class="w-full h-[2.5px] bg-blue-500 opacity-90 shrink-0 animate-pulse transition-all duration-300" /> }
            } else {
                rsx! { <div class="w-0 h-0" /> }
            }}

            // Rendered Native Strata Blueprint with full-width Chromium CSS parity.
            // No theme text color here: the document root owns its own `color`
            // (black on the white UA canvas) and author CSS overrides it.
            <div class="w-full min-h-full flex flex-col transition-opacity duration-200">
                { document_blueprint }
            </div>
        </div>
    }
}
