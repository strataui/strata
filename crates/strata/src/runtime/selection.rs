//! Text input layout synchronization, caret offset calculation, and caret rendering.

use super::*;

impl AppRuntime {
    /// Resets the caret blink cycle so the insertion caret is immediately solid and visible.
    #[inline]
    pub fn reset_caret_blink(&mut self) {
        self.caret_blink_epoch = std::time::Instant::now();
        self.last_caret_blink_state = true;
    }

    /// Polls caret blink phase and returns true if blink state toggled (requiring a redraw).
    pub fn poll_caret_blink(&mut self) -> bool {
        if self.active_focus.is_none() {
            return false;
        }
        let elapsed_ms = self.caret_blink_epoch.elapsed().as_millis();
        let is_blink_on = (elapsed_ms / 530).is_multiple_of(2);
        if is_blink_on != self.last_caret_blink_state {
            self.last_caret_blink_state = is_blink_on;
            true
        } else {
            false
        }
    }

    /// Synchronizes the active focused text input's live buffer and styling into the layout tree.
    pub fn sync_active_input_layout(&mut self) {
        let Some(ref active) = self.active_focus else {
            return;
        };
        let target_idx = active.input_index;
        let target_id = active.input_id.clone();
        let mut counter = 0;
        let text_val = active.state.text.get();

        if let Some(layout) = Self::find_input_layout_mut(
            &self.root_blueprint,
            &mut self.layout_root,
            target_idx,
            target_id.as_deref(),
            &mut counter,
        ) {
            let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
            if !text_val.is_empty() {
                layout.text = Some(text_val.clone());
                if let Some(c) = layout.style.caret_color {
                    layout.style.color = c;
                }
                let (tw, th) = strata_layout::text::measure_text_bounds_with_family(
                    &text_val,
                    layout.style.font_size,
                    layout.style.line_height,
                    None,
                    is_mono,
                    layout.style.font_family,
                );
                layout.geometry.content_box.width = tw.max(layout.geometry.content_box.width);
                layout.geometry.content_box.height = th.max(layout.geometry.content_box.height);
            } else {
                let mut ph = None;
                let mut bp_counter = 0;
                if let Some(bp) = Self::find_input_blueprint(
                    &self.root_blueprint,
                    target_idx,
                    target_id.as_deref(),
                    &mut bp_counter,
                ) {
                    if let NodeBlueprint::Element { attributes, .. } = bp {
                        for attr in attributes {
                            if let strata_dom::Attribute::Property { key, value } = attr {
                                if key == "placeholder" && !value.is_empty() {
                                    ph = Some(value.clone());
                                }
                            }
                        }
                    }
                }
                if let Some(ph_str) = ph {
                    layout.text = Some(ph_str);
                    layout.style.color = strata_style::types::Color::new(140, 145, 155, 255);
                } else {
                    layout.text = Some(String::new());
                }
            }
        }
    }

    /// Calculates the exact character index from an x offset within an input using active font metrics and character advances.
    pub fn calculate_exact_char_index_at_offset(
        text: &str,
        rel_x: f32,
        font_size: f32,
        line_height: f32,
        is_mono: bool,
        font_family: strata_primitives::AtomId,
    ) -> usize {
        if rel_x <= 0.0 || text.is_empty() {
            return 0;
        }
        let chars: Vec<char> = text.chars().collect();
        let total = chars.len();

        let (full_w, _) = strata_layout::text::measure_text_bounds_with_family(
            text,
            font_size,
            line_height,
            None,
            is_mono,
            font_family,
        );
        if rel_x >= full_w {
            return total;
        }

        let mut prev_w = 0.0f32;
        for i in 1..=total {
            let prefix: String = chars[..i].iter().collect();
            let (w, _) = strata_layout::text::measure_text_bounds_with_family(
                &prefix,
                font_size,
                line_height,
                None,
                is_mono,
                font_family,
            );
            let mid = (prev_w + w) / 2.0;
            if rel_x < mid {
                return i - 1;
            } else if rel_x <= w {
                return i;
            }
            prev_w = w;
        }
        total
    }

