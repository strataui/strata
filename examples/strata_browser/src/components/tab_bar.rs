//! Interactive tab bar and tab switching controls.

use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};
use crate::state::BrowserState;

#[component]
pub fn TabBar(state: BrowserState) -> View {
    let active_id = state.active_tab_id.get();
    let tabs = state.tabs.get();
    let new_tab_state = state.clone();

    rsx! {
        <div class="flex items-center gap-1.5 px-3 pt-2 bg-neutral-950 border-b border-neutral-800/80 select-none">
            // Tabs List
            <div class="flex items-center gap-1 overflow-x-auto flex-1 min-w-0">
                {tabs.into_iter().map(|tab| {
                    let is_active = tab.id == active_id;
                    let tab_id = tab.id;
                    let click_state = state.clone();
                    let close_state = state.clone();

                    let icon = if tab.url.starts_with("strata://newtab") || tab.url.contains("google.com") {
                        IconName::Search
                    } else if tab.url.contains("wikipedia.org") {
                        IconName::Code
                    } else if tab.url.contains("ycombinator.com") {
                        IconName::Terminal
                    } else if tab.url.contains("strata.larvance.com") {
                        IconName::ShieldCheck
                    } else {
                        IconName::Globe
                    };

                    let (bg, border, text_color) = if is_active {
                        ("bg-neutral-900", "border-neutral-700/80", "text-white")
                    } else {
                        ("bg-neutral-950 hover:bg-neutral-900/60", "border-transparent", "text-neutral-400 hover:text-neutral-200")
                    };

                    rsx! {
                        <div
                            class={format!("group flex items-center gap-2 h-9 px-3 rounded-t-lg border-t border-x w-44 {} {} {} transition-colors cursor-pointer select-none", bg, border, text_color)}
                            onclick={move |_| {
                                click_state.switch_tab(tab_id);
                            }}
                        >
                            {Icon(icon, Some("w-3.5 h-3.5 text-neutral-400 flex-shrink-0")).root}
                            <span class="truncate text-xs font-medium flex-1 min-w-0">
                                {tab.title}
                            </span>
                            <button
                                type="button"
                                class="flex items-center justify-center w-5 h-5 rounded hover:bg-neutral-700/70 text-neutral-400 hover:text-white transition-colors cursor-pointer text-sm font-semibold flex-shrink-0 ml-1"
                                onclick={move |e: Event| {
                                    e.stop_propagation();
                                    close_state.close_tab(tab_id);
                                }}
                            >
                                {"×"}
                            </button>
                        </div>
                    }
                })}

                // Plus New Tab Button
                <button
                    type="button"
                    class="flex items-center justify-center w-7 h-7 rounded-lg hover:bg-neutral-800 text-neutral-400 hover:text-white transition-colors cursor-pointer ml-1 text-base font-bold"
                    onclick={move |_| {
                        new_tab_state.new_tab("strata://newtab");
                    }}
                >
                    {"+"}
                </button>
            </div>

            // Top-Right Window Controls (CSD Frameless Window)
            <div class="flex items-center gap-1 ml-3 flex-shrink-0">
                <button
                    type="button"
                    class="flex items-center justify-center w-8 h-7 rounded hover:bg-neutral-800 text-neutral-400 hover:text-white transition-colors cursor-pointer text-sm font-bold"
                    onclick={move |_| {
                        #[cfg(target_os = "linux")]
                        strata::gtk_minimize();
                    }}
                >
                    {"−"}
                </button>
                <button
                    type="button"
                    class="flex items-center justify-center w-8 h-7 rounded hover:bg-neutral-800 text-neutral-400 hover:text-white transition-colors cursor-pointer text-xs"
                    onclick={move |_| {
                        #[cfg(target_os = "linux")]
                        strata::gtk_toggle_maximize();
                    }}
                >
                    {"□"}
                </button>
                <button
                    type="button"
                    class="flex items-center justify-center w-8 h-7 rounded hover:bg-red-600 text-neutral-400 hover:text-white transition-colors cursor-pointer text-xs font-semibold"
                    onclick={move |_| {
                        #[cfg(target_os = "linux")]
                        strata::gtk_close();
                    }}
                >
                    {"✕"}
                </button>
            </div>
        </div>
    }
}
