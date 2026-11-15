//! Deployments Table with real-time status and search filtering for Strata SaaS Dashboard.

use crate::state::DashboardState;
use strata::prelude::*;

#[component]
pub fn DeploymentsTable(state: DashboardState) -> View {
    let query = state.search_query.get().to_lowercase();
    let filter = state.status_filter.get();

    let all_deployments = state.deployments.get();
    let filtered_deployments: Vec<_> = all_deployments
        .into_iter()
        .filter(|dep| {
            let matches_query = query.is_empty()
                || dep.name.to_lowercase().contains(&query)
                || dep.model.to_lowercase().contains(&query);
            let matches_status = filter == "ALL" || dep.status == filter;
            matches_query && matches_status
        })
        .collect();

    rsx! {
        <div class="rounded-2xl bg-neutral-900/60 border border-neutral-800 shadow-xl overflow-hidden flex flex-col">
            <div class="p-4 border-b border-neutral-800 flex items-center justify-between">
                <div class="flex items-center gap-3">
                    <h3 class="text-sm font-semibold text-white">{"Active Model Deployments"}</h3>
                    <span class="px-2 py-0.5 rounded-full bg-neutral-800 text-xs font-mono text-neutral-400">
                        {filtered_deployments.len().to_string()}
                    </span>
                </div>

                <div class="flex items-center gap-2">
                    {let s = state.clone(); rsx! {
                        <button
                            class={if filter == "ALL" { "px-3 py-1 text-xs rounded-lg bg-neutral-800 text-white font-medium" } else { "px-3 py-1 text-xs rounded-lg text-neutral-400 hover:text-white" }}
                            onclick={move |_| s.status_filter.set("ALL".into())}
                        >
                            {"All"}
                        </button>
                    }}
                    {let s = state.clone(); rsx! {
                        <button
                            class={if filter == "RUNNING" { "px-3 py-1 text-xs rounded-lg bg-neutral-800 text-white font-medium" } else { "px-3 py-1 text-xs rounded-lg text-neutral-400 hover:text-white" }}
                            onclick={move |_| s.status_filter.set("RUNNING".into())}
                        >
                            {"Running"}
                        </button>
                    }}
                    {let s = state.clone(); rsx! {
                        <button
                            class={if filter == "PROVISIONING" { "px-3 py-1 text-xs rounded-lg bg-neutral-800 text-white font-medium" } else { "px-3 py-1 text-xs rounded-lg text-neutral-400 hover:text-white" }}
                            onclick={move |_| s.status_filter.set("PROVISIONING".into())}
                        >
                            {"Provisioning"}
                        </button>
                    }}
                </div>
            </div>

            <div class="overflow-x-auto">
                <div class="min-w-full divide-y divide-neutral-800/60">
                    <div class="grid grid-cols-12 px-6 py-3 text-xs font-mono text-neutral-500 uppercase tracking-wider bg-neutral-950/40">
                        <span class="col-span-4">{"Deployment & Model"}</span>
                        <span class="col-span-2">{"Region"}</span>
                        <span class="col-span-2">{"Status"}</span>
                        <span class="col-span-2">{"Replicas"}</span>
                        <span class="col-span-2 text-right">{"P99 Latency"}</span>
                    </div>

                    <div class="divide-y divide-neutral-800/40">
                        {filtered_deployments.into_iter().map(|dep| {
                            let status_color = match dep.status {
                                "RUNNING" => "text-emerald-400 bg-emerald-500/10 border-emerald-500/20",
                                "PROVISIONING" => "text-amber-400 bg-amber-500/10 border-amber-500/20",
                                _ => "text-rose-400 bg-rose-500/10 border-rose-500/20",
                            };

                            rsx! {
                                <div class="grid grid-cols-12 px-6 py-4 items-center hover:bg-neutral-800/30 transition-colors">
                                    <div class="col-span-4 flex flex-col">
                                        <span class="text-sm font-medium text-white font-mono">{dep.name}</span>
                                        <span class="text-xs text-neutral-400">{dep.model}</span>
                                    </div>
                                    <div class="col-span-2">
                                        <span class="text-xs font-mono text-neutral-300 px-2 py-0.5 rounded bg-neutral-800">
                                            {dep.region}
                                        </span>
                                    </div>
                                    <div class="col-span-2">
                                        <span class={format!("text-xs font-mono px-2.5 py-0.5 rounded-full border {}", status_color)}>
                                            {dep.status}
                                        </span>
                                    </div>
                                    <div class="col-span-2 text-sm font-mono text-white">
                                        {format!("{} GPUs", dep.replicas)}
                                    </div>
                                    <div class="col-span-2 text-right text-sm font-mono font-semibold text-neutral-200">
                                        {format!("{:.1}ms", dep.latency_p99)}
                                    </div>
                                </div>
                            }
                        })}
                    </div>
                </div>
            </div>
        </div>
    }
}
