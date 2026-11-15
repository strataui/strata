//! Top Application Bar for Strata Creative Studio.

use crate::state::StudioState;
use strata::prelude::*;

#[component]
pub fn TopBar(state: StudioState) -> View {
    let zoom = state.zoom_level.get();

    rsx! {
        <header class="h-14 border-b border-neutral-800 bg-neutral-950 px-4 flex items-center justify-between z-30 select-none">
            <div class="flex items-center gap-4">
                <div class="flex items-center gap-2">
                    <span class="w-6 h-6 rounded bg-gradient-to-tr from-indigo-500 to-purple-600 flex items-center justify-center text-xs font-bold text-white">
                        {"S"}
                    </span>
                    <span class="text-xs font-bold text-white tracking-wider">{"STRATA STUDIO"}</span>
                </div>

                <span class="text-neutral-700">{"/"}</span>

                <span class="text-xs font-medium text-neutral-300 hover:text-white cursor-pointer">
                    {"Untitled Design 01.strata"}
                </span>
            </div>

            <div class="flex items-center gap-3">
                <div class="flex items-center gap-1 bg-neutral-900 border border-neutral-800 rounded-lg p-1">
                    {let s = state.clone(); rsx! {
                        <button
                            class="w-7 h-7 flex items-center justify-center text-xs text-neutral-400 hover:text-white rounded hover:bg-neutral-800"
                            onclick={move |_| s.zoom_level.update(|z| *z = (*z - 10.0).max(25.0))}
                        >
                            {"-"}
                        </button>
                    }}
                    <span class="px-2 text-xs font-mono font-medium text-neutral-200">
                        {format!("{}%", zoom as i32)}
                    </span>
                    {let s = state.clone(); rsx! {
                        <button
                            class="w-7 h-7 flex items-center justify-center text-xs text-neutral-400 hover:text-white rounded hover:bg-neutral-800"
                            onclick={move |_| s.zoom_level.update(|z| *z = (*z + 10.0).min(500.0))}
                        >
                            {"+"}
                        </button>
                    }}
                </div>

                <button class="px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 active:scale-95 text-white text-xs font-medium shadow-md shadow-indigo-600/30 flex items-center gap-2">
                    <span>{"Export RSX"}</span>
                </button>
            </div>
        </header>
    }
}
