//! Declarative Immediate-Mode `<Canvas2D>` Component for Strata UI.

#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::Arc;
use strata_dom::{
    Attribute, ClassList, Event, EventHandlerMarker, EventType, IntoChildren, IntoClassList,
    IntoOptionalString, NodeBlueprint, Tag, View,
};
pub use strata_render::canvas2d::{
    Canvas2DContext, Canvas2DPaintCallback, CanvasCommand, CanvasLineCap, CanvasLineJoin,
    CanvasPathVerb, CanvasTextAlign, CanvasTextBaseline, CanvasTextMetrics, IntoRoundRectRadii,
};

/// Ergonomic conversion into CSS class tokens for `<Canvas2D class={...} />`.
pub trait IntoCanvasClass {
    fn into_canvas_class_tokens(self) -> Vec<String>;
}

impl<T: IntoClassList> IntoCanvasClass for T {
    fn into_canvas_class_tokens(self) -> Vec<String> {
        self.into_class_list().iter().map(str::to_string).collect()
    }
}

/// Marker conversion for `on_draw={...}` supporting `Rc<dyn Fn(&mut Canvas2DContext)>`,
/// `Option<Rc<...>>`, and inline closures.
pub trait IntoCanvasDrawProp<Marker = ()> {
    fn into_canvas_draw_prop(self) -> Option<Rc<dyn Fn(&mut Canvas2DContext)>>;
}

impl IntoCanvasDrawProp<()> for Option<Rc<dyn Fn(&mut Canvas2DContext)>> {
    fn into_canvas_draw_prop(self) -> Option<Rc<dyn Fn(&mut Canvas2DContext)>> {
        self
    }
}

impl IntoCanvasDrawProp<()> for Rc<dyn Fn(&mut Canvas2DContext)> {
    fn into_canvas_draw_prop(self) -> Option<Rc<dyn Fn(&mut Canvas2DContext)>> {
        Some(self)
    }
}

impl<F: Fn(&mut Canvas2DContext) + 'static> IntoCanvasDrawProp<((),)> for F {
    fn into_canvas_draw_prop(self) -> Option<Rc<dyn Fn(&mut Canvas2DContext)>> {
        Some(Rc::new(self))
    }
}

/// Marker conversion for post-layout `on_paint={|ctx, width, height| ...}` callbacks.
pub trait IntoCanvasPaintProp<Marker = ()> {
    fn into_canvas_paint_prop(self) -> Option<Rc<dyn Fn(&mut Canvas2DContext, f32, f32)>>;
}

impl IntoCanvasPaintProp<()> for Option<Rc<dyn Fn(&mut Canvas2DContext, f32, f32)>> {
    fn into_canvas_paint_prop(self) -> Option<Rc<dyn Fn(&mut Canvas2DContext, f32, f32)>> {
        self
    }
}

impl IntoCanvasPaintProp<()> for Rc<dyn Fn(&mut Canvas2DContext, f32, f32)> {
    fn into_canvas_paint_prop(self) -> Option<Rc<dyn Fn(&mut Canvas2DContext, f32, f32)>> {
        Some(self)
    }
}

impl<F: Fn(&mut Canvas2DContext, f32, f32) + 'static> IntoCanvasPaintProp<((),)> for F {
    fn into_canvas_paint_prop(self) -> Option<Rc<dyn Fn(&mut Canvas2DContext, f32, f32)>> {
        Some(Rc::new(self))
    }
}

/// Declarative Immediate-Mode 2D Vector Canvas Component.
///
/// Supports either fixed-size eager drawing (`width`/`height` + `on_draw`) or responsive
/// container sizing (`responsive: true` or omitted `width`/`height` with `class="w-full h-full"` /
/// flex growth) paired with post-layout `on_paint(|ctx, w, h| ...)`.
#[derive(Default)]
pub struct Canvas2D {}

impl Canvas2D {
    pub fn builder() -> Canvas2DBuilder {
        Canvas2DBuilder::default()
    }
}

