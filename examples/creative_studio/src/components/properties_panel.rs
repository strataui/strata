//! Right Properties Inspector Panel for Strata Creative Studio.

use crate::state::StudioState;
use strata::prelude::*;

#[component]
pub fn PropertiesPanel(state: StudioState) -> View {
    let maybe_shape = state.selected_shape();

    rsx! {
        <aside class="w-72 border-l border-neutral-800 bg-neutral-950 flex flex-col justify-between h-full select-none z-20 overflow-y-auto">
            {if let Some(shape) = maybe_shape {
                rsx! {
                    <div class="flex flex-col p-4 gap-6">
                        <div class="flex items-center justify-between border-b border-neutral-800 pb-3">
                            <span class="text-xs font-mono uppercase tracking-wider text-neutral-400">{"Transform"}</span>
                            <span class="text-xs font-mono text-neutral-500">{shape.id.clone()}</span>
                        </div>

                        <div class="grid grid-cols-2 gap-3">
                            <div class="flex flex-col gap-1">
                                <label class="text-[10px] font-mono text-neutral-500 uppercase">{"X Position"}</label>
                                <div class="h-8 bg-neutral-900 border border-neutral-800 rounded px-2.5 flex items-center text-xs font-mono text-white">
                                    {format!("{:.0}", shape.x)}
                                </div>
                            </div>
                            <div class="flex flex-col gap-1">
                                <label class="text-[10px] font-mono text-neutral-500 uppercase">{"Y Position"}</label>
                                <div class="h-8 bg-neutral-900 border border-neutral-800 rounded px-2.5 flex items-center text-xs font-mono text-white">
                                    {format!("{:.0}", shape.y)}
                                </div>
                            </div>
                            <div class="flex flex-col gap-1">
                                <label class="text-[10px] font-mono text-neutral-500 uppercase">{"Width"}</label>
                                <div class="h-8 bg-neutral-900 border border-neutral-800 rounded px-2.5 flex items-center text-xs font-mono text-white">
                                    {format!("{:.0}", shape.width)}
                                </div>
                            </div>
                            <div class="flex flex-col gap-1">
                                <label class="text-[10px] font-mono text-neutral-500 uppercase">{"Height"}</label>
                                <div class="h-8 bg-neutral-900 border border-neutral-800 rounded px-2.5 flex items-center text-xs font-mono text-white">
                                    {format!("{:.0}", shape.height)}
                                </div>
                            </div>
                        </div>

                        <div class="flex flex-col gap-2">
                            <div class="flex justify-between items-center text-[10px] font-mono text-neutral-500 uppercase">
                                <span>{"Corner Radius"}</span>
                                <span class="text-neutral-300 font-mono">{format!("{}px", shape.corner_radius as i32)}</span>
                            </div>
                            <div class="h-8 bg-neutral-900 border border-neutral-800 rounded px-2.5 flex items-center text-xs font-mono text-white">
                                {format!("{:.0}px", shape.corner_radius)}
                            </div>
                        </div>

                        <div class="flex flex-col gap-3 border-t border-neutral-800 pt-4">
                            <span class="text-xs font-mono uppercase tracking-wider text-neutral-400">{"Fill & Paint"}</span>
                            <div class="flex items-center gap-3">
                                <div class="w-8 h-8 rounded-lg border border-neutral-700" style={format!("background-color: {};", shape.fill_color)}></div>
                                <div class="flex-1 h-8 bg-neutral-900 border border-neutral-800 rounded px-2.5 flex items-center text-xs font-mono text-white">
                                    {shape.fill_color}
                                </div>
                                <div class="w-16 h-8 bg-neutral-900 border border-neutral-800 rounded px-2 flex items-center justify-center text-xs font-mono text-neutral-300">
                                    {format!("{}%", (shape.opacity * 100.0) as i32)}
                                </div>
                            </div>
                        </div>

                        <div class="flex flex-col gap-3 border-t border-neutral-800 pt-4">
                            <span class="text-xs font-mono uppercase tracking-wider text-neutral-400">{"Layer Security"}</span>
                            <Switch
                                label={Some("Lock Layer")}
                                description={Some("Prevent canvas dragging and resizing")}
                                checked={use_signal(shape.is_locked)}
                            />
                            <Switch
                                label={Some("Visible")}
                                description={Some("Show or hide layer in viewport")}
                                checked={use_signal(shape.is_visible)}
                            />
                        </div>
                    </div>
                }
            } else {
                rsx! {
                    <div class="p-6 text-center text-neutral-500 text-xs">
                        {"Select a layer on the canvas to inspect its properties."}
                    </div>
                }
            }}
        </aside>
    }
}
