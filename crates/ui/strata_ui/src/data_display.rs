//! High-Level Accessible Data Display Components for Strata UI.

#![allow(non_snake_case)]

use std::rc::Rc;
use strata_dom::{NodeBlueprint, Signal, View};
use strata_icons::prelude::{Icon, IconName};
use strata_macro::{component, rsx};

/// Card container with subtle glassmorphic backdrop and border.
#[component]
pub fn Card(
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let base = "rounded-2xl bg-neutral-900/60 border border-neutral-800 shadow-xl overflow-hidden backdrop-blur-sm";
    let combined = format!("{base} {custom}");
    let classes: Vec<String> = combined.split_whitespace().map(|s| s.to_string()).collect();

    View::new(NodeBlueprint::element(
        strata_dom::Tag::Div,
        classes,
        Vec::new(),
        children.unwrap_or_default(),
    ))
}

/// Header slot for Card containers.
#[component]
pub fn CardHeader(
    title: Option<&'static str>,
    description: Option<&'static str>,
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    rsx! {
        <div class={format!("p-6 border-b border-neutral-800/80 flex flex-col gap-1.5 {}", custom)}>
            {if let Some(t) = title {
                rsx! { <h3 class="text-base font-semibold text-white tracking-tight">{t}</h3> }
            } else {
                rsx! { <></> }
            }}
            {if let Some(d) = description {
                rsx! { <p class="text-xs text-neutral-400">{d}</p> }
            } else {
                rsx! { <></> }
            }}
            {children.unwrap_or_default()}
        </div>
    }
}

/// Body content slot for Card containers.
#[component]
pub fn CardContent(
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    rsx! {
        <div class={format!("p-6 {}", custom)}>
            {children.unwrap_or_default()}
        </div>
    }
}

/// Footer slot for Card containers with top border divider.
#[component]
pub fn CardFooter(
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    rsx! {
        <div class={format!("px-6 py-4 border-t border-neutral-800/80 flex items-center justify-between {}", custom)}>
            {children.unwrap_or_default()}
        </div>
    }
}

/// Visual status badge variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeVariant {
    #[default]
    Default,
    Success,
    Warning,
    Destructive,
    Outline,
    Indigo,
}

/// Visual status Badge pill.
#[component]
pub fn Badge(
    variant: Option<BadgeVariant>,
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let var = variant.unwrap_or_default();
    let var_classes = match var {
        BadgeVariant::Default => "bg-neutral-800 text-neutral-300 border-neutral-700",
        BadgeVariant::Success => "bg-emerald-500/10 text-emerald-400 border-emerald-500/20",
        BadgeVariant::Warning => "bg-amber-500/10 text-amber-400 border-amber-500/20",
        BadgeVariant::Destructive => "bg-rose-500/10 text-rose-400 border-rose-500/20",
        BadgeVariant::Outline => "bg-transparent text-white border-neutral-700",
        BadgeVariant::Indigo => "bg-indigo-500/10 text-indigo-400 border-indigo-500/20",
    };

    let custom = class.unwrap_or("");
    let combined = format!("inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-mono font-medium border select-none {var_classes} {custom}");

    rsx! {
        <span class={combined}>
            {children.unwrap_or_default()}
        </span>
    }
}

/// Interactive dismissable Tag / Chip with close button.
#[component]
pub fn Tag(
    label: &'static str,
    onclose: Option<Rc<dyn Fn()>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let cb = onclose.clone();

    rsx! {
        <span class={format!("inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium bg-neutral-800 border border-neutral-700 text-neutral-200 select-none {}", custom)}>
            <span>{label}</span>
            {if let Some(on_c) = cb {
                rsx! {
                    <button
                        type="button"
                        class="text-neutral-400 hover:text-white cursor-pointer -mr-0.5"
                        onclick={move |_| on_c()}
                    >
                        {Icon(IconName::X, Some("w-3 h-3 text-current")).root}
                    </button>
                }
            } else {
                rsx! { <></> }
            }}
        </span>
    }
}

