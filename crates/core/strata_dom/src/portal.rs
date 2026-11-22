//! Top-layer `<Portal>` component and `create_portal` / `create_anchored_portal` primitives.
//!
//! Renders dropdowns, context menus, popovers, tooltips, and modals in the root Top Layer
//! (analogous to React's `createPortal(children, document.body)` combined with Floating UI /
//! CSS Anchor Positioning), escaping any nested `overflow-hidden`, `overflow-y-auto`,
//! `transform`, `filter`, `backdrop-filter`, or atomic `z-index` stacking contexts while
//! preserving full reactive signal & event closure routing in place.

use crate::{
    Attribute, ClassList, Event, EventHandlerMarker, EventType, IntoNodeBlueprint, NodeBlueprint,
    Tag, View,
};

/// Placement of an anchored top-layer `<Portal>` relative to its trigger element's live box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnchorPlacement {
    Bottom,
    BottomStart,
    #[default]
    BottomEnd,
    Top,
    TopStart,
    TopEnd,
    Left,
    LeftStart,
    LeftEnd,
    Right,
    RightStart,
    RightEnd,
}

impl AnchorPlacement {
    /// Parses a placement string such as `"bottom-end"`, `"top-end"`, `"bottom-start"`, etc.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "bottom" | "block-end" => Some(Self::Bottom),
            "bottom-start" | "bottom_start" | "bottom-left" => Some(Self::BottomStart),
            "bottom-end" | "bottom_end" | "bottom-right" => Some(Self::BottomEnd),
            "top" | "block-start" => Some(Self::Top),
            "top-start" | "top_start" | "top-left" => Some(Self::TopStart),
            "top-end" | "top_end" | "top-right" => Some(Self::TopEnd),
            "left" | "inline-start" => Some(Self::Left),
            "left-start" | "left_start" | "left-top" => Some(Self::LeftStart),
            "left-end" | "left_end" | "left-bottom" => Some(Self::LeftEnd),
            "right" | "inline-end" => Some(Self::Right),
            "right-start" | "right_start" | "right-top" => Some(Self::RightStart),
            "right-end" | "right_end" | "right-bottom" => Some(Self::RightEnd),
            _ => None,
        }
    }

    /// Canonical kebab-case string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::BottomStart => "bottom-start",
            Self::BottomEnd => "bottom-end",
            Self::Top => "top",
            Self::TopStart => "top-start",
            Self::TopEnd => "top-end",
            Self::Left => "left",
            Self::LeftStart => "left-start",
            Self::LeftEnd => "left-end",
            Self::Right => "right",
            Self::RightStart => "right-start",
            Self::RightEnd => "right-end",
        }
    }

    /// Opposite primary-axis placement preserving cross-axis alignment (`bottom-end <-> top-end`).
    pub const fn opposite(self) -> Self {
        match self {
            Self::Bottom => Self::Top,
            Self::BottomStart => Self::TopStart,
            Self::BottomEnd => Self::TopEnd,
            Self::Top => Self::Bottom,
            Self::TopStart => Self::BottomStart,
            Self::TopEnd => Self::BottomEnd,
            Self::Left => Self::Right,
            Self::LeftStart => Self::RightStart,
            Self::LeftEnd => Self::RightEnd,
            Self::Right => Self::Left,
            Self::RightStart => Self::LeftStart,
            Self::RightEnd => Self::LeftEnd,
        }
    }
}

impl From<&str> for AnchorPlacement {
    fn from(s: &str) -> Self {
        Self::parse(s).unwrap_or_default()
    }
}

impl From<String> for AnchorPlacement {
    fn from(s: String) -> Self {
        Self::parse(&s).unwrap_or_default()
    }
}

impl std::fmt::Display for AnchorPlacement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Conversion helper accepting string or optional string values on `<Portal anchor=...>` props.
pub trait IntoPortalOptString {
    fn into_portal_opt_string(self) -> Option<String>;
}

