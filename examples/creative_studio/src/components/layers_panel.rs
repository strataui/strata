//! Left Layers Tree Panel for Strata Creative Studio.

use strata::prelude::*;
use crate::state::StudioState;

#[component]
pub fn LayersPanel(state: StudioState) -> View {
    let shapes = state.shapes.get();
    let selected_id = state.selected_shape_id.get();

    rsx! {
        <aside class="w-60 border-r border-neutral-800 bg-neutral-950 flex flex-col justify-between h-full select-none z-20">
            <div class="flex flex-col">
                <div class="h-10 px-4 border-b border-neutral-800 flex items-center justify-between">
                    <span class="text-xs font-mono uppercase tracking-wider text-neutral-400">{"Layers"}</span>
                    <span class="text-xs text-neutral-600 font-mono">{format!("{} items", shapes.len())}</span>
                </div>

                <div class="flex flex-col p-2 gap-0.5">
                    {shapes.into_iter().rev().map(|shape| {
                        let id_clone = shape.id.clone();
                        let is_selected = selected_id.as_deref() == Some(&shape.id);
                        let state_clone = state.clone();

                        let item_style = if is_selected {
                            "bg-indigo-600/20 text-indigo-400 font-medium border border-indigo-500/30"
                        } else {
                            "text-neutral-400 hover:text-white hover:bg-neutral-900/60"
                        };

                        rsx! {
                            <div
                                class={format!("flex items-center justify-between px-3 py-2 rounded-lg text-xs cursor-pointer transition-colors {}", item_style)}
                                onclick={move |_| state_clone.selected_shape_id.set(Some(id_clone.clone()))}
                            >
                                <div class="flex items-center gap-2">
                                    <span class="opacity-60">{match shape.shape_type {
                                        "frame" => "⊞",
                                        "rectangle" => "▭",
                                        "ellipse" => "○",
                                        _ => "◇",
                                    }}</span>
                                    <span class="truncate">{shape.name}</span>
                                </div>
                                <div class="flex items-center gap-1.5 opacity-60 hover:opacity-100">
                                    <span>{if shape.is_visible { "[V]" } else { "[-]" }}</span>
                                    <span>{if shape.is_locked { "[L]" } else { "" }}</span>
                                </div>
                            </div>
                        }
                    })}
                </div>
            </div>

            <div class="p-3 border-t border-neutral-800 text-[10px] font-mono text-neutral-500">
                {"GPU Canvas: Metal / Vulkan Direct"}
            </div>
        </aside>
    }
}