/// Builder powering `<Canvas2D ... />` in `rsx!`.
#[derive(Default)]
pub struct Canvas2DBuilder {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub responsive: bool,
    pub class_tokens: Vec<String>,
    pub style: Option<String>,
    pub id: Option<String>,
    pub title: Option<String>,
    pub __strata_key: Option<String>,
    pub on_draw: Option<Rc<dyn Fn(&mut Canvas2DContext)>>,
    pub on_paint: Option<Rc<dyn Fn(&mut Canvas2DContext, f32, f32)>>,
    pub events: Vec<Attribute>,
    pub children: Option<Vec<NodeBlueprint>>,
}

impl Canvas2DBuilder {
    pub fn width(mut self, width: impl Into<Option<f32>>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Option<f32>>) -> Self {
        self.height = height.into();
        self
    }

    /// When `true`, omits inline pixel `width`/`height` styles and attributes so CSS/flex classes
    /// (`w-full h-full`, `flex-1`, split panes) control the resolved viewport size.
    pub fn responsive(mut self, responsive: bool) -> Self {
        self.responsive = responsive;
        self
    }

    pub fn class(mut self, class: impl IntoCanvasClass) -> Self {
        self.class_tokens = class.into_canvas_class_tokens();
        self
    }

    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn title(mut self, title: impl IntoOptionalString) -> Self {
        self.title = title.into_optional_string();
        self
    }

    pub fn key(mut self, key: impl std::fmt::Display) -> Self {
        self.__strata_key = Some(key.to_string());
        self
    }

    pub fn children(mut self, children: impl IntoChildren<Vec<NodeBlueprint>>) -> Self {
        self.children = Some(children.into_children());
        self
    }

    pub fn on_draw<M>(mut self, cb: impl IntoCanvasDrawProp<M>) -> Self {
        self.on_draw = cb.into_canvas_draw_prop();
        self
    }

    pub fn on_draw_fn(mut self, cb: impl Fn(&mut Canvas2DContext) + 'static) -> Self {
        self.on_draw = Some(Rc::new(cb));
        self
    }

    /// Registers a post-layout paint hook invoked during display-list compilation with the
    /// resolved `LayoutNode` dimensions `(width, height)`.
    pub fn on_paint<M>(mut self, cb: impl IntoCanvasPaintProp<M>) -> Self {
        self.on_paint = cb.into_canvas_paint_prop();
        self
    }

    pub fn on_paint_fn(mut self, cb: impl Fn(&mut Canvas2DContext, f32, f32) + 'static) -> Self {
        self.on_paint = Some(Rc::new(cb));
        self
    }

    fn push_event<Args>(
        mut self,
        ty: EventType,
        handler: impl EventHandlerMarker<Args> + 'static,
    ) -> Self {
        self.events.push(Attribute::event_listener(ty, handler));
        self
    }

