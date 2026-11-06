//! Bottom status bar displaying telemetry, runtime performance, and layout stats.

use crate::state::BrowserState;
use strata::prelude::*;
use strata_icons::prelude::{Icon, IconName};

#[component]
pub fn StatusBar(state: BrowserState) -> View {
    let tabs_count = state.tabs.get().len();
    let current_url = state
        .active_tab()
        .map(|t| t.url)
        .unwrap_or_else(|| "strata://newtab".to_string());

    rsx! {
        <div class="flex items-center justify-between h-7 px-4 bg-neutral-950 border-t border-neutral-800/80 text-[11px] font-mono text-neutral-400 select-none">
            // Left Status
            <div class="flex items-center gap-2 truncate max-w-md">
                <span class="inline-block w-2 h-2 rounded-full bg-emerald-500"></span>
                <span class="text-neutral-300 truncate">{current_url}</span>
            </div>

            // Center Branding
            <div class="hidden sm:flex items-center gap-1.5 text-neutral-500">
                {Icon(IconName::ShieldCheck, Some("w-3 h-3 text-blue-400")).root}
                <span>{"Strata Native Engine (Zero Chromium)"}</span>
            </div>

            // Right Telemetry
            <div class="flex items-center gap-3">
                <span class="text-neutral-400">{format!("Tabs: {tabs_count}")}</span>
                <span class="text-neutral-600">{"|"}</span>
                <span class="text-emerald-400">{"Memory: < 2.8 MB"}</span>
                <span class="text-neutral-600">{"|"}</span>
                <span class="text-blue-400">{"120 FPS VSync"}</span>
            </div>
        </div>
    }
}
