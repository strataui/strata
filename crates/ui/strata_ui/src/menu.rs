//! High-Level Accessible Menu, ContextMenu & Dropdown System for Strata UI.

#![allow(non_snake_case)]

use std::rc::Rc;
use strata_dom::{Event, NodeBlueprint, View};
use strata_icons::prelude::{Icon, IconName};
use strata_macro::{component, rsx};

/// Dropdown menu container rendered as a floating popup overlay.
#[component]
pub fn DropdownMenu(
    is_open: bool,
    class: Option<&'static str>,
    children: Vec<NodeBlueprint>,
) -> View {
    if !is_open {
        return rsx! { <div class="hidden" /> };
    }

    let custom = class.unwrap_or("");
    let base = "absolute right-0 top-10 w-72 bg-[#2b2b2b] border border-neutral-700/80 rounded-xl shadow-2xl py-1.5 z-50 text-neutral-200 text-xs font-sans select-none overflow-hidden animate-in fade-in zoom-in-95 duration-100";

    rsx! {
        <div class={format!("{} {}", base, custom)}>
            {children}
        </div>
    }
}

/// Context menu container positioned absolutely at specific coordinate offsets.
#[component]
pub fn ContextMenu(
    is_open: bool,
    position: (f32, f32),
    class: Option<&'static str>,
    children: Vec<NodeBlueprint>,
) -> View {
    if !is_open {
        return rsx! { <div class="hidden" /> };
    }

    let (x, y) = position;
    let custom = class.unwrap_or("");

    rsx! {
        <div
            data-clamp-viewport="true"
            data-keep-focus="true"
            class={format!("fixed w-64 bg-[#2b2b2b] border border-neutral-700/80 rounded-xl shadow-2xl py-1.5 z-50 text-neutral-200 text-xs font-sans select-none overflow-hidden {}", custom)}
            style={format!("left: {}px; top: {}px;", x, y)}
        >
            {children}
        </div>
    }
}

/// Individual interactive item within a Menu or ContextMenu.
#[component]
pub fn MenuItem(
    label: String,
    icon: Option<IconName>,
    shortcut: Option<String>,
    disabled: Option<bool>,
    destructive: Option<bool>,
    onclick: Option<Rc<dyn Fn()>>,
) -> View {
    let is_disabled = disabled.unwrap_or(false);
    let is_destructive = destructive.unwrap_or(false);

    let (text_color, hover_bg) = if is_disabled {
        ("text-neutral-500 pointer-events-none", "")
    } else if is_destructive {
        ("text-rose-400 hover:text-rose-300", "hover:bg-rose-600/20")
    } else {
        ("text-neutral-200 hover:text-white", "hover:bg-blue-600/30")
    };

    let click_handler = onclick.clone();

    rsx! {
        <button
            type="button"
            class={format!("w-full flex items-center justify-between px-4 py-2 transition-colors cursor-pointer text-left {} {}", text_color, hover_bg)}
            onclick={move |e: Event| {
                e.stop_propagation();
                if let Some(ref cb) = click_handler {
                    cb();
                }
            }}
        >
            <div class="flex items-center gap-2.5 min-w-0 flex-1">
                {if let Some(ic) = icon {
                    rsx! {
                        <div class="flex items-center justify-center flex-shrink-0 text-neutral-400">
                            {Icon(ic, Some("w-4 h-4 text-current")).root}
                        </div>
                    }
                } else {
                    rsx! { <div class="hidden" /> }
                }}
                <span class="truncate">{label}</span>
            </div>
            {if let Some(sc) = shortcut {
                rsx! {
                    <span class="text-[11px] text-neutral-500 font-mono tracking-wider ml-4 flex-shrink-0">
                        {sc}
                    </span>
                }
            } else {
                rsx! { <div class="hidden" /> }
            }}
        </button>
    }
}

/// Subtle horizontal separator dividing groups of related menu items.
#[component]
pub fn MenuSeparator() -> View {
    rsx! {
        <div class="h-px bg-neutral-700/60 my-1 w-full" />
    }
}

/// Text header labeling a sub-section of menu items.
#[component]
pub fn MenuSectionHeader(title: String) -> View {
    rsx! {
        <div class="px-4 py-1 text-[10px] font-semibold text-neutral-500 uppercase tracking-wider select-none">
            {title}
        </div>
    }
}
