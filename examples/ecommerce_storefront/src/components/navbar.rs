//! Top Navigation Bar for Apex Athletics Storefront.

use crate::state::StoreState;
use strata::prelude::*;

#[component]
pub fn Navbar(state: StoreState) -> View {
    let cart_count = state.cart_count();

    rsx! {
        <nav class="h-20 border-b border-neutral-800 bg-neutral-950/90 backdrop-blur-md px-8 flex items-center justify-between sticky top-0 z-40">
            <div class="flex items-center gap-8">
                <a class="flex items-center gap-2 cursor-pointer">
                    <span class="text-2xl font-black tracking-tighter text-white">{"APEX"}</span>
                    <span class="text-xs font-mono font-bold px-1.5 py-0.5 rounded bg-indigo-600 text-white">{"PRO"}</span>
                </a>

                <div class="hidden md:flex items-center gap-6 text-sm font-medium text-neutral-300">
                    <a class="hover:text-white transition-colors cursor-pointer">{"Men"}</a>
                    <a class="hover:text-white transition-colors cursor-pointer">{"Women"}</a>
                    <a class="hover:text-white transition-colors cursor-pointer">{"Footwear"}</a>
                    <a class="hover:text-white transition-colors cursor-pointer">{"Gear & Labs"}</a>
                </div>
            </div>

            <div class="flex items-center gap-4">
                <div class="relative w-64">
                    <Input
                        placeholder="Search products..."
                        value={Some(state.search_query.clone())}
                    />
                </div>

                <Button
                    variant={Some(ButtonVariant::Secondary)}
                    size={Some(ButtonSize::Sm)}
                    class={Some("rounded-full px-4 border border-neutral-800")}
                    onclick={move |_| state.is_cart_open.set(true)}
                >
                    <Icons::ShoppingBag class={Some("w-4 h-4 mr-1")} />
                    <span>{"Cart"}</span>
                    {if cart_count > 0 {
                        rsx! {
                            <Badge variant={Some(BadgeVariant::Default)} class={Some("ml-1 bg-indigo-600 text-white font-bold")}>
                                {cart_count.to_string()}
                            </Badge>
                        }
                    } else {
                        rsx! { <></> }
                    }}
                </Button>
            </div>
        </nav>
    }
}
