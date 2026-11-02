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
            // Window Traffic Lights
            <div class="flex items-center gap-1.5 mr-3 pl-1">
                <div class="w-3 h-3 rounded-full bg-rose-500/80"></div>
                <div class="w-3 h-3 rounded-full bg-amber-500/80"></div>
                <div class="w-3 h-3 rounded-full bg-emerald-500/80"></div>
            </div>

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
                            class={format!("group flex items-center gap-2 h-9 px-3.5 rounded-t-lg border-t border-x {} {} {} transition-colors cursor-pointer max-w-[200px] min-w-[120px]", bg, border, text_color)}
                            onclick={move |_| {
                                click_state.switch_tab(tab_id);
                            }}
                        >
                            <div class="flex-shrink-0 text-neutral-400">
                                {Icon(icon, Some("w-3.5 h-3.5 text-current")).root}
                            </div>
                            <span class="truncate text-xs font-medium flex-1">
                                {tab.title}
                            </span>
                            <button
                                type="button"
                                class="flex-shrink-0 p-0.5 rounded hover:bg-neutral-700/70 text-neutral-400 hover:text-white transition-colors cursor-pointer ml-1"
                                onclick={move |e: Event| {
                                    e.stop_propagation();
                                    close_state.close_tab(tab_id);
                                }}
                            >
                                {Icon(IconName::X, Some("w-3 h-3 text-current")).root}
                            </button>
                        </div>
                    }
                })}

                // Plus New Tab Button
                <button
                    type="button"
                    class="p-1.5 rounded-lg hover:bg-neutral-800 text-neutral-400 hover:text-white transition-colors cursor-pointer ml-1"
                    onclick={move |_| {
                        new_tab_state.new_tab("strata://newtab");
                    }}
                >
                    {Icon(IconName::Plus, Some("w-4 h-4 text-current")).root}
                </button>
            </div>
        </div>
    }
}
