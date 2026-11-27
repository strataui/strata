//! Model Context Protocol (MCP) cursor control, keyboard typing, and element inspection.

use super::*;

impl AppRuntime {
    /// Ensures that an emulated MCP cursor state is active, initializing it at center viewport if needed.
    pub fn ensure_mcp_cursor(&mut self) -> &mut strata_platform::McpCursorState {
        if self.mcp_cursor.is_none() {
            self.mcp_cursor = Some(strata_platform::McpCursorState::new(
                self.viewport_width / 2.0,
                self.viewport_height / 2.0,
            ));
        }
        self.mcp_cursor.as_mut().unwrap()
    }

    /// Emulates AI pointer movement to `(x, y)` and returns `(needs_rebuild, needs_redraw)`.
    ///
    /// Paint-only hover transitions and cursor-only motion return `(false, true)` so automation
    /// benchmarks exercise the real fast hover path instead of forcing full DOM rebuilds.
    pub fn mcp_move_pointer_outcome(&mut self, x: f32, y: f32, smooth: bool) -> (bool, bool) {
        let cursor_icon = self.resolved_cursor_at(x, y);
        let cursor = self.ensure_mcp_cursor();
        let cursor_moved = (cursor.target_x - x).abs() > 0.01
            || (cursor.target_y - y).abs() > 0.01
            || (cursor.current_x - x).abs() > 0.01
            || (cursor.current_y - y).abs() > 0.01
            || cursor.cursor_icon != cursor_icon;
        cursor.move_to(x, y, smooth);
        cursor.cursor_icon = cursor_icon;

        self.last_mouse_pos = (x, y);
        self.mark_pending_click_dragged_if_moved(x, y);
        let mut needs_rebuild = false;
        let mut needs_redraw = cursor_moved;

        let signals_before = strata_dom::reactive::has_dirty_signals();
        if self.dispatch_pointer_move(x, y) {
            let retained_only = !signals_before
                && !strata_dom::reactive::has_dirty_signals()
                && self.target_is_retained_canvas3d(x, y);
            if !retained_only {
                needs_rebuild = true;
            }
        }
        if self.update_hover(x, y) {
            if self.hover_is_paint_only() {
                needs_redraw = true;
            } else {
                needs_rebuild = true;
            }
        }
        let post_cursor_icon = self.resolved_cursor_at(x, y);
        if self.current_cursor != post_cursor_icon {
            self.current_cursor = post_cursor_icon;
            needs_redraw = true;
        }
        if let Some(ref mut c) = self.mcp_cursor {
            if c.cursor_icon != post_cursor_icon {
                c.cursor_icon = post_cursor_icon;
                needs_redraw = true;
            }
        }
        if strata_dom::reactive::has_dirty_signals() {
            needs_rebuild = true;
            needs_redraw = true;
        }
        if needs_redraw {
            self.mark_paint_dirty();
        }

        (needs_rebuild, needs_redraw)
    }

    /// Emulates AI pointer movement to `(x, y)`. Smooth interpolation is enabled by default.
    ///
    /// Automatically resolves and applies the element's CSS `cursor` style (e.g. pointer, text).
    /// Returns true only when a view rebuild or display list redraw is genuinely required.
    pub fn mcp_move_pointer(&mut self, x: f32, y: f32, smooth: bool) -> bool {
        let (rebuild, redraw) = self.mcp_move_pointer_outcome(x, y, smooth);
        rebuild || redraw
    }

