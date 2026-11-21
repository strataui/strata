//! Text input layout synchronization, caret offset calculation, and caret rendering.

use super::*;

impl AppRuntime {
    /// Resets the caret blink cycle so the insertion caret is immediately solid and visible.
    #[inline]
    pub fn reset_caret_blink(&mut self) {
        self.caret_blink_epoch = std::time::Instant::now();
        self.last_caret_blink_state = true;
    }

    /// Polls caret blink cycle and returns true if blink state toggled (requiring a redraw).
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

    /// Returns `true` when the `<input>` identified by `(input_index, input_id, entity_id)`
    /// currently carries `type="password"` in `self.root_blueprint`.
    pub(crate) fn input_is_password(
        &self,
        input_index: usize,
        input_id: Option<&str>,
        entity_id: Option<strata_primitives::EntityId>,
    ) -> bool {
        let mut bp_counter = 0;
        let bp = entity_id
            .and_then(|eid| self.root_blueprint.find_by_entity_id(eid))
            .or_else(|| {
                Self::find_input_blueprint(
                    &self.root_blueprint,
                    input_index,
                    input_id,
                    &mut bp_counter,
                )
            });
        matches!(
            bp,
            Some(NodeBlueprint::Element { tag: Tag::Input, attributes, .. })
                if Self::is_password_input_attrs(attributes)
        )
    }

    /// Returns `true` when `active` currently points to an `<input type="password">`.
    pub(crate) fn active_input_is_password(&self, active: &ActiveInput) -> bool {
        self.input_is_password(
            active.input_index,
            active.input_id.as_deref(),
            active.entity_id,
        )
    }

    /// Computes `(align_offset, scroll_x, cursor_x_offset)` for a single-line input so the
    /// active caret remains visible within `content_w`.
    pub(crate) fn compute_input_text_metrics(
        text: &str,
        cursor_idx: usize,
        prev_scroll_x: f32,
        content_w: f32,
        text_align: strata_style::types::TextAlign,
        font_size: f32,
        line_height: f32,
        is_mono: bool,
        font_family: strata_primitives::AtomId,
    ) -> (f32, f32, f32) {
        let total_chars = text.chars().count();
        let clamped_cursor = cursor_idx.min(total_chars);
        let (full_w, _) = strata_layout::text::measure_text_bounds_with_family(
            text,
            font_size,
            line_height,
            None,
            is_mono,
            font_family,
        );
        let prefix: String = text.chars().take(clamped_cursor).collect();
        let (cursor_w, _) = strata_layout::text::measure_text_bounds_with_family(
            &prefix,
            font_size,
            line_height,
            None,
            is_mono,
            font_family,
        );

        let caret_w = 2.0f32;
        let visible_w = (content_w - caret_w).max(0.0);
        if content_w <= 0.0 || full_w <= visible_w {
            let align_offset = match text_align {
                strata_style::types::TextAlign::Center => ((content_w - full_w) / 2.0).max(0.0),
                strata_style::types::TextAlign::Right => (content_w - full_w).max(0.0),
                _ => 0.0,
            };
            return (align_offset, 0.0, cursor_w);
        }

        let max_scroll_x = (full_w - visible_w).max(0.0);
        let mut next_scroll_x = prev_scroll_x;
        if cursor_w < next_scroll_x {
            next_scroll_x = cursor_w;
        } else if cursor_w > next_scroll_x + visible_w {
            next_scroll_x = cursor_w - visible_w;
        }
        (0.0, next_scroll_x.clamp(0.0, max_scroll_x), cursor_w)
    }

