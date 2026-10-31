//! Navigation Sidebar for Strata Cloud SaaS Dashboard.

use strata::prelude::*;

#[component]
pub fn Sidebar() -> View {
    rsx! {
        <aside class="w-64 border-r border-neutral-800 bg-neutral-950 flex flex-col justify-between p-4 min-h-screen">
            <div class="flex flex-col gap-6">
                <div class="flex items-center gap-3 px-2">
                    <div class="w-8 h-8 rounded-lg bg-indigo-600 flex items-center justify-center font-bold text-white shadow-md shadow-indigo-600/30">
                        {"[*]"}
                    </div>
                    <div class="flex flex-col">
                        <span class="text-sm font-bold text-white tracking-tight">{"STRATA CLOUD"}</span>
                        <span class="text-xs text-neutral-500 font-mono">{"v0.1.0-alpha"}</span>
                    </div>
                </div>

                <nav class="flex flex-col gap-1">
                    <a class="flex items-center gap-3 px-3 py-2 rounded-lg bg-neutral-900 text-white text-sm font-medium transition-colors">
                        <span>{"[Charts]"}</span>
                        <span>{"Overview"}</span>
                    </a>
                    <a class="flex items-center gap-3 px-3 py-2 rounded-lg text-neutral-400 hover:text-white hover:bg-neutral-900/50 text-sm font-medium transition-colors">
                        <span>{"[>]"}</span>
                        <span>{"Deployments"}</span>
                    </a>
                    <a class="flex items-center gap-3 px-3 py-2 rounded-lg text-neutral-400 hover:text-white hover:bg-neutral-900/50 text-sm font-medium transition-colors">
                        <span>{"[Clusters]"}</span>
                        <span>{"GPU Clusters"}</span>
                    </a>
                    <a class="flex items-center gap-3 px-3 py-2 rounded-lg text-neutral-400 hover:text-white hover:bg-neutral-900/50 text-sm font-medium transition-colors">
                        <span>{"[Telemetry]"}</span>
                        <span>{"Telemetry & Logs"}</span>
                    </a>
                    <a class="flex items-center gap-3 px-3 py-2 rounded-lg text-neutral-400 hover:text-white hover:bg-neutral-900/50 text-sm font-medium transition-colors">
                        <span>{"[Pay]"}</span>
                        <span>{"Billing & Quotas"}</span>
                    </a>
                    <a class="flex items-center gap-3 px-3 py-2 rounded-lg text-neutral-400 hover:text-white hover:bg-neutral-900/50 text-sm font-medium transition-colors">
                        <span>{"[Settings]"}</span>
                        <span>{"Settings"}</span>
                    </a>
                </nav>
            </div>

            <div class="p-3 bg-neutral-900/60 border border-neutral-800 rounded-xl flex flex-col gap-2">
                <div class="flex items-center justify-between text-xs text-neutral-400 font-medium">
                    <span>{"GPU Pool Quota"}</span>
                    <span class="text-white font-mono">{"64 / 72 H100s"}</span>
                </div>
                <div class="w-full h-1.5 bg-neutral-800 rounded-full overflow-hidden">
                    <div class="w-[88%] h-full bg-indigo-500 rounded-full"></div>
                </div>
            </div>
        </aside>
    }
}
