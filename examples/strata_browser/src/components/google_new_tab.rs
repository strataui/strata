//! Google Search New Tab Page Component.

use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};
use crate::state::BrowserState;

#[component]
pub fn GoogleNewTab(state: BrowserState) -> View {
    let query_str = state.search_input.get();
    let search_state = state.clone();
    let lucky_state = state.clone();
    let input_state = state.clone();

    // Shortcuts
    let wiki_state = state.clone();
    let hn_state = state.clone();
    let docs_state = state.clone();
    let rust_state = state.clone();
    let about_state = state.clone();

    rsx! {
        <div class="flex-1 flex flex-col items-center justify-center w-full px-4 py-8 select-none">
            // Google Brand Logo
            <div class="flex flex-col items-center mb-8">
                <div class="flex items-center text-6xl font-bold gap-1">
                    <span class="text-blue-500">{"G"}</span>
                    <span class="text-red-500">{"o"}</span>
                    <span class="text-amber-400">{"o"}</span>
                    <span class="text-blue-500 ml-1">{"g"}</span>
                    <span class="text-emerald-500">{"l"}</span>
                    <span class="text-red-500">{"e"}</span>
                </div>
                <div class="flex items-center justify-center mt-2 text-xs font-mono uppercase tracking-widest text-neutral-500">
                    {"Strata Native Zero-Chromium Edition"}
                </div>
            </div>

            // Google Search Input Box
            <div class="w-full max-w-2xl bg-neutral-900 border border-neutral-700 hover:border-neutral-500 rounded-full px-5 py-3 flex items-center gap-3 shadow-xl">
                {Icon(IconName::Search, Some("w-5 h-5 text-neutral-400 flex-shrink-0")).root}
                <input
                    type="text"
                    class="flex-1 bg-transparent border-0 outline-none text-white text-base placeholder-neutral-500 font-sans"
                    placeholder="Search Google or type a URL"
                    value={query_str.clone()}
                    oninput={move |val: String| {
                        input_state.search_input.set(val);
                    }}
                />
                <button
                    type="button"
                    class="px-4 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-xs font-semibold rounded-full transition-colors cursor-pointer flex-shrink-0"
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
            <div class="flex items-center gap-3 mt-7">
                <button
                    type="button"
                    class="flex items-center justify-center px-5 py-2.5 bg-neutral-800/80 hover:bg-neutral-700 text-neutral-200 hover:text-white text-sm font-medium rounded-lg border border-neutral-700/60 transition-all cursor-pointer shadow-sm"
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
                    class="flex items-center justify-center px-5 py-2.5 bg-neutral-800/80 hover:bg-neutral-700 text-neutral-200 hover:text-white text-sm font-medium rounded-lg border border-neutral-700/60 transition-all cursor-pointer shadow-sm"
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
                    class="flex flex-col items-center gap-2 p-3.5 rounded-2xl bg-neutral-900/50 hover:bg-neutral-800/80 border border-neutral-800 hover:border-neutral-700 transition-all cursor-pointer group"
                    onclick={move |_| {
                        wiki_state.navigate("https://en.wikipedia.org/wiki/Rust_(programming_language)");
                    }}
                >
                    <div class="w-11 h-11 rounded-full bg-neutral-800 group-hover:bg-indigo-600/20 group-hover:text-indigo-400 flex items-center justify-center text-neutral-300 transition-colors">
                        {Icon(IconName::Code, Some("w-5 h-5 text-current")).root}
                    </div>
                    <span class="text-xs font-medium text-neutral-300 group-hover:text-white truncate max-w-[80px]">
                        {"Wikipedia"}
                    </span>
                </button>

                // Hacker News
                <button
                    type="button"
                    class="flex flex-col items-center gap-2 p-3.5 rounded-2xl bg-neutral-900/50 hover:bg-neutral-800/80 border border-neutral-800 hover:border-neutral-700 transition-all cursor-pointer group"
                    onclick={move |_| {
                        hn_state.navigate("https://news.ycombinator.com");
                    }}
                >
                    <div class="w-11 h-11 rounded-full bg-neutral-800 group-hover:bg-amber-600/20 group-hover:text-amber-400 flex items-center justify-center text-neutral-300 transition-colors">
                        {Icon(IconName::Terminal, Some("w-5 h-5 text-current")).root}
                    </div>
                    <span class="text-xs font-medium text-neutral-300 group-hover:text-white truncate max-w-[80px]">
                        {"Hacker News"}
                    </span>
                </button>

                // Strata Docs
                <button
                    type="button"
                    class="flex flex-col items-center gap-2 p-3.5 rounded-2xl bg-neutral-900/50 hover:bg-neutral-800/80 border border-neutral-800 hover:border-neutral-700 transition-all cursor-pointer group"
                    onclick={move |_| {
                        docs_state.navigate("https://strata.larvance.com");
                    }}
                >
                    <div class="w-11 h-11 rounded-full bg-neutral-800 group-hover:bg-emerald-600/20 group-hover:text-emerald-400 flex items-center justify-center text-neutral-300 transition-colors">
                        {Icon(IconName::ShieldCheck, Some("w-5 h-5 text-current")).root}
                    </div>
                    <span class="text-xs font-medium text-neutral-300 group-hover:text-white truncate max-w-[80px]">
                        {"Strata Docs"}
                    </span>
                </button>

                // Rust Lang
                <button
                    type="button"
                    class="flex flex-col items-center gap-2 p-3.5 rounded-2xl bg-neutral-900/50 hover:bg-neutral-800/80 border border-neutral-800 hover:border-neutral-700 transition-all cursor-pointer group"
                    onclick={move |_| {
                        rust_state.navigate("https://www.rust-lang.org");
                    }}
                >
                    <div class="w-11 h-11 rounded-full bg-neutral-800 group-hover:bg-rose-600/20 group-hover:text-rose-400 flex items-center justify-center text-neutral-300 transition-colors">
                        {Icon(IconName::Cpu, Some("w-5 h-5 text-current")).root}
                    </div>
                    <span class="text-xs font-medium text-neutral-300 group-hover:text-white truncate max-w-[80px]">
                        {"Rust Lang"}
                    </span>
                </button>

                // About Browser
                <button
                    type="button"
                    class="flex flex-col items-center gap-2 p-3.5 rounded-2xl bg-neutral-900/50 hover:bg-neutral-800/80 border border-neutral-800 hover:border-neutral-700 transition-all cursor-pointer group"
                    onclick={move |_| {
                        about_state.navigate("strata://about");
                    }}
                >
                    <div class="w-11 h-11 rounded-full bg-neutral-800 group-hover:bg-blue-600/20 group-hover:text-blue-400 flex items-center justify-center text-neutral-300 transition-colors">
                        {Icon(IconName::Compass, Some("w-5 h-5 text-current")).root}
                    </div>
                    <span class="text-xs font-medium text-neutral-300 group-hover:text-white truncate max-w-[80px]">
                        {"About"}
                    </span>
                </button>
            </div>
        </div>
    }
}
