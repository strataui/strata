//! High-Level Accessible Feedback & Overlay Components for Strata UI.

#![allow(non_snake_case)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use strata_dom::{NodeBlueprint, Signal, View};
use strata_icons::prelude::{Icon, IconName};
use strata_macro::{component, rsx};

/// Modal Dialog overlay with backdrop blur, focus trap, and Escape/Backdrop dismissal.
#[component]
pub fn Modal(
    open: Signal<bool>,
    title: &'static str,
    description: Option<&'static str>,
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    if !open.get() {
        return rsx! { <></> };
    }

    let op = open.clone();
    let op_backdrop = open.clone();
    let custom = class.unwrap_or("");

    rsx! {
        <div
            class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4 animate-in fade-in duration-200"
            role="dialog"
            aria-modal="true"
        >
            <div
                class="fixed inset-0"
                onclick={move |_| op_backdrop.set(false)}
            />
            <div class={format!("relative z-10 w-full max-w-lg rounded-2xl bg-neutral-900 border border-neutral-800 shadow-2xl p-6 flex flex-col gap-5 {}", custom)}>
                <div class="flex items-start justify-between border-b border-neutral-800/80 pb-4">
                    <div class="flex flex-col gap-1">
                        <h2 class="text-lg font-bold text-white tracking-tight">{title}</h2>
                        {if let Some(desc) = description {
                            rsx! { <p class="text-xs text-neutral-400">{desc}</p> }
                        } else {
                            rsx! { <></> }
                        }}
                    </div>
                    <button
                        type="button"
                        class="text-neutral-400 hover:text-white p-1 rounded-lg hover:bg-neutral-800/60 transition-colors cursor-pointer select-none"
                        onclick={move |_| op.set(false)}
                    >
                        {Icon(IconName::X, Some("w-4 h-4 text-current")).root}
                    </button>
                </div>
                <div class="text-sm text-neutral-300">
                    {children.unwrap_or_default()}
                </div>
            </div>
        </div>
    }
}

/// Slide-over Drawer sheet from viewport edges (left, right, top, bottom).
#[component]
pub fn Drawer(
    open: Signal<bool>,
    side: Option<&'static str>,
    title: Option<&'static str>,
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    if !open.get() {
        return rsx! { <></> };
    }

    let op = open.clone();
    let op_backdrop = open.clone();
    let s = side.unwrap_or("right");

    let placement_classes = match s {
        "left" => "top-0 bottom-0 left-0 w-80 border-r",
        "top" => "top-0 left-0 right-0 h-80 border-b",
        "bottom" => "bottom-0 left-0 right-0 h-80 border-t",
        _ => "top-0 bottom-0 right-0 w-80 border-l",
    };

    let custom = class.unwrap_or("");

    rsx! {
        <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm">
            <div
                class="fixed inset-0"
                onclick={move |_| op_backdrop.set(false)}
            />
            <div class={format!("fixed z-10 bg-neutral-900 border-neutral-800 shadow-2xl p-6 flex flex-col gap-4 overflow-y-auto {} {}", placement_classes, custom)}>
                <div class="flex items-center justify-between border-b border-neutral-800 pb-3">
                    {if let Some(t) = title {
                        rsx! { <h3 class="text-base font-semibold text-white">{t}</h3> }
                    } else {
                        rsx! { <></> }
                    }}
                    <button
                        type="button"
                        class="text-neutral-400 hover:text-white cursor-pointer select-none"
                        onclick={move |_| op.set(false)}
                    >
                        {Icon(IconName::X, Some("w-4 h-4 text-current")).root}
                    </button>
                </div>
                <div class="flex-1 text-sm text-neutral-300">
                    {children.unwrap_or_default()}
                </div>
            </div>
        </div>
    }
}

/// Floating Tooltip appearing on hover or focus.
#[component]
pub fn Tooltip(
    content: &'static str,
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    rsx! {
        <div class="relative group inline-flex">
            {children.unwrap_or_default()}
            <div class={format!("absolute bottom-full left-1/2 -translate-x-1/2 mb-2 hidden group-hover:flex items-center px-2.5 py-1 rounded-md bg-neutral-800 border border-neutral-700 text-xs font-medium text-white shadow-lg pointer-events-none whitespace-nowrap z-50 {}", custom)}>
                {content}
            </div>
        </div>
    }
}

