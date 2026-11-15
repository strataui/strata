//! Root Application for Strata Creative Studio.

use crate::components::*;
use crate::state::StudioState;
use strata::prelude::*;

#[component]
pub fn App() -> View {
    let state = StudioState::new();

    rsx! {
        <div class="h-screen w-screen bg-neutral-950 text-white font-sans flex flex-col overflow-hidden select-none">
            <TopBar state={state.clone()} />

            <div class="flex-1 flex min-h-0 relative">
                <LayersPanel state={state.clone()} />
                <CanvasViewport state={state.clone()} />
                <PropertiesPanel state={state.clone()} />

                <ToolDock state={state} />
            </div>
        </div>
    }
}
