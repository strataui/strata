//! Filter and Sorting Bar for Apex Athletics Storefront.

use crate::state::StoreState;
use strata::prelude::*;

#[component]
pub fn FilterBar(state: StoreState) -> View {
    let current_category = state.selected_category.get();

    let categories = ["ALL", "Apparel", "Footwear", "Gear"];

    rsx! {
        <div class="flex items-center justify-between py-6 border-b border-neutral-800">
            <div class="flex items-center gap-2">
                {categories.iter().map(|&cat| {
                    let is_active = current_category == cat;
                    let active_style = if is_active {
                        "bg-white text-black font-semibold shadow-md"
                    } else {
                        "bg-neutral-900 text-neutral-400 hover:text-white border border-neutral-800"
                    };
                    let state = state.clone();

                    rsx! {
                        <button
                            class={format!("px-4 py-2 rounded-full text-xs transition-all {}", active_style)}
                            onclick={move |_| state.selected_category.set(cat.into())}
                        >
                            {cat}
                        </button>
                    }
                })}
            </div>

            <div class="flex items-center gap-3">
                <span class="text-xs font-mono text-neutral-500">{"Sort by:"}</span>
                <select class="h-9 bg-neutral-900 border border-neutral-800 rounded-lg px-3 text-xs text-white focus:outline-none">
                    <option>{"Featured & Popular"}</option>
                    <option>{"Price: Low to High"}</option>
                    <option>{"Price: High to Low"}</option>
                    <option>{"Highest Rated"}</option>
                </select>
            </div>
        </div>
    }
}
