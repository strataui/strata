//! Interactive tab bar and tab switching controls.

use crate::state::{BrowserState, ContextTarget};
use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};

#[component]
pub fn TabBar(state: BrowserState) -> View {
    let active_id = state.active_tab_id.get();
    let tabs = state.tabs.get();
    let is_maximized = state.is_window_maximized.get();
    let new_tab_state = state.clone();
    let tab_strip_ctx_state = state.clone();
    let toggle_max_state = state.clone();

    rsx! {
        <div
            data-app-region="drag"
            class="relative z-50 h-11 flex items-center gap-1.5 px-3 bg-neutral-950 border-b border-neutral-800/80 select-none"
        >
            // Tabs List (empty space is draggable and double-click maximizable)
            <div
                class="flex items-center gap-1 overflow-x-auto flex-1 min-w-0"
                oncontextmenu={move |e: Event| {
                    tab_strip_ctx_state.open_context_menu_for(
                        e.client_x as f32,
                        e.client_y as f32,
                        ContextTarget::TabStrip,
                    );
                }}
            >
                {tabs.into_iter().map(|tab| {
                    let is_active = tab.id == active_id;
                    let tab_id = tab.id;
                    let click_state = state.clone();
                    let close_state = state.clone();
                    let middle_close_state = state.clone();
                    let tab_ctx_state = state.clone();

                    let (icon, icon_class) = if tab.is_loading {
                        (IconName::Loader2, "w-3.5 h-3.5 text-blue-400 animate-spin flex-shrink-0")
                    } else if tab.url.starts_with("strata://newtab") || tab.url.contains("google.com") {
                        (IconName::Search, "w-3.5 h-3.5 text-neutral-400 flex-shrink-0")
                    } else if tab.url.contains("wikipedia.org") {
                        (IconName::Code, "w-3.5 h-3.5 text-neutral-400 flex-shrink-0")
                    } else if tab.url.contains("ycombinator.com") {
                        (IconName::Terminal, "w-3.5 h-3.5 text-neutral-400 flex-shrink-0")
                    } else if tab.url.contains("strata.larvance.com") {
                        (IconName::ShieldCheck, "w-3.5 h-3.5 text-neutral-400 flex-shrink-0")
                    } else {
                        (IconName::Globe, "w-3.5 h-3.5 text-neutral-400 flex-shrink-0")
                    };

                    let (bg, border, text_color) = if is_active {
                        ("bg-neutral-900", "border-neutral-700/80", "text-white")
                    } else {
                        ("bg-neutral-950 hover:bg-neutral-900/60", "border-transparent", "text-neutral-400 hover:text-neutral-200")
                    };

                    let display_title = if tab.title.chars().count() > 18 {
                        let mut s: String = tab.title.chars().take(15).collect();
                        s.push_str("...");
                        s
                    } else {
                        tab.title.clone()
                    };

                    rsx! {
                        <div
                            data-app-region="no-drag"
                            class={format!("group flex items-center gap-2 h-9 px-3 rounded-t-lg border-t border-x w-44 {} {} {} transition-all duration-150 ease-out cursor-pointer select-none", bg, border, text_color)}
                            onclick={move |_| {
                                click_state.switch_tab(tab_id);
                            }}
                            onauxclick={move |e: Event| {
                                if e.button == 1 {
                                    e.stop_propagation();
                                    middle_close_state.close_tab(tab_id);
                                }
                            }}
                            oncontextmenu={move |e: Event| {
                                e.stop_propagation();
                                tab_ctx_state.open_context_menu_for(
                                    e.client_x as f32,
                                    e.client_y as f32,
                                    ContextTarget::Tab(tab_id),
                                );
                            }}
                        >
                            {Icon(icon, Some(icon_class)).root}
                            <span class="truncate text-xs font-medium flex-1 min-w-0">
                                {display_title}
                            </span>
                            <button
                                type="button"
                                class="flex items-center justify-center w-5 h-5 rounded hover:bg-neutral-700/70 active:bg-neutral-600 text-neutral-400 hover:text-white transition-colors duration-100 cursor-pointer text-sm font-semibold flex-shrink-0 ml-1"
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
                    data-app-region="no-drag"
                    class="w-7 h-7 flex items-center justify-center rounded-md bg-neutral-900/80 hover:bg-neutral-800 active:bg-neutral-700 text-neutral-200 hover:text-white border border-neutral-700/60 transition-colors duration-150 cursor-pointer ml-1 text-sm font-semibold flex-shrink-0"
                    onclick={move |_| {
                        new_tab_state.new_tab("strata://newtab");
                    }}
                >
                    {"+"}
                </button>
            </div>

            // Top-Right Window Controls (CSD Frameless Window)
            <div
                data-app-region="no-drag"
                class="flex items-center gap-1 ml-3 flex-shrink-0"
            >
                <button
                    type="button"
                    class="flex items-center justify-center w-8 h-7 rounded hover:bg-neutral-800 active:bg-neutral-700 text-neutral-400 hover:text-white transition-colors duration-150 cursor-pointer"
                    onclick={move |_| {
                        #[cfg(target_os = "linux")]
                        strata::gtk_minimize();
                    }}
                >
                    {Icon(IconName::Minus, Some("w-3.5 h-3.5 text-current")).root}
                </button>
                <button
                    type="button"
                    class="flex items-center justify-center w-8 h-7 rounded hover:bg-neutral-800 active:bg-neutral-700 text-neutral-400 hover:text-white transition-colors duration-150 cursor-pointer"
                    onclick={move |_| {
                        #[cfg(target_os = "linux")]
                        {
                            let new_max = strata::gtk_toggle_maximize();
                            toggle_max_state.is_window_maximized.set(new_max);
                        }
                    }}
                >
                    {if is_maximized {
                        Icon(IconName::WindowRestore, Some("w-3.5 h-3.5 text-current")).root
                    } else {
                        Icon(IconName::Square, Some("w-3.5 h-3.5 text-current")).root
                    }}
                </button>
                <button
                    type="button"
                    class="flex items-center justify-center w-8 h-7 rounded hover:bg-red-600 active:bg-red-700 text-neutral-400 hover:text-white transition-colors duration-150 cursor-pointer"
                    onclick={move |_| {
                        #[cfg(target_os = "linux")]
                        strata::gtk_close();
                    }}
                >
                    {Icon(IconName::X, Some("w-3.5 h-3.5 text-current")).root}
                </button>
            </div>
        </div>
    }
}