    /// Emulates AI mouse button down event, returning `(needs_rebuild, needs_redraw)`.
    pub fn mcp_mouse_down_outcome(
        &mut self,
        button: strata_platform::MouseButton,
        x: Option<f32>,
        y: Option<f32>,
    ) -> (bool, bool) {
        let mut needs_rebuild = false;
        let mut needs_redraw = false;
        let (px, py) = match (x, y) {
            (Some(px), Some(py)) => {
                let (rb, rd) = self.mcp_move_pointer_outcome(px, py, false);
                needs_rebuild |= rb;
                needs_redraw |= rd;
                (px, py)
            }
            _ => {
                let c = self.ensure_mcp_cursor();
                (c.current_x, c.current_y)
            }
        };

        let cursor = self.ensure_mcp_cursor();
        let down_changed = !cursor.is_down;
        cursor.set_down(true);
        if down_changed {
            needs_redraw = true;
        }

        let btn_idx = button.to_button_index();
        let signals_before = strata_dom::reactive::has_dirty_signals();

        if button == strata_platform::MouseButton::Left {
            self.is_mouse_down = true;
            if self.check_scrollbar_thumb_press(px, py) {
                self.cancel_pending_click();
                self.mark_paint_dirty();
                return (needs_rebuild, true);
            }
            self.record_pending_click(btn_idx, px, py);
            self.begin_window_drag_candidate(px, py);
            if self.dispatch_pointer_down_with_modifiers(
                px,
                py,
                strata_platform::Modifiers::default(),
            ) {
                let retained_only = !signals_before
                    && !strata_dom::reactive::has_dirty_signals()
                    && self.target_is_retained_canvas3d(px, py);
                if !retained_only {
                    needs_rebuild = true;
                }
            }
            if self.drain_edit_commands() {
                needs_rebuild = true;
            }
            if !needs_rebuild && self.restyle_for_active_change() {
                needs_redraw = true;
            }
        } else if button == strata_platform::MouseButton::Middle {
            self.record_pending_click(btn_idx, px, py);
            if self.dispatch_pointer_down_action_with_button_and_modifiers(
                px,
                py,
                btn_idx,
                strata_platform::Modifiers::default(),
            )
            .is_handled()
            {
                let retained_only = !signals_before
                    && !strata_dom::reactive::has_dirty_signals()
                    && self.target_is_retained_canvas3d(px, py);
                if !retained_only {
                    needs_rebuild = true;
                }
            }
        } else if button == strata_platform::MouseButton::Right {
            let _ = self.dispatch_pointer_down_action_with_button_and_modifiers(
                px,
                py,
                btn_idx,
                strata_platform::Modifiers::default(),
            );
            if self.dispatch_context_menu(px, py) {
                needs_rebuild = true;
            }
            if strata_dom::reactive::has_dirty_signals() {
                needs_rebuild = true;
            }
            if self.drain_edit_commands() {
                needs_rebuild = true;
            }
        }

        if needs_redraw {
            self.mark_paint_dirty();
        }
        (needs_rebuild, needs_redraw)
    }

    /// Emulates AI mouse button release up event, returning `(needs_rebuild, needs_redraw)`.
    pub fn mcp_mouse_down(
        &mut self,
        button: strata_platform::MouseButton,
        x: Option<f32>,
        y: Option<f32>,
    ) -> bool {
        let (rebuild, redraw) = self.mcp_mouse_down_outcome(button, x, y);
        rebuild || redraw
    }

    /// Emulates AI mouse button release up event, returning `(needs_rebuild, needs_redraw)`.
    pub fn mcp_mouse_up_outcome(
        &mut self,
        button: strata_platform::MouseButton,
        x: Option<f32>,
        y: Option<f32>,
    ) -> (bool, bool) {
        let mut needs_rebuild = false;
        let mut needs_redraw = false;
        let (px, py) = match (x, y) {
            (Some(px), Some(py)) => {
                let (rb, rd) = self.mcp_move_pointer_outcome(px, py, false);
                needs_rebuild |= rb;
                needs_redraw |= rd;
                (px, py)
            }
            _ => {
                let c = self.ensure_mcp_cursor();
                (c.current_x, c.current_y)
            }
        };

        let cursor = self.ensure_mcp_cursor();
        if cursor.is_down {
            needs_redraw = true;
        }
        cursor.set_down(false);
        let btn_idx = button.to_button_index();
        let was_mouse_down = self.is_mouse_down;
        if button == strata_platform::MouseButton::Left {
            self.is_mouse_down = false;
        }

        let suppressed_by_drag = self.is_pointer_drag_suppressing_click();
        if self.scrollbar_drag.is_some() {
            self.scrollbar_drag = None;
            needs_redraw = true;
        }
        self.pending_window_drag = None;
        let signals_before = strata_dom::reactive::has_dirty_signals();
        let up_action = self.dispatch_pointer_up_action_with_button(px, py, btn_idx);
        if up_action.needs_rebuild() {
            let retained_only = !signals_before
                && !strata_dom::reactive::has_dirty_signals()
                && self.target_is_retained_canvas3d(px, py);
            if !retained_only {
                needs_rebuild = true;
            }
        } else if up_action.needs_redraw() {
            needs_redraw = true;
        }
        if button != strata_platform::MouseButton::Right
            && self.dispatch_click_on_mouse_up(btn_idx, px, py, suppressed_by_drag)
        {
            needs_rebuild = true;
        }
        if self.drain_edit_commands() {
            needs_rebuild = true;
        }
        if was_mouse_down && !needs_rebuild && self.restyle_for_active_change() {
            needs_redraw = true;
        }

        if needs_redraw {
            self.mark_paint_dirty();
        }
        (needs_rebuild, needs_redraw)
    }

