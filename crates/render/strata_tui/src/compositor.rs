//! Automated RSX Component and LayoutNode to Terminal Layout Pipeline.
//!
//! Provides `TuiCompositor`, which traverses `LayoutNode` geometries, quantizes
//! floating-point positions and dimensions into discrete terminal cells using
//! Largest Remainder Quantization (Hare-Niemeyer Algorithm), and renders
//! backgrounds, Unicode borders, text, and colors to `TerminalScreen`.

use crate::borders::{draw_box, BorderJunctionStyle};
use crate::quantization::largest_remainder_quantization;
use crate::screen::TerminalScreen;
use crate::types::{AttrBits, Color24};
use strata_dom::{NodeBlueprint, View};
use strata_layout::tree::LayoutNode;
use strata_style::types::{ComputedStyle, Display, FlexDirection, FontStyle};

fn tui_style() -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.font_size = 1.0;
    style.line_height = 1.0;
    style
}

/// Lowers an RSX `NodeBlueprint` into a `LayoutNode` tree.
pub fn blueprint_to_layout_node(bp: &NodeBlueprint) -> LayoutNode {
    match bp {
        NodeBlueprint::Element { children, .. } => {
            let style = tui_style();
            let child_nodes: Vec<LayoutNode> = children
                .iter()
                .filter_map(|c| {
                    if matches!(c, NodeBlueprint::Empty) {
                        None
                    } else {
                        Some(blueprint_to_layout_node(c))
                    }
                })
                .collect();
            LayoutNode::with_children(style, child_nodes)
                .with_entity_id(bp.entity_id().unwrap_or(strata_primitives::EntityId::NONE))
        }
        NodeBlueprint::Text(s) => {
            let mut node = LayoutNode::new(tui_style());
            node.text = Some(s.clone());
            node
        }
        NodeBlueprint::DynamicText(f) => {
            let mut node = LayoutNode::new(tui_style());
            node.text = Some(f());
            node
        }
        NodeBlueprint::Fragment(children) => {
            let style = tui_style();
            let child_nodes: Vec<LayoutNode> = children
                .iter()
                .filter_map(|c| {
                    if matches!(c, NodeBlueprint::Empty) {
                        None
                    } else {
                        Some(blueprint_to_layout_node(c))
                    }
                })
                .collect();
            LayoutNode::with_children(style, child_nodes)
        }
        NodeBlueprint::Empty => LayoutNode::new(tui_style()),
    }
}

/// Terminal User Interface Compositor.
///
/// Bridges the Strata Layout engine and RSX component model with discrete character cell
/// grids. Uses `largest_remainder_quantization` to guarantee zero-gap and zero-overflow
/// discrete cell allocation.
#[derive(Debug, Clone)]
pub struct TuiCompositor {
    cols: u16,
    rows: u16,
}

