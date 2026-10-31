//! Slide-Over Shopping Cart Drawer for Apex Athletics Storefront.

use strata::prelude::*;
use crate::state::StoreState;

#[component]
pub fn CartDrawer(state: StoreState) -> View {
    if !state.is_cart_open.get() {
        return rsx! { <></> };
    }

    let items = state.cart.get();
    let subtotal = state.subtotal();
    let discount_pct = state.discount_percent.get();
    let discount_amount = subtotal * (discount_pct as f64 / 100.0);
    let final_total = (subtotal - discount_amount).max(0.0);

    let close_state = state.clone();
    let promo_input_state = state.clone();
    let promo_apply_state = state.clone();

    rsx! {
        <div class="fixed inset-0 z-50 flex justify-end bg-black/60 backdrop-blur-sm">
            <div class="w-full max-w-md h-full bg-neutral-900 border-l border-neutral-800 shadow-2xl flex flex-col justify-between p-6">
                <div class="flex items-center justify-between border-b border-neutral-800 pb-4">
                    <div class="flex items-center gap-2">
                        <h2 class="text-lg font-bold text-white">{"Your Bag"}</h2>
                        <span class="px-2 py-0.5 rounded-full bg-neutral-800 text-xs font-mono text-neutral-300">
                            {format!("{} items", state.cart_count())}
                        </span>
                    </div>
                    <button
                        class="text-neutral-400 hover:text-white text-lg font-bold p-1"
                        onclick={move |_| close_state.is_cart_open.set(false)}
                    >
                        {"x"}
                    </button>
                </div>

                <div class="flex-1 overflow-y-auto py-4 divide-y divide-neutral-800/60">
                    {if items.is_empty() {
                        rsx! {
                            <div class="flex flex-col items-center justify-center h-64 text-center">
                                <span class="text-4xl mb-3">{"[Bag]"}</span>
                                <p class="text-sm text-neutral-400">{"Your shopping bag is currently empty."}</p>
                            </div>
                        }
                    } else {
                        rsx! {
                            <div class="flex flex-col gap-4">
                                {items.into_iter().enumerate().map(|(idx, item)| {
                                    let s_minus = state.clone();
                                    let s_plus = state.clone();

                                    rsx! {
                                        <div class="flex items-center gap-4 py-3">
                                            <div class="w-16 h-16 rounded-xl bg-neutral-950 border border-neutral-800 flex items-center justify-center text-xl">
                                                {"[*]"}
                                            </div>
                                            <div class="flex-1">
                                                <h4 class="text-xs font-semibold text-white">{item.product.name}</h4>
                                                <span class="text-xs font-mono text-neutral-400">{format!("${:.2}", item.product.price)}</span>
                                            </div>
                                            <div class="flex items-center gap-2">
                                                <button
                                                    class="w-6 h-6 rounded bg-neutral-800 hover:bg-neutral-700 text-white text-xs"
                                                    onclick={move |_| {
                                                        s_minus.cart.update(|c| {
                                                            if let Some(it) = c.get_mut(idx) {
                                                                if it.quantity > 1 {
                                                                    it.quantity -= 1;
                                                                } else {
                                                                    c.remove(idx);
                                                                }
                                                            }
                                                        });
                                                    }}
                                                >
                                                    {"-"}
                                                </button>
                                                <span class="text-xs font-mono font-bold text-white">{item.quantity.to_string()}</span>
                                                <button
                                                    class="w-6 h-6 rounded bg-neutral-800 hover:bg-neutral-700 text-white text-xs"
                                                    onclick={move |_| {
                                                        s_plus.cart.update(|c| {
                                                            if let Some(it) = c.get_mut(idx) {
                                                                it.quantity += 1;
                                                            }
                                                        });
                                                    }}
                                                >
                                                    {"+"}
                                                </button>
                                            </div>
                                        </div>
                                    }
                                })}
                            </div>
                        }
                    }}
                </div>

                <div class="border-t border-neutral-800 pt-4 flex flex-col gap-4">
                    <div class="flex items-center gap-2">
                        <input
                            type="text"
                            placeholder="Promo Code (Try APEX20)"
                            class="flex-1 h-9 bg-neutral-950 border border-neutral-800 rounded-lg px-3 text-xs text-white uppercase font-mono placeholder-neutral-500 focus:outline-none"
                            oninput={move |val: String| promo_input_state.promo_code.set(val)}
                        />
                        <button
                            class="h-9 px-3 bg-neutral-800 hover:bg-neutral-700 text-white text-xs font-medium rounded-lg"
                            onclick={move |_| {
                                if promo_apply_state.promo_code.get().trim() == "APEX20" {
                                    promo_apply_state.discount_percent.set(20.0);
                                }
                            }}
                        >
                            {"Apply"}
                        </button>
                    </div>

                    <div class="flex flex-col gap-1 text-xs text-neutral-400 font-mono">
                        <div class="flex justify-between">
                            <span>{"Subtotal"}</span>
                            <span>{format!("${:.2}", subtotal)}</span>
                        </div>
                        {if discount_pct > 0.0 {
                            rsx! {
                                <div class="flex justify-between text-emerald-400">
                                    <span>{format!("Discount ({}%)", discount_pct)}</span>
                                    <span>{format!("-${:.2}", discount_amount)}</span>
                                </div>
                            }
                        } else {
                            rsx! { <></> }
                        }}
                        <div class="flex justify-between">
                            <span>{"Shipping"}</span>
                            <span class="text-emerald-400">{"FREE"}</span>
                        </div>
                        <div class="flex justify-between text-base font-bold text-white pt-2 border-t border-neutral-800">
                            <span>{"Estimated Total"}</span>
                            <span>{format!("${:.2}", final_total)}</span>
                        </div>
                    </div>

                    <button
                        class="w-full py-3.5 rounded-xl bg-indigo-600 hover:bg-indigo-500 active:scale-95 text-white font-bold text-sm shadow-xl shadow-indigo-600/30 transition-all"
                        onclick={move |_| println!("[Pay] Initializing secure checkout...")}
                    >
                        {"Checkout Now"}
                    </button>
                </div>
            </div>
        </div>
    }
}
