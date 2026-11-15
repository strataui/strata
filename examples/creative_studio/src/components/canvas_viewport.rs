//! Infinite Canvas Viewport for Strata Creative Studio.

use crate::state::StudioState;
use strata::prelude::*;

#[component]
pub fn CanvasViewport(state: StudioState) -> View {
    let shapes = state.shapes.get();
    let selected_id = state.selected_shape_id.get();

    rsx! {
        <div class="flex-1 h-full bg-[#0c0c0e] relative overflow-hidden cursor-crosshair">
            <div class="absolute inset-0 opacity-20 pointer-events-none" style="background-image: radial-gradient(#3f3f46 1px, transparent 1px); background-size: 24px 24px;"></div>

            {shapes.into_iter().map(|shape| {
                let id_clone = shape.id.clone();
                let is_selected = selected_id.as_deref() == Some(&shape.id);
                let state_clone = state.clone();

                let border_style = if is_selected {
                    "ring-2 ring-indigo-500 shadow-2xl"
                } else {
                    "hover:ring-1 hover:ring-neutral-600"
                };

                rsx! {
                    <div
                        class={format!("absolute transition-shadow duration-100 cursor-move {}", border_style)}
                        style={format!(
                            "left: {}px; top: {}px; width: {}px; height: {}px; background-color: {}; border-radius: {}px; opacity: {};",
                            shape.x, shape.y, shape.width, shape.height, shape.fill_color, shape.corner_radius, shape.opacity
                        )}
                        onclick={move |_| state_clone.selected_shape_id.set(Some(id_clone.clone()))}
                    >
                        {if is_selected {
                            rsx! {
                                <div class="absolute -top-6 left-0 px-1.5 py-0.5 rounded bg-indigo-600 text-white text-[10px] font-mono whitespace-nowrap">
                                    {format!("{} ({}x{})", shape.name, shape.width as i32, shape.height as i32)}
                                </div>
                            }
                        } else {
                            rsx! { <></> }
                        }}
                    </div>
                }
            })}
        </div>
    }
}
