//! Declarative Router Components (<Router>, <Route>, <Link>, <Outlet>, <Navigate>).

use std::rc::Rc;
use strata_dom::*;
use strata_macro::component;
use strata_macro::rsx;

use crate::context::{use_location, use_navigate, use_router};
use crate::matcher::match_path;

/// Declarative Link component for client-side navigation.
#[component]
pub fn Link(
    to: String,
    class: Option<String>,
    children: Option<Vec<NodeBlueprint>>,
) -> View {
    let to_path = to.clone();
    let class_str = class.unwrap_or_default();
    let nav = use_navigate();

    rsx! {
        <a
            class={class_str}
            href={to}
            onclick={move |_| {
                nav.push(&to_path);
            }}
        >
            {children.unwrap_or_default()}
        </a>
    }
}

/// Declarative client-side redirect.
#[component]
pub fn Navigate(to: String, replace: Option<bool>) -> View {
    let nav = use_navigate();
    let replace_val = replace.unwrap_or(false);
    if replace_val {
        nav.replace(&to);
    } else {
        nav.push(&to);
    }

    rsx! {
        <span style="display: none;" />
    }
}

/// A registered route branch pairing a path pattern with its component builder.
#[derive(Clone)]
pub struct RouteEntry {
    pub path: String,
    pub builder: Rc<dyn Fn() -> View>,
}

impl RouteEntry {
    pub fn new(path: impl Into<String>, builder: impl Fn() -> View + 'static) -> Self {
        Self {
            path: path.into(),
            builder: Rc::new(builder),
        }
    }
}

/// Declarative Switch/Routes container resolving the first matching route.
pub fn render_routes(routes: &[RouteEntry]) -> View {
    let router = use_router();
    for r in routes {
        router.register_route(&r.path);
    }

    let loc = use_location().get();

    for r in routes {
        if match_path(&r.path, &loc.pathname).is_some() {
            return (r.builder)();
        }
    }

    // Fallback 404 view
    rsx! {
        <div class="p-8 text-center text-neutral-400 font-mono">
            <h2 class="text-xl font-bold text-neutral-200">{"404 - Not Found"}</h2>
            <p class="mt-2 text-sm">{format!("No route matching '{}'", loc.pathname)}</p>
        </div>
    }
}
