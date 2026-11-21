//! React-style [`ErrorBoundary`] catching render panics in child subtrees and displaying
//! fallback UI with recovery (`reset` callback or `reset_key`).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use crate::reactive::{use_ref, use_signal, with_component_key, ComponentScopeGuard};
use crate::{Attribute, ClassList, NodeBlueprint, Tag, View};

/// Context passed to an [`ErrorBoundary`] fallback renderer when a child panics.
#[derive(Clone)]
pub struct ErrorBoundaryInfo {
    /// Human-readable message extracted from the caught panic payload.
    pub message: String,
    /// Resets the latched error state and marks the reactive graph dirty so the next render
    /// retries the child subtree.
    pub reset: Rc<dyn Fn()>,
}

impl std::fmt::Debug for ErrorBoundaryInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ErrorBoundaryInfo")
            .field("message", &self.message)
            .finish()
    }
}

fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown render panic".to_string()
    }
}

fn default_fallback_view(info: &ErrorBoundaryInfo) -> View {
    View::new(NodeBlueprint::element(
        Tag::Div,
        ClassList::from("p-4 rounded-lg border border-rose-500/40 bg-rose-950/40 text-rose-200"),
        vec![Attribute::property("role", "alert")],
        vec![NodeBlueprint::text(format!(
            "Render error: {}",
            info.message
        ))],
    ))
}

/// React-style error boundary component catching render panics in its child subtree.
///
/// Use either declaratively in `rsx!`:
/// ```ignore
/// rsx! {
///     <ErrorBoundary
///         render={move || rsx! { <RiskyWidget /> }}
///         fallback={move |err| rsx! { <div class="text-rose-400">{err.message}</div> }}
///     />
/// }
/// ```
/// or functionally via [`ErrorBoundary::catch`].
#[allow(non_snake_case)]
pub struct ErrorBoundary;

impl ErrorBoundary {
    /// Creates a builder for `<ErrorBoundary ... />` in `rsx!`.
    pub fn builder() -> ErrorBoundaryBuilder {
        ErrorBoundaryBuilder::default()
    }

    /// Evaluates `render()` inside a panic-catching boundary, rendering `fallback(info)` if
    /// `render()` panics.
    pub fn catch(
        render: impl FnOnce() -> View,
        fallback: impl FnOnce(ErrorBoundaryInfo) -> View,
    ) -> View {
        let _scope = ComponentScopeGuard::enter("strata_dom::ErrorBoundary");
        let latched_error = use_signal::<Option<String>>(None);
        let reset_sig = latched_error.clone();
        let reset: Rc<dyn Fn()> = Rc::new(move || {
            reset_sig.set(None);
        });

        if let Some(msg) = latched_error.get() {
            return fallback(ErrorBoundaryInfo {
                message: msg,
                reset,
            });
        }

        match catch_unwind(AssertUnwindSafe(render)) {
            Ok(view) => view,
            Err(payload) => {
                let message = panic_payload_to_string(payload);
                latched_error.set(Some(message.clone()));
                fallback(ErrorBoundaryInfo { message, reset })
            }
        }
    }
}

type RenderFn = Box<dyn FnOnce() -> View>;
type FallbackFn = Box<dyn FnOnce(ErrorBoundaryInfo) -> View>;
type OnErrorFn = Box<dyn FnOnce(&str)>;

/// Companion builder backing `<ErrorBoundary ... />` in `rsx!`.
#[derive(Default)]
pub struct ErrorBoundaryBuilder {
    render: Option<RenderFn>,
    fallback: Option<FallbackFn>,
    on_error: Option<OnErrorFn>,
    reset_key: Option<String>,
    key: Option<String>,
    children: Option<Vec<NodeBlueprint>>,
}