/// User profile Avatar with initials fallback and status dot.
#[component]
pub fn Avatar(
    fallback: &'static str,
    status: Option<&'static str>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let base = "w-9 h-9 rounded-full bg-gradient-to-tr from-indigo-500 to-purple-600 flex items-center justify-center text-xs font-bold text-white ring-2 ring-neutral-800 select-none relative";
    let combined = format!("{base} {custom}");

    rsx! {
        <div class={combined}>
            {fallback}
            {if let Some(st) = status {
                let dot_color = match st {
                    "online" => "bg-emerald-500",
                    "busy" => "bg-rose-500",
                    "away" => "bg-amber-500",
                    _ => "bg-neutral-500",
                };
                rsx! {
                    <span class={format!("absolute bottom-0 right-0 w-2.5 h-2.5 rounded-full ring-2 ring-neutral-900 {}", dot_color)} />
                }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}

/// KPI Metric / Statistic block with value, label, and trend pill.
#[component]
pub fn Stat(
    label: &'static str,
    value: &'static str,
    trend: Option<f32>,
    subtitle: Option<&'static str>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");

    rsx! {
        <div class={format!("p-5 rounded-2xl bg-neutral-900/60 border border-neutral-800 shadow-lg flex flex-col justify-between {}", custom)}>
            <div class="flex items-center justify-between text-neutral-400 text-xs font-mono uppercase">
                <span>{label}</span>
                {if let Some(t) = trend {
                    let (color, icon) = if t >= 0.0 {
                        ("text-emerald-400 bg-emerald-500/10 border-emerald-500/20", IconName::ArrowUpRight)
                    } else {
                        ("text-rose-400 bg-rose-500/10 border-rose-500/20", IconName::ArrowDownLeft)
                    };
                    rsx! {
                        <span class={format!("inline-flex items-center gap-1 px-1.5 py-0.5 rounded border text-xs font-mono font-semibold {}", color)}>
                            {Icon(icon, Some("w-3 h-3 text-current")).root}
                            {format!("{t:+.1}%")}
                        </span>
                    }
                } else {
                    rsx! { <></> }
                }}
            </div>
            <div class="mt-3 text-2xl font-bold font-mono text-white tracking-tight">
                {value}
            </div>
            {if let Some(sub) = subtitle {
                rsx! { <div class="mt-1 text-xs text-neutral-500">{sub}</div> }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}

/// Segmented tab switcher and tab trigger button.
#[component]
pub fn Tabs(
    items: Vec<&'static str>,
    active_index: Signal<usize>,
    class: Option<&'static str>,
) -> View {
    let active = active_index.get();
    let custom = class.unwrap_or("");

    rsx! {
        <div class={format!("inline-flex items-center p-1 rounded-xl bg-neutral-900 border border-neutral-800 gap-1 {}", custom)}>
            {items.into_iter().enumerate().map(|(idx, title)| {
                let is_sel = idx == active;
                let active_sig = active_index.clone();
                let (bg, text) = if is_sel {
                    ("bg-neutral-800 text-white shadow-sm", "font-semibold")
                } else {
                    ("bg-transparent text-neutral-400 hover:text-neutral-200", "font-normal")
                };
                rsx! {
                    <button
                        type="button"
                        class={format!("px-3.5 py-1.5 rounded-lg text-xs transition-all duration-150 cursor-pointer select-none {} {}", bg, text)}
                        onclick={move |_| active_sig.set(idx)}
                    >
                        {title}
                    </button>
                }
            })}
        </div>
    }
}

/// Collapsible Accordion section with toggleable expansion state.
#[component]
pub fn AccordionItem(
    title: &'static str,
    open: Signal<bool>,
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let is_open = open.get();
    let op = open.clone();
    let custom = class.unwrap_or("");
    let chevron_rot = if is_open { "rotate-180" } else { "rotate-0" };

    rsx! {
        <div class={format!("border-b border-neutral-800 py-3 w-full {}", custom)}>
            <button
                type="button"
                class="flex items-center justify-between w-full text-left font-medium text-sm text-neutral-200 hover:text-white cursor-pointer select-none"
                onclick={move |_| op.update(|o| *o = !*o)}
            >
                <span>{title}</span>
                <span class={format!("transition-transform duration-200 text-neutral-400 {}", chevron_rot)}>
                    {Icon(IconName::ChevronDown, Some("w-4 h-4 text-current")).root}
                </span>
            </button>
            {if is_open {
                rsx! {
                    <div class="pt-3 text-xs text-neutral-400 leading-relaxed">
                        {children.unwrap_or_default()}
                    </div>
                }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}

/// Semantic data Table container.
#[component]
pub fn Table(
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    rsx! {
        <div class="w-full overflow-x-auto rounded-xl border border-neutral-800">
            <table class={format!("w-full text-left text-sm text-neutral-300 {}", custom)}>
                {children.unwrap_or_default()}
            </table>
        </div>
    }
}

/// Table head row container.
#[component]
pub fn TableHeader(children: Option<Vec<NodeBlueprint>>) -> View {
    rsx! {
        <thead class="bg-neutral-900/80 border-b border-neutral-800 text-xs font-mono uppercase tracking-wider text-neutral-400">
            {children.unwrap_or_default()}
        </thead>
    }
}

/// Table body container.
#[component]
pub fn TableBody(children: Option<Vec<NodeBlueprint>>) -> View {
    rsx! {
        <tbody class="divide-y divide-neutral-800/60 bg-neutral-950/40">
            {children.unwrap_or_default()}
        </tbody>
    }
}

/// Table row.
#[component]
pub fn TableRow(
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    rsx! {
        <tr class={format!("hover:bg-neutral-800/30 transition-colors {}", custom)}>
            {children.unwrap_or_default()}
        </tr>
    }
}

/// Table header cell.
#[component]
pub fn TableHead(
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    rsx! {
        <th class={format!("px-4 py-3 font-semibold text-neutral-300 {}", custom)}>
            {children.unwrap_or_default()}
        </th>
    }
}

/// Table data cell.
#[component]
pub fn TableCell(
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    rsx! {
        <td class={format!("px-4 py-3 text-neutral-300 {}", custom)}>
            {children.unwrap_or_default()}
        </td>
    }
}

/// Vertical timeline item for activity feeds.
#[component]
pub fn TimelineItem(
    title: &'static str,
    timestamp: &'static str,
    description: Option<&'static str>,
    is_last: Option<bool>,
) -> View {
    let last = is_last.unwrap_or(false);

    rsx! {
        <div class="relative pl-6 pb-6 group">
            {if !last {
                rsx! { <div class="absolute left-2.5 top-3 bottom-0 w-px bg-neutral-800" /> }
            } else {
                rsx! { <></> }
            }}
            <div class="absolute left-1 top-1.5 w-3.5 h-3.5 rounded-full border-2 border-indigo-600 bg-neutral-950" />
            <div class="flex items-center justify-between text-xs font-mono text-neutral-400">
                <span class="font-semibold text-neutral-200">{title}</span>
                <span>{timestamp}</span>
            </div>
            {if let Some(desc) = description {
                rsx! { <p class="mt-1 text-xs text-neutral-400 leading-relaxed">{desc}</p> }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}

/// Keyboard shortcut key badge (e.g. ⌘K, Shift+Enter).
#[component]
pub fn Kbd(
    keys: &'static str,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    rsx! {
        <kbd class={format!("inline-flex items-center px-1.5 py-0.5 rounded border border-neutral-700 bg-neutral-800 font-mono text-[10px] font-semibold text-neutral-300 shadow-sm select-none {}", custom)}>
            {keys}
        </kbd>
    }
}

/// Visual horizontal or vertical separator divider.
#[component]
pub fn Separator(
    vertical: Option<bool>,
    class: Option<&'static str>,
) -> View {
    let is_vert = vertical.unwrap_or(false);
    let custom = class.unwrap_or("");
    let dim = if is_vert { "w-px h-full" } else { "h-px w-full" };

    rsx! {
        <div class={format!("bg-neutral-800 {} {}", dim, custom)} />
    }
}
