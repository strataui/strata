//! Root Application Layout for Strata Cloud SaaS Dashboard.

use crate::components::Sidebar;
use crate::components::*;
use crate::state::DashboardState;
use strata::prelude::*;

#[component]
pub fn App() -> View {
    let state = DashboardState::new();

    rsx! {
        <div class="flex h-screen w-screen bg-neutral-950 text-white overflow-hidden font-sans">
            <Sidebar />

            <div class="flex flex-col flex-1 min-w-0 overflow-y-auto">
                <Header state={state.clone()} />

                <main class="p-8 flex flex-col gap-8 max-w-7xl mx-auto w-full">
                    <div class="flex items-center justify-between">
                        <div>
                            <h1 class="text-2xl font-bold tracking-tight text-white">
                                {"AI Infrastructure & Model Telemetry"}
                            </h1>
                            <p class="text-sm text-neutral-400 mt-1">
                                {"Global compute nodes, active inference sessions, and GPU clusters."}
                            </p>
                        </div>
                    </div>

                    <MetricsGrid state={state.clone()} />

                    <div class="grid grid-cols-2 gap-8">
                        <TelemetryChart />
                        <DatacenterGlobe3D />
                    </div>

                    <DeploymentsTable state={state.clone()} />
                </main>
            </div>

            <DeployModal state={state} />
        </div>
    }
}