    /// Emulates AI mouse button release up event.
    pub fn mcp_mouse_up(
        &mut self,
        button: strata_platform::MouseButton,
        x: Option<f32>,
        y: Option<f32>,
    ) -> bool {
        let (rebuild, redraw) = self.mcp_mouse_up_outcome(button, x, y);
        rebuild || redraw
    }

    /// Emulates AI mouse click (press down and release), returning `(needs_rebuild, needs_redraw)`.
    pub fn mcp_mouse_click_outcome(
        &mut self,
        button: strata_platform::MouseButton,
        x: Option<f32>,
        y: Option<f32>,
        count: usize,
    ) -> (bool, bool) {
        let mut needs_rebuild = false;
        let mut needs_redraw = false;
        let (px, py) = match (x, y) {
            (Some(px), Some(py)) => {
                let (rb, rd) = self.mcp_move_pointer_outcome(px, py, false);
                needs_rebuild |= rb;
                needs_redraw |= rd;
                (px, py)
            }
            _ => {
                let c = self.ensure_mcp_cursor();
                (c.current_x, c.current_y)
            }
        };

        for _ in 0..count.max(1) {
            let (d_rb, d_rd) = self.mcp_mouse_down_outcome(button, Some(px), Some(py));
            let (u_rb, u_rd) = self.mcp_mouse_up_outcome(button, Some(px), Some(py));
            needs_rebuild |= d_rb || u_rb;
            needs_redraw |= d_rd || u_rd;
        }

        (needs_rebuild, needs_redraw)
    }

    /// Emulates AI mouse click (press down and release).
    pub fn mcp_mouse_click(
        &mut self,
        button: strata_platform::MouseButton,
        x: Option<f32>,
        y: Option<f32>,
        count: usize,
    ) -> bool {
        let (rebuild, redraw) = self.mcp_mouse_click_outcome(button, x, y, count);
        rebuild || redraw
    }

    /// Emulates AI mouse wheel scroll event, returning `(needs_rebuild, needs_redraw)`.
    pub fn mcp_mouse_scroll_outcome(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        x: Option<f32>,
        y: Option<f32>,
    ) -> (bool, bool) {
        let mut needs_rebuild = false;
        let mut needs_redraw = false;
        let (px, py) = match (x, y) {
            (Some(px), Some(py)) => {
                let (rb, rd) = self.mcp_move_pointer_outcome(px, py, false);
                needs_rebuild |= rb;
                needs_redraw |= rd;
                (px, py)
            }
            _ => {
                let c = self.ensure_mcp_cursor();
                (c.current_x, c.current_y)
            }
        };

        self.ensure_mcp_cursor().touch_activity();
        let signals_before = strata_dom::reactive::has_dirty_signals();
        let (wheel_handled, scroll_changed) =
            self.dispatch_smooth_scroll_action(px, py, delta_x, delta_y);
        if wheel_handled {
            let retained_only = !signals_before
                && !strata_dom::reactive::has_dirty_signals()
                && self.target_is_retained_canvas3d(px, py);
            if !retained_only {
                needs_rebuild = true;
            }
        }
        if scroll_changed {
            needs_redraw = true;
            self.mark_paint_dirty();
        }
        (needs_rebuild, needs_redraw)
    }

    /// Emulates AI mouse wheel scroll event.
    pub fn mcp_mouse_scroll(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        x: Option<f32>,
        y: Option<f32>,
    ) -> bool {
        let (rebuild, redraw) = self.mcp_mouse_scroll_outcome(delta_x, delta_y, x, y);
        rebuild || redraw
    }

    /// Emulates AI keyboard character input, returning `(needs_rebuild, needs_redraw)`.
    pub fn mcp_keyboard_type_outcome(&mut self, text: &str) -> (bool, bool) {
        self.ensure_mcp_cursor().touch_activity();
        let mut needs_rebuild = false;
        let mut needs_redraw = false;
        for ch in text.chars() {
            let ev = strata_platform::PlatformEvent::ReceivedCharacter(ch);
            let res = self.dispatch_key_event(&ev);
            if res.needs_rebuild() {
                needs_rebuild = true;
            } else if res.needs_redraw() {
                needs_redraw = true;
            }
        }
        if needs_redraw {
            self.mark_paint_dirty();
        }
        (needs_rebuild, needs_redraw)
    }

    /// Emulates AI keyboard character input.
    pub fn mcp_keyboard_type(&mut self, text: &str) -> bool {
        let (rebuild, redraw) = self.mcp_keyboard_type_outcome(text);
        rebuild || redraw
    }