    /// Renders the visible insertion caret cursor and selection highlight for the currently focused text input.
    pub fn render_active_input_caret(&mut self) {
        let Some(ref active) = self.active_focus else {
            return;
        };

        let mut current_idx = 0;
        let target_idx = active.input_index;
        let target_id = active.input_id.as_deref();

        let Some(layout) = Self::find_input_layout(
            &self.root_blueprint,
            &self.layout_root,
            target_idx,
            target_id,
            &mut current_idx,
        ) else {
            return;
        };

        let content_box = layout.geometry.content_box;
        if content_box.width <= 0.0 || content_box.height <= 0.0 {
            return;
        }

        let font_size = layout.style.font_size;
        let line_height = layout.style.line_height;
        let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
        let font_family = layout.style.font_family;
        let text_color = layout.style.color;
        let mut caret_color = layout.style.caret_color.unwrap_or(text_color);
        if caret_color.a == 0 {
            caret_color = strata_style::types::Color::new(236, 233, 225, 255);
        }
        let text = active.state.text.get();
        let total_chars = text.chars().count();
        let cursor_idx = active.state.cursor_index.get().min(total_chars);

        // Alignment offset for text-align: center / right
        let align_offset = match layout.style.text_align {
            strata_style::types::TextAlign::Center => {
                let (tw, _) = strata_layout::text::measure_text_bounds_with_family(
                    &text,
                    font_size,
                    line_height,
                    None,
                    is_mono,
                    font_family,
                );
                ((content_box.width - tw) / 2.0).max(0.0)
            }
            strata_style::types::TextAlign::Right => {
                let (tw, _) = strata_layout::text::measure_text_bounds_with_family(
                    &text,
                    font_size,
                    line_height,
                    None,
                    is_mono,
                    font_family,
                );
                (content_box.width - tw).max(0.0)
            }
            _ => 0.0,
        };

        let caret_w = 2.0f32;
        let caret_h = (font_size * 1.2).min(content_box.height).max(14.0);
        let caret_y = content_box.y + ((content_box.height - caret_h) / 2.0).max(0.0);

        // 1. Selection range highlight (CSS ::selection)
        if let Some((start, end)) = active.state.selection_range() {
            let start_clamped = start.min(total_chars);
            let end_clamped = end.min(total_chars);
            if start_clamped < end_clamped {
                let start_prefix: String = text.chars().take(start_clamped).collect();
                let (start_w, _) = strata_layout::text::measure_text_bounds_with_family(
                    &start_prefix,
                    font_size,
                    line_height,
                    None,
                    is_mono,
                    font_family,
                );
                let sel_slice: String = text
                    .chars()
                    .skip(start_clamped)
                    .take(end_clamped - start_clamped)
                    .collect();
                let (sel_w, _) = strata_layout::text::measure_text_bounds_with_family(
                    &sel_slice,
                    font_size,
                    line_height,
                    None,
                    is_mono,
                    font_family,
                );

                let sel_h = (font_size * 1.25).min(content_box.height);
                let sel_y = content_box.y + ((content_box.height - sel_h) / 2.0).max(0.0);
                let sel_x = content_box.x + align_offset + start_w;

                self.display_list
                    .push(strata_render::compositor::DrawCommand::DrawRect {
                        bounds: strata_layout::Rect::new(sel_x, sel_y, sel_w, sel_h),
                        color: strata_style::types::Color::rgba(59, 130, 246, 120), // Standard CSS selection blue
                    });
            }
        }

        // 2. Visible Caret Line Cursor (CSS caret-color, blinking, and insertion cursor)
        let has_sel = active.state.has_selection();
        let elapsed_ms = self.caret_blink_epoch.elapsed().as_millis();
        let is_blink_on = (elapsed_ms / 530).is_multiple_of(2);

        if !has_sel && is_blink_on {
            let prefix: String = text.chars().take(cursor_idx).collect();
            let (cursor_w, _) = strata_layout::text::measure_text_bounds_with_family(
                &prefix,
                font_size,
                line_height,
                None,
                is_mono,
                font_family,
            );
            let caret_x = (content_box.x + align_offset + cursor_w)
                .min(content_box.x + content_box.width - caret_w);

            self.display_list
                .push(strata_render::compositor::DrawCommand::DrawRect {
                    bounds: strata_layout::Rect::new(caret_x, caret_y, caret_w, caret_h),
                    color: caret_color,
                });
        }
    }

}
