//! High-Level Accessible Button & Action Components for Strata UI.

#![allow(non_snake_case)]

use std::rc::Rc;
use strata_dom::{Attribute, EventType, NodeBlueprint, Tag, View};
use strata_icons::prelude::{Icon, IconName};
use strata_macro::{component, rsx};

/// Visual presentation style variant for buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Outline,
    Ghost,
    Destructive,
    Subtle,
}

/// Sizing scale for buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

/// Primary interactive Button component with variants, loading state, and icon slots.
#[component]
pub fn Button(
    variant: Option<ButtonVariant>,
    size: Option<ButtonSize>,
    loading: Option<bool>,
    disabled: Option<bool>,
    class: Option<&'static str>,
    onclick: Option<Rc<dyn Fn()>>,
    prefix_icon: Option<IconName>,
    suffix_icon: Option<IconName>,
    children: Option<Vec<NodeBlueprint>>,
) -> View {
    let var = variant.unwrap_or_default();
    let sz = size.unwrap_or_default();
    let is_disabled = disabled.unwrap_or(false) || loading.unwrap_or(false);

    let base = "inline-flex items-center justify-center font-medium rounded-lg transition-all duration-150 active:scale-95 disabled:opacity-50 disabled:pointer-events-none select-none cursor-pointer focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500";

    let var_classes = match var {
        ButtonVariant::Primary => "bg-indigo-600 hover:bg-indigo-500 text-white shadow-md shadow-indigo-600/20 border border-transparent",
        ButtonVariant::Secondary => "bg-neutral-800 hover:bg-neutral-700 text-neutral-100 border border-neutral-700",
        ButtonVariant::Outline => "border border-neutral-700 hover:border-neutral-500 text-neutral-200 hover:text-white bg-transparent hover:bg-neutral-800/40",
        ButtonVariant::Ghost => "text-neutral-400 hover:text-white hover:bg-neutral-800/60 border border-transparent",
        ButtonVariant::Destructive => "bg-rose-600 hover:bg-rose-500 text-white shadow-md shadow-rose-600/20 border border-transparent",
        ButtonVariant::Subtle => "bg-indigo-500/10 hover:bg-indigo-500/20 text-indigo-400 border border-indigo-500/20",
    };

    let sz_classes = match sz {
        ButtonSize::Xs => "h-7 px-2.5 text-xs gap-1",
        ButtonSize::Sm => "h-8 px-3 text-xs gap-1.5",
        ButtonSize::Md => "h-10 px-4 text-sm gap-2",
        ButtonSize::Lg => "h-12 px-6 text-base gap-2.5",
        ButtonSize::Xl => "h-14 px-8 text-lg gap-3",
    };

    let custom = class.unwrap_or("");
    let combined = format!("{base} {var_classes} {sz_classes} {custom}");
    let classes: Vec<String> = combined.split_whitespace().map(|s| s.to_string()).collect();

    let mut attrs = Vec::new();
    attrs.push(Attribute::property("role", "button"));
    if is_disabled {
        attrs.push(Attribute::property("disabled", "true"));
        attrs.push(Attribute::property("aria-disabled", "true"));
    }
    if let Some(cb) = onclick {
        if !is_disabled {
            attrs.push(Attribute::event_listener(EventType::Click, move || cb()));
        }
    }

    let mut btn_children = Vec::new();
    if loading.unwrap_or(false) {
        btn_children.push(rsx! {
            <span class="inline-block animate-spin mr-1.5 text-current">
                {Icon(IconName::Loader2, Some("w-4 h-4 animate-spin")).root}
            </span>
        }.root);
    } else if let Some(icon) = prefix_icon {
        btn_children.push(Icon(icon, Some("w-4 h-4 text-current")).root);
    }

    if let Some(ch) = children {
        btn_children.extend(ch);
    }

    if let Some(icon) = suffix_icon {
        if !loading.unwrap_or(false) {
            btn_children.push(Icon(icon, Some("w-4 h-4 text-current")).root);
        }
    }

    View::new(NodeBlueprint::element(
        Tag::Button,
        classes,
        attrs,
        btn_children,
    ))
}

/// Compact square or circular button designed specifically for standalone vector icons.
#[component]
pub fn IconButton(
    icon: IconName,
    variant: Option<ButtonVariant>,
    size: Option<ButtonSize>,
    rounded: Option<bool>,
    tooltip: Option<&'static str>,
    disabled: Option<bool>,
    class: Option<&'static str>,
    onclick: Option<Rc<dyn Fn()>>,
) -> View {
    let var = variant.unwrap_or(ButtonVariant::Ghost);
    let sz = size.unwrap_or(ButtonSize::Md);
    let is_rounded = rounded.unwrap_or(false);

    let shape = if is_rounded { "rounded-full" } else { "rounded-lg" };
    let dim = match sz {
        ButtonSize::Xs => "w-7 h-7 p-1",
        ButtonSize::Sm => "w-8 h-8 p-1.5",
        ButtonSize::Md => "w-10 h-10 p-2",
        ButtonSize::Lg => "w-12 h-12 p-2.5",
        ButtonSize::Xl => "w-14 h-14 p-3",
    };

    let title_attr = tooltip.map(|t| Attribute::property("title", t));

    let mut btn = Button(
        Some(var),
        Some(sz),
        None,
        disabled,
        Some("p-0"),
        onclick,
        None,
        None,
        Some(vec![Icon(icon, Some("w-full h-full text-current")).root]),
    );

    if let NodeBlueprint::Element { ref mut classes, ref mut attributes, .. } = btn.root {
        classes.push(shape.to_string());
        classes.push(dim.to_string());
        if let Some(custom) = class {
            classes.extend(custom.split_whitespace().map(|s| s.to_string()));
        }
        if let Some(t) = title_attr {
            attributes.push(t);
        }
    }

    btn
}

/// Clustered button group visually merging adjacent action buttons.
#[component]
pub fn ButtonGroup(
    children: Vec<NodeBlueprint>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let base = "inline-flex items-center rounded-lg shadow-sm [&>button:not(:first-child)]:-ml-px [&>button:not(:first-child)]:rounded-l-none [&>button:not(:last-child)]:rounded-r-none";
    let combined = format!("{base} {custom}");
    let classes: Vec<String> = combined.split_whitespace().map(|s| s.to_string()).collect();

    View::new(NodeBlueprint::element(
        Tag::Div,
        classes,
        vec![Attribute::property("role", "group")],
        children,
    ))
}

/// Split button combining primary action with menu trigger chevron.
#[component]
pub fn SplitButton(
    label: &'static str,
    on_action: Rc<dyn Fn()>,
    on_menu: Rc<dyn Fn()>,
    variant: Option<ButtonVariant>,
    size: Option<ButtonSize>,
    class: Option<&'static str>,
) -> View {
    let var = variant.unwrap_or(ButtonVariant::Primary);
    let sz = size.unwrap_or(ButtonSize::Md);

    rsx! {
        <div class={format!("inline-flex rounded-lg shadow-sm {}", class.unwrap_or(""))}>
            <Button
                variant={Some(var)}
                size={Some(sz)}
                onclick={on_action}
                class={Some("rounded-r-none")}
            >
                {label}
            </Button>
            <Button
                variant={Some(var)}
                size={Some(sz)}
                onclick={on_menu}
                class={Some("rounded-l-none -ml-px px-2")}
                prefix_icon={Some(IconName::ChevronDown)}
            />
        </div>
    }
}
