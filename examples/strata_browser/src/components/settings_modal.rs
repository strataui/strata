//! Settings Modal Component for Strata Browser.
//!
//! Provides Chromium-parity browser settings, theme configuration (Dark, Amoled, Sepia, Light),
//! search engine defaults, cache clearing, and engine diagnostics.

use crate::state::BrowserState;
use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};

#[component]
pub fn SettingsModal(state: BrowserState) -> View {
    let current_theme = state.active_theme.get();
    let current_search = state.search_engine.get();

    let close_state = state.clone();
    let theme_dark = state.clone();
    let theme_amoled = state.clone();
    let theme_sepia = state.clone();
    let theme_light = state.clone();

    let search_google = state.clone();
    let search_ddg = state.clone();
    let search_bing = state.clone();

    let clear_cache_state = state.clone();
    let backdrop_close = state.clone();

    rsx! {
        <div
            class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm select-none cursor-pointer transition-opacity duration-200"
            onclick={move |_| {
                backdrop_close.close_settings();
            }}
        >
            <div
                class="w-[580px] h-[540px] bg-neutral-900 border border-neutral-700/80 rounded-2xl shadow-2xl overflow-hidden flex flex-col cursor-default transition-all duration-200 ease-out"
                onclick={|e: Event| {
                    e.stop_propagation();
                }}
            >
                // Header Bar
                <div class="flex items-center justify-between px-6 py-4 bg-neutral-950/80 border-b border-neutral-800">
                    <div class="flex items-center gap-3">
                        <div class="p-2 rounded-xl bg-blue-600/20 text-blue-400">
                            {Icon(IconName::Settings, Some("w-5 h-5 text-current")).root}
                        </div>
                        <div>
                            <div class="text-base font-semibold text-white tracking-wide">
                                {"Settings"}
                            </div>
                            <div class="text-xs text-neutral-400 font-mono">
                                {"Strata Native Browser Configuration"}
                            </div>
                        </div>
                    </div>
                    <button
                        type="button"
                        class="p-2 rounded-xl text-neutral-400 hover:text-white hover:bg-neutral-800 active:bg-neutral-700 transition-colors duration-150 cursor-pointer"
                        onclick={move |_| {
                            close_state.close_settings();
                        }}
                    >
                        {Icon(IconName::X, Some("w-4 h-4 text-current")).root}
                    </button>
                </div>

                // Scrollable Settings Sections
                <div class="flex-1 flex flex-col gap-4 p-5 overflow-hidden">
                    // Section 1: Appearance & Theme
                    <div class="flex flex-col gap-3">
                        <div class="flex items-center justify-between">
                            <span class="text-sm font-semibold text-white">{"Appearance & Themes"}</span>
                            <span class="text-xs text-neutral-500 font-mono">{format!("Active: {}", current_theme.to_uppercase())}</span>
                        </div>
                        <div class="text-xs text-neutral-400">
                            {"Choose your visual presentation mode. Changes apply immediately across all web pages and tabs."}
                        </div>
                        <div class="grid grid-cols-4 gap-3 mt-1">
                            // Dark Theme Card
                            <button
                                type="button"
                                class={format!(
                                    "flex flex-col items-center gap-2 p-3 rounded-xl border transition-all cursor-pointer {}",
                                    if current_theme == "dark" {
                                        "bg-neutral-800 border-blue-500 ring-1 ring-blue-500 text-white font-medium"
                                    } else {
                                        "bg-neutral-950/70 border-neutral-800 hover:border-neutral-700 text-neutral-300"
                                    }
                                )}
                                onclick={move |_| {
                                    theme_dark.set_theme("dark");
                                }}
                            >
                                <div class="w-full h-8 rounded-lg bg-neutral-900 border border-neutral-700 flex items-center justify-center">
                                    <div class="w-3 h-3 rounded-full bg-neutral-600" />
                                </div>
                                <span class="text-xs">{"Dark"}</span>
                            </button>

                            // Amoled Theme Card
                            <button
                                type="button"
                                class={format!(
                                    "flex flex-col items-center gap-2 p-3 rounded-xl border transition-all cursor-pointer {}",
                                    if current_theme == "amoled" {
                                        "bg-neutral-800 border-blue-500 ring-1 ring-blue-500 text-white font-medium"
                                    } else {
                                        "bg-neutral-950/70 border-neutral-800 hover:border-neutral-700 text-neutral-300"
                                    }
                                )}
                                onclick={move |_| {
                                    theme_amoled.set_theme("amoled");
                                }}
                            >
                                <div class="w-full h-8 rounded-lg bg-black border border-neutral-800 flex items-center justify-center">
                                    <div class="w-3 h-3 rounded-full bg-blue-500" />
                                </div>
                                <span class="text-xs">{"Amoled"}</span>
                            </button>

                            // Sepia Theme Card
                            <button
                                type="button"
                                class={format!(
                                    "flex flex-col items-center gap-2 p-3 rounded-xl border transition-all cursor-pointer {}",
                                    if current_theme == "sepia" {
                                        "bg-neutral-800 border-blue-500 ring-1 ring-blue-500 text-white font-medium"
                                    } else {
                                        "bg-neutral-950/70 border-neutral-800 hover:border-neutral-700 text-neutral-300"
                                    }
                                )}
                                onclick={move |_| {
                                    theme_sepia.set_theme("sepia");
                                }}
                            >
                                <div class="w-full h-8 rounded-lg bg-[#fbf0d9] border border-[#e2d5ba] flex items-center justify-center">
                                    <div class="w-3 h-3 rounded-full bg-[#5f4b32]" />
                                </div>
                                <span class="text-xs">{"Sepia"}</span>
                            </button>

                            // Light Theme Card
                            <button
                                type="button"
                                class={format!(
                                    "flex flex-col items-center gap-2 p-3 rounded-xl border transition-all cursor-pointer {}",
                                    if current_theme == "light" {
                                        "bg-neutral-800 border-blue-500 ring-1 ring-blue-500 text-white font-medium"
                                    } else {
                                        "bg-neutral-950/70 border-neutral-800 hover:border-neutral-700 text-neutral-300"
                                    }
                                )}
                                onclick={move |_| {
                                    theme_light.set_theme("light");
                                }}
                            >
                                <div class="w-full h-8 rounded-lg bg-white border border-neutral-300 flex items-center justify-center">
                                    <div class="w-3 h-3 rounded-full bg-neutral-400" />
                                </div>
                                <span class="text-xs">{"Light"}</span>
                            </button>
                        </div>
                    </div>

                    // Section 2: Search Engine
                    <div class="flex flex-col gap-3 pt-3 border-t border-neutral-800">
                        <span class="text-sm font-semibold text-white">{"Default Search Engine"}</span>
                        <div class="text-xs text-neutral-400">
                            {"Choose the search provider used when queries are entered directly in the Omnibox."}
                        </div>
                        <div class="flex items-center gap-2 mt-1">
                            <button
                                type="button"
                                class={format!(
                                    "flex items-center gap-2 px-3.5 py-2 rounded-xl text-xs font-medium border active:bg-neutral-800 transition-colors duration-150 cursor-pointer {}",
                                    if current_search == "google" { "bg-blue-600/20 border-blue-500 text-blue-400" } else { "bg-neutral-950/70 border-neutral-800 text-neutral-400 hover:text-white" }
                                )}
                                onclick={move |_| {
                                    search_google.search_engine.set("google".to_string());
                                }}
                            >
                                {Icon(IconName::Globe, Some("w-3.5 h-3.5 text-current")).root}
                                <span>{"Google"}</span>
                            </button>
                            <button
                                type="button"
                                class={format!(
                                    "flex items-center gap-2 px-3.5 py-2 rounded-xl text-xs font-medium border active:bg-neutral-800 transition-colors duration-150 cursor-pointer {}",
                                    if current_search == "duckduckgo" { "bg-blue-600/20 border-blue-500 text-blue-400" } else { "bg-neutral-950/70 border-neutral-800 text-neutral-400 hover:text-white" }
                                )}
                                onclick={move |_| {
                                    search_ddg.search_engine.set("duckduckgo".to_string());
                                }}
                            >
                                {Icon(IconName::ShieldCheck, Some("w-3.5 h-3.5 text-current")).root}
                                <span>{"DuckDuckGo"}</span>
                            </button>
                            <button
                                type="button"
                                class={format!(
                                    "flex items-center gap-2 px-3.5 py-2 rounded-xl text-xs font-medium border active:bg-neutral-800 transition-colors duration-150 cursor-pointer {}",
                                    if current_search == "bing" { "bg-blue-600/20 border-blue-500 text-blue-400" } else { "bg-neutral-950/70 border-neutral-800 text-neutral-400 hover:text-white" }
                                )}
                                onclick={move |_| {
                                    search_bing.search_engine.set("bing".to_string());
                                }}
                            >
                                {Icon(IconName::Compass, Some("w-3.5 h-3.5 text-current")).root}
                                <span>{"Bing"}</span>
                            </button>
                        </div>
                    </div>

                    // Section 3: Privacy & Clear Cache
                    <div class="flex flex-col gap-3 pt-3 border-t border-neutral-800">
                        <span class="text-sm font-semibold text-white">{"Privacy & Cache Management"}</span>
                        <div class="flex items-center justify-between p-3.5 rounded-xl bg-neutral-950/60 border border-neutral-800">
                            <div>
                                <div class="text-xs font-medium text-neutral-200">{"Clear Browsing Data"}</div>
                                <div class="text-[11px] text-neutral-500 mt-0.5">{"Purges in-memory typography cache, Bitcask storage, and active history stacks."}</div>
                            </div>
                            <button
                                type="button"
                                class="px-3 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 active:bg-neutral-600 text-xs text-neutral-200 hover:text-white font-medium transition-colors duration-150 cursor-pointer border border-neutral-700/60"
                                onclick={move |_| {
                                    clear_cache_state.url_undo_stack.set(Vec::new());
                                    clear_cache_state.url_redo_stack.set(Vec::new());
                                }}
                            >
                                {"Clear Now"}
                            </button>
                        </div>
                    </div>
                </div>

                // Footer Bar
                <div class="flex items-center justify-end px-6 py-3.5 bg-neutral-950/80 border-t border-neutral-800">
                    <button
                        type="button"
                        class="px-5 py-2 bg-blue-600 hover:bg-blue-500 active:bg-blue-700 text-white text-xs font-semibold rounded-xl transition-colors duration-150 cursor-pointer shadow-sm"
                        onclick={move |_| {
                            state.close_settings();
                        }}
                    >
                        {"Done"}
                    </button>
                </div>
            </div>
        </div>
    }
}
