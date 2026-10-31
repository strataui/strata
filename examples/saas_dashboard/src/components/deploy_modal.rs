//! Slide-Over Deployment Modal for Strata SaaS Dashboard.

use strata::prelude::*;
use crate::state::DashboardState;

#[component]
pub fn DeployModal(state: DashboardState) -> View {
    let is_open = state.is_deploy_modal_open.get();
    if !is_open {
        return rsx! { <></> };
    }

    let replica_count = use_signal(8);
    let selected_model = use_signal(String::from("meta/llama-3.3-70b-instruct"));
    let selected_region = use_signal(String::from("us-east-1"));

    let close_state_1 = state.clone();
    let close_state_2 = state.clone();
    let close_state_3 = state.clone();
    let rc_change = replica_count.clone();

    rsx! {
        <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
            <div class="w-full max-w-xl rounded-2xl bg-neutral-900 border border-neutral-800 shadow-2xl p-6 flex flex-col gap-6">
                <div class="flex items-center justify-between border-b border-neutral-800 pb-4">
                    <div>
                        <h2 class="text-lg font-bold text-white">{"Deploy Model Endpoint"}</h2>
                        <p class="text-xs text-neutral-400">{"Provision dedicated H100 SXM5 GPU nodes with sub-20ms SLAs"}</p>
                    </div>
                    <button
                        class="text-neutral-400 hover:text-white text-lg font-bold p-1"
                        onclick={move |_| close_state_1.is_deploy_modal_open.set(false)}
                    >
                        {"x"}
                    </button>
                </div>

                <div class="flex flex-col gap-4">
                    <div class="flex flex-col gap-1.5">
                        <label class="text-xs font-mono uppercase tracking-wider text-neutral-400">{"Model Foundation"}</label>
                        <select
                            class="h-10 bg-neutral-950 border border-neutral-800 rounded-lg px-3 text-sm text-white focus:outline-none focus:border-indigo-500"
                        >
                            <option>{"meta/llama-3.3-70b-instruct (FP8 Optimized)"}</option>
                            <option>{"mistralai/mistral-large-2-123b (BF16)"}</option>
                            <option>{"deepseek-ai/deepseek-coder-v2 (236B MoE)"}</option>
                            <option>{"qwen/qwen-2.5-coder-32b-instruct"}</option>
                        </select>
                    </div>

                    <div class="grid grid-cols-2 gap-4">
                        <div class="flex flex-col gap-1.5">
                            <label class="text-xs font-mono uppercase tracking-wider text-neutral-400">{"Target Region"}</label>
                            <select
                                class="h-10 bg-neutral-950 border border-neutral-800 rounded-lg px-3 text-sm text-white focus:outline-none focus:border-indigo-500"
                            >
                                <option>{"us-east-1 (N. Virginia, USA)"}</option>
                                <option>{"us-west-2 (Oregon, USA)"}</option>
                                <option>{"eu-central-1 (Frankfurt, Germany)"}</option>
                                <option>{"ap-northeast-1 (Tokyo, Japan)"}</option>
                            </select>
                        </div>

                        <div class="flex flex-col gap-1.5">
                            <div class="flex justify-between items-center text-xs font-mono text-neutral-400">
                                <span>{"GPU Replicas"}</span>
                                <span class="text-indigo-400 font-bold">{format!("{} Nodes", replica_count.get())}</span>
                            </div>
                            <input
                                type="range"
                                min="1"
                                max="32"
                                value={replica_count.get().to_string()}
                                class="h-10 accent-indigo-500 cursor-pointer"
                                onchange={move |val: String| {
                                    if let Ok(n) = val.parse::<usize>() {
                                        rc_change.set(n);
                                    }
                                }}
                            />
                        </div>
                    </div>

                    <div class="p-4 rounded-xl bg-neutral-950 border border-neutral-800 flex items-center justify-between">
                        <div class="flex flex-col">
                            <span class="text-xs text-neutral-400">{"Estimated Running Cost"}</span>
                            <span class="text-xs font-mono text-neutral-500">{"Billed per second of compute"}</span>
                        </div>
                        <div class="text-xl font-bold font-mono text-white">
                            {format!("${:.2} / hr", replica_count.get() as f64 * 3.49)}
                        </div>
                    </div>
                </div>

                <div class="flex items-center justify-end gap-3 pt-2">
                    <button
                        class="px-4 py-2 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-300 text-sm font-medium transition-colors"
                        onclick={move |_| close_state_2.is_deploy_modal_open.set(false)}
                    >
                        {"Cancel"}
                    </button>
                    <button
                        class="px-5 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 active:scale-95 text-white text-sm font-medium shadow-lg shadow-indigo-600/30 transition-all"
                        onclick={move |_| {
                            println!("[>] Provisioning model endpoint...");
                            close_state_3.is_deploy_modal_open.set(false);
                        }}
                    >
                        {"Confirm & Deploy"}
                    </button>
                </div>
            </div>
        </div>
    }
}