impl IntoPortalOptString for &str {
    fn into_portal_opt_string(self) -> Option<String> {
        let trimmed = self.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

impl IntoPortalOptString for String {
    fn into_portal_opt_string(self) -> Option<String> {
        self.as_str().into_portal_opt_string()
    }
}

impl IntoPortalOptString for &String {
    fn into_portal_opt_string(self) -> Option<String> {
        self.as_str().into_portal_opt_string()
    }
}

impl IntoPortalOptString for Option<&str> {
    fn into_portal_opt_string(self) -> Option<String> {
        self.and_then(IntoPortalOptString::into_portal_opt_string)
    }
}

impl IntoPortalOptString for Option<String> {
    fn into_portal_opt_string(self) -> Option<String> {
        self.and_then(IntoPortalOptString::into_portal_opt_string)
    }
}

/// Conversion helper accepting `f32`, `i32`, `&str`, or `String` on `<Portal offset=...>`.
pub trait IntoPortalF32 {
    fn into_portal_f32(self) -> f32;
}

impl IntoPortalF32 for f32 {
    fn into_portal_f32(self) -> f32 {
        self
    }
}

impl IntoPortalF32 for f64 {
    fn into_portal_f32(self) -> f32 {
        self as f32
    }
}

impl IntoPortalF32 for i32 {
    fn into_portal_f32(self) -> f32 {
        self as f32
    }
}

impl IntoPortalF32 for &str {
    fn into_portal_f32(self) -> f32 {
        self.trim()
            .trim_end_matches("px")
            .trim()
            .parse::<f32>()
            .unwrap_or(0.0)
    }
}

impl IntoPortalF32 for String {
    fn into_portal_f32(self) -> f32 {
        self.as_str().into_portal_f32()
    }
}

/// Conversion helper accepting `bool` or `"true"` / `"false"` strings on `<Portal flip=...>`.
pub trait IntoPortalBool {
    fn into_portal_bool(self) -> bool;
}

impl IntoPortalBool for bool {
    fn into_portal_bool(self) -> bool {
        self
    }
}

impl IntoPortalBool for &str {
    fn into_portal_bool(self) -> bool {
        !matches!(
            self.trim().to_ascii_lowercase().as_str(),
            "false" | "0" | "no" | "off"
        )
    }
}

/// Root Top-Layer `<Portal>` RSX component.
///
/// # Examples
/// ```ignore
/// rsx! {
///     <button id="card-1-trigger" onclick={toggle}>"..."</button>
///     if open.get() {
///         <Portal anchor="card-1-trigger" placement="bottom-end" offset={6.0}>
///             <div class="w-48 rounded-lg bg-zinc-900 border border-zinc-800 p-1 shadow-xl">
///                 <button onclick={uninstall}>"Uninstall"</button>
///             </div>
///         </Portal>
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Portal();

impl Portal {
    pub fn builder() -> PortalBuilder {
        PortalBuilder::default()
    }
}

impl IntoNodeBlueprint for Portal {
    fn into_node_blueprint(self) -> NodeBlueprint {
        PortalBuilder::default().build().root
    }
}

/// Companion RSX builder for [`Portal`].
#[derive(Default)]
pub struct PortalBuilder {
    id: Option<String>,
    anchor: Option<String>,
    placement: Option<AnchorPlacement>,
    offset: Option<f32>,
    flip: Option<bool>,
    shift: Option<bool>,
    padding: Option<f32>,
    match_width: Option<bool>,
    min_width_from_anchor: Option<bool>,
    class: Option<ClassList>,
    style: Option<String>,
    onclick: Option<Attribute>,
    children: Vec<NodeBlueprint>,
}

impl PortalBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn anchor(mut self, anchor: impl IntoPortalOptString) -> Self {
        self.anchor = anchor.into_portal_opt_string();
        self
    }

    pub fn anchor_id(mut self, anchor: impl IntoPortalOptString) -> Self {
        self.anchor = anchor.into_portal_opt_string();
        self
    }

    pub fn position_anchor(mut self, anchor: impl IntoPortalOptString) -> Self {
        self.anchor = anchor.into_portal_opt_string();
        self
    }

    pub fn placement(mut self, placement: impl Into<AnchorPlacement>) -> Self {
        self.placement = Some(placement.into());
        self
    }

    pub fn offset(mut self, offset: impl IntoPortalF32) -> Self {
        self.offset = Some(offset.into_portal_f32());
        self
    }

    pub fn flip(mut self, flip: impl IntoPortalBool) -> Self {
        self.flip = Some(flip.into_portal_bool());
        self
    }

    pub fn shift(mut self, shift: impl IntoPortalBool) -> Self {
        self.shift = Some(shift.into_portal_bool());
        self
    }

    pub fn padding(mut self, padding: impl IntoPortalF32) -> Self {
        self.padding = Some(padding.into_portal_f32());
        self
    }

    pub fn viewport_padding(mut self, padding: impl IntoPortalF32) -> Self {
        self.padding = Some(padding.into_portal_f32());
        self
    }

    pub fn match_width(mut self, match_width: impl IntoPortalBool) -> Self {
        self.match_width = Some(match_width.into_portal_bool());
        self
    }

    pub fn min_width_from_anchor(mut self, min_width_from_anchor: impl IntoPortalBool) -> Self {
        self.min_width_from_anchor = Some(min_width_from_anchor.into_portal_bool());
        self
    }

    pub fn class(mut self, class: impl Into<ClassList>) -> Self {
        self.class = Some(class.into());
        self
    }

    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    pub fn onclick<Args, F: EventHandlerMarker<Args> + 'static>(mut self, handler: F) -> Self {
        self.onclick = Some(Attribute::event_listener(EventType::Click, handler));
        self
    }

