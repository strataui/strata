//! Root application layout for the Strata Native Browser.

use strata::prelude::*;
use crate::components::status_bar::StatusBar;
use crate::components::tab_bar::TabBar;
use crate::components::toolbar::Toolbar;
use crate::components::viewport::Viewport;
use crate::state::BrowserState;

#[component]
pub fn BrowserApp(state: BrowserState) -> View {
    rsx! {
        <div class="flex flex-col h-screen w-screen bg-neutral-950 text-white overflow-hidden font-sans">
            <TabBar state={state.clone()} />
            <Toolbar state={state.clone()} />
            <Viewport state={state.clone()} />
            <StatusBar state={state} />
        </div>
    }
}

/// Creates a default BrowserApp View with fresh state.
pub fn default_browser_app() -> View {
    let state = BrowserState::new();
    BrowserApp(state)
}
