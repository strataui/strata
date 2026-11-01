//! Component Inspector, Prop Introspection, and CSS Box Model Visualizer.

use strata_dom::NodeId;
use strata_primitives::geom::{EdgeInsets, FixedRect};

/// Source code location where a component or element was defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
}

impl Default for SourceLocation {
    fn default() -> Self {
        Self {
            file: "unknown.rs",
            line: 0,
            column: 0,
        }
    }
}

/// Dynamic property value representations for live inspection and mutation.
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Color([f32; 4]),
}

/// Description of an inspectable component prop.
#[derive(Debug, Clone, PartialEq)]
pub struct PropDescriptor {
    pub name: String,
    pub value: PropValue,
    pub is_readonly: bool,
}

/// Trait implemented by inspectable components for prop reflection.
pub trait ReflectProp {
    fn reflect_props(&self) -> Vec<PropDescriptor>;
    fn apply_prop_mutation(&mut self, prop_name: &str, new_value: PropValue) -> Result<(), String>;
}

/// Component registration and boundary metadata for the DevTools tree.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentMeta {
    pub id: usize,
    pub name: String,
    pub source: SourceLocation,
    pub root_node: NodeId,
    pub child_components: Vec<usize>,
    pub render_count: u32,
    pub last_render_us: u32,
}

/// CSS Box Model geometry representation with 1/64th pixel precision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxModelMetrics {
    pub content_x: f32,
    pub content_y: f32,
    pub content_w: f32,
    pub content_h: f32,
    pub padding: EdgeInsets,
    pub border: EdgeInsets,
    pub margin: EdgeInsets,
}

impl BoxModelMetrics {
    /// Constructs box model metrics from content dimensions and insets.
    pub fn new(
        content_x: f32,
        content_y: f32,
        content_w: f32,
        content_h: f32,
        padding: EdgeInsets,
        border: EdgeInsets,
        margin: EdgeInsets,
    ) -> Self {
        Self {
            content_x,
            content_y,
            content_w: content_w.max(0.0),
            content_h: content_h.max(0.0),
            padding,
            border,
            margin,
        }
    }

    /// Content box rectangle.
    pub fn content_box(&self) -> FixedRect {
        FixedRect::from_f32(self.content_x, self.content_y, self.content_w, self.content_h)
    }

    /// Padding box rectangle (content + padding).
    pub fn padding_box(&self) -> FixedRect {
        let x = self.content_x - self.padding.left;
        let y = self.content_y - self.padding.top;
        let w = self.content_w + self.padding.left + self.padding.right;
        let h = self.content_h + self.padding.top + self.padding.bottom;
        FixedRect::from_f32(x, y, w, h)
    }

    /// Border box rectangle (content + padding + border).
    pub fn border_box(&self) -> FixedRect {
        let x = self.content_x - self.padding.left - self.border.left;
        let y = self.content_y - self.padding.top - self.border.top;
        let w = self.content_w + self.padding.left + self.padding.right + self.border.left + self.border.right;
        let h = self.content_h + self.padding.top + self.padding.bottom + self.border.top + self.border.bottom;
        FixedRect::from_f32(x, y, w, h)
    }

    /// Margin box rectangle (content + padding + border + margin).
    pub fn margin_box(&self) -> FixedRect {
        let x = self.content_x - self.padding.left - self.border.left - self.margin.left;
        let y = self.content_y - self.padding.top - self.border.top - self.margin.top;
        let w = self.content_w
            + self.padding.left
            + self.padding.right
            + self.border.left
            + self.border.right
            + self.margin.left
            + self.margin.right;
        let h = self.content_h
            + self.padding.top
            + self.padding.bottom
            + self.border.top
            + self.border.bottom
            + self.margin.top
            + self.margin.bottom;
        FixedRect::from_f32(x, y, w, h)
    }
}

/// Standard Chrome / W3C DevTools Box Model overlay colors.
pub struct BoxModelColors;

impl BoxModelColors {
    /// Margin overlay color: Orange (#F99B52, alpha: 0.35)
    pub const MARGIN_RGBA: [f32; 4] = [0.976, 0.608, 0.322, 0.35];

    /// Border overlay color: Yellow (#F5D678, alpha: 0.45)
    pub const BORDER_RGBA: [f32; 4] = [0.961, 0.839, 0.471, 0.45];

    /// Padding overlay color: Green (#C3DEAB, alpha: 0.40)
    pub const PADDING_RGBA: [f32; 4] = [0.765, 0.871, 0.671, 0.40];

    /// Content overlay color: Blue (#8DB8D5, alpha: 0.45)
    pub const CONTENT_RGBA: [f32; 4] = [0.553, 0.722, 0.835, 0.45];
}

/// Live Tailwind utility class editor applying real-time class modifications.
pub struct LiveTailwindEditor;

impl LiveTailwindEditor {
    /// Applies a mutation string (space-separated classes) to an existing class list.
    /// Classes prefixed with `!` or `-` are removed; all others are added if not present.
    pub fn apply_class_mutation(classes: &mut Vec<String>, mutation_str: &str) -> bool {
        let mut modified = false;

        for token in mutation_str.split_whitespace() {
            if let Some(to_remove) = token.strip_prefix('-').or_else(|| token.strip_prefix('!')) {
                if let Some(pos) = classes.iter().position(|c| c == to_remove) {
                    classes.remove(pos);
                    modified = true;
                }
            } else if !classes.contains(&token.to_string()) {
                classes.push(token.to_string());
                modified = true;
            }
        }

        modified
    }
}
