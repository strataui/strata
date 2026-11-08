//! Reactive Router Context and Navigation Hooks.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use strata_dom::reactive::Signal;

use crate::history::{Location, MemoryHistory};
use crate::matcher::{match_path, RouteMatch};

thread_local! {
    static CURRENT_ROUTER: RefCell<Option<RouterContext>> = const { RefCell::new(None) };
}

/// Navigator helper for programmatic navigation.
#[derive(Clone)]
pub struct Navigator {
    context: RouterContext,
}

impl Navigator {
    pub fn push(&self, path: &str) {
        self.context.navigate(path, false);
    }

    pub fn replace(&self, path: &str) {
        self.context.navigate(path, true);
    }

    pub fn back(&self) -> bool {
        self.context.back()
    }

    pub fn forward(&self) -> bool {
        self.context.forward()
    }
}

/// Core reactive router coordinator.
#[derive(Clone)]
pub struct RouterContext {
    location: Signal<Location>,
    params: Signal<HashMap<String, String>>,
    history: Rc<RefCell<MemoryHistory>>,
    routes: Rc<RefCell<Vec<String>>>,
}

impl RouterContext {
    pub fn new(initial_path: &str) -> Self {
        let history = MemoryHistory::new(initial_path);
        let current_loc = history.current();
        Self {
            location: Signal::new(current_loc),
            params: Signal::new(HashMap::new()),
            history: Rc::new(RefCell::new(history)),
            routes: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn register_route(&self, pattern: &str) {
        {
            let mut routes = self.routes.borrow_mut();
            if !routes.contains(&pattern.to_string()) {
                routes.push(pattern.to_string());
            }
        }
        self.recompute_match(&self.location.get());
    }

    pub fn location(&self) -> Signal<Location> {
        self.location.clone()
    }

    pub fn params(&self) -> Signal<HashMap<String, String>> {
        self.params.clone()
    }

    pub fn navigate(&self, path: &str, replace: bool) {
        let (new_loc, _action) = if replace {
            self.history.borrow_mut().replace(path)
        } else {
            self.history.borrow_mut().push(path)
        };

        self.recompute_match(&new_loc);
        self.location.set(new_loc);
    }

    pub fn back(&self) -> bool {
        if let Some((prev_loc, _)) = self.history.borrow_mut().back() {
            self.recompute_match(&prev_loc);
            self.location.set(prev_loc);
            true
        } else {
            false
        }
    }

    pub fn forward(&self) -> bool {
        if let Some((fwd_loc, _)) = self.history.borrow_mut().forward() {
            self.recompute_match(&fwd_loc);
            self.location.set(fwd_loc);
            true
        } else {
            false
        }
    }

    fn recompute_match(&self, loc: &Location) {
        let routes = self.routes.borrow();
        let mut best_match: Option<RouteMatch> = None;

        for pattern in routes.iter() {
            if let Some(m) = match_path(pattern, &loc.pathname) {
                if best_match.as_ref().is_none_or(|best| m.score > best.score) {
                    best_match = Some(m);
                }
            }
        }

        if let Some(m) = best_match {
            self.params.set(m.params);
        } else {
            self.params.set(HashMap::new());
        }
    }
}

/// Initialize the thread-local router context.
pub fn init_router(initial_path: &str) -> RouterContext {
    let ctx = RouterContext::new(initial_path);
    CURRENT_ROUTER.with(|r| {
        *r.borrow_mut() = Some(ctx.clone());
    });
    ctx
}

/// Hook to get the active router context.
pub fn use_router() -> RouterContext {
    CURRENT_ROUTER.with(|r| {
        if let Some(ref ctx) = *r.borrow() {
            ctx.clone()
        } else {
            let default_ctx = RouterContext::new("/");
            *r.borrow_mut() = Some(default_ctx.clone());
            default_ctx
        }
    })
}

/// Hook to get the current reactive location signal.
pub fn use_location() -> Signal<Location> {
    use_router().location()
}

/// Hook to get the current route parameters signal.
pub fn use_params() -> Signal<HashMap<String, String>> {
    use_router().params()
}

/// Hook to get a navigator instance.
pub fn use_navigate() -> Navigator {
    Navigator {
        context: use_router(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_context_and_hooks() {
        let router = init_router("/");
        router.register_route("/dashboard");
        router.register_route("/users/:id");

        let loc = use_location();
        let params = use_params();
        let nav = use_navigate();

        assert_eq!(loc.get().pathname, "/");
        assert!(params.get().is_empty());

        nav.push("/users/99");
        assert_eq!(loc.get().pathname, "/users/99");
        assert_eq!(params.get().get("id").unwrap(), "99");

        nav.push("/dashboard");
        assert_eq!(loc.get().pathname, "/dashboard");

        nav.back();
        assert_eq!(loc.get().pathname, "/users/99");
        assert_eq!(params.get().get("id").unwrap(), "99");
    }
}