    /// Synchronizes the active focused text input's live buffer, single-line scroll offset,
    /// and styling into the layout tree.
    pub fn sync_active_input_layout(&mut self) {
        let Some(ref active) = self.active_focus else {
            return;
        };
        let target_idx = active.input_index;
        let target_id = active.input_id.clone();
        let target_entity_id = active.entity_id;
        let is_password =
            self.input_is_password(target_idx, target_id.as_deref(), target_entity_id);
        let mut counter = 0;
        let text_val = active.state.text.get();
        let cursor_idx = active.state.cursor_index.get();
        let prev_scroll_x = active.state.scroll_x.get();

        let layout_by_entity = if let Some(eid) = target_entity_id {
            self.layout_root.find_by_entity_id_mut(eid)
        } else {
            None
        };
        let layout_match = match layout_by_entity {
            Some(l) => Some(l),
            None => Self::find_input_layout_mut(
                &self.root_blueprint,
                &mut self.layout_root,
                target_idx,
                target_id.as_deref(),
                &mut counter,
            ),
        };

        let mut next_scroll_x = 0.0f32;
        if let Some(layout) = layout_match {
            let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
            if !text_val.is_empty() {
                let display_val = if is_password {
                    Self::mask_password_text(&text_val)
                } else {
                    text_val
                };
                layout.text = Some(display_val.clone());
                if let Some(c) = layout.style.caret_color {
                    layout.style.color = c;
                }
                let (tw, th) = strata_layout::text::measure_text_bounds_with_family(
                    &display_val,
                    layout.style.font_size,
                    layout.style.line_height,
                    None,
                    is_mono,
                    layout.style.font_family,
                );
                if layout.geometry.content_box.width <= 0.0 {
                    layout.geometry.content_box.width = tw;
                }
                layout.geometry.content_box.height = th.max(layout.geometry.content_box.height);
                let (_, computed_scroll_x, _) = Self::compute_input_text_metrics(
                    &display_val,
                    cursor_idx,
                    prev_scroll_x,
                    layout.geometry.content_box.width,
                    layout.style.text_align,
                    layout.style.font_size,
                    layout.style.line_height,
                    is_mono,
                    layout.style.font_family,
                );
                next_scroll_x = computed_scroll_x;
                layout.geometry.scroll_offset.x = next_scroll_x;
            } else {
                layout.geometry.scroll_offset.x = 0.0;
                let mut ph = None;
                let mut bp_counter = 0;
                let bp_match = if let Some(eid) = target_entity_id {
                    self.root_blueprint.find_by_entity_id(eid)
                } else {
                    None
                };
                let bp_match = bp_match.or_else(|| {
                    Self::find_input_blueprint(
                        &self.root_blueprint,
                        target_idx,
                        target_id.as_deref(),
                        &mut bp_counter,
                    )
                });
                if let Some(bp) = bp_match {
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

        if let Some(ref active) = self.active_focus {
            active.state.scroll_x.set(next_scroll_x);
        }
        self.update_gtk_ime_location();
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
        let is_password = self.input_is_password(target_idx, target_id, active.entity_id);

        let layout_by_entity = if let Some(eid) = active.entity_id {
            self.layout_root.find_by_entity_id(eid)
        } else {
            None
        };
        let Some(layout) = layout_by_entity.or_else(|| {
            Self::find_input_layout(
                &self.root_blueprint,
                &self.layout_root,
                target_idx,
                target_id,
                &mut current_idx,
            )
        }) else {
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
        let raw_text = active.state.text.get();
        let text = if is_password {
            Self::mask_password_text(&raw_text)
        } else {
            raw_text
        };
        let total_chars = text.chars().count();
        let cursor_idx = active.state.cursor_index.get().min(total_chars);
        let prev_scroll_x = active.state.scroll_x.get();

        let (align_offset, scroll_x, cursor_w) = Self::compute_input_text_metrics(
            &text,
            cursor_idx,
            prev_scroll_x,
            content_box.width,
            layout.style.text_align,
            font_size,
            line_height,
            is_mono,
            font_family,
        );
        active.state.scroll_x.set(scroll_x);

        let caret_w = 2.0f32;
        let caret_h = (font_size * 1.2).min(content_box.height).max(14.0);
        let caret_y = content_box.y + ((content_box.height - caret_h) / 2.0).max(0.0);

        let selection_rect = active.state.selection_range().and_then(|(start, end)| {
            let start_clamped = start.min(total_chars);
            let end_clamped = end.min(total_chars);
            if start_clamped >= end_clamped {
                return None;
            }
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
            let sel_x = content_box.x + align_offset + start_w - scroll_x;
            Some(strata_layout::Rect::new(sel_x, sel_y, sel_w, sel_h))
        });

        let has_sel = active.state.has_selection();
        let elapsed_ms = self.caret_blink_epoch.elapsed().as_millis();
        let is_blink_on = (elapsed_ms / 530).is_multiple_of(2);
        let draw_caret = !has_sel && is_blink_on;

        if selection_rect.is_none() && !draw_caret {
            return;
        }

        self.display_list
            .push(strata_render::compositor::DrawCommand::PushClip {
                bounds: content_box,
                radius: strata_style::types::BorderRadius::all(0.0),
            });

        // 1. Selection range highlight (CSS ::selection)
        if let Some(sel_bounds) = selection_rect {
            self.display_list
                .push(strata_render::compositor::DrawCommand::DrawRect {
                    bounds: sel_bounds,
                    color: strata_style::types::Color::rgba(59, 130, 246, 120), // Standard CSS selection blue
                });
        }

        // 2. Visible Caret Line Cursor (CSS caret-color, blinking, and insertion cursor)
        if draw_caret {
            let max_caret_x = content_box.x + (content_box.width - caret_w).max(0.0);
            let caret_x =
                (content_box.x + align_offset + cursor_w - scroll_x).clamp(content_box.x, max_caret_x);

            self.display_list
                .push(strata_render::compositor::DrawCommand::DrawRect {
                    bounds: strata_layout::Rect::new(caret_x, caret_y, caret_w, caret_h),
                    color: caret_color,
                });
        }

        self.display_list
            .push(strata_render::compositor::DrawCommand::PopClip);
    }

    /// Computes the exact bounding box of the active text caret if an input is currently focused.
    pub fn active_caret_rect(&self) -> Option<strata_layout::Rect> {
        let active = self.active_focus.as_ref()?;
        let mut current_idx = 0;
        let target_idx = active.input_index;
        let target_id = active.input_id.as_deref();
        let is_password = self.input_is_password(target_idx, target_id, active.entity_id);

        let layout_by_entity = if let Some(eid) = active.entity_id {
            self.layout_root.find_by_entity_id(eid)
        } else {
            None
        };
        let layout = layout_by_entity.or_else(|| {
            Self::find_input_layout(
                &self.root_blueprint,
                &self.layout_root,
                target_idx,
                target_id,
                &mut current_idx,
            )
        })?;

        let content_box = layout.geometry.content_box;
        if content_box.width <= 0.0 || content_box.height <= 0.0 {
            return None;
        }

        let font_size = layout.style.font_size;
        let line_height = layout.style.line_height;
        let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
        let font_family = layout.style.font_family;
        let raw_text = active.state.text.get();
        let text = if is_password {
            Self::mask_password_text(&raw_text)
        } else {
            raw_text
        };
        let total_chars = text.chars().count();
        let cursor_idx = active.state.cursor_index.get().min(total_chars);

        let (align_offset, scroll_x, cursor_w) = Self::compute_input_text_metrics(
            &text,
            cursor_idx,
            active.state.scroll_x.get(),
            content_box.width,
            layout.style.text_align,
            font_size,
            line_height,
            is_mono,
            font_family,
        );

        let caret_w = 2.0f32;
        let caret_h = (font_size * 1.2).min(content_box.height).max(14.0);
        let caret_y = content_box.y + ((content_box.height - caret_h) / 2.0).max(0.0);
        let max_caret_x = content_box.x + (content_box.width - caret_w).max(0.0);
        let caret_x =
            (content_box.x + align_offset + cursor_w - scroll_x).clamp(content_box.x, max_caret_x);

        Some(strata_layout::Rect::new(caret_x, caret_y, caret_w, caret_h))
    }

    /// Synchronizes the IME candidate window location with the active caret position.
    pub fn sync_ime_cursor_location<W: strata_platform::PlatformWindow>(&self, window: &mut W) {
        if let Some(rect) = self.active_caret_rect() {
            window.set_ime_cursor_location(
                rect.x.round() as i32,
                rect.y.round() as i32,
                rect.width.round() as i32,
                rect.height.round() as i32,
            );
            if let Some(ref active) = self.active_focus {
                if self.active_input_is_password(active) {
                    window.set_ime_surrounding_text("", 0);
                } else {
                    window.set_ime_surrounding_text(
                        &active.state.text.get(),
                        active.state.cursor_index.get() as i32,
                    );
                }
            }
        } else {
            window.reset_ime();
        }
    }

    /// Directly updates GTK IME context with current caret location if running on Linux GTK.
    pub fn update_gtk_ime_location(&self) {
        #[cfg(target_os = "linux")]
        {
            if let Some(rect) = self.active_caret_rect() {
                strata_platform::gtk_set_ime_cursor_location(
                    rect.x.round() as i32,
                    rect.y.round() as i32,
                    rect.width.round() as i32,
                    rect.height.round() as i32,
                );
                if let Some(ref active) = self.active_focus {
                    if self.active_input_is_password(active) {
                        strata_platform::gtk_set_ime_surrounding_text("", 0);
                    } else {
                        strata_platform::gtk_set_ime_surrounding_text(
                            &active.state.text.get(),
                            active.state.cursor_index.get() as i32,
                        );
                    }
                }
            } else {
                strata_platform::gtk_reset_ime();
            }
        }
    }
}
