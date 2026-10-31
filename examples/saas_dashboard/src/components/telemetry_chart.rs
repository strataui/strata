//! Real-time Vector Time-Series Telemetry Chart for Strata SaaS Dashboard.

use strata::prelude::*;

#[component]
pub fn TelemetryChart() -> View {
    rsx! {
        <div class="p-6 rounded-2xl bg-neutral-900/60 border border-neutral-800 shadow-xl flex flex-col gap-4">
            <div class="flex items-center justify-between">
                <div>
                    <h3 class="text-sm font-semibold text-white">{"Cluster Latency Distribution (Last 60 Minutes)"}</h3>
                    <p class="text-xs text-neutral-400">{"Real-time p50, p95, and p99 millisecond telemetry streams"}</p>
                </div>
                <div class="flex items-center gap-2">
                    <span class="px-2.5 py-1 rounded-md bg-neutral-800 text-neutral-300 text-xs font-mono">{"1H"}</span>
                    <span class="px-2.5 py-1 rounded-md text-neutral-500 hover:text-neutral-300 text-xs font-mono">{"6H"}</span>
                    <span class="px-2.5 py-1 rounded-md text-neutral-500 hover:text-neutral-300 text-xs font-mono">{"24H"}</span>
                </div>
            </div>

            <div class="h-48 w-full bg-neutral-950/50 rounded-xl border border-neutral-800/80 p-4 flex flex-col justify-between relative overflow-hidden">
                <div class="flex justify-between text-[10px] text-neutral-600 font-mono border-b border-neutral-800/40 pb-1">
                    <span>{"30ms"}</span>
                    <span class="w-full border-b border-dashed border-neutral-800/30 mx-2"></span>
                </div>
                <div class="flex justify-between text-[10px] text-neutral-600 font-mono border-b border-neutral-800/40 pb-1">
                    <span>{"20ms"}</span>
                    <span class="w-full border-b border-dashed border-neutral-800/30 mx-2"></span>
                </div>
                <div class="flex justify-between text-[10px] text-neutral-600 font-mono border-b border-neutral-800/40 pb-1">
                    <span>{"10ms"}</span>
                    <span class="w-full border-b border-dashed border-neutral-800/30 mx-2"></span>
                </div>
                <div class="flex justify-between text-[10px] text-neutral-600 font-mono">
                    <span>{"0ms"}</span>
                    <span class="w-full border-b border-dashed border-neutral-800/30 mx-2"></span>
                </div>

                <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
                    <span class="text-xs font-mono text-indigo-400/80 bg-neutral-900/90 px-3 py-1 rounded border border-indigo-500/30 shadow-lg">
                        {"[*] Telemetry Live: Mean 14.8ms | Stable"}
                    </span>
                </div>
            </div>
        </div>
    }
}
