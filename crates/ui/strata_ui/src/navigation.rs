//! High-Level Accessible Navigation Components for Strata UI.

#![allow(non_snake_case)]

use std::rc::Rc;
use strata_dom::{NodeBlueprint, Signal, View};
use strata_icons::prelude::{Icon, IconName};
use strata_macro::{component, rsx};

/// Top navigation bar with slots for branding, center navigation links, and action buttons.
#[component]
pub fn Navbar(
    brand: Option<Vec<NodeBlueprint>>,
    links: Option<Vec<NodeBlueprint>>,
    actions: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let base = "w-full h-16 border-b border-neutral-800 bg-neutral-950/80 backdrop-blur-md px-6 flex items-center justify-between z-40 sticky top-0";

    rsx! {
        <nav class={format!("{} {}", base, custom)}>
            <div class="flex items-center gap-6">
                {brand.unwrap_or_default()}
                <div class="hidden md:flex items-center gap-4">
                    {links.unwrap_or_default()}
                </div>
            </div>
            <div class="flex items-center gap-3">
                {actions.unwrap_or_default()}
            </div>
        </nav>
    }
}

/// Navigation item descriptor for Sidebar.
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarItemData {
    pub label: &'static str,
    pub icon: IconName,
    pub href: Option<&'static str>,
    pub badge: Option<&'static str>,
    pub is_active: bool,
}

/// Collapsible vertical Sidebar navigation.
#[component]
pub fn Sidebar(
    items: Vec<SidebarItemData>,
    collapsed: Option<Signal<bool>>,
    header: Option<Vec<NodeBlueprint>>,
    footer: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let is_coll = collapsed.as_ref().map(|s| s.get()).unwrap_or(false);
    let width = if is_coll { "w-16" } else { "w-64" };
    let custom = class.unwrap_or("");

    rsx! {
        <aside class={format!("h-full flex flex-col justify-between border-r border-neutral-800 bg-neutral-950 transition-all duration-200 select-none {} {}", width, custom)}>
            <div class="flex flex-col">
                {if let Some(h) = header {
                    rsx! { <div class="p-4 border-b border-neutral-800/80">{h}</div> }
                } else {
                    rsx! { <></> }
                }}
                <div class="flex flex-col gap-1 p-2">
                    {items.into_iter().map(|item| {
                        let (bg, text) = if item.is_active {
                            ("bg-indigo-600/10 text-indigo-400 font-semibold", "text-indigo-400")
                        } else {
                            ("bg-transparent text-neutral-400 hover:text-white hover:bg-neutral-800/40 font-medium", "text-neutral-400 group-hover:text-white")
                        };
                        rsx! {
                            <a
                                href={item.href.unwrap_or("#")}
                                class={format!("group flex items-center gap-3 px-3 py-2.5 rounded-xl text-xs transition-colors cursor-pointer {}", bg)}
                            >
                                <span class={text}>
                                    {Icon(item.icon, Some("w-4 h-4 text-current")).root}
                                </span>
                                {if !is_coll {
                                    rsx! {
                                        <span class="flex-1 truncate">{item.label}</span>
                                    }
                                } else {
                                    rsx! { <></> }
                                }}
                                {if let (false, Some(badge_text)) = (is_coll, item.badge) {
                                    rsx! {
                                        <span class="px-1.5 py-0.5 rounded-full text-[10px] font-mono bg-neutral-800 text-neutral-300">
                                            {badge_text}
                                        </span>
                                    }
                                } else {
                                    rsx! { <></> }
                                }}
                            </a>
                        }
                    })}
                </div>
            </div>
            {if let Some(f) = footer {
                rsx! { <div class="p-4 border-t border-neutral-800/80">{f}</div> }
            } else {
                rsx! { <></> }
            }}
        </aside>
    }
}

/// Breadcrumb navigation item.
#[derive(Debug, Clone, PartialEq)]
pub struct BreadcrumbItemData {
    pub label: &'static str,
    pub href: Option<&'static str>,
    pub is_current: bool,
}

/// Breadcrumb hierarchical navigation trail.
#[component]
pub fn Breadcrumb(
    items: Vec<BreadcrumbItemData>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let total = items.len();

    rsx! {
        <nav aria-label="Breadcrumb" class={format!("flex items-center gap-1.5 text-xs font-mono {}", custom)}>
            {items.into_iter().enumerate().map(|(idx, item)| {
                let is_last = idx + 1 == total;
                let text_color = if item.is_current || is_last {
                    "text-white font-semibold"
                } else {
                    "text-neutral-400 hover:text-neutral-200"
                };

                rsx! {
                    <div class="flex items-center gap-1.5">
                        {if let Some(href) = item.href {
                            rsx! { <a href={href} class={format!("transition-colors {}", text_color)}>{item.label}</a> }
                        } else {
                            rsx! { <span class={text_color}>{item.label}</span> }
                        }}
                        {if !is_last {
                            rsx! {
                                <span class="text-neutral-600">
                                    {Icon(IconName::ChevronRight, Some("w-3 h-3 text-current")).root}
                                </span>
                            }
                        } else {
                            rsx! { <></> }
                        }}
                    </div>
                }
            })}
        </nav>
    }
}

