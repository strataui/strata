//! Declarative Immediate-Mode `<Canvas2D>` Component for Strata UI.

#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::Arc;
use strata_dom::{Attribute, ClassList, NodeBlueprint, Tag, View};
use strata_macro::component;
pub use strata_render::canvas2d::{Canvas2DContext, CanvasCommand, CanvasPathVerb};

/// Declarative Immediate-Mode 2D Vector Canvas Component.
///
/// Executes an immediate-mode drawing closure `on_draw` via `Canvas2DContext`
/// with subpixel anti-aliasing and affine transforms.
#[component]
pub fn Canvas2D(
    width: Option<f32>,
    height: Option<f32>,
    class: Option<&'static str>,
    on_draw: Option<Rc<dyn Fn(&mut Canvas2DContext)>>,
) -> View {
    let w = width.unwrap_or(400.0);
    let h = height.unwrap_or(300.0);
    let custom = class.unwrap_or("");

    let mut ctx = Canvas2DContext::new(w, h);
    if let Some(draw_fn) = &on_draw {
        draw_fn(&mut ctx);
    }
    let commands: Arc<Vec<CanvasCommand>> = Arc::new(ctx.finish());
    let payload: Arc<dyn std::any::Any + Send + Sync> = commands;

    let mut attrs = Vec::with_capacity(4);
    attrs.push(Attribute::property("width", w.to_string()));
    attrs.push(Attribute::property("height", h.to_string()));
    let mut style_str = String::new();
    if let Some(ew) = width {
        style_str.push_str(&format!("width: {}px; ", ew));
    }
    if let Some(eh) = height {
        style_str.push_str(&format!("height: {}px; ", eh));
    }
    if !style_str.is_empty() {
        attrs.push(Attribute::property("style", style_str.trim()));
    }
    attrs.push(Attribute::scene_payload_arc(payload));

    let class_str = if custom.is_empty() {
        "relative overflow-hidden bg-neutral-950 rounded-xl border border-neutral-800/80".to_string()
    } else {
        format!(
            "relative overflow-hidden bg-neutral-950 rounded-xl border border-neutral-800/80 {}",
            custom
        )
    };

    View::new(NodeBlueprint::element(
        Tag::Div,
        ClassList::from(vec![class_str]),
        attrs,
        Vec::new(),
    ))
}