    pub fn onclick<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::Click, handler)
    }

    pub fn onclick_fn(self, handler: impl Fn() + 'static) -> Self {
        self.push_event(EventType::Click, handler)
    }

    pub fn ondblclick<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::DblClick, handler)
    }

    pub fn ondblclick_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::DblClick, handler)
    }

    pub fn oncontextmenu<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::ContextMenu, handler)
    }

    pub fn oncontextmenu_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::ContextMenu, handler)
    }

    pub fn onmousedown<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::MouseDown, handler)
    }

    pub fn onmousedown_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::MouseDown, handler)
    }

    pub fn onmousemove<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::MouseMove, handler)
    }

    pub fn onmousemove_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::MouseMove, handler)
    }

    pub fn onmouseup<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::MouseUp, handler)
    }

    pub fn onmouseup_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::MouseUp, handler)
    }

    pub fn onmouseenter<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::MouseEnter, handler)
    }

    pub fn onmouseenter_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::MouseEnter, handler)
    }

    pub fn onmouseleave<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::MouseLeave, handler)
    }

    pub fn onmouseleave_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::MouseLeave, handler)
    }

    pub fn onwheel<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::Wheel, handler)
    }

    pub fn onwheel_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::Wheel, handler)
    }

    pub fn onpointerdown<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::PointerDown, handler)
    }

    pub fn onpointerdown_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::PointerDown, handler)
    }

    pub fn onpointermove<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::PointerMove, handler)
    }

    pub fn onpointermove_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::PointerMove, handler)
    }

    pub fn onpointerup<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::PointerUp, handler)
    }

    pub fn onpointerup_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::PointerUp, handler)
    }

    pub fn onresize<Args>(self, handler: impl EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(EventType::Resize, handler)
    }

    pub fn onresize_fn(self, handler: impl Fn(Event) + 'static) -> Self {
        self.push_event(EventType::Resize, handler)
    }

    pub fn build(self) -> View {
        let render = move || {
            let _scope = strata_dom::ComponentScopeGuard::enter(concat!(
                module_path!(),
                "::Canvas2D"
            ));
            let is_responsive = self.responsive || (self.width.is_none() && self.height.is_none());

            let mut attrs = Vec::with_capacity(7 + self.events.len());
            attrs.push(Attribute::property("data-strata-canvas2d", "true"));
            if let Some(ref id) = self.id {
                attrs.push(Attribute::property("id", id.clone()));
            }
            if let Some(ref title) = self.title {
                attrs.push(Attribute::property("title", title.clone()));
            }

            let mut style_str = String::new();
            if !is_responsive {
                if let Some(ew) = self.width {
                    attrs.push(Attribute::property("width", ew.to_string()));
                    style_str.push_str(&format!("width: {}px; ", ew));
                }
                if let Some(eh) = self.height {
                    attrs.push(Attribute::property("height", eh.to_string()));
                    style_str.push_str(&format!("height: {}px; ", eh));
                }
            }
            if let Some(ref extra_style) = self.style {
                if !extra_style.trim().is_empty() {
                    style_str.push_str(extra_style.trim());
                }
            }
            if !style_str.trim().is_empty() {
                attrs.push(Attribute::property("style", style_str.trim()));
            }

            if let Some(paint_fn) = self.on_paint {
                let payload: Arc<dyn std::any::Any + Send + Sync> =
                    Canvas2DPaintCallback::new(paint_fn);
                attrs.push(Attribute::scene_payload_arc(payload));
            } else {
                let w = self.width.unwrap_or(400.0);
                let h = self.height.unwrap_or(300.0);
                let mut ctx = Canvas2DContext::new(w, h);
                if let Some(draw_fn) = &self.on_draw {
                    draw_fn(&mut ctx);
                }
                let commands: Arc<Vec<CanvasCommand>> = Arc::new(ctx.finish());
                let payload: Arc<dyn std::any::Any + Send + Sync> = commands;
                attrs.push(Attribute::scene_payload_arc(payload));
            }
            attrs.extend(self.events);

            let mut class_vec: Vec<String> = [
                "relative",
                "overflow-hidden",
                "bg-neutral-950",
                "rounded-xl",
                "border",
                "border-neutral-800/80",
            ]
            .into_iter()
            .map(str::to_string)
            .collect();
            class_vec.extend(self.class_tokens);

            View::new(NodeBlueprint::element(
                Tag::Div,
                ClassList::from(class_vec),
                attrs,
                self.children.unwrap_or_default(),
            ))
        };

        if let Some(k) = self.__strata_key {
            strata_dom::with_component_key(k, render)
        } else {
            render()
        }
    }
}

/// Declarative Immediate-Mode 2D Vector Canvas Component function entrypoint.
///
/// Preserves positional calls `Canvas2D(width, height, class, on_draw)`; for responsive
/// post-layout sizing (`responsive`, `on_paint`, dynamic `ClassList`/`String` classes), use
/// `<Canvas2D ... />` or [`Canvas2D::builder`].
pub fn Canvas2D(
    width: Option<f32>,
    height: Option<f32>,
    class: Option<&'static str>,
    on_draw: Option<Rc<dyn Fn(&mut Canvas2DContext)>>,
) -> View {
    Canvas2D::builder()
        .width(width)
        .height(height)
        .class(class.map(str::to_string))
        .on_draw(on_draw)
        .build()
}