impl ErrorBoundaryBuilder {
    /// Sets the lazy render closure evaluated inside the panic-catching boundary.
    pub fn render<F: FnOnce() -> View + 'static>(mut self, f: F) -> Self {
        self.render = Some(Box::new(f));
        self
    }

    /// Typed closure overload targeted automatically by `rsx!` inline closures.
    pub fn render_fn<F: FnOnce() -> View + 'static>(self, f: F) -> Self {
        self.render(f)
    }

    /// Sets the fallback UI renderer invoked with [`ErrorBoundaryInfo`] when a panic occurs.
    pub fn fallback<F: FnOnce(ErrorBoundaryInfo) -> View + 'static>(mut self, f: F) -> Self {
        self.fallback = Some(Box::new(f));
        self
    }

    /// Typed closure overload targeted automatically by `rsx!` inline closures so `|err|`
    /// infers `ErrorBoundaryInfo` without explicit type annotations.
    pub fn fallback_fn<F: FnOnce(ErrorBoundaryInfo) -> View + 'static>(self, f: F) -> Self {
        self.fallback(f)
    }

    /// Optional error observer invoked with the panic message when a render panic is caught.
    pub fn on_error<F: FnOnce(&str) + 'static>(mut self, f: F) -> Self {
        self.on_error = Some(Box::new(f));
        self
    }

    /// Typed closure overload targeted automatically by `rsx!` inline closures.
    pub fn on_error_fn<F: FnOnce(&str) + 'static>(self, f: F) -> Self {
        self.on_error(f)
    }

    /// Resets any latched error automatically whenever `reset_key` changes across renders.
    pub fn reset_key(mut self, key: impl std::fmt::Display) -> Self {
        self.reset_key = Some(key.to_string());
        self
    }

    /// Explicit reconciliation key distinguishing sibling `<ErrorBoundary>` instances.
    pub fn key(mut self, key: impl std::fmt::Display) -> Self {
        self.key = Some(key.to_string());
        self
    }

    /// Static children fallback when no `render` closure was provided.
    pub fn children(mut self, children: Vec<NodeBlueprint>) -> Self {
        self.children = Some(children);
        self
    }

    /// Merges non-empty fields from `other` (powers JSX `{..props}` spread).
    pub fn __spread(mut self, other: Self) -> Self {
        if other.render.is_some() {
            self.render = other.render;
        }
        if other.fallback.is_some() {
            self.fallback = other.fallback;
        }
        if other.on_error.is_some() {
            self.on_error = other.on_error;
        }
        if other.reset_key.is_some() {
            self.reset_key = other.reset_key;
        }
        if other.key.is_some() {
            self.key = other.key;
        }
        if other.children.is_some() {
            self.children = other.children;
        }
        self
    }

    /// Evaluates the boundary and returns either the child [`View`] or the fallback [`View`].
    pub fn build(self) -> View {
        let key = self.key;
        let run = move || {
            let _scope = ComponentScopeGuard::enter("strata_dom::ErrorBoundary");
            let latched_error = use_signal::<Option<String>>(None);
            let prev_reset_key = use_ref::<Option<String>>(self.reset_key.clone());

            if *prev_reset_key.borrow() != self.reset_key {
                prev_reset_key.set(self.reset_key.clone());
                if latched_error.untracked_get().is_some() {
                    latched_error.set(None);
                }
            }

            let reset_sig = latched_error.clone();
            let reset: Rc<dyn Fn()> = Rc::new(move || {
                reset_sig.set(None);
            });

            if let Some(message) = latched_error.get() {
                let info = ErrorBoundaryInfo { message, reset };
                return match self.fallback {
                    Some(fb) => fb(info),
                    None => default_fallback_view(&info),
                };
            }

            let render_fn: RenderFn = match self.render {
                Some(f) => f,
                None => {
                    let children = self.children.unwrap_or_default();
                    return View::new(NodeBlueprint::fragment(children));
                }
            };

            match catch_unwind(AssertUnwindSafe(render_fn)) {
                Ok(view) => view,
                Err(payload) => {
                    let message = panic_payload_to_string(payload);
                    if let Some(on_error) = self.on_error {
                        on_error(&message);
                    }
                    latched_error.set(Some(message.clone()));
                    let info = ErrorBoundaryInfo { message, reset };
                    match self.fallback {
                        Some(fb) => fb(info),
                        None => default_fallback_view(&info),
                    }
                }
            }
        };

        if let Some(k) = key {
            with_component_key(k, run)
        } else {
            run()
        }
    }
}