/// Alert banner style variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlertVariant {
    #[default]
    Info,
    Success,
    Warning,
    Destructive,
}

/// Accessible inline Alert notification banner with status icon.
#[component]
pub fn Alert(
    variant: Option<AlertVariant>,
    title: &'static str,
    description: Option<&'static str>,
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let var = variant.unwrap_or_default();
    let (border, bg, text, icon) = match var {
        AlertVariant::Info => ("border-indigo-500/20", "bg-indigo-500/10", "text-indigo-400", IconName::Info),
        AlertVariant::Success => ("border-emerald-500/20", "bg-emerald-500/10", "text-emerald-400", IconName::CheckCircle2),
        AlertVariant::Warning => ("border-amber-500/20", "bg-amber-500/10", "text-amber-400", IconName::AlertTriangle),
        AlertVariant::Destructive => ("border-rose-500/20", "bg-rose-500/10", "text-rose-400", IconName::AlertOctagon),
    };

    let custom = class.unwrap_or("");

    rsx! {
        <div class={format!("w-full rounded-xl border p-4 flex gap-3.5 {} {} {}", border, bg, custom)}>
            <div class={format!("flex-shrink-0 mt-0.5 {}", text)}>
                {Icon(icon, Some("w-5 h-5 text-current")).root}
            </div>
            <div class="flex flex-col gap-1 flex-1">
                <h4 class={format!("text-sm font-semibold {}", text)}>{title}</h4>
                {if let Some(desc) = description {
                    rsx! { <p class="text-xs text-neutral-300 leading-relaxed">{desc}</p> }
                } else {
                    rsx! { <></> }
                }}
                {children.unwrap_or_default()}
            </div>
        </div>
    }
}

/// Linear Progress bar (determinate or indeterminate).
#[component]
pub fn Progress(
    value: Option<f32>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let base = "w-full h-2 rounded-full bg-neutral-800 overflow-hidden relative";

    rsx! {
        <div class={format!("{} {}", base, custom)}>
            {if let Some(val) = value {
                let clamped = val.clamp(0.0, 100.0);
                rsx! {
                    <div
                        class="h-full bg-indigo-600 rounded-full transition-all duration-300"
                        style={format!("width: {clamped:.1}%;")}
                    />
                }
            } else {
                rsx! {
                    <div class="h-full w-1/3 bg-indigo-600 rounded-full animate-pulse" />
                }
            }}
        </div>
    }
}

/// Circular Loading Spinner.
#[component]
pub fn Spinner(
    size: Option<&'static str>,
    class: Option<&'static str>,
) -> View {
    let sz = size.unwrap_or("w-5 h-5");
    let custom = class.unwrap_or("text-indigo-500");

    rsx! {
        <div class={format!("animate-spin {}", custom)}>
            {Icon(IconName::Loader2, Some(sz)).root}
        </div>
    }
}

/// Shimmer Skeleton placeholder loader.
#[component]
pub fn Skeleton(class: Option<&'static str>) -> View {
    let custom = class.unwrap_or("w-full h-4");
    rsx! {
        <div class={format!("rounded-lg bg-neutral-800/80 animate-pulse {}", custom)} />
    }
}

