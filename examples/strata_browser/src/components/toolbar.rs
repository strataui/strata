use crate::state::{BrowserState, ContextTarget};
use std::rc::Rc;
use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};
use strata_ui::prelude::*;

#[component]
pub fn Toolbar(state: BrowserState) -> View {
    let url_value = state.url_input.get();
    let can_back = state.can_go_back();
    let can_fwd = state.can_go_forward();

    let back_state = state.clone();
    let fwd_state = state.clone();
    let reload_state = state.clone();
    let home_state = state.clone();

    // Kebab Menu Handlers
    let kebab_toggle = state.clone();
    let menu_new_tab = state.clone();
    let menu_new_win = state.clone();
    let menu_incognito = state.clone();
    let menu_history = state.clone();
    let menu_downloads = state.clone();
    let menu_bookmarks = state.clone();
    let menu_zoom_in = state.clone();
    let menu_zoom_out = state.clone();
    let menu_zoom_reset = state.clone();
    let menu_print = state.clone();
    let menu_find = state.clone();
    let menu_cut = state.clone();
    let menu_copy = state.clone();
    let menu_paste = state.clone();
    let menu_settings = state.clone();
    let menu_help = state.clone();
    let is_menu_open = state.is_kebab_menu_open.get();
    let current_zoom = state.zoom_level.get();

    // Bookmarks & Omnibox Context
    let bookmarks = state.bookmarks.get();
    let bm_bar_ctx_state = state.clone();
    let add_bm_state = state.clone();
    let omni_ctx_state = state.clone();

    // Omnibox & Menu State Handles
    let is_omnibox_editing = state.is_omnibox_editing.get();
    let input_on_focus = state.clone();
    let input_on_input = state.clone();
    let input_on_submit = state.clone();
    let input_on_blur = state.clone();
    let backdrop_close = state.clone();

    // Security indicator icon and color
    let (sec_icon, sec_color) = if url_value.starts_with("strata://") {
        (IconName::ShieldCheck, "text-blue-400")
    } else if url_value.starts_with("https://") {
        (IconName::Lock, "text-emerald-400")
    } else {
        (IconName::Globe, "text-neutral-400")
    };

    let is_loading = state.active_tab().map(|t| t.is_loading).unwrap_or(false);
    let back_opacity = if can_back {
        "text-neutral-300 hover:text-white hover:bg-neutral-800 active:bg-neutral-700 cursor-pointer"
    } else {
        "text-neutral-600 pointer-events-none opacity-50"
    };
    let fwd_opacity = if can_fwd {
        "text-neutral-300 hover:text-white hover:bg-neutral-800 active:bg-neutral-700 cursor-pointer"
    } else {
        "text-neutral-600 pointer-events-none opacity-50"
    };

    rsx! {
        <div class="relative z-40 flex flex-col bg-neutral-900 border-b border-neutral-800 select-none">
            // Main Navigation Controls and Omnibox
            <div class="flex items-center gap-2 px-3 py-2">
                // Back Button
                <button
                    type="button"
                    class={format!("p-1.5 rounded-lg transition-colors duration-150 {}", back_opacity)}
                    onclick={move |_| {
                        back_state.go_back();
                    }}
                >
                    {Icon(IconName::ChevronLeft, Some("w-4 h-4 text-current")).root}
                </button>

                // Forward Button
                <button
                    type="button"
                    class={format!("p-1.5 rounded-lg transition-colors duration-150 {}", fwd_opacity)}
                    onclick={move |_| {
                        fwd_state.go_forward();
                    }}
                >
                    {Icon(IconName::ChevronRight, Some("w-4 h-4 text-current")).root}
                </button>

                // Reload Button
                <button
                    type="button"
                    class="p-1.5 rounded-lg text-neutral-300 hover:text-white hover:bg-neutral-800 active:bg-neutral-700 transition-colors duration-150 cursor-pointer"
                    onclick={move |_| {
                        reload_state.reload();
                    }}
                >
                    {if is_loading {
                        Icon(IconName::RefreshCw, Some("w-3.5 h-3.5 text-blue-400 animate-spin")).root
                    } else {
                        Icon(IconName::RefreshCw, Some("w-3.5 h-3.5 text-current")).root
                    }}
                </button>

                // Home Button
                <button
                    type="button"
                    class="p-1.5 rounded-lg text-neutral-300 hover:text-white hover:bg-neutral-800 active:bg-neutral-700 transition-colors duration-150 cursor-pointer"
                    onclick={move |_| {
                        home_state.go_home();
                    }}
                >
                    {Icon(IconName::Home, Some("w-4 h-4 text-current")).root}
                </button>

                // Omnibox URL Bar
                <div
                    class={format!("flex-1 h-9 flex items-center bg-neutral-950 {} rounded-lg px-3.5 transition-all duration-200 overflow-hidden", if is_omnibox_editing { "border border-blue-500 shadow-sm shadow-blue-500/20" } else { "border border-neutral-700/80 hover:border-neutral-500" })}
                    oncontextmenu={move |e: Event| {
                        e.stop_propagation();
                        omni_ctx_state.open_context_menu_for(
                            e.client_x as f32,
                            e.client_y as f32,
                            ContextTarget::Omnibox,
                        );
                    }}
                >
                    <div class={format!("flex items-center mr-2 flex-shrink-0 {}", sec_color)}>
                        {Icon(sec_icon, Some("w-3.5 h-3.5 text-current")).root}
                    </div>
                    <input
                        type="text"
                        id="omnibox-input"
                        class="w-full h-full bg-transparent border-0 outline-none text-xs text-white placeholder-neutral-500 font-mono flex items-center leading-normal"
                        placeholder="Enter URL or search query..."
                        value={url_value.clone()}
                        onfocus={move |_| {
                            input_on_focus.is_omnibox_editing.set(true);
                        }}
                        onblur={move |_| {
                            input_on_blur.is_omnibox_editing.set(false);
                        }}
                        oninput={move |val: String| {
                            input_on_input.url_input.set(val);
                        }}
                        onsubmit={move |val: String| {
                            input_on_submit.navigate(&val);
                            input_on_submit.is_omnibox_editing.set(false);
                        }}
                    />
                </div>

                // Chromium 3-dot Kebab Menu Button & Dropdown
                <div class="relative">
                    <button
                        type="button"
                        class="p-1.5 rounded-lg text-neutral-300 hover:text-white hover:bg-neutral-800 active:bg-neutral-700 transition-colors duration-150 cursor-pointer"
                        onclick={move |_| {
                            kebab_toggle.toggle_kebab_menu();
                        }}
                    >
                        {Icon(IconName::MoreVertical, Some("w-4 h-4 text-current")).root}
                    </button>

                    // Backdrop overlay to close menu on outside click
                    {if is_menu_open {
                        rsx! {
                            <div
                                class="fixed inset-0 z-40"
                                onclick={move |_| {
                                    backdrop_close.close_kebab_menu();
                                }}
                            />
                        }
                    } else {
                        rsx! { <div class="hidden" /> }
                    }}

                    {DropdownMenu(
                        is_menu_open,
                        Some("w-72"),
                        vec![
                            MenuItem("New tab".to_string(), None, Some("Ctrl+T".to_string()), None, None, Some(Rc::new({
                                let s = menu_new_tab.clone();
                                move || {
                                    s.new_tab("strata://newtab");
                                    s.close_kebab_menu();
                                }
                            }))).root,
                            MenuItem("New window".to_string(), None, Some("Ctrl+N".to_string()), None, None, Some(Rc::new({
                                let s = menu_new_win.clone();
                                move || {
                                    s.new_tab("strata://newtab");
                                    s.close_kebab_menu();
                                }
                            }))).root,
                            MenuItem("New incognito window".to_string(), None, Some("Ctrl+Shift+N".to_string()), None, None, Some(Rc::new({
                                let s = menu_incognito.clone();
                                move || {
                                    s.new_tab("strata://newtab");
                                    s.close_kebab_menu();
                                }
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("History".to_string(), None, Some("\u{25B6}".to_string()), None, None, Some(Rc::new({
                                let s = menu_history.clone();
                                move || {
                                    s.navigate("strata://about");
                                    s.close_kebab_menu();
                                }
                            }))).root,
                            MenuItem("Downloads".to_string(), None, Some("Ctrl+J".to_string()), None, None, Some(Rc::new({
                                let s = menu_downloads.clone();
                                move || {
                                    s.close_kebab_menu();
                                }
                            }))).root,
                            MenuItem("Bookmarks".to_string(), None, Some("\u{25B6}".to_string()), None, None, Some(Rc::new({
                                let s = menu_bookmarks.clone();
                                move || {
                                    s.close_kebab_menu();
                                }
                            }))).root,
                            MenuSeparator().root,
                            // Zoom custom row
                            rsx! {
                                <div class="w-full flex items-center justify-between px-4 py-1.5 text-neutral-300">
                                    <span>{"Zoom"}</span>
                                    <div class="flex items-center gap-2">
                                        <button
                                            type="button"
                                            class="w-6 h-6 flex items-center justify-center rounded hover:bg-neutral-700 text-white font-bold transition-colors cursor-pointer"
                                            onclick={move |_| {
                                                menu_zoom_out.zoom_out();
                                            }}
                                        >
                                            {"-"}
                                        </button>
                                        <button
                                            type="button"
                                            class="px-1.5 py-0.5 rounded text-neutral-300 hover:text-white font-mono text-xs cursor-pointer"
                                            onclick={move |_| {
                                                menu_zoom_reset.zoom_reset();
                                            }}
                                        >
                                            {format!("{}%", current_zoom)}
                                        </button>
                                        <button
                                            type="button"
                                            class="w-6 h-6 flex items-center justify-center rounded hover:bg-neutral-700 text-white font-bold transition-colors cursor-pointer"
                                            onclick={move |_| {
                                                menu_zoom_in.zoom_in();
                                            }}
                                        >
                                            {"+"}
                                        </button>
                                        <button
                                            type="button"
                                            class="p-1 rounded hover:bg-neutral-700 text-neutral-300 hover:text-white transition-colors cursor-pointer ml-1"
                                            onclick={|_| {
                                                #[cfg(target_os = "linux")]
                                                strata_platform::gtk_toggle_maximize();
                                            }}
                                        >
                                            {Icon(IconName::Maximize, Some("w-3.5 h-3.5 text-current")).root}
                                        </button>
                                    </div>
                                </div>
                            }.root,
                            MenuSeparator().root,
                            MenuItem("Print...".to_string(), None, Some("Ctrl+P".to_string()), None, None, Some(Rc::new({
                                let s = menu_print.clone();
                                move || {
                                    s.close_kebab_menu();
                                }
                            }))).root,
                            MenuItem("Find...".to_string(), None, Some("Ctrl+F".to_string()), None, None, Some(Rc::new({
                                let s = menu_find.clone();
                                move || {
                                    s.start_editing_omnibox();
                                    s.close_kebab_menu();
                                }
                            }))).root,
                            MenuSeparator().root,
                            // Edit custom row
                            rsx! {
                                <div class="w-full flex items-center justify-between px-4 py-1.5 text-neutral-300">
                                    <span>{"Edit"}</span>
                                    <div class="flex items-center gap-2">
                                        <button
                                            type="button"
                                            class="px-2 py-0.5 rounded hover:bg-neutral-700 text-xs text-neutral-200 hover:text-white transition-colors cursor-pointer"
                                            onclick={move |_| {
                                                let text = menu_cut.url_input.get();
                                                #[cfg(target_os = "linux")]
                                                strata_platform::gtk_set_clipboard_text(&text);
                                                menu_cut.push_url_undo(text);
                                                menu_cut.url_input.set(String::new());
                                                menu_cut.close_kebab_menu();
                                            }}
                                        >
                                            {"Cut"}
                                        </button>
                                        <button
                                            type="button"
                                            class="px-2 py-0.5 rounded hover:bg-neutral-700 text-xs text-neutral-200 hover:text-white transition-colors cursor-pointer"
                                            onclick={move |_| {
                                                let text = menu_copy.url_input.get();
                                                #[cfg(target_os = "linux")]
                                                strata_platform::gtk_set_clipboard_text(&text);
                                                menu_copy.close_kebab_menu();
                                            }}
                                        >
                                            {"Copy"}
                                        </button>
                                        <button
                                            type="button"
                                            class="px-2 py-0.5 rounded hover:bg-neutral-700 text-xs text-neutral-200 hover:text-white transition-colors cursor-pointer"
                                            onclick={move |_| {
                                                #[cfg(target_os = "linux")]
                                                if let Some(clip) = strata_platform::gtk_get_clipboard_text() {
                                                    let prev = menu_paste.url_input.get();
                                                    menu_paste.push_url_undo(prev);
                                                    menu_paste.url_input.set(clip);
                                                }
                                                menu_paste.close_kebab_menu();
                                            }}
                                        >
                                            {"Paste"}
                                        </button>
                                    </div>
                                </div>
                            }.root,
                            MenuSeparator().root,
                            MenuItem("Settings".to_string(), Some(IconName::Settings), None, None, None, Some(Rc::new({
                                let s = menu_settings.clone();
                                move || {
                                    s.open_settings();
                                }
                            }))).root,
                            MenuItem("Help".to_string(), None, Some("\u{25B6}".to_string()), None, None, Some(Rc::new({
                                let s = menu_help.clone();
                                move || {
                                    s.navigate("strata://about");
                                    s.close_kebab_menu();
                                }
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("Exit".to_string(), None, None, None, Some(true), Some(Rc::new(|| {
                                #[cfg(target_os = "linux")]
                                strata_platform::gtk_close();
                            }))).root,
                        ]
                    ).root}
                </div>
            </div>

            // Bookmarks Quick Access Bar (whitespace-nowrap & vertically centered icons)
            // Shown exclusively on the New Tab page per browser design standard
            { if state.active_tab().map_or(false, |t| t.url.starts_with("strata://newtab") || t.url.is_empty() || t.url == "about:blank") {
                rsx! {
                    <div
                        class="flex items-center gap-1.5 px-3 py-1 bg-neutral-950/60 border-t border-neutral-800/60 text-xs text-neutral-400 overflow-x-auto whitespace-nowrap flex-nowrap"
                        oncontextmenu={move |e: Event| {
                            bm_bar_ctx_state.open_context_menu_for(
                                e.client_x as f32,
                                e.client_y as f32,
                                ContextTarget::BookmarksBar,
                            );
                        }}
                    >
                        {bookmarks.into_iter().map(|bm| {
                            let bm_id = bm.id;
                            let bm_url = bm.url.clone();
                            let bm_nav = state.clone();
                            let bm_ctx = state.clone();

                            let icon = if bm.url.starts_with("strata://newtab") {
                                IconName::Home
                            } else if bm.url.contains("google.com") {
                                IconName::Globe
                            } else if bm.url.contains("search") {
                                IconName::Search
                            } else if bm.url.contains("wikipedia.org") {
                                IconName::Code
                            } else if bm.url.contains("ycombinator.com") {
                                IconName::Terminal
                            } else if bm.url.contains("strata.larvance.com") {
                                IconName::ShieldCheck
                            } else {
                                IconName::Globe
                            };

                            rsx! {
                                <button
                                    type="button"
                                    class="flex items-center gap-1.5 px-2.5 py-1 rounded-md hover:bg-neutral-800 hover:text-white active:bg-neutral-700 transition-colors duration-150 cursor-pointer whitespace-nowrap flex-shrink-0 text-neutral-300 font-medium"
                                    onclick={move |_| {
                                        bm_nav.navigate(&bm_url);
                                    }}
                                    oncontextmenu={move |e: Event| {
                                        e.stop_propagation();
                                        bm_ctx.open_context_menu_for(
                                            e.client_x as f32,
                                            e.client_y as f32,
                                            ContextTarget::Bookmark(bm_id),
                                        );
                                    }}
                                >
                                    <div class="flex items-center justify-center flex-shrink-0">
                                        {Icon(icon, Some("w-3.5 h-3.5 text-current")).root}
                                    </div>
                                    <span class="whitespace-nowrap flex-shrink-0">{bm.title}</span>
                                </button>
                            }
                        })}

                        <button
                            type="button"
                            title="Add Bookmark"
                            class="flex items-center justify-center w-6 h-6 rounded-md hover:bg-neutral-800 hover:text-white text-neutral-400 active:bg-neutral-700 transition-colors duration-150 cursor-pointer ml-1 flex-shrink-0 text-xs"
                            onclick={move |_| {
                                add_bm_state.open_bookmark_editor(None);
                            }}
                        >
                            {Icon(IconName::Plus, Some("w-3.5 h-3.5 text-current")).root}
                        </button>
                    </div>
                }
            } else {
                rsx! {
                    <div class="hidden" />
                }
            }}
        </div>
    }
}