impl TuiCompositor {
    /// Creates a new TUI compositor targeting a terminal grid of `cols` by `rows`.
    pub fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }

    /// Column count of the target terminal.
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// Row count of the target terminal.
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Resizes the target terminal dimensions.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
    }

    /// Renders a `LayoutNode` tree onto the specified `TerminalScreen`.
    /// Renders a `LayoutNode` tree onto the specified `TerminalScreen`.
    pub fn render(&self, root: &LayoutNode, screen: &mut TerminalScreen) {
        let root_x = root.geometry.border_box.x;
        let root_y = root.geometry.border_box.y;
        self.render_node(root, screen, root_x, root_y, 0, 0, self.cols, self.rows);
    }

    /// Computes layout for a `LayoutNode` tree if uncomputed and renders it to a fresh `TerminalScreen`.
    pub fn render_to_screen(&self, root: &mut LayoutNode) -> TerminalScreen {
        if root.geometry.border_box.width <= 0.0 || root.geometry.border_box.height <= 0.0 {
            root.compute_layout(self.cols as f32, self.rows as f32);
        }
        let mut screen = TerminalScreen::new(self.cols, self.rows);
        self.render(root, &mut screen);
        screen
    }

    /// Compiles an RSX `View` component to layout and renders it to a `TerminalScreen`.
    pub fn render_view_to_screen(&self, view: &View) -> TerminalScreen {
        let mut root = blueprint_to_layout_node(&view.root);
        root.compute_layout(self.cols as f32, self.rows as f32);
        let mut screen = TerminalScreen::new(self.cols, self.rows);
        self.render(&root, &mut screen);
        screen
    }

    /// Compiles an RSX `View` component directly to an ANSI terminal escape sequence.
    pub fn render_view_to_ansi(&self, view: &View) -> String {
        let mut screen = self.render_view_to_screen(view);
        screen.flush()
    }

    fn render_node(
        &self,
        node: &LayoutNode,
        screen: &mut TerminalScreen,
        root_x: f32,
        root_y: f32,
        forced_col: u16,
        forced_row: u16,
        forced_w: u16,
        forced_h: u16,
    ) {
        let bbox = node.geometry.border_box;
        let text_len = node
            .text
            .as_ref()
            .map(|t| t.chars().count() as u16)
            .unwrap_or(0);
        let text_lines = node
            .text
            .as_ref()
            .map(|t| t.lines().count().max(1) as u16)
            .unwrap_or(0);

        let col = if forced_w > 0 && forced_col > 0 {
            forced_col
        } else {
            (bbox.x - root_x).max(0.0).round() as u16
        };
        let row = if forced_h > 0 && forced_row > 0 {
            forced_row
        } else {
            (bbox.y - root_y).max(0.0).round() as u16
        };

        let width = if forced_w > 0 {
            forced_w
        } else if text_len > 0 {
            text_len.min(self.cols.saturating_sub(col))
        } else if bbox.width > 0.0 {
            (bbox.width.round() as u16).min(self.cols.saturating_sub(col))
        } else {
            self.cols.saturating_sub(col)
        };

        let height = if forced_h > 0 {
            forced_h
        } else if text_lines > 0 {
            text_lines.min(self.rows.saturating_sub(row))
        } else if bbox.height > 0.0 {
            (bbox.height.round() as u16).min(self.rows.saturating_sub(row))
        } else {
            self.rows.saturating_sub(row)
        };

        if width == 0 || height == 0 || col >= self.cols || row >= self.rows {
            return;
        }

        let z_index = node.style.z_index.unwrap_or(0) as i16;

        // 1. Background fill
        if let Some(bg_color) = node.style.background_color {
            if bg_color.a > 0 {
                let bg24 = Color24::from_rgba(bg_color);
                for r in row..(row + height).min(self.rows) {
                    for c in col..(col + width).min(self.cols) {
                        screen.put_char(c, r, ' ', Color24::Default, bg24, AttrBits::NONE, z_index);
                    }
                }
            }
        }

        // 2. Borders
        let has_border = node.style.border_width.top > 0.0
            || node.style.border_width.left > 0.0
            || node.style.border_width.bottom > 0.0
            || node.style.border_width.right > 0.0;

        if has_border && width >= 2 && height >= 2 {
            let border_style = if node.style.border_radius.top_left > 0.0 {
                BorderJunctionStyle::Rounded
            } else if node.style.border_width.top >= 2.0 {
                BorderJunctionStyle::Heavy
            } else {
                BorderJunctionStyle::Solid
            };

            let border_fg = node
                .style
                .border_color
                .map(Color24::from_rgba)
                .unwrap_or_else(|| Color24::from_rgba(node.style.color));
            let border_bg = node
                .style
                .background_color
                .map(Color24::from_rgba)
                .unwrap_or(Color24::Default);

            draw_box(
                screen,
                col,
                row,
                width,
                height,
                border_style,
                border_fg,
                border_bg,
                z_index,
            );
        }

        // 3. Text content
        let inset = if has_border { 1u16 } else { 0u16 };
        let pad_l = node.style.padding.left.max(0.0).round() as u16;
        let pad_t = node.style.padding.top.max(0.0).round() as u16;
        let content_col = col + inset + pad_l;
        let content_row = row + inset + pad_t;
        let content_w = width.saturating_sub(inset * 2 + pad_l).max(text_len);
        let content_h = height.saturating_sub(inset * 2 + pad_t).max(text_lines);

        if let Some(ref text) = node.text {
            let fg = Color24::from_rgba(node.style.color);
            let bg = node
                .style
                .background_color
                .map(Color24::from_rgba)
                .unwrap_or(Color24::Default);

            let mut attr = AttrBits::NONE;
            if (node.style.font_weight as u16) >= 600 {
                attr |= AttrBits::BOLD;
            }
            if node.style.font_style == FontStyle::Italic {
                attr |= AttrBits::ITALIC;
            }

            let mut curr_c = content_col;
            let mut curr_r = content_row;

            for ch in text.chars() {
                if ch == '\n' {
                    curr_r += 1;
                    curr_c = content_col;
                    if curr_r >= content_row + content_h || curr_r >= self.rows {
                        break;
                    }
                    continue;
                }
                if curr_c >= content_col + content_w || curr_c >= self.cols {
                    curr_r += 1;
                    curr_c = content_col;
                    if curr_r >= content_row + content_h || curr_r >= self.rows {
                        break;
                    }
                }
                screen.put_char(curr_c, curr_r, ch, fg, bg, attr, z_index);
                curr_c += 1;
            }
        }

        // 4. Children with Largest Remainder Quantization
        if !node.children.is_empty() {
            let is_horizontal = (node.style.display == Display::Flex
                && node.style.flex_direction == FlexDirection::Row)
                || node.style.display == Display::Grid;

            if is_horizontal && width > 0 {
                let shares: Vec<f32> = node
                    .children
                    .iter()
                    .map(|c| c.geometry.border_box.width.max(0.0))
                    .collect();
                let quantized_widths = largest_remainder_quantization(&shares, width);

                let mut child_offset_x = 0u16;
                for (child, &qw) in node.children.iter().zip(quantized_widths.iter()) {
                    if qw == 0 {
                        continue;
                    }
                    let child_c = col + child_offset_x;
                    self.render_node(child, screen, root_x, root_y, child_c, row, qw, height);
                    child_offset_x += qw;
                }
            } else {
                for child in &node.children {
                    self.render_node(child, screen, root_x, root_y, 0, 0, 0, 0);
                }
            }
        }
    }
}