    pub fn children(mut self, children: Vec<NodeBlueprint>) -> Self {
        self.children = children;
        self
    }

    pub fn build(self) -> View {
        let mut attrs = Vec::with_capacity(8);
        attrs.push(Attribute::property("data-portal", "true"));
        if let Some(id) = self.id {
            attrs.push(Attribute::property("id", id));
        }
        if let Some(anchor) = self.anchor {
            attrs.push(Attribute::property("data-portal-anchor", anchor));
        }
        if let Some(placement) = self.placement {
            attrs.push(Attribute::property(
                "data-portal-placement",
                placement.as_str(),
            ));
        }
        if let Some(offset) = self.offset {
            attrs.push(Attribute::property(
                "data-portal-offset",
                offset.to_string(),
            ));
        }
        if let Some(flip) = self.flip {
            attrs.push(Attribute::property(
                "data-portal-flip",
                if flip { "true" } else { "false" },
            ));
        }
        if let Some(shift) = self.shift {
            attrs.push(Attribute::property(
                "data-portal-shift",
                if shift { "true" } else { "false" },
            ));
        }
        if let Some(padding) = self.padding {
            attrs.push(Attribute::property(
                "data-portal-padding",
                padding.to_string(),
            ));
        }
        if let Some(match_width) = self.match_width {
            attrs.push(Attribute::property(
                "data-portal-match-width",
                if match_width { "true" } else { "false" },
            ));
        }
        if let Some(min_width_from_anchor) = self.min_width_from_anchor {
            attrs.push(Attribute::property(
                "data-portal-min-width",
                if min_width_from_anchor { "true" } else { "false" },
            ));
        }
        if let Some(style) = self.style {
            attrs.push(Attribute::property("style", style));
        }
        if let Some(onclick) = self.onclick {
            attrs.push(onclick);
        }

        let classes = self.class.unwrap_or_default();
        View::new(NodeBlueprint::element(
            Tag::Custom("portal"),
            classes,
            attrs,
            self.children,
        ))
    }
}

/// Functional React-style `createPortal(children)` helper rendering `children` in the root
/// Top Layer above all document stacking contexts and outside any ancestor overflow clips.
pub fn create_portal(children: impl IntoNodeBlueprint) -> View {
    Portal::builder()
        .children(vec![children.into_node_blueprint()])
        .build()
}

/// Functional `create_anchored_portal` helper rendering `children` in the root Top Layer
/// anchored to `anchor_id`'s live post-scroll bounding box with automatic viewport collision
/// flipping (`bottom-end <-> top-end`, `left <-> right`) and viewport padding clamp.
pub fn create_anchored_portal(
    anchor_id: impl Into<String>,
    placement: impl Into<AnchorPlacement>,
    offset: f32,
    children: impl IntoNodeBlueprint,
) -> View {
    Portal::builder()
        .anchor(anchor_id.into())
        .placement(placement)
        .offset(offset)
        .children(vec![children.into_node_blueprint()])
        .build()
}

/// Convenience helper for wrapping a click-outside backdrop + anchored menu in the Top Layer.
pub fn create_dropdown_portal<Args, F>(
    anchor_id: impl Into<String>,
    placement: impl Into<AnchorPlacement>,
    offset: f32,
    on_dismiss: F,
    menu: impl IntoNodeBlueprint,
) -> View
where
    F: EventHandlerMarker<Args> + 'static,
{
    let backdrop = NodeBlueprint::element(
        Tag::Custom("portal"),
        "fixed inset-0",
        vec![
            Attribute::property("data-portal", "true"),
            Attribute::property("data-portal-backdrop", "true"),
            Attribute::event_listener(EventType::Click, move |ev: Event| {
                on_dismiss.call_handler(ev);
            }),
        ],
        Vec::new(),
    );
    let anchored_menu = Portal::builder()
        .anchor(anchor_id.into())
        .placement(placement)
        .offset(offset)
        .min_width_from_anchor(true)
        .children(vec![menu.into_node_blueprint()])
        .build()
        .root;
    View::new(NodeBlueprint::Fragment(vec![backdrop, anchored_menu]))
}
