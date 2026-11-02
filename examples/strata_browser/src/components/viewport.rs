//! Viewport renderer executing native Strata RSX or WebTranspiler HTML pipelines.

use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};
use strata_web_reader::{HtmlParser, ReaderTheme, WebTranspiler};
use crate::components::google_new_tab::GoogleNewTab;
use crate::state::BrowserState;

#[component]
pub fn Viewport(state: BrowserState) -> View {
    let active_tab = state.active_tab();

    let tab = match active_tab {
        Some(t) => t,
        None => return GoogleNewTab(state),
    };

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

    // Tokenize, parse, and transpile raw HTML into native Strata NodeBlueprint
    let nodes = HtmlParser::parse(&tab.html_content);
    let transpiler = WebTranspiler::new(&theme);
    let document_blueprint = transpiler.transpile_document(&nodes, Some(&tab.title));

    let (bg_color, text_color) = match theme_mode.as_str() {
        "amoled" => ("bg-black", "text-zinc-200"),
        "sepia" => ("bg-[#fbf0d9]", "text-[#5f4b32]"),
        "light" => ("bg-white", "text-slate-800"),
        _ => ("bg-neutral-950", "text-neutral-200"),
    };

    rsx! {
        <div class={format!("flex-1 flex flex-col min-h-0 w-full overflow-y-auto {}", bg_color)}>
            // Document Reader Header Bar
            <div class="flex items-center justify-between px-6 py-2.5 bg-neutral-900/60 border-b border-neutral-800/80 text-xs text-neutral-400 select-none">
                <div class="flex items-center gap-2">
                    {Icon(IconName::ShieldCheck, Some("w-3.5 h-3.5 text-blue-400")).root}
                    <span class="font-mono text-neutral-300">{"Strata WebTranspiler"}</span>
                    <span class="text-neutral-600">{"•"}</span>
                    <span class="text-neutral-400">{"Zero Chromium / Pure Rust Layout"}</span>
                </div>
                <div class="flex items-center gap-3 text-[11px] font-mono text-neutral-500">
                    <span>{format!("Theme: {}", theme.name)}</span>
                    <span>{"•"}</span>
                    <span>{format!("DOM Nodes: {}", nodes.len())}</span>
                </div>
            </div>

            // Rendered Native Strata Blueprint
            <div class={format!("flex-1 px-4 py-8 max-w-4xl mx-auto w-full {}", text_color)}>
                { document_blueprint }
            </div>
        </div>
    }
}
