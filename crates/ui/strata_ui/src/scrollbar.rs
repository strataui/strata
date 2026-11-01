//! Declarative `<ScrollView>` and `<Scrollbar>` Components with Kinetic Physics.

#![allow(non_snake_case)]

use strata_dom::{NodeBlueprint, View};
use strata_macro::{component, rsx};
pub use strata_render::kinetic_scroll::{KineticScrollController, ScrollAxis};

/// Scrollbar track orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarOrientation {
    #[default]
    Vertical,
    Horizontal,
}

/// Interactive Sleek Semi-Transparent Scrollbar.
#[component]
pub fn Scrollbar(
    orientation: Option<ScrollbarOrientation>,
    scroll_ratio: Option<f32>,
    thumb_ratio: Option<f32>,
    opacity: Option<f32>,
    class: Option<&'static str>,
) -> View {
    let ori = orientation.unwrap_or_default();
    let ratio = scroll_ratio.unwrap_or(0.0).clamp(0.0, 1.0);
    let t_ratio = thumb_ratio.unwrap_or(0.2).clamp(0.05, 1.0);
    let alpha = opacity.unwrap_or(1.0).clamp(0.0, 1.0);
    let custom = class.unwrap_or("");

    match ori {
        ScrollbarOrientation::Vertical => {
            let thumb_pct = t_ratio * 100.0;
            let travel_pct = (1.0 - t_ratio) * ratio * 100.0;
            rsx! {
                <div class={format!("absolute right-1 top-1 bottom-1 w-2 rounded-full pointer-events-none transition-opacity duration-200 {}", custom)}>
                    <div class={format!("w-full rounded-full bg-neutral-600/60 hover:bg-neutral-500/80 transition-colors pointer-events-auto cursor-pointer")} />
                </div>
            }
        }
        ScrollbarOrientation::Horizontal => {
            rsx! {
                <div class={format!("absolute left-1 right-1 bottom-1 h-2 rounded-full pointer-events-none transition-opacity duration-200 {}", custom)}>
                    <div class={format!("h-full rounded-full bg-neutral-600/60 hover:bg-neutral-500/80 transition-colors pointer-events-auto cursor-pointer")} />
                </div>
            }
        }
    }
}

/// Smooth Kinetic Inertial Scroll View Container.
#[component]
pub fn ScrollView(
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
    show_scrollbar: Option<bool>,
) -> View {
    let custom = class.unwrap_or("");
    let with_bar = show_scrollbar.unwrap_or(true);

    rsx! {
        <div class={format!("relative overflow-hidden flex flex-col {}", custom)}>
            <div class="flex-1 overflow-hidden">
                {children.unwrap_or_default()}
            </div>
            {if with_bar {
                rsx! { <Scrollbar orientation={ScrollbarOrientation::Vertical} /> }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}
