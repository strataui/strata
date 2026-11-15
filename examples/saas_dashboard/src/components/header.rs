//! Top Navigation Header for Strata Cloud SaaS Dashboard.

use crate::state::DashboardState;
use strata::prelude::*;

#[component]
pub fn Header(state: DashboardState) -> View {
    rsx! {
        <header class="h-16 border-b border-neutral-800 bg-neutral-950/80 backdrop-blur-md px-6 flex items-center justify-between sticky top-0 z-40">
            <div class="flex items-center gap-4">
                <div class="flex items-center gap-2">
                    <span class="w-2.5 h-2.5 rounded-full bg-emerald-400 animate-pulse"></span>
                    <span class="text-xs font-mono font-medium text-neutral-300 uppercase tracking-wider">
                        {"All Systems Operational"}
                    </span>
                </div>
                <span class="text-neutral-700">{"/"}</span>
                <span class="text-xs font-mono text-neutral-400">
                    {"Cluster: us-east-prod-01"}
                </span>
            </div>

            <div class="flex items-center gap-3">
                <div class="relative w-80">
                    <Input
                        placeholder="Search deployments, models, clusters... (⌘K)"
                        value={Some(state.search_query.clone())}
                    />
                </div>

                <Button
                    variant={Some(ButtonVariant::Primary)}
                    size={Some(ButtonSize::Sm)}
                    onclick={move |_| state.is_deploy_modal_open.set(true)}
                >
                    <Icons::Plus class={Some("w-4 h-4 mr-1")} />
                    <span>{"Deploy Cluster"}</span>
                </Button>

                <Avatar fallback="LV" />
            </div>
        </header>
    }
}
