//! Bookmark Modal Component for Strata Browser.
//!
//! Provides Chromium-parity bookmark creation and editing dialog with Name and URL inputs,
//! validation, and instant persistence.

use crate::state::BrowserState;
use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};

#[component]
pub fn BookmarkModal(state: BrowserState) -> View {
    let Some(editor) = state.bookmark_editor.get() else {
        return rsx! { <div class="hidden" /> };
    };

    let is_edit = editor.bookmark_id.is_some();
    let modal_title = if is_edit {
        "Edit Bookmark"
    } else {
        "Add Bookmark"
    };
    let modal_desc = if is_edit {
        "Update the bookmark's name or destination address."
    } else {
        "Save a page to your personal quick access bookmarks bar."
    };

    let title_val = Signal::new(editor.title_input);
    let url_val = Signal::new(editor.url_input);

    let close_header_state = state.clone();
    let close_footer_state = state.clone();
    let backdrop_close = state.clone();
    let save_state = state.clone();

    let title_for_input = title_val.clone();
    let url_for_input = url_val.clone();
    let title_for_save = title_val.clone();
    let url_for_save = url_val.clone();

    rsx! {
        <div
            class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm select-none cursor-pointer transition-opacity duration-200"
            onclick={move |_| {
                backdrop_close.close_bookmark_editor();
            }}
        >
            <div
                class="w-[480px] bg-neutral-900 border border-neutral-700/80 rounded-2xl shadow-2xl overflow-hidden flex flex-col cursor-default transition-all duration-200 ease-out"
                onclick={|e: Event| {
                    e.stop_propagation();
                }}
            >
                // Header Bar
                <div class="flex items-center justify-between px-6 py-4 bg-neutral-950/80 border-b border-neutral-800">
                    <div class="flex items-center gap-3">
                        <div class="p-2 rounded-xl bg-blue-600/20 text-blue-400">
                            {Icon(IconName::Bookmark, Some("w-5 h-5 text-current")).root}
                        </div>
                        <div>
                            <div class="text-base font-semibold text-white tracking-wide">
                                {modal_title}
                            </div>
                            <div class="text-xs text-neutral-400 font-mono">
                                {modal_desc}
                            </div>
                        </div>
                    </div>
                    <button
                        type="button"
                        class="p-2 rounded-xl text-neutral-400 hover:text-white hover:bg-neutral-800 active:bg-neutral-700 transition-colors duration-150 cursor-pointer"
                        onclick={move |_| {
                            close_header_state.close_bookmark_editor();
                        }}
                    >
                        {Icon(IconName::X, Some("w-4 h-4 text-current")).root}
                    </button>
                </div>

                // Form Body
                <div class="p-6 flex flex-col gap-4">
                    // Name Field
                    <div class="flex flex-col gap-1.5">
                        <label class="text-xs font-medium text-neutral-300">
                            {"Name"}
                        </label>
                        <div class="h-9 px-3 bg-neutral-950 border border-neutral-700/80 rounded-lg flex items-center focus-within:border-blue-500 transition-colors">
                            <input
                                type="text"
                                id="bookmark-name-input"
                                class="w-full bg-transparent border-0 outline-none text-xs text-white placeholder-neutral-500 font-sans leading-normal"
                                placeholder="Bookmark Name"
                                value={title_val.get()}
                                oninput={move |val: String| {
                                    title_for_input.set(val);
                                }}
                            />
                        </div>
                    </div>

                    // URL Field
                    <div class="flex flex-col gap-1.5">
                        <label class="text-xs font-medium text-neutral-300">
                            {"URL"}
                        </label>
                        <div class="h-9 px-3 bg-neutral-950 border border-neutral-700/80 rounded-lg flex items-center focus-within:border-blue-500 transition-colors">
                            <input
                                type="text"
                                id="bookmark-url-input"
                                class="w-full bg-transparent border-0 outline-none text-xs text-white placeholder-neutral-500 font-mono leading-normal"
                                placeholder="https://example.com"
                                value={url_val.get()}
                                oninput={move |val: String| {
                                    url_for_input.set(val);
                                }}
                            />
                        </div>
                    </div>
                </div>

                // Footer Controls
                <div class="flex items-center justify-end gap-2 px-6 py-4 bg-neutral-950/60 border-t border-neutral-800/80">
                    <button
                        type="button"
                        class="px-4 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:text-white hover:bg-neutral-800 active:bg-neutral-700 transition-colors duration-150 cursor-pointer"
                        onclick={move |_| {
                            close_footer_state.close_bookmark_editor();
                        }}
                    >
                        {"Cancel"}
                    </button>
                    <button
                        type="button"
                        class="px-4 py-2 rounded-lg bg-blue-600 hover:bg-blue-500 active:bg-blue-700 text-xs font-semibold text-white shadow-md transition-colors duration-150 cursor-pointer"
                        onclick={move |_| {
                            let t = title_for_save.get();
                            let u = url_for_save.get();
                            save_state.save_bookmark_editor(&t, &u);
                        }}
                    >
                        {"Save"}
                    </button>
                </div>
            </div>
        </div>
    }
}
