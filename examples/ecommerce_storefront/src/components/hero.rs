//! Hero Banner for Apex Athletics Storefront.

use strata::prelude::*;

#[component]
pub fn Hero() -> View {
    rsx! {
        <section class="relative w-full py-20 px-8 bg-gradient-to-b from-neutral-900 to-neutral-950 border-b border-neutral-800 flex flex-col items-center justify-center text-center overflow-hidden">
            <div class="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-indigo-500/10 border border-indigo-500/30 text-indigo-400 text-xs font-mono mb-6">
                <span>{"[*] FW26 AEROTECH COLLECTION DROPPED"}</span>
            </div>

            <h1 class="text-5xl md:text-7xl font-extrabold tracking-tight text-white max-w-4xl leading-tight">
                {"Engineered for Zero Air Resistance & Maximum Velocity."}
            </h1>

            <p class="mt-6 text-lg text-neutral-400 max-w-2xl leading-relaxed">
                {"Precision carbon-matrix compression fibers with thermoregulating graphene microstructures. Tested under wind-tunnel aerodynamic extremes."}
            </p>

            <div class="mt-8 flex items-center gap-4">
                <button class="px-8 py-3.5 rounded-full bg-indigo-600 hover:bg-indigo-500 active:scale-95 text-white font-semibold text-sm shadow-xl shadow-indigo-600/30 transition-all">
                    {"Shop The Collection"}
                </button>
                <button class="px-8 py-3.5 rounded-full bg-neutral-900 border border-neutral-800 hover:border-neutral-700 text-white font-semibold text-sm transition-all">
                    {"View Lab Whitepaper"}
                </button>
            </div>
        </section>
    }
}
