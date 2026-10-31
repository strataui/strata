//! Floating Tool Dock for Strata Creative Studio.

use strata::prelude::*;
use crate::state::{StudioState, Tool};

#[component]
pub fn ToolDock(state: StudioState) -> View {
    let current_tool = state.active_tool.get();

    let tools = [
        (Tool::Select, "↖", "V"),
        (Tool::Frame, "⊞", "F"),
        (Tool::Rectangle, "▭", "R"),
        (Tool::Ellipse, "○", "O"),
        (Tool::Text, "T", "T"),
        (Tool::Pan, "[Pan]", "H"),
    ];

    rsx! {
        <div class="absolute bottom-6 left-1/2 -translate-x-1/2 z-40 bg-neutral-900/90 backdrop-blur-md border border-neutral-800 rounded-2xl p-1.5 shadow-2xl flex items-center gap-1">
            {tools.iter().map(|&(tool, icon, shortcut)| {
                let is_active = current_tool == tool;
                let active_style = if is_active {
                    "bg-indigo-600 text-white shadow-md shadow-indigo-600/30 font-bold"
                } else {
                    "text-neutral-400 hover:text-white hover:bg-neutral-800/80"
                };
                let state = state.clone();

                rsx! {
                    <button
                        class={format!("relative w-10 h-10 rounded-xl flex items-center justify-center text-sm transition-all {}", active_style)}
                        onclick={move |_| state.active_tool.set(tool)}
                    >
                        <span>{icon}</span>
                        <span class="absolute bottom-1 right-1 text-[8px] font-mono opacity-50">{shortcut}</span>
                    </button>
                }
            })}
        </div>
    }
}
