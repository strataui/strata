//! Headline README Hero component demonstration.
//!
//! Validates that the README quickstart example compiles and executes cleanly.

use strata::prelude::*;

#[component]
pub fn MetricCard(title: String, value: usize, trend: f32) -> View {
    let (_is_hovered, set_hovered) = use_state(|| false);
    let set_hovered_leave = set_hovered.clone();

    rsx! {
        <div
            class="p-6 rounded-2xl bg-neutral-900 border border-neutral-800 shadow-xl transition-all duration-300 hover:scale-[1.02] cursor-pointer"
            onmouseenter={move |_| set_hovered.set(true)}
            onmouseleave={move |_| set_hovered_leave.set(false)}
        >
            <div class="flex items-center justify-between text-neutral-400 text-sm font-medium">
                <span>{title}</span>
                <span class={if trend >= 0.0 { "text-emerald-400" } else { "text-rose-400" }}>
                    {format!("{trend:+.1}%")}
                </span>
            </div>
            <div class="mt-4 text-3xl font-bold tracking-tight text-white font-mono">
                {value.to_string()}
            </div>
        </div>
    }
}

fn main() {
    println!("Rendering Strata README Hero MetricCard...");
    let card = MetricCard("Active Users".to_string(), 12480, 14.2);
    let blueprint = card.into_node_blueprint();
    match blueprint {
        NodeBlueprint::Element { tag, classes, .. } => {
            println!("Hero Component successfully rendered: tag={:?}, classes={:?}", tag, classes);
        }
        _ => panic!("Expected Element blueprint"),
    }
}
