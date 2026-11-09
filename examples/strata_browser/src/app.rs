//! Root GUI View for Strata Browser Harness.

use crate::state::BrowserState;
use strata::prelude::*;
use strata_icons::prelude::Icons;
use strata_web_reader::{HtmlParser, ReaderTheme, WebTranspiler};

#[component]
pub fn BrowserApp(state: BrowserState) -> View {
    let tabs = state.tabs.get();
    let active_idx = state.active_index.get();
    let current_url = state.url_input.get();
    let status_msg = state.status_message.get();

    let active_tab = tabs.get(active_idx).cloned();

    // Transpile active tab HTML into a native Strata NodeBlueprint
    let content_blueprint = if let Some(ref tab) = active_tab {
        let nodes = HtmlParser::parse(&tab.html);
        let theme = ReaderTheme::dark();
        let transpiler = WebTranspiler::new(&theme);
        transpiler.transpile_document(&nodes, Some(&tab.title))
    } else {
        NodeBlueprint::Text("No active tab open.".to_string())
    };

    let back_state = state.clone();
    let forward_state = state.clone();
    let reload_state = state.clone();
    let home_state = state.clone();
    let new_tab_state = state.clone();
    let welcome_bm_state = state.clone();
    let about_bm_state = state.clone();
    let bench_bm_state = state.clone();

    rsx! {
        <div class="flex flex-col h-screen w-screen bg-[#090d16] text-[#e2e8f0] font-sans overflow-hidden select-none">
            // Tab Bar
            <div class="flex flex-row items-center bg-[#0d1322] border-b border-[#1e293b] px-2 pt-2 gap-1 overflow-x-auto min-h-[42px]">
                {tabs.iter().enumerate().map(|(idx, tab)| {
                    let is_active = idx == active_idx;
                    let select_state = state.clone();
                    let close_state = state.clone();
                    let tab_title = tab.title.clone();

                    rsx! {
                        <div
                            key={tab.id}
                            class={format!(
                                "flex flex-row items-center gap-2 px-3 py-1.5 rounded-t-md text-xs font-medium cursor-pointer transition-colors max-w-[200px] border-t border-x {}",
                                if is_active {
                                    "bg-[#090d16] text-[#38bdf8] border-[#334155] border-b-transparent"
                                } else {
                                    "bg-[#131c31] text-[#94a3b8] border-transparent hover:bg-[#1a2540] hover:text-[#cbd5e1]"
                                }
                            )}
                            onclick={move |_| select_state.active_index.set(idx)}
                        >
                            <span class="truncate">{tab_title}</span>
                            <span
                                class="text-[#64748b] hover:text-[#ef4444] rounded px-1 cursor-pointer transition-colors"
                                onclick={move |_| close_state.close_tab(idx)}
                            >
                                "×"
                            </span>
                        </div>
                    }
                }).collect::<Vec<_>>()}

                <button
                    class="p-1.5 text-[#94a3b8] hover:text-[#38bdf8] hover:bg-[#1e293b] rounded transition-colors text-xs flex items-center justify-center min-w-[28px]"
                    onclick={move |_| new_tab_state.new_tab("New Tab", "strata://welcome", &BrowserState::welcome_page_html())}
                >
                    {Icons::Plus(Some("w-3.5 h-3.5"))}
                </button>
            </div>

            // Navigation Toolbar
            <div class="flex flex-row items-center gap-2 px-3 py-2 bg-[#090d16] border-b border-[#1e293b]">
                <button
                    class="p-1.5 text-[#94a3b8] hover:text-white hover:bg-[#1e293b] rounded transition-colors"
                    onclick={move |_| back_state.back()}
                >
                    {Icons::ArrowLeft(Some("w-4 h-4"))}
                </button>
                <button
                    class="p-1.5 text-[#94a3b8] hover:text-white hover:bg-[#1e293b] rounded transition-colors"
                    onclick={move |_| forward_state.forward()}
                >
                    {Icons::ArrowRight(Some("w-4 h-4"))}
                </button>
                <button
                    class="p-1.5 text-[#94a3b8] hover:text-white hover:bg-[#1e293b] rounded transition-colors"
                    onclick={move |_| reload_state.reload()}
                >
                    {Icons::RotateCw(Some("w-4 h-4"))}
                </button>
                <button
                    class="p-1.5 text-[#94a3b8] hover:text-white hover:bg-[#1e293b] rounded transition-colors"
                    onclick={move |_| home_state.navigate("strata://welcome")}
                >
                    {Icons::Home(Some("w-4 h-4"))}
                </button>

                // URL Address Bar
                <div class="flex-1 flex flex-row items-center bg-[#131c31] border border-[#334155] rounded-lg px-3 py-1 gap-2 focus-within:border-[#38bdf8] transition-colors">
                    {Icons::Lock(Some("w-3.5 h-3.5 text-emerald-400"))}
                    <span class="text-xs font-mono text-[#cbd5e1] truncate flex-1">{current_url}</span>
                    <button
                        class="text-[10px] uppercase font-semibold tracking-wider text-[#38bdf8] bg-[#0284c7]/20 px-2 py-0.5 rounded border border-[#0284c7]/40 hover:bg-[#0284c7]/30 transition-colors"
                        onclick={move |_| {}}
                    >
                        "Zero-Chromium"
                    </button>
                </div>

                // Quick Navigation Shortcuts
                <div class="flex flex-row items-center gap-1">
                    <button
                        class="px-2 py-1 text-xs text-[#94a3b8] hover:text-white hover:bg-[#1e293b] rounded transition-colors"
                        onclick={move |_| welcome_bm_state.navigate("strata://welcome")}
                    >
                        "Welcome"
                    </button>
                    <button
                        class="px-2 py-1 text-xs text-[#94a3b8] hover:text-white hover:bg-[#1e293b] rounded transition-colors"
                        onclick={move |_| about_bm_state.navigate("strata://about")}
                    >
                        "About"
                    </button>
                    <button
                        class="px-2 py-1 text-xs text-[#94a3b8] hover:text-white hover:bg-[#1e293b] rounded transition-colors"
                        onclick={move |_| bench_bm_state.navigate("strata://benchmarks")}
                    >
                        "Benchmarks"
                    </button>
                </div>
            </div>

            // Web Document Viewport
            <div class="flex-1 overflow-auto bg-[#090d16] p-6">
                <div class="max-w-4xl mx-auto">
                    {content_blueprint}
                </div>
            </div>

            // Status Bar
            <div class="flex flex-row items-center justify-between px-3 py-1 bg-[#0d1322] border-t border-[#1e293b] text-[11px] text-[#64748b]">
                <div class="flex flex-row items-center gap-2">
                    <span class="w-2 h-2 rounded-full bg-[#22c55e]"></span>
                    <span>{status_msg}</span>
                </div>
                <div class="flex flex-row items-center gap-3">
                    <span>"Tabs: " {tabs.len()}</span>
                    <span>"Pure-Rust Layout Engine"</span>
                </div>
            </div>
        </div>
    }
}
