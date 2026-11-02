//! Root application layout for the Strata Native Browser.

use strata::prelude::*;
use crate::components::devtools::DevToolsPanel;
use crate::components::tab_bar::TabBar;
use crate::components::toolbar::Toolbar;
use crate::components::viewport::Viewport;
use crate::state::BrowserState;

#[component]
pub fn BrowserApp(state: BrowserState) -> View {
    let is_devtools = state.is_devtools_open.get();

    rsx! {
        <div class="flex flex-col h-screen w-screen bg-neutral-950 text-white overflow-hidden font-sans">
            <TabBar state={state.clone()} />
            <Toolbar state={state.clone()} />
            {if is_devtools {
                let dev_state = state.clone();
                rsx! {
                    <div class="flex-1 flex flex-row min-h-0 w-full overflow-hidden">
                        <div class="flex-1 flex flex-col min-h-0 overflow-y-auto">
                            <Viewport state={state} />
                        </div>
                        <DevToolsPanel state={dev_state} />
                    </div>
                }
            } else {
                rsx! {
                    <div class="flex-1 flex flex-col min-h-0 w-full overflow-y-auto">
                        <Viewport state={state} />
                    </div>
                }
            }}
        </div>
    }
}

/// Creates a default BrowserApp View with fresh state.
pub fn default_browser_app() -> View {
    let state = BrowserState::new();
    BrowserApp(state)
}