/// Pagination controls with page navigation and previous/next buttons.
#[component]
pub fn Pagination(
    total_pages: usize,
    current_page: Signal<usize>,
    class: Option<&'static str>,
) -> View {
    let curr = current_page.get();
    let prev_sig = current_page.clone();
    let next_sig = current_page.clone();
    let custom = class.unwrap_or("");

    rsx! {
        <div class={format!("inline-flex items-center gap-1.5 select-none {}", custom)}>
            <button
                type="button"
                class={format!("p-2 rounded-lg border border-neutral-800 bg-neutral-900 text-neutral-400 hover:text-white transition-colors cursor-pointer {}", if curr <= 1 { "opacity-40 pointer-events-none" } else { "" })}
                onclick={move |_| {
                    if curr > 1 {
                        prev_sig.set(curr - 1);
                    }
                }}
            >
                {Icon(IconName::ChevronLeft, Some("w-4 h-4 text-current")).root}
            </button>
            <div class="px-3 py-1 text-xs font-mono text-neutral-300">
                {format!("Page {curr} of {total_pages}")}
            </div>
            <button
                type="button"
                class={format!("p-2 rounded-lg border border-neutral-800 bg-neutral-900 text-neutral-400 hover:text-white transition-colors cursor-pointer {}", if curr >= total_pages { "opacity-40 pointer-events-none" } else { "" })}
                onclick={move |_| {
                    if curr < total_pages {
                        next_sig.set(curr + 1);
                    }
                }}
            >
                {Icon(IconName::ChevronRight, Some("w-4 h-4 text-current")).root}
            </button>
        </div>
    }
}

/// Command item descriptor for CommandPalette (⌘K).
#[derive(Debug, Clone, PartialEq)]
pub struct CommandItemData {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub icon: Option<IconName>,
    pub shortcut: Option<&'static str>,
}

/// Spotlight Search / Command Palette modal (⌘K).
#[component]
pub fn CommandPalette(
    open: Signal<bool>,
    query: Signal<String>,
    items: Vec<CommandItemData>,
    on_select: Option<Rc<dyn Fn(&'static str)>>,
) -> View {
    if !open.get() {
        return rsx! { <></> };
    }

    let q = query.get().to_lowercase();
    let filtered: Vec<CommandItemData> = items
        .into_iter()
        .filter(|item| q.is_empty() || item.title.to_lowercase().contains(&q) || item.category.to_lowercase().contains(&q))
        .collect();

    let op = open.clone();
    let op_backdrop = open.clone();

    rsx! {
        <div class="fixed inset-0 z-50 flex items-start justify-center pt-24 bg-black/60 backdrop-blur-sm p-4">
            <div
                class="fixed inset-0"
                onclick={move |_| op_backdrop.set(false)}
            />
            <div class="relative z-10 w-full max-w-xl rounded-2xl bg-neutral-900 border border-neutral-800 shadow-2xl overflow-hidden flex flex-col">
                <div class="flex items-center px-4 border-b border-neutral-800">
                    <span class="text-neutral-500 mr-3">
                        {Icon(IconName::Search, Some("w-4 h-4 text-current")).root}
                    </span>
                    <input
                        type="text"
                        class="w-full h-14 bg-transparent text-sm text-white placeholder-neutral-500 focus:outline-none"
                        placeholder="Type a command or search..."
                        value={query.get()}
                    />
                    <button
                        type="button"
                        class="text-neutral-500 hover:text-white p-1"
                        onclick={move |_| op.set(false)}
                    >
                        {Icon(IconName::X, Some("w-4 h-4 text-current")).root}
                    </button>
                </div>
                <div class="max-h-80 overflow-y-auto p-2 flex flex-col gap-1">
                    {if filtered.is_empty() {
                        rsx! {
                            <div class="py-8 text-center text-xs text-neutral-500 font-mono">
                                {"No commands found."}
                            </div>
                        }
                    } else {
                        rsx! {
                            <div>
                                {filtered.into_iter().map(|item| {
                                    let id = item.id;
                                    let sel_cb = on_select.clone();
                                    let close_sig = open.clone();
                                    rsx! {
                                        <div
                                            class="flex items-center justify-between px-3 py-2.5 rounded-xl hover:bg-neutral-800/80 cursor-pointer select-none group transition-colors"
                                            onclick={move |_| {
                                                if let Some(cb) = &sel_cb {
                                                    cb(id);
                                                }
                                                close_sig.set(false);
                                            }}
                                        >
                                            <div class="flex items-center gap-3">
                                                {if let Some(ic) = item.icon {
                                                    rsx! {
                                                        <span class="text-neutral-400 group-hover:text-indigo-400">
                                                            {Icon(ic, Some("w-4 h-4 text-current")).root}
                                                        </span>
                                                    }
                                                } else {
                                                    rsx! { <></> }
                                                }}
                                                <span class="text-xs font-medium text-neutral-200 group-hover:text-white">{item.title}</span>
                                            </div>
                                            {if let Some(sc) = item.shortcut {
                                                rsx! {
                                                    <span class="px-1.5 py-0.5 rounded border border-neutral-700 bg-neutral-800 text-[10px] font-mono text-neutral-400">
                                                        {sc}
                                                    </span>
                                                }
                                            } else {
                                                rsx! { <></> }
                                            }}
                                        </div>
                                    }
                                })}
                            </div>
                        }
                    }}
                </div>
            </div>
        </div>
    }
}
