//! Root Application for Apex Athletics E-Commerce Storefront.

use crate::components::Navbar;
use crate::components::*;
use crate::state::StoreState;
use strata::prelude::*;

#[component]
pub fn App() -> View {
    let state = StoreState::new();

    rsx! {
        <div class="min-h-screen w-screen bg-neutral-950 text-white font-sans flex flex-col">
            <Navbar state={state.clone()} />

            <main class="flex-1 flex flex-col">
                <Hero />

                <div class="max-w-7xl mx-auto px-8 w-full flex flex-col">
                    <FilterBar state={state.clone()} />
                    <ProductGrid state={state.clone()} />
                </div>
            </main>

            <footer class="border-t border-neutral-800 bg-neutral-950 py-12 px-8 text-center text-xs text-neutral-500 font-mono">
                <p>{"© 2026 APEX ATHLETICS. POWERED ENTIRELY BY STRATA IN PURE RUST. ZERO CHROMIUM."}</p>
            </footer>

            <CartDrawer state={state} />
        </div>
    }
}
