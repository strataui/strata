//! Root application layout for the Strata Native Browser.

use crate::components::devtools::DevToolsPanel;
use crate::components::tab_bar::TabBar;
use crate::components::toolbar::Toolbar;
use crate::components::viewport::Viewport;
use crate::state::{BrowserState, ContextTarget};
use std::rc::Rc;
use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};
use strata_ui::prelude::*;

#[component]
pub fn BrowserApp(state: BrowserState) -> View {
    let is_devtools = state.is_devtools_open.get();
    let theme_mode = state.active_theme.get();
    let (root_bg, root_text) = match theme_mode.as_str() {
        "amoled" => ("bg-black", "text-white"),
        "sepia" => ("bg-[#fbf0d9]", "text-[#433422]"),
        "light" => ("bg-neutral-50", "text-neutral-900"),
        _ => ("bg-neutral-950", "text-white"),
    };

    // Context Menu Handlers
    let is_ctx_open = state.is_context_menu_open.get();
    let (ctx_x, ctx_y) = state.context_menu_position.get();
    let target = state.context_menu_target.get();
    let ctx_close = state.clone();

    rsx! {
        <div class={format!("flex flex-col h-screen w-screen {} {} overflow-hidden font-sans", root_bg, root_text)}>
            <TabBar state={state.clone()} />
            <Toolbar state={state.clone()} />
            {if is_devtools {
                let dev_state = state.clone();
                rsx! {
                    <div class="flex-1 flex flex-row min-h-0 w-full overflow-hidden">
                        <Viewport state={state.clone()} />
                        <DevToolsPanel state={dev_state} />
                    </div>
                }
            } else {
                let vp_state = state.clone();
                rsx! {
                    <Viewport state={vp_state} />
                }
            }}
            {if state.is_settings_modal_open.get() {
                let modal_state = state.clone();
                rsx! {
                    <crate::components::SettingsModal state={modal_state} />
                }
            } else {
                rsx! { <div class="hidden" /> }
            }}
            {if state.bookmark_editor.get().is_some() {
                let bm_modal_state = state.clone();
                rsx! {
                    <crate::components::BookmarkModal state={bm_modal_state} />
                }
            } else {
                rsx! { <div class="hidden" /> }
            }}
            {if is_ctx_open {
                let items: Vec<NodeBlueprint> = match target {
                    ContextTarget::Page => {
                        let can_back = state.can_go_back();
                        let can_fwd = state.can_go_forward();
                        let s_back = state.clone();
                        let s_fwd = state.clone();
                        let s_reload = state.clone();
                        let s_bm = state.clone();
                        let s_inspect = state.clone();
                        let s_src = state.clone();
                        let s_settings = state.clone();
                        vec![
                            MenuItem("Back".to_string(), Some(IconName::ChevronLeft), Some("Alt+Left".to_string()), Some(!can_back), None, Some(Rc::new(move || {
                                s_back.go_back();
                                s_back.close_context_menu();
                            }))).root,
                            MenuItem("Forward".to_string(), Some(IconName::ChevronRight), Some("Alt+Right".to_string()), Some(!can_fwd), None, Some(Rc::new(move || {
                                s_fwd.go_forward();
                                s_fwd.close_context_menu();
                            }))).root,
                            MenuItem("Reload".to_string(), Some(IconName::RefreshCw), Some("Ctrl+R".to_string()), None, None, Some(Rc::new(move || {
                                s_reload.reload();
                                s_reload.close_context_menu();
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("Bookmark this tab...".to_string(), Some(IconName::Bookmark), Some("Ctrl+D".to_string()), None, None, Some(Rc::new(move || {
                                s_bm.open_bookmark_editor(None);
                                s_bm.close_context_menu();
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("Inspect".to_string(), Some(IconName::Search), Some("Ctrl+Shift+I".to_string()), None, None, Some(Rc::new(move || {
                                s_inspect.toggle_inspect();
                                s_inspect.close_context_menu();
                            }))).root,
                            MenuItem("View Page Source".to_string(), Some(IconName::Code), None, None, None, Some(Rc::new(move || {
                                s_src.open_devtools(crate::state::DevToolsTab::Console);
                                s_src.close_context_menu();
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("Settings".to_string(), Some(IconName::Settings), None, None, None, Some(Rc::new(move || {
                                s_settings.open_settings();
                                s_settings.close_context_menu();
                            }))).root,
                        ]
                    }
                    ContextTarget::Tab(tab_id) => {
                        let s_new = state.clone();
                        let s_reload = state.clone();
                        let s_dup = state.clone();
                        let s_close = state.clone();
                        let s_close_others = state.clone();
                        let s_close_right = state.clone();
                        vec![
                            MenuItem("New tab".to_string(), Some(IconName::Plus), Some("Ctrl+T".to_string()), None, None, Some(Rc::new(move || {
                                s_new.new_tab("strata://newtab");
                                s_new.close_context_menu();
                            }))).root,
                            MenuItem("Reload tab".to_string(), Some(IconName::RefreshCw), Some("Ctrl+R".to_string()), None, None, Some(Rc::new(move || {
                                s_reload.reload();
                                s_reload.close_context_menu();
                            }))).root,
                            MenuItem("Duplicate tab".to_string(), Some(IconName::Copy), None, None, None, Some(Rc::new(move || {
                                s_dup.duplicate_tab(tab_id);
                                s_dup.close_context_menu();
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("Close tab".to_string(), Some(IconName::X), Some("Ctrl+W".to_string()), None, None, Some(Rc::new(move || {
                                s_close.close_tab(tab_id);
                                s_close.close_context_menu();
                            }))).root,
                            MenuItem("Close other tabs".to_string(), None, None, None, None, Some(Rc::new(move || {
                                s_close_others.close_other_tabs(tab_id);
                                s_close_others.close_context_menu();
                            }))).root,
                            MenuItem("Close tabs to the right".to_string(), None, None, None, None, Some(Rc::new(move || {
                                s_close_right.close_tabs_to_right(tab_id);
                                s_close_right.close_context_menu();
                            }))).root,
                        ]
                    }
                    ContextTarget::TabStrip => {
                        let s_new = state.clone();
                        let s_reopen = state.clone();
                        let s_bm_all = state.clone();
                        let can_reopen = !state.closed_tabs.get().is_empty();
                        vec![
                            MenuItem("New tab".to_string(), Some(IconName::Plus), Some("Ctrl+T".to_string()), None, None, Some(Rc::new(move || {
                                s_new.new_tab("strata://newtab");
                                s_new.close_context_menu();
                            }))).root,
                            MenuItem("Reopen closed tab".to_string(), Some(IconName::RotateCw), Some("Ctrl+Shift+T".to_string()), Some(!can_reopen), None, Some(Rc::new(move || {
                                s_reopen.reopen_closed_tab();
                                s_reopen.close_context_menu();
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("Bookmark all tabs...".to_string(), Some(IconName::Bookmark), None, None, None, Some(Rc::new(move || {
                                for t in s_bm_all.tabs.get() {
                                    s_bm_all.add_bookmark(&t.title, &t.url);
                                }
                                s_bm_all.close_context_menu();
                            }))).root,
                        ]
                    }
                    ContextTarget::Bookmark(bm_id) => {
                        let s_open = state.clone();
                        let s_new_tab = state.clone();
                        let s_edit = state.clone();
                        let s_copy = state.clone();
                        let s_move_left = state.clone();
                        let s_move_right = state.clone();
                        let s_del = state.clone();
                        let bm_opt = state.bookmarks.get().into_iter().find(|b| b.id == bm_id);
                        let bm_url = bm_opt.map(|b| b.url).unwrap_or_default();
                        let bm_url_copy = bm_url.clone();
                        let bm_url_newtab = bm_url.clone();
                        vec![
                            MenuItem("Open bookmark".to_string(), Some(IconName::ExternalLink), None, None, None, Some(Rc::new(move || {
                                s_open.navigate(&bm_url);
                                s_open.close_context_menu();
                            }))).root,
                            MenuItem("Open in new tab".to_string(), Some(IconName::Plus), None, None, None, Some(Rc::new(move || {
                                s_new_tab.new_tab(&bm_url_newtab);
                                s_new_tab.close_context_menu();
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("Edit bookmark...".to_string(), Some(IconName::Edit), None, None, None, Some(Rc::new(move || {
                                s_edit.open_bookmark_editor(Some(bm_id));
                                s_edit.close_context_menu();
                            }))).root,
                            MenuItem("Copy link address".to_string(), Some(IconName::Copy), None, None, None, Some(Rc::new(move || {
                                #[cfg(target_os = "linux")]
                                strata::gtk_set_clipboard_text(&bm_url_copy);
                                s_copy.close_context_menu();
                            }))).root,
                            MenuItem("Move left".to_string(), Some(IconName::ArrowLeft), None, None, None, Some(Rc::new(move || {
                                s_move_left.move_bookmark(bm_id, -1);
                                s_move_left.close_context_menu();
                            }))).root,
                            MenuItem("Move right".to_string(), Some(IconName::ArrowRight), None, None, None, Some(Rc::new(move || {
                                s_move_right.move_bookmark(bm_id, 1);
                                s_move_right.close_context_menu();
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("Delete".to_string(), Some(IconName::Trash), None, None, Some(true), Some(Rc::new(move || {
                                s_del.remove_bookmark(bm_id);
                                s_del.close_context_menu();
                            }))).root,
                        ]
                    }
                    ContextTarget::BookmarksBar => {
                        let s_add_cur = state.clone();
                        let s_add_new = state.clone();
                        vec![
                            MenuItem("Add bookmark for current page...".to_string(), Some(IconName::Bookmark), None, None, None, Some(Rc::new(move || {
                                s_add_cur.open_bookmark_editor(None);
                                s_add_cur.close_context_menu();
                            }))).root,
                            MenuItem("Add new bookmark...".to_string(), Some(IconName::Plus), None, None, None, Some(Rc::new(move || {
                                s_add_new.open_bookmark_editor(None);
                                s_add_new.close_context_menu();
                            }))).root,
                        ]
                    }
                    ContextTarget::Omnibox => {
                        let s_undo = state.clone();
                        let s_redo = state.clone();
                        let s_cut = state.clone();
                        let s_copy = state.clone();
                        let s_paste = state.clone();
                        let s_sel_all = state.clone();
                        vec![
                            MenuItem("Undo".to_string(), None, Some("Ctrl+Z".to_string()), None, None, Some(Rc::new(move || {
                                strata::queue_edit_command(strata::EditCommand::Undo);
                                s_undo.close_context_menu();
                            }))).root,
                            MenuItem("Redo".to_string(), None, Some("Ctrl+Y".to_string()), None, None, Some(Rc::new(move || {
                                strata::queue_edit_command(strata::EditCommand::Redo);
                                s_redo.close_context_menu();
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("Cut".to_string(), Some(IconName::Edit), Some("Ctrl+X".to_string()), None, None, Some(Rc::new(move || {
                                strata::queue_edit_command(strata::EditCommand::Cut);
                                s_cut.close_context_menu();
                            }))).root,
                            MenuItem("Copy".to_string(), Some(IconName::Copy), Some("Ctrl+C".to_string()), None, None, Some(Rc::new(move || {
                                strata::queue_edit_command(strata::EditCommand::Copy);
                                s_copy.close_context_menu();
                            }))).root,
                            MenuItem("Paste".to_string(), None, Some("Ctrl+V".to_string()), None, None, Some(Rc::new(move || {
                                strata::queue_edit_command(strata::EditCommand::Paste);
                                s_paste.close_context_menu();
                            }))).root,
                            MenuSeparator().root,
                            MenuItem("Select all".to_string(), None, Some("Ctrl+A".to_string()), None, None, Some(Rc::new(move || {
                                strata::queue_edit_command(strata::EditCommand::SelectAll);
                                s_sel_all.close_context_menu();
                            }))).root,
                        ]
                    }
                };

                rsx! {
                    <div
                        class="fixed inset-0 z-40"
                        onclick={move |_| {
                            ctx_close.close_context_menu();
                        }}
                    />
                    {ContextMenu(
                        true,
                        (ctx_x, ctx_y),
                        None,
                        items,
                    ).root}
                }
            } else {
                rsx! { <div class="hidden" /> }
            }}
        </div>
    }
}

/// Creates a default BrowserApp View with fresh state.
pub fn default_browser_app() -> View {
    let state = BrowserState::new();
    BrowserApp(state)
}
