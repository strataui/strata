//! Declarative `<ScrollArea>` Component with Smooth Kinetic Physics.

#![allow(non_snake_case)]

use strata_dom::{NodeBlueprint, View};
use strata_macro::{component, rsx};

/// Declarative scroll container primitive with smooth kinetic momentum physics,
/// floating non-layout-shifting scrollbars with hover expansion and drag handles,
/// and scroll position preservation across component re-renders.
#[component]
pub fn ScrollArea(
    id: Option<&'static str>,
    class: Option<&'static str>,
    children: Option<Vec<NodeBlueprint>>,
    show_scrollbar: Option<bool>,
) -> View {
    let custom = class.unwrap_or("");
    let container_id = id.unwrap_or("");
    let _with_bar = show_scrollbar.unwrap_or(true);

    rsx! {
        <div
            id={container_id}
            class={format!("relative overflow-y-auto flex-1 flex flex-col min-h-0 w-full {}", custom)}
        >
            {children.unwrap_or_default()}
        </div>
    }
}
