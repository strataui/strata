//! Browser navigation toolbar, Omnibox address bar, and bookmarks strip.

use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};
use crate::state::BrowserState;

#[component]
pub fn Toolbar(state: BrowserState) -> View {
    let url_value = state.url_input.get();
    let can_back = state.can_go_back();
    let can_fwd = state.can_go_forward();

    let back_state = state.clone();
    let fwd_state = state.clone();
    let reload_state = state.clone();
    let home_state = state.clone();
    let go_state = state.clone();
    let input_state = state.clone();

    // Theme toggles
    let theme_dark = state.clone();
    let theme_amoled = state.clone();
    let theme_sepia = state.clone();
    let current_theme = state.active_theme.get();

    // Bookmarks
    let bm_google = state.clone();
    let bm_wiki = state.clone();
    let bm_hn = state.clone();
    let bm_docs = state.clone();
    let bm_about = state.clone();

    // Security indicator icon and color
    let (sec_icon, sec_color) = if url_value.starts_with("strata://") {
        (IconName::ShieldCheck, "text-blue-400")
    } else if url_value.starts_with("https://") {
        (IconName::Lock, "text-emerald-400")
    } else {
        (IconName::Globe, "text-neutral-400")
    };

    let back_opacity = if can_back { "text-neutral-300 hover:text-white hover:bg-neutral-800" } else { "text-neutral-600 pointer-events-none" };
    let fwd_opacity = if can_fwd { "text-neutral-300 hover:text-white hover:bg-neutral-800" } else { "text-neutral-600 pointer-events-none" };

    rsx! {
        <div class="flex flex-col bg-neutral-900 border-b border-neutral-800 select-none">
            // Main Navigation Controls and Omnibox
            <div class="flex items-center gap-2 px-3 py-2">
                // Back Button
                <button
                    type="button"
                    class={format!("p-1.5 rounded-lg transition-colors cursor-pointer {}", back_opacity)}
                    onclick={move |_| {
                        back_state.go_back();
                    }}
                >
                    {Icon(IconName::ChevronLeft, Some("w-4 h-4 text-current")).root}
                </button>

                // Forward Button
                <button
                    type="button"
                    class={format!("p-1.5 rounded-lg transition-colors cursor-pointer {}", fwd_opacity)}
                    onclick={move |_| {
                        fwd_state.go_forward();
                    }}
                >
                    {Icon(IconName::ChevronRight, Some("w-4 h-4 text-current")).root}
                </button>

                // Reload Button
                <button
                    type="button"
                    class="p-1.5 rounded-lg text-neutral-300 hover:text-white hover:bg-neutral-800 transition-colors cursor-pointer"
                    onclick={move |_| {
                        reload_state.reload();
                    }}
                >
                    {Icon(IconName::RefreshCw, Some("w-3.5 h-3.5 text-current")).root}
                </button>

                // Home Button
                <button
                    type="button"
                    class="p-1.5 rounded-lg text-neutral-300 hover:text-white hover:bg-neutral-800 transition-colors cursor-pointer"
                    onclick={move |_| {
                        home_state.go_home();
                    }}
                >
                    {Icon(IconName::Compass, Some("w-4 h-4 text-current")).root}
                </button>

                // Omnibox URL Bar
                <div class="flex-1 flex items-center bg-neutral-950 border border-neutral-700/80 rounded-lg px-3 py-1.5 focus-within:border-blue-500 transition-colors">
                    <div class={format!("flex items-center mr-2 {}", sec_color)}>
                        {Icon(sec_icon, Some("w-3.5 h-3.5 text-current")).root}
                    </div>

                    <input
                        type="text"
                        class="flex-1 bg-transparent border-0 outline-none text-xs text-white placeholder-neutral-500 font-mono"
                        placeholder="Enter URL or search query..."
                        value={url_value.clone()}
                        oninput={move |val: String| {
                            input_state.url_input.set(val);
                        }}
                    />

                    <button
                        type="button"
                        class="px-2.5 py-0.5 bg-blue-600 hover:bg-blue-500 text-white text-xs font-semibold rounded transition-colors cursor-pointer ml-2 flex items-center gap-1"
                        onclick={move |_| {
                            let dest = go_state.url_input.get();
                            go_state.navigate(&dest);
                        }}
                    >
                        <span>{"Go"}</span>
                        {Icon(IconName::ArrowRight, Some("w-3 h-3 text-current")).root}
                    </button>
                </div>

                // Theme Switcher Pills
                <div class="flex items-center gap-1 p-0.5 rounded-lg bg-neutral-950 border border-neutral-800 text-xs">
                    <button
                        type="button"
                        class={format!("px-2 py-1 rounded transition-colors cursor-pointer {}", if current_theme == "dark" { "bg-neutral-800 text-white font-medium" } else { "text-neutral-400 hover:text-neutral-200" })}
                        onclick={move |_| {
                            theme_dark.set_theme("dark");
                        }}
                    >
                        {"Dark"}
                    </button>
                    <button
                        type="button"
                        class={format!("px-2 py-1 rounded transition-colors cursor-pointer {}", if current_theme == "amoled" { "bg-neutral-800 text-white font-medium" } else { "text-neutral-400 hover:text-neutral-200" })}
                        onclick={move |_| {
                            theme_amoled.set_theme("amoled");
                        }}
                    >
                        {"Amoled"}
                    </button>
                    <button
                        type="button"
                        class={format!("px-2 py-1 rounded transition-colors cursor-pointer {}", if current_theme == "sepia" { "bg-neutral-800 text-white font-medium" } else { "text-neutral-400 hover:text-neutral-200" })}
                        onclick={move |_| {
                            theme_sepia.set_theme("sepia");
                        }}
                    >
                        {"Sepia"}
                    </button>
                </div>
            </div>

            // Bookmarks Quick Access Bar
            <div class="flex items-center gap-2 px-3 py-1 bg-neutral-950/60 border-t border-neutral-800/60 text-xs text-neutral-400 overflow-x-auto">
                <span class="text-neutral-500 font-mono text-[10px] uppercase tracking-wider pl-1">
                    {"Bookmarks:"}
                </span>

                <button
                    type="button"
                    class="flex items-center gap-1.5 px-2 py-0.5 rounded hover:bg-neutral-800 hover:text-white transition-colors cursor-pointer"
                    onclick={move |_| {
                        bm_google.navigate("strata://newtab");
                    }}
                >
                    {Icon(IconName::Search, Some("w-3 h-3 text-current")).root}
                    <span>{"Google"}</span>
                </button>

                <button
                    type="button"
                    class="flex items-center gap-1.5 px-2 py-0.5 rounded hover:bg-neutral-800 hover:text-white transition-colors cursor-pointer"
                    onclick={move |_| {
                        bm_wiki.navigate("https://en.wikipedia.org/wiki/Rust_(programming_language)");
                    }}
                >
                    {Icon(IconName::Code, Some("w-3 h-3 text-current")).root}
                    <span>{"Wikipedia: Rust"}</span>
                </button>

                <button
                    type="button"
                    class="flex items-center gap-1.5 px-2 py-0.5 rounded hover:bg-neutral-800 hover:text-white transition-colors cursor-pointer"
                    onclick={move |_| {
                        bm_hn.navigate("https://news.ycombinator.com");
                    }}
                >
                    {Icon(IconName::Terminal, Some("w-3 h-3 text-current")).root}
                    <span>{"Hacker News"}</span>
                </button>

                <button
                    type="button"
                    class="flex items-center gap-1.5 px-2 py-0.5 rounded hover:bg-neutral-800 hover:text-white transition-colors cursor-pointer"
                    onclick={move |_| {
                        bm_docs.navigate("https://strata.larvance.com");
                    }}
                >
                    {Icon(IconName::ShieldCheck, Some("w-3 h-3 text-current")).root}
                    <span>{"Strata Docs"}</span>
                </button>

                <button
                    type="button"
                    class="flex items-center gap-1.5 px-2 py-0.5 rounded hover:bg-neutral-800 hover:text-white transition-colors cursor-pointer"
                    onclick={move |_| {
                        bm_about.navigate("strata://about");
                    }}
                >
                    {Icon(IconName::Compass, Some("w-3 h-3 text-current")).root}
                    <span>{"About"}</span>
                </button>
            </div>
        </div>
    }
}
