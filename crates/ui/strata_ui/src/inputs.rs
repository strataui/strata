//! High-Level Accessible Form Controls and Inputs for Strata UI.

#![allow(non_snake_case)]

use strata_dom::{Signal, View};
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

    let pl = if prefix_icon.is_some() { "pl-10" } else { "pl-3.5" };
    let pr = if suffix_icon.is_some() { "pr-10" } else { "pr-3.5" };

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
    let icon = if show { IconName::EyeOff } else { IconName::Eye };
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
            >
                {value.map(|s| s.get()).unwrap_or_default()}
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
        ("bg-neutral-900", "border-neutral-700 hover:border-neutral-500")
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

    let bg_color = if is_active { "bg-indigo-600" } else { "bg-neutral-800" };
    let knob_pos = if is_active { "translate-x-5" } else { "translate-x-0" };

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

/// Accessible dropdown select field.
#[component]
pub fn Select(
    label: Option<&'static str>,
    options: Vec<&'static str>,
    value: Option<Signal<String>>,
    disabled: Option<bool>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let base = "w-full h-10 bg-neutral-900 border border-neutral-800 rounded-lg px-3.5 text-sm text-white focus:outline-none focus:border-indigo-500 transition-colors cursor-pointer";
    let combined = format!("{base} {custom}");

    rsx! {
        <div class="flex flex-col gap-1.5 w-full">
            {if let Some(lbl) = label {
                rsx! { <label class="text-xs font-mono uppercase tracking-wider text-neutral-400 select-none">{lbl}</label> }
            } else {
                rsx! { <></> }
            }}
            <select
                class={combined}
                disabled={disabled.unwrap_or(false)}
            >
                {options.into_iter().map(|opt| rsx! { <option value={opt}>{opt}</option> })}
            </select>
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
pub fn PinInput(
    length: usize,
    value: Signal<String>,
    disabled: Option<bool>,
) -> View {
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
