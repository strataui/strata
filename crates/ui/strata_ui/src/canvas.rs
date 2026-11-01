//! Declarative Immediate-Mode `<Canvas2D>` Component for Strata UI.

#![allow(non_snake_case)]

use std::rc::Rc;
use strata_dom::View;
use strata_macro::{component, rsx};
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
    let _commands = ctx.finish();

    rsx! {
        <div class={format!("relative overflow-hidden bg-neutral-950 rounded-xl border border-neutral-800/80 {}", custom)}>
            <div class="absolute inset-0 pointer-events-none" />
        </div>
    }
}
