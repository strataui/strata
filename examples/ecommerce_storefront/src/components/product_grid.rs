//! Product Grid Catalog for Apex Athletics Storefront.

use strata::prelude::*;
use crate::state::StoreState;

#[component]
pub fn ProductGrid(state: StoreState) -> View {
    let cat = state.selected_category.get();
    let query = state.search_query.get().to_lowercase();

    let all_products = state.products.get();
    let filtered_products: Vec<_> = all_products
        .into_iter()
        .filter(|p| {
            let matches_cat = cat == "ALL" || p.category == cat;
            let matches_query = query.is_empty() || p.name.to_lowercase().contains(&query);
            matches_cat && matches_query
        })
        .collect();

    rsx! {
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 py-8">
            {filtered_products.into_iter().map(|product| {
                let p_clone = product.clone();
                let state_clone = state.clone();

                rsx! {
                    <div class="group flex flex-col rounded-2xl bg-neutral-900/60 border border-neutral-800 overflow-hidden hover:border-neutral-700 transition-all duration-300">
                        <div class="h-72 w-full bg-neutral-950/60 relative flex items-center justify-center p-6 overflow-hidden">
                            {if let Some(badge) = product.badge {
                                rsx! {
                                    <span class="absolute top-4 left-4 px-2.5 py-1 rounded-md bg-indigo-600 text-[10px] font-mono font-bold text-white tracking-wider">
                                        {badge}
                                    </span>
                                }
                            } else {
                                rsx! { <></> }
                            }}

                            <div class="w-32 h-32 rounded-2xl bg-neutral-800/80 border border-neutral-700/50 flex items-center justify-center text-4xl shadow-2xl group-hover:scale-110 transition-transform duration-500">
                                {"[*]"}
                            </div>

                            <button
                                class="absolute bottom-4 right-4 h-10 px-4 rounded-full bg-white text-black font-semibold text-xs shadow-xl opacity-0 group-hover:opacity-100 translate-y-2 group-hover:translate-y-0 transition-all duration-200"
                                onclick={move |_| {
                                    state_clone.add_to_cart(p_clone.clone(), p_clone.colors.first().copied().unwrap_or("#000000"));
                                }}
                            >
                                {"+ Quick Add"}
                            </button>
                        </div>

                        <div class="p-6 flex flex-col justify-between flex-1">
                            <div>
                                <div class="flex items-center justify-between text-xs text-neutral-400 mb-2">
                                    <span>{product.category}</span>
                                    <span class="flex items-center gap-1 text-amber-400 font-mono">
                                        {"*"} {format!("{:.1}", product.rating)}
                                        <span class="text-neutral-500 font-sans">{format!("({})", product.review_count)}</span>
                                    </span>
                                </div>

                                <h3 class="text-base font-bold text-white group-hover:text-indigo-400 transition-colors">
                                    {product.name}
                                </h3>
                            </div>

                            <div class="mt-6 flex items-baseline justify-between">
                                <div class="flex items-baseline gap-2">
                                    <span class="text-xl font-bold font-mono text-white">
                                        {format!("${:.2}", product.price)}
                                    </span>
                                    {if let Some(orig) = product.original_price {
                                        rsx! {
                                            <span class="text-xs font-mono line-through text-neutral-500">
                                                {format!("${:.2}", orig)}
                                            </span>
                                        }
                                    } else {
                                        rsx! { <></> }
                                    }}
                                </div>

                                <div class="flex items-center gap-1.5">
                                    {product.colors.iter().map(|&color_hex| rsx! {
                                        <span
                                            class="w-3.5 h-3.5 rounded-full ring-1 ring-neutral-700"
                                            style={format!("background-color: {};", color_hex)}
                                        ></span>
                                    })}
                                </div>
                            </div>
                        </div>
                    </div>
                }
            })}
        </div>
    }
}