    /// Emulates AI key combination or non-character navigation key, returning `(needs_rebuild, needs_redraw)`.
    pub fn mcp_keyboard_key_outcome(
        &mut self,
        key: strata_platform::KeyCode,
        modifiers: strata_platform::Modifiers,
    ) -> (bool, bool) {
        self.ensure_mcp_cursor().touch_activity();
        let ev_down = strata_platform::PlatformEvent::KeyDown { key, modifiers };
        let res_down = self.dispatch_key_event(&ev_down);
        let ev_up = strata_platform::PlatformEvent::KeyUp { key, modifiers };
        let res_up = self.dispatch_key_event(&ev_up);
        let needs_rebuild = res_down.needs_rebuild() || res_up.needs_rebuild();
        let needs_redraw = res_down.needs_redraw() || res_up.needs_redraw();
        if needs_redraw {
            self.mark_paint_dirty();
        }
        (needs_rebuild, needs_redraw)
    }

    /// Emulates AI key combination or non-character navigation key.
    pub fn mcp_keyboard_key(
        &mut self,
        key: strata_platform::KeyCode,
        modifiers: strata_platform::Modifiers,
    ) -> bool {
        let (rebuild, redraw) = self.mcp_keyboard_key_outcome(key, modifiers);
        rebuild || redraw
    }

    fn mcp_input_text(
        &self,
        bp: &NodeBlueprint,
        attributes: &[strata_dom::Attribute],
        layout: &LayoutNode,
    ) -> String {
        let is_password = Self::is_password_input_attrs(attributes);
        let mask_if_pw = |s: String| {
            if is_password {
                Self::mask_password_text(&s)
            } else {
                s
            }
        };
        let input_id = attributes.iter().find_map(|attr| match attr {
            strata_dom::Attribute::Property { key, value } if key == "id" && !value.is_empty() => {
                Some(value.as_str())
            }
            _ => None,
        });

        if let Some(ref active) = self.active_focus {
            let matches_active = Self::find_layout_for_focus(
                &self.root_blueprint,
                &self.layout_root,
                active.input_index,
                active.input_id.as_deref(),
                active.entity_id,
            )
            .is_some_and(|l| std::ptr::eq(l, layout));
            if matches_active {
                return mask_if_pw(active.state.text.get());
            }
        }

        if let Some(id) = input_id {
            if let Some(state) = self.input_states.get(id) {
                return mask_if_pw(state.text.get());
            }
        } else {
            let mut idx = 0usize;
            while let Some(found) =
                Self::find_input_layout(&self.root_blueprint, &self.layout_root, idx, None, &mut 0)
            {
                if std::ptr::eq(found, layout) {
                    if let Some(state) = self.input_states.get(&format!("input:{idx}")) {
                        return mask_if_pw(state.text.get());
                    }
                    break;
                }
                idx += 1;
            }
        }

        for attr in attributes {
            if let strata_dom::Attribute::Property { key, value } = attr {
                if key == "value" && !value.is_empty() {
                    return mask_if_pw(value.clone());
                }
            }
        }

        Self::extract_text_content(bp)
    }

    /// Inspects the layout element at coordinates `(x, y)` returning tag, CSS classes, bounds, text, and cursor style.
    pub fn mcp_inspect_element(
        &self,
        x: f32,
        y: f32,
    ) -> (
        String,
        Vec<String>,
        strata_layout::Rect,
        String,
        strata_platform::window::CursorIcon,
    ) {
        let cursor_icon = Self::hit_test_cursor(&self.root_blueprint, &self.layout_root, x, y);
        if let Some(path) = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, x, y)
        {
            if let Some((bp, layout)) =
                Self::find_node_by_path(&self.root_blueprint, &self.layout_root, &path)
            {
                let (tag_name, classes) = match bp {
                    NodeBlueprint::Element { tag, classes, .. } => {
                        (format!("{:?}", tag).to_ascii_lowercase(), classes.clone())
                    }
                    NodeBlueprint::Text(_) | NodeBlueprint::DynamicText(_) => {
                        ("#text".to_string(), strata_dom::ClassList::Empty)
                    }
                    _ => ("div".to_string(), strata_dom::ClassList::Empty),
                };
                let text = match bp {
                    NodeBlueprint::Element {
                        tag: Tag::Input | Tag::Textarea,
                        attributes,
                        ..
                    } => self.mcp_input_text(bp, attributes, layout),
                    _ => Self::extract_text_content(bp),
                };
                return (
                    tag_name,
                    classes.to_vec(),
                    layout.geometry.border_box,
                    text,
                    cursor_icon,
                );
            }
        }

        (
            "body".to_string(),
            Vec::new(),
            strata_layout::Rect {
                x: 0.0,
                y: 0.0,
                width: self.viewport_width,
                height: self.viewport_height,
            },
            String::new(),
            cursor_icon,
        )
    }
}