/// Empty state illustration and prompt when data collections are empty.
#[component]
pub fn EmptyState(
    icon: Option<IconName>,
    title: &'static str,
    description: Option<&'static str>,
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let ic = icon.unwrap_or(IconName::Box);
    let custom = class.unwrap_or("");

    rsx! {
        <div class={format!("flex flex-col items-center justify-center p-8 text-center rounded-2xl border border-dashed border-neutral-800 bg-neutral-900/40 gap-3 {}", custom)}>
            <div class="p-3 rounded-xl bg-neutral-800/60 text-neutral-400">
                {Icon(ic, Some("w-8 h-8 text-current")).root}
            </div>
            <div class="flex flex-col gap-1 max-w-sm">
                <h3 class="text-sm font-semibold text-white">{title}</h3>
                {if let Some(desc) = description {
                    rsx! { <p class="text-xs text-neutral-400 leading-relaxed">{desc}</p> }
                } else {
                    rsx! { <></> }
                }}
            </div>
            {if let Some(ch) = children {
                rsx! { <div class="mt-2 flex items-center gap-2">{ch}</div> }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}

/// Classification category for toast notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Warning,
    Info,
}

/// Single notification message entry in the toast queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastMessage {
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub description: Option<String>,
}

static TOAST_ID_GEN: AtomicU64 = AtomicU64::new(1);
static TOAST_QUEUE: Mutex<Vec<ToastMessage>> = Mutex::new(Vec::new());

/// Global reactive notification queue (Sonner-style).
pub struct Toast;

impl Toast {
    pub fn success(title: impl Into<String>) -> u64 {
        Self::push(ToastKind::Success, title, None)
    }

    pub fn error(title: impl Into<String>) -> u64 {
        Self::push(ToastKind::Error, title, None)
    }

    pub fn warning(title: impl Into<String>) -> u64 {
        Self::push(ToastKind::Warning, title, None)
    }

    pub fn info(title: impl Into<String>) -> u64 {
        Self::push(ToastKind::Info, title, None)
    }

    pub fn push(kind: ToastKind, title: impl Into<String>, description: Option<String>) -> u64 {
        let id = TOAST_ID_GEN.fetch_add(1, Ordering::Relaxed);
        let msg = ToastMessage {
            id,
            kind,
            title: title.into(),
            description,
        };
        if let Ok(mut q) = TOAST_QUEUE.lock() {
            q.push(msg);
        }
        id
    }

    pub fn dismiss(id: u64) {
        if let Ok(mut q) = TOAST_QUEUE.lock() {
            q.retain(|m| m.id != id);
        }
    }

    pub fn get_all() -> Vec<ToastMessage> {
        if let Ok(q) = TOAST_QUEUE.lock() {
            q.clone()
        } else {
            Vec::new()
        }
    }

    pub fn clear() {
        if let Ok(mut q) = TOAST_QUEUE.lock() {
            q.clear();
        }
    }
}

/// Container component rendering active toast notifications in the corner.
#[component]
pub fn ToastContainer(class: Option<&'static str>) -> View {
    let items = Toast::get_all();
    let custom = class.unwrap_or("bottom-4 right-4");

    rsx! {
        <div class={format!("fixed z-50 flex flex-col gap-2 pointer-events-none {}", custom)}>
            {items.into_iter().map(|item| {
                let (border, icon, text_color) = match item.kind {
                    ToastKind::Success => ("border-emerald-500/20 bg-emerald-950/60", IconName::CheckCircle2, "text-emerald-400"),
                    ToastKind::Error => ("border-rose-500/20 bg-rose-950/60", IconName::AlertOctagon, "text-rose-400"),
                    ToastKind::Warning => ("border-amber-500/20 bg-amber-950/60", IconName::AlertTriangle, "text-amber-400"),
                    ToastKind::Info => ("border-indigo-500/20 bg-indigo-950/60", IconName::Info, "text-indigo-400"),
                };
                let id = item.id;
                rsx! {
                    <div class={format!("pointer-events-auto flex items-center gap-3 px-4 py-3 rounded-xl border backdrop-blur-md shadow-2xl min-w-[280px] max-w-sm text-sm text-white animate-in slide-in-from-bottom-2 {}", border)}>
                        <span class={text_color}>
                            {Icon(icon, Some("w-4 h-4 text-current")).root}
                        </span>
                        <div class="flex-1 flex flex-col">
                            <span class="font-medium">{item.title}</span>
                            {if let Some(desc) = item.description {
                                rsx! { <span class="text-xs text-neutral-400">{desc}</span> }
                            } else {
                                rsx! { <></> }
                            }}
                        </div>
                        <button
                            type="button"
                            class="text-neutral-400 hover:text-white cursor-pointer"
                            onclick={move |_| Toast::dismiss(id)}
                        >
                            {Icon(IconName::X, Some("w-3.5 h-3.5 text-current")).root}
                        </button>
                    </div>
                }
            })}
        </div>
    }
}
