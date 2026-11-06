//! Declarative Window Dragging & Frameless Desktop Chrome Components.

#![allow(non_snake_case)]

use strata_dom::{NodeBlueprint, View};
use strata_macro::{component, rsx};

/// Window drag handle component that automatically initiates native OS window dragging on pointer down
/// and toggles maximize on double click.
#[component]
pub fn WindowDragHandle(
    class: Option<&'static str>,
    children: Option<Vec<NodeBlueprint>>,
) -> View {
    let custom = class.unwrap_or("");
    rsx! {
        <div data-app-region="drag" class={format!("flex items-center select-none cursor-default {}", custom)}>
            {children.unwrap_or_default()}
        </div>
    }
}

/// Declarative frameless window TitleBar component with native drag regions, double-click to maximize,
/// and integrated window control buttons.
#[component]
pub fn TitleBar(
    title: Option<String>,
    drag_region: Option<bool>,
    show_controls: Option<bool>,
    on_minimize: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    on_maximize: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    on_close: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    class: Option<&'static str>,
    children: Option<Vec<NodeBlueprint>>,
) -> View {
    let custom = class.unwrap_or("");
    let is_drag = drag_region.unwrap_or(true);
    let app_region = if is_drag { "drag" } else { "no-drag" };
    let controls = show_controls.unwrap_or(true);
    let window_title = title.unwrap_or_else(|| "Strata Application".to_string());

    let min_cb = on_minimize.unwrap_or_else(|| std::rc::Rc::new(|_| {}));
    let max_cb = on_maximize.unwrap_or_else(|| std::rc::Rc::new(|_| {}));
    let close_cb = on_close.unwrap_or_else(|| std::rc::Rc::new(|_| {}));

    rsx! {
        <header
            data-app-region={app_region}
            class={format!("w-full h-10 px-3 bg-[#1e1e1e] border-b border-[#2d2d2d] flex items-center justify-between select-none {}", custom)}
        >
            <div data-app-region={app_region} class="flex items-center space-x-2 flex-1 min-w-0">
                <span data-app-region={app_region} class="text-xs font-medium text-neutral-300 truncate">
                    {window_title}
                </span>
            </div>

            <div data-app-region="no-drag" class="flex items-center space-x-1 shrink-0">
                {children.unwrap_or_default()}

                {if controls {
                    rsx! {
                        <div class="flex items-center space-x-1 ml-2">
                            <button
                                data-app-region="no-drag"
                                data-window-action="minimize"
                                class="w-7 h-7 flex items-center justify-center rounded hover:bg-neutral-700/60 text-neutral-400 hover:text-neutral-200 transition-colors cursor-pointer"
                                onclick={min_cb}
                            >
                                <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                    <line x1="5" y1="12" x2="19" y2="12" />
                                </svg>
                            </button>
                            <button
                                data-app-region="no-drag"
                                data-window-action="maximize"
                                class="w-7 h-7 flex items-center justify-center rounded hover:bg-neutral-700/60 text-neutral-400 hover:text-neutral-200 transition-colors cursor-pointer"
                                onclick={max_cb}
                            >
                                <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                    <rect x="4" y="4" width="16" height="16" rx="2" />
                                </svg>
                            </button>
                            <button
                                data-app-region="no-drag"
                                data-window-action="close"
                                class="w-7 h-7 flex items-center justify-center rounded hover:bg-red-600/80 text-neutral-400 hover:text-white transition-colors cursor-pointer"
                                onclick={close_cb}
                            >
                                <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                    <line x1="18" y1="6" x2="6" y2="18" />
                                    <line x1="6" y1="6" x2="18" y2="18" />
                                </svg>
                            </button>
                        </div>
                    }
                } else {
                    rsx! { <></> }
                }}
            </div>
        </header>
    }
}
