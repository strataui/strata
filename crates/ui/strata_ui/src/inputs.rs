//! High-Level Accessible Form Controls and Inputs for Strata UI.

#![allow(non_snake_case)]

use strata_dom::{create_dropdown_portal, use_persistent_signal, AnchorPlacement, Signal, View};
use strata_icons::prelude::{Icon, IconName};
use strata_macro::{component, rsx};

/// Standard text input field with prefix/suffix icons, helper text, and error states.
#[component]
pub fn Input(
    label: Option<&'static str>,
    placeholder: Option<&'static str>,
    value: Option<Signal<String>>,
    prefix_icon: Option<IconName>,
    suffix_icon: Option<IconName>,
    helper_text: Option<&'static str>,
    is_error: Option<bool>,
    error_message: Option<&'static str>,
    disabled: Option<bool>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let err = is_error.unwrap_or(false);
    let border_color = if err {
        "border-rose-500 focus:border-rose-500 focus:ring-rose-500/20"
    } else {
        "border-neutral-800 focus:border-indigo-500 focus:ring-indigo-500/20"
    };

    let pl = if prefix_icon.is_some() {
        "pl-10"
    } else {
        "pl-3.5"
    };
    let pr = if suffix_icon.is_some() {
        "pr-10"
    } else {
        "pr-3.5"
    };

    let input_classes = format!(
        "w-full h-10 bg-neutral-900 border rounded-lg text-sm text-white placeholder-neutral-500 focus:outline-none focus:ring-2 transition-all duration-150 disabled:opacity-50 disabled:pointer-events-none {border_color} {pl} {pr} {custom}"
    );

    let val_str = value.map(|s| s.get()).unwrap_or_default();

    rsx! {
        <div class="flex flex-col gap-1.5 w-full">
            {if let Some(lbl) = label {
                rsx! { <label class="text-xs font-mono uppercase tracking-wider text-neutral-400 select-none">{lbl}</label> }
            } else {
                rsx! { <></> }
            }}
            <div class="relative flex items-center w-full">
                {if let Some(icon) = prefix_icon {
                    rsx! {
                        <div class="absolute left-3 flex items-center pointer-events-none text-neutral-500">
                            {Icon(icon, Some("w-4 h-4 text-neutral-400")).root}
                        </div>
                    }
                } else {
                    rsx! { <></> }
                }}
                <input
                    type="text"
                    class={input_classes}
                    placeholder={placeholder.unwrap_or("")}
                    disabled={disabled.unwrap_or(false)}
                    value={val_str}
                />
                {if let Some(icon) = suffix_icon {
                    rsx! {
                        <div class="absolute right-3 flex items-center pointer-events-none text-neutral-500">
                            {Icon(icon, Some("w-4 h-4 text-neutral-400")).root}
                        </div>
                    }
                } else {
                    rsx! { <></> }
                }}
            </div>
            {if let (true, Some(err_msg)) = (err, error_message) {
                rsx! { <span class="text-xs text-rose-400 font-medium">{err_msg}</span> }
            } else if let Some(help) = helper_text {
                rsx! { <span class="text-xs text-neutral-500">{help}</span> }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}

/// Password input with interactive eye toggle button for visibility.
#[component]
pub fn PasswordInput(
    label: Option<&'static str>,
    placeholder: Option<&'static str>,
    value: Option<Signal<String>>,
    show_password: Option<Signal<bool>>,
    disabled: Option<bool>,
    class: Option<&'static str>,
) -> View {
    let show = show_password.as_ref().map(|s| s.get()).unwrap_or(false);
    let input_type = if show { "text" } else { "password" };
    let icon = if show {
        IconName::EyeOff
    } else {
        IconName::Eye
    };
    let show_sig = show_password.clone();

    rsx! {
        <div class="flex flex-col gap-1.5 w-full">
            {if let Some(lbl) = label {
                rsx! { <label class="text-xs font-mono uppercase tracking-wider text-neutral-400 select-none">{lbl}</label> }
            } else {
                rsx! { <></> }
            }}
            <div class="relative flex items-center w-full">
                <input
                    type={input_type}
                    class={format!("w-full h-10 bg-neutral-900 border border-neutral-800 rounded-lg pl-3.5 pr-10 text-sm text-white placeholder-neutral-500 focus:outline-none focus:border-indigo-500 transition-colors {}", class.unwrap_or(""))}
                    placeholder={placeholder.unwrap_or("••••••••")}
                    disabled={disabled.unwrap_or(false)}
                    value={value.map(|s| s.get()).unwrap_or_default()}
                />
                <button
                    type="button"
                    class="absolute right-3 text-neutral-400 hover:text-white cursor-pointer select-none"
                    onclick={move |_| {
                        if let Some(s) = &show_sig {
                            s.update(|v| *v = !*v);
                        }
                    }}
                >
                    {Icon(icon, Some("w-4 h-4 text-neutral-400 hover:text-white")).root}
                </button>
            </div>
        </div>
    }
}

/// Numeric input with stepper increment and decrement buttons.
#[component]
pub fn NumberInput(
    label: Option<&'static str>,
    value: Signal<i64>,
    min: Option<i64>,
    max: Option<i64>,
    step: Option<i64>,
    disabled: Option<bool>,
    class: Option<&'static str>,
) -> View {
    let s = step.unwrap_or(1);
    let mi = min.unwrap_or(i64::MIN);
    let ma = max.unwrap_or(i64::MAX);

    let val = value.get();
    let val_dec = value.clone();
    let val_inc = value.clone();

    rsx! {
        <div class="flex flex-col gap-1.5 w-full">
            {if let Some(lbl) = label {
                rsx! { <label class="text-xs font-mono uppercase tracking-wider text-neutral-400 select-none">{lbl}</label> }
            } else {
                rsx! { <></> }
            }}
            <div class="flex items-center rounded-lg border border-neutral-800 bg-neutral-900 overflow-hidden w-full h-10">
                <button
                    type="button"
                    class="h-full px-3 text-neutral-400 hover:text-white hover:bg-neutral-800/60 cursor-pointer select-none transition-colors"
                    onclick={move |_| {
                        val_dec.update(|v| *v = (*v - s).max(mi));
                    }}
                >
                    {"-"}
                </button>
                <div class="flex-1 text-center font-mono text-sm text-white select-none">
                    {val.to_string()}
                </div>
                <button
                    type="button"
                    class="h-full px-3 text-neutral-400 hover:text-white hover:bg-neutral-800/60 cursor-pointer select-none transition-colors"
                    onclick={move |_| {
                        val_inc.update(|v| *v = (*v + s).min(ma));
                    }}
                >
                    {"+"}
                </button>
            </div>
        </div>
    }
}

/// Search input with search icon prefix and clear button.
#[component]
pub fn SearchInput(
    placeholder: Option<&'static str>,
    value: Signal<String>,
    class: Option<&'static str>,
) -> View {
    let val = value.get();
    let has_content = !val.is_empty();
    let val_clear = value.clone();

    rsx! {
        <div class="relative flex items-center w-full">
            <div class="absolute left-3 flex items-center pointer-events-none text-neutral-500">
                {Icon(IconName::Search, Some("w-4 h-4 text-neutral-400")).root}
            </div>
            <input
                type="text"
                class={format!("w-full h-10 bg-neutral-900 border border-neutral-800 rounded-lg pl-10 pr-10 text-sm text-white placeholder-neutral-500 focus:outline-none focus:border-indigo-500 transition-colors {}", class.unwrap_or(""))}
                placeholder={placeholder.unwrap_or("Search...")}
                value={val}
            />
            {if has_content {
                rsx! {
                    <button
                        type="button"
                        class="absolute right-3 text-neutral-400 hover:text-white cursor-pointer select-none"
                        onclick={move |_| val_clear.set(String::new())}
                    >
                        {Icon(IconName::X, Some("w-4 h-4 text-neutral-400 hover:text-white")).root}
                    </button>
                }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}

/// Multi-line expanding text area editor.
#[component]
pub fn Textarea(
    label: Option<&'static str>,
    placeholder: Option<&'static str>,
    rows: Option<usize>,
    value: Option<Signal<String>>,
    disabled: Option<bool>,
    class: Option<&'static str>,
) -> View {
    let r = rows.unwrap_or(4);
    let custom = class.unwrap_or("");
    let val_str = value.as_ref().map(|s| s.get()).unwrap_or_default();
    let val_sig = value.clone();

    rsx! {
        <div class="flex flex-col gap-1.5 w-full">
            {if let Some(lbl) = label {
                rsx! { <label class="text-xs font-mono uppercase tracking-wider text-neutral-400 select-none">{lbl}</label> }
            } else {
                rsx! { <></> }
            }}
            <textarea
                rows={r.to_string()}
                class={format!("w-full p-3.5 bg-neutral-900 border border-neutral-800 rounded-lg text-sm text-white placeholder-neutral-500 focus:outline-none focus:border-indigo-500 transition-colors resize-y {}", custom)}
                placeholder={placeholder.unwrap_or("")}
                disabled={disabled.unwrap_or(false)}
                oninput={move |val: String| {
                    if let Some(ref sig) = val_sig {
                        if sig.get() != val {
                            sig.set(val);
                        }
                    }
                }}
            >
                {val_str}
            </textarea>
        </div>
    }
}

/// Accessible tri-state Checkbox component.
#[component]
pub fn Checkbox(
    label: Option<&'static str>,
    checked: Signal<bool>,
    disabled: Option<bool>,
    class: Option<&'static str>,
) -> View {
    let is_checked = checked.get();
    let is_dis = disabled.unwrap_or(false);
    let chk = checked.clone();

    let (bg, border) = if is_checked {
        ("bg-indigo-600", "border-indigo-600")
    } else {
        (
            "bg-neutral-900",
            "border-neutral-700 hover:border-neutral-500",
        )
    };

    rsx! {
        <label class={format!("inline-flex items-center gap-2.5 cursor-pointer select-none group {}", if is_dis { "opacity-50 pointer-events-none" } else { "" })}>
            <div
                class={format!("w-5 h-5 rounded-md border flex items-center justify-center transition-all duration-150 {} {}", bg, border)}
                onclick={move |_| {
                    if !is_dis {
                        chk.update(|c| *c = !*c);
                    }
                }}
            >
                {if is_checked {
                    Icon(IconName::Check, Some("w-3.5 h-3.5 text-white stroke-[2.5]"))
                } else {
                    rsx! { <></> }
                }}
            </div>
            {if let Some(lbl) = label {
                rsx! { <span class="text-sm text-neutral-300 group-hover:text-white transition-colors">{lbl}</span> }
            } else {
                rsx! { <></> }
            }}
        </label>
    }
}

/// iOS-style animated toggle switch.
#[component]
pub fn Switch(
    label: Option<&'static str>,
    description: Option<&'static str>,
    checked: Signal<bool>,
    disabled: Option<bool>,
) -> View {
    let is_active = checked.get();
    let is_dis = disabled.unwrap_or(false);
    let chk = checked.clone();

    let bg_color = if is_active {
        "bg-indigo-600"
    } else {
        "bg-neutral-800"
    };
    let knob_pos = if is_active {
        "translate-x-5"
    } else {
        "translate-x-0"
    };

    rsx! {
        <div class={format!("flex items-center justify-between w-full py-1.5 {}", if is_dis { "opacity-50 pointer-events-none" } else { "" })}>
            <div class="flex flex-col">
                {if let Some(lbl) = label {
                    rsx! { <span class="text-sm font-medium text-white select-none">{lbl}</span> }
                } else {
                    rsx! { <></> }
                }}
                {if let Some(desc) = description {
                    rsx! { <span class="text-xs text-neutral-400 select-none">{desc}</span> }
                } else {
                    rsx! { <></> }
                }}
            </div>
            <button
                type="button"
                role="switch"
                aria-checked={if is_active { "true" } else { "false" }}
                class={format!("w-11 h-6 rounded-full transition-colors duration-200 relative p-0.5 cursor-pointer focus:outline-none {}", bg_color)}
                onclick={move |_| {
                    if !is_dis {
                        chk.update(|c| *c = !*c);
                    }
                }}
            >
                <div class={format!("w-5 h-5 rounded-full bg-white shadow-md transition-transform duration-200 {}", knob_pos)} />
            </button>
        </div>
    }
}

/// One entry of a [`Select`]: the submitted `value` and the displayed `label`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

impl SelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

impl From<&str> for SelectOption {
    fn from(text: &str) -> Self {
        Self::new(text, text)
    }
}

impl From<String> for SelectOption {
    fn from(text: String) -> Self {
        Self::new(text.clone(), text)
    }
}

/// Moves `current` by `delta` within `len` options, clamping at both ends.
pub fn select_step_index(len: usize, current: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    (current as isize + delta).clamp(0, len as isize - 1) as usize
}

/// Accessible dropdown select with built-in open/close, highlight and value state.
///
/// State lives in persistent signals keyed by `id` (or by label and options when no `id` is
/// given), so it survives root re-invocation without the caller owning any signal. Pass
/// `value` to control the selection from outside, and `on_change` to observe it. Give each
/// `Select` that shares the same options and label its own `id`.
///
/// Keyboard: Arrow keys move the highlight (and open the list), Home/End jump, Enter or
/// Space picks, Escape closes. The list renders in the top layer anchored to the trigger, so
/// it escapes `overflow` containers and flips when it would leave the viewport.
#[component]
pub fn Select(
    id: Option<&'static str>,
    label: Option<&'static str>,
    options: Vec<SelectOption>,
    value: Option<Signal<String>>,
    on_change: Option<std::rc::Rc<dyn Fn(&str)>>,
    disabled: Option<bool>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let is_dis = disabled.unwrap_or(false);
    let key = match id {
        Some(id) => format!("select:{id}"),
        None => format!(
            "select:{}:{}",
            label.unwrap_or(""),
            options
                .iter()
                .map(|o| o.value.as_str())
                .collect::<Vec<_>>()
                .join("|")
        ),
    };
    let trigger_id = format!("{key}-trigger");

    let open = use_persistent_signal(&format!("{key}:open"), || false);
    let first_value = options.first().map(|o| o.value.clone()).unwrap_or_default();
    let value_sig =
        value.unwrap_or_else(|| use_persistent_signal(&format!("{key}:value"), || first_value));
    let current_value = value_sig.get();
    let selected_idx = options.iter().position(|o| o.value == current_value);
    let active = use_persistent_signal(&format!("{key}:active"), || 0usize);
    let is_open = open.get() && !is_dis;
    let active_idx = active.get().min(options.len().saturating_sub(1));

    let selected_label = selected_idx
        .map(|i| options[i].label.clone())
        .unwrap_or_default();

    let option_values: Vec<String> = options.iter().map(|o| o.value.clone()).collect();
    let pick = {
        let value_sig = value_sig.clone();
        let open = open.clone();
        let on_change = on_change.clone();
        move |idx: usize| {
            if let Some(v) = option_values.get(idx) {
                value_sig.set(v.clone());
                open.set(false);
                if let Some(cb) = on_change.as_ref() {
                    cb(v);
                }
            }
        }
    };

    let on_keydown = {
        let open = open.clone();
        let active = active.clone();
        let pick = pick.clone();
        let len = options.len();
        move |e: strata_dom::Event| {
            if is_dis {
                return;
            }
            let is_open_now = open.get();
            match e.key.as_str() {
                "ArrowDown" | "ArrowUp" => {
                    let delta = if e.key == "ArrowDown" { 1 } else { -1 };
                    if is_open_now {
                        active.set(select_step_index(len, active.get(), delta));
                    } else {
                        open.set(true);
                    }
                    e.prevent_default();
                }
                "Home" if is_open_now => {
                    active.set(0);
                    e.prevent_default();
                }
                "End" if is_open_now => {
                    active.set(len.saturating_sub(1));
                    e.prevent_default();
                }
                "Enter" | " " => {
                    if is_open_now {
                        pick(active.get());
                    } else {
                        open.set(true);
                    }
                    e.prevent_default();
                }
                "Escape" if is_open_now => {
                    open.set(false);
                    e.prevent_default();
                }
                _ => {}
            }
        }
    };

    let base = "w-full h-10 flex items-center justify-between bg-neutral-900 border border-neutral-800 rounded-lg px-3.5 text-sm text-white focus:outline-none focus:border-indigo-500 transition-colors cursor-pointer";
    let combined = format!("{base} {custom}");

    let menu = if is_open {
        let items: Vec<_> = options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let chosen = Some(i) == selected_idx;
                let highlighted = i == active_idx;
                let pick = pick.clone();
                let row_class = format!(
                    "px-3.5 py-2 text-sm whitespace-nowrap cursor-pointer {}",
                    if highlighted {
                        "bg-neutral-800 text-white"
                    } else {
                        "text-neutral-300"
                    }
                );
                rsx! {
                    <div
                        role="option"
                        aria-selected={if chosen { "true" } else { "false" }}
                        class={row_class}
                        onclick={move |_| pick(i)}
                    >
                        {opt.label.clone()}
                    </div>
                }
            })
            .collect();
        let close = open.clone();
        Some(create_dropdown_portal(
            trigger_id.clone(),
            AnchorPlacement::BottomStart,
            4.0,
            move |_| close.set(false),
            rsx! {
                <div
                    role="listbox"
                    class="w-full min-w-40 max-h-64 overflow-y-auto rounded-lg bg-neutral-900 border border-neutral-800 shadow-xl py-1"
                >
                    {items}
                </div>
            },
        ))
    } else {
        None
    };

    rsx! {
        <div class="flex flex-col gap-1.5 w-full">
            {if let Some(lbl) = label {
                rsx! { <label class="text-xs font-mono uppercase tracking-wider text-neutral-400 select-none">{lbl}</label> }
            } else {
                rsx! { <></> }
            }}
            <button
                type="button"
                id={trigger_id.clone()}
                role="combobox"
                aria-haspopup="listbox"
                aria-expanded={if is_open { "true" } else { "false" }}
                disabled={is_dis}
                class={combined}
                onclick={{
                    let open = open.clone();
                    move |_| {
                        if !is_dis {
                            open.update(|o| *o = !*o);
                        }
                    }
                }}
                onkeydown={on_keydown}
            >
                <span>{selected_label}</span>
                {Icon(IconName::ChevronDown, Some("w-4 h-4 text-neutral-400")).root}
            </button>
            {if let Some(menu) = menu { menu } else { rsx! { <></> } }}
        </div>
    }
}

/// Continuous range slider with value track and thumb.
#[component]
pub fn Slider(
    min: f32,
    max: f32,
    value: Signal<f32>,
    disabled: Option<bool>,
    class: Option<&'static str>,
) -> View {
    let val = value.get();
    let pct = ((val - min) / (max - min)).clamp(0.0, 1.0) * 100.0;
    let custom = class.unwrap_or("");

    rsx! {
        <div class={format!("relative flex items-center w-full py-2 {}", custom)}>
            <div class="w-full h-2 rounded-full bg-neutral-800 overflow-hidden">
                <div
                    class="h-full bg-indigo-600 rounded-full transition-all duration-75"
                    style={format!("width: {pct:.1}%;")}
                />
            </div>
            <div
                class="absolute w-4 h-4 rounded-full bg-white shadow-md border-2 border-indigo-600 cursor-pointer -ml-2 top-1"
                style={format!("left: {pct:.1}%;")}
            />
        </div>
    }
}

/// Multi-digit OTP / 2FA PinInput boxes.
#[component]
pub fn PinInput(length: usize, value: Signal<String>, disabled: Option<bool>) -> View {
    let len = length.max(1);
    let val = value.get();

    rsx! {
        <div class="flex items-center gap-2">
            {(0..len).map(|i| {
                let char_at = val.chars().nth(i).map(|c| c.to_string()).unwrap_or_default();
                rsx! {
                    <div class="w-12 h-14 rounded-xl border border-neutral-800 bg-neutral-900 flex items-center justify-center text-xl font-mono font-bold text-white shadow-inner">
                        {char_at}
                    </div>
                }
            })}
        </div>
    }
}
