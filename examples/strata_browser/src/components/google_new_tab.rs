//! Google Search New Tab Page Component.

use crate::state::BrowserState;
use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};

#[component]
pub fn GoogleNewTab(state: BrowserState) -> View {
    let query_str = state.search_input.get();
    let theme_mode = state.active_theme.get();
    let search_state = state.clone();
    let lucky_state = state.clone();
    let input_state = state.clone();
    let submit_search_state = state.clone();

    // Shortcuts
    let wiki_state = state.clone();
    let hn_state = state.clone();
    let docs_state = state.clone();
    let rust_state = state.clone();
    let about_state = state.clone();

    // Dynamic theme styling
    let (bg_page, text_sub, input_box_bg, input_box_border, input_text, search_btn_bg, action_btn_bg, tile_bg, tile_border, tile_icon_bg, tile_text) = match theme_mode.as_str() {
        "amoled" => (
            "bg-black",
            "text-neutral-500",
            "bg-neutral-950",
            "border-neutral-800 hover:border-neutral-700",
            "text-white placeholder-neutral-500",
            "bg-blue-600 hover:bg-blue-500 text-white",
            "bg-neutral-900/90 hover:bg-neutral-800 text-neutral-200 hover:text-white border-neutral-800",
            "bg-neutral-950/80 hover:bg-neutral-900",
            "border-neutral-900 hover:border-neutral-800",
            "bg-neutral-900 text-neutral-300",
            "text-neutral-400 group-hover:text-white",
        ),
        "sepia" => (
            "bg-[#fbf0d9]",
            "text-[#8c735a]",
            "bg-[#f4e4c1]",
            "border-[#dfcca4] hover:border-[#bda377]",
            "text-[#433422] placeholder-[#8c735a]",
            "bg-[#8c5b36] hover:bg-[#734a2c] text-white",
            "bg-[#eedcb4] hover:bg-[#e4cfa1] text-[#433422] hover:text-black border-[#dfcca4]",
            "bg-[#f4e4c1]/90 hover:bg-[#eedcb4]",
            "border-[#dfcca4] hover:border-[#cbb389]",
            "bg-[#eedcb4] text-[#433422]",
            "text-[#5a4632] group-hover:text-black",
        ),
        "light" => (
            "bg-neutral-50",
            "text-neutral-500",
            "bg-white",
            "border-neutral-300 hover:border-neutral-400",
            "text-neutral-900 placeholder-neutral-400",
            "bg-blue-600 hover:bg-blue-500 text-white",
            "bg-neutral-100 hover:bg-neutral-200 text-neutral-800 hover:text-black border-neutral-300",
            "bg-white hover:bg-neutral-100",
            "border-neutral-200 hover:border-neutral-300",
            "bg-neutral-100 text-neutral-700",
            "text-neutral-600 group-hover:text-neutral-900",
        ),
        _ => ( // "dark"
            "bg-neutral-950",
            "text-neutral-500",
            "bg-neutral-900",
            "border-neutral-700/80 hover:border-neutral-500",
            "text-white placeholder-neutral-400",
            "bg-blue-600 hover:bg-blue-500 text-white",
            "bg-neutral-800/90 hover:bg-neutral-700 text-neutral-200 hover:text-white border-neutral-700/60",
            "bg-neutral-900/60 hover:bg-neutral-800/90",
            "border-neutral-800 hover:border-neutral-700",
            "bg-neutral-800 text-neutral-300",
            "text-neutral-300 group-hover:text-white",
        ),
    };

    let page_ctx_state = state.clone();

    rsx! {
        <div
            class={format!("flex-1 flex flex-col items-center justify-center w-full min-h-0 px-4 py-8 select-none {}", bg_page)}
            oncontextmenu={move |e: Event| {
                page_ctx_state.open_context_menu_for(
                    e.client_x as f32,
                    e.client_y as f32,
                    crate::state::ContextTarget::Page,
                );
            }}
        >
            // Google Brand Logo
            <div class="flex flex-col items-center mb-8">
                <div class="flex items-center justify-center text-7xl font-bold tracking-tight select-none">
                    <span class="text-[#4285F4]">{"G"}</span>
                    <span class="text-[#EA4335]">{"o"}</span>
                    <span class="text-[#FBBC05]">{"o"}</span>
                    <span class="text-[#4285F4]">{"g"}</span>
                    <span class="text-[#34A853]">{"l"}</span>
                    <span class="text-[#EA4335]">{"e"}</span>
                </div>
                <div class={format!("w-full flex items-center justify-center text-center mt-3 text-xs font-mono uppercase tracking-widest {}", text_sub)}>
                    {"Strata Native Zero-Chromium Edition"}
                </div>
            </div>

            // Google Search Input Box
            <div class={format!("w-full max-w-2xl h-12 {} border {} rounded-full px-4 flex items-center gap-3 shadow-xl", input_box_bg, input_box_border)}>
                {Icon(IconName::Search, Some("w-5 h-5 text-neutral-400 flex-shrink-0")).root}
                <input
                    type="text"
                    id="google-search-input"
                    class={format!("flex-1 h-8 bg-transparent border-0 outline-none text-sm font-sans flex items-center leading-normal {}", input_text)}
                    placeholder="Search Google or type a URL"
                    value={query_str.clone()}
                    oninput={move |val: String| {
                        input_state.search_input.set(val);
                    }}
                    onsubmit={move |val: String| {
                        let target = if val.trim().is_empty() {
                            "Rust programming language"
                        } else {
                            val.as_str()
                        };
                        submit_search_state.navigate(target);
                    }}
                />
                <button
                    type="button"
                    class={format!("h-8 px-4 {} text-xs font-semibold rounded-full transition-colors cursor-pointer flex items-center justify-center flex-shrink-0", search_btn_bg)}
                    onclick={move |_| {
                        let q = search_state.search_input.get();
                        let target = if q.trim().is_empty() {
                            "Rust programming language"
                        } else {
                            q.as_str()
                        };
                        search_state.navigate(target);
                    }}
                >
                    {"Search"}
                </button>
            </div>

            // Search Action Buttons
            <div class="flex items-center justify-center gap-3 mt-7">
                <button
                    type="button"
                    class={format!("h-10 px-5 {} text-sm font-medium rounded-lg border transition-all cursor-pointer flex items-center justify-center shadow-sm", action_btn_bg)}
                    onclick={move |_| {
                        let q = state.search_input.get();
                        let target = if q.trim().is_empty() {
                            "Strata framework"
                        } else {
                            q.as_str()
                        };
                        state.navigate(target);
                    }}
                >
                    {"Google Search"}
                </button>
                <button
                    type="button"
                    class={format!("h-10 px-5 {} text-sm font-medium rounded-lg border transition-all cursor-pointer flex items-center justify-center shadow-sm", action_btn_bg)}
                    onclick={move |_| {
                        lucky_state.navigate("https://en.wikipedia.org/wiki/Rust_(programming_language)");
                    }}
                >
                    {"I'm Feeling Lucky"}
                </button>
            </div>

            // Quick Access Shortcut Tiles
            <div class="grid grid-cols-5 gap-4 mt-8 max-w-2xl w-full">
                // Wikipedia
                <button
                    type="button"
                    class={format!("flex flex-col items-center justify-center gap-2.5 h-28 p-3.5 rounded-2xl {} border {} hover:-translate-y-1 hover:shadow-xl transition-all cursor-pointer group w-full", tile_bg, tile_border)}
                    onclick={move |_| {
                        wiki_state.navigate("https://en.wikipedia.org/wiki/Rust_(programming_language)");
                    }}
                >
                    <div class={format!("w-12 h-12 rounded-full {} group-hover:bg-indigo-600/20 group-hover:text-indigo-400 group-hover:scale-105 flex items-center justify-center transition-all", tile_icon_bg)}>
                        {Icon(IconName::Code, Some("w-5 h-5 text-current")).root}
                    </div>
                    <span class={format!("text-xs font-medium {} text-center truncate max-w-[90px] w-full", tile_text)}>
                        {"Wikipedia"}
                    </span>
                </button>

                // Hacker News
                <button
                    type="button"
                    class={format!("flex flex-col items-center justify-center gap-2.5 h-28 p-3.5 rounded-2xl {} border {} hover:-translate-y-1 hover:shadow-xl transition-all cursor-pointer group w-full", tile_bg, tile_border)}
                    onclick={move |_| {
                        hn_state.navigate("https://news.ycombinator.com");
                    }}
                >
                    <div class={format!("w-12 h-12 rounded-full {} group-hover:bg-amber-600/20 group-hover:text-amber-400 group-hover:scale-105 flex items-center justify-center transition-all", tile_icon_bg)}>
                        {Icon(IconName::Terminal, Some("w-5 h-5 text-current")).root}
                    </div>
                    <span class={format!("text-xs font-medium {} text-center truncate max-w-[90px] w-full", tile_text)}>
                        {"Hacker News"}
                    </span>
                </button>

                // Strata Docs
                <button
                    type="button"
                    class={format!("flex flex-col items-center justify-center gap-2.5 h-28 p-3.5 rounded-2xl {} border {} hover:-translate-y-1 hover:shadow-xl transition-all cursor-pointer group w-full", tile_bg, tile_border)}
                    onclick={move |_| {
                        docs_state.navigate("https://strata.larvance.com");
                    }}
                >
                    <div class={format!("w-12 h-12 rounded-full {} group-hover:bg-emerald-600/20 group-hover:text-emerald-400 group-hover:scale-105 flex items-center justify-center transition-all", tile_icon_bg)}>
                        {Icon(IconName::ShieldCheck, Some("w-5 h-5 text-current")).root}
                    </div>
                    <span class={format!("text-xs font-medium {} text-center truncate max-w-[90px] w-full", tile_text)}>
                        {"Strata Docs"}
                    </span>
                </button>

                // Rust Lang
                <button
                    type="button"
                    class={format!("flex flex-col items-center justify-center gap-2.5 h-28 p-3.5 rounded-2xl {} border {} hover:-translate-y-1 hover:shadow-xl transition-all cursor-pointer group w-full", tile_bg, tile_border)}
                    onclick={move |_| {
                        rust_state.navigate("https://www.rust-lang.org");
                    }}
                >
                    <div class={format!("w-12 h-12 rounded-full {} group-hover:bg-rose-600/20 group-hover:text-rose-400 group-hover:scale-105 flex items-center justify-center transition-all", tile_icon_bg)}>
                        {Icon(IconName::Cpu, Some("w-5 h-5 text-current")).root}
                    </div>
                    <span class={format!("text-xs font-medium {} text-center truncate max-w-[90px] w-full", tile_text)}>
                        {"Rust Lang"}
                    </span>
                </button>

                // About Browser
                <button
                    type="button"
                    class={format!("flex flex-col items-center justify-center gap-2.5 h-28 p-3.5 rounded-2xl {} border {} hover:-translate-y-1 hover:shadow-xl transition-all cursor-pointer group w-full", tile_bg, tile_border)}
                    onclick={move |_| {
                        about_state.navigate("strata://about");
                    }}
                >
                    <div class={format!("w-12 h-12 rounded-full {} group-hover:bg-blue-600/20 group-hover:text-blue-400 group-hover:scale-105 flex items-center justify-center transition-all", tile_icon_bg)}>
                        {Icon(IconName::Home, Some("w-5 h-5 text-current")).root}
                    </div>
                    <span class={format!("text-xs font-medium {} text-center truncate max-w-[90px] w-full", tile_text)}>
                        {"About"}
                    </span>
                </button>
            </div>
        </div>
    }
}
