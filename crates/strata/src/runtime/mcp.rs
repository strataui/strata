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

    /// Emulates AI pointer movement to `(x, y)`. Smooth interpolation is enabled by default.
    ///
    /// Automatically resolves and applies the element's CSS `cursor` style (e.g. pointer, text).
    /// Returns true if a view rebuild or display list redraw is required.
    pub fn mcp_move_pointer(&mut self, x: f32, y: f32, smooth: bool) -> bool {
        let cursor_icon = Self::hit_test_cursor(&self.root_blueprint, &self.layout_root, x, y);
        let cursor = self.ensure_mcp_cursor();
        cursor.move_to(x, y, smooth);
        cursor.cursor_icon = cursor_icon;

        self.last_mouse_pos = (x, y);
        let mut needs_rebuild = false;
        let mut needs_redraw = false;

        if self.dispatch_pointer_move(x, y) {
            needs_rebuild = true;
        }
        if self.update_hover(x, y) {
            if self.hover_is_paint_only() {
                needs_redraw = true;
            } else {
                needs_rebuild = true;
            }
        }

        needs_redraw || needs_rebuild || true
    }

    /// Emulates AI mouse button down event.
    pub fn mcp_mouse_down(
        &mut self,
        button: strata_platform::MouseButton,
        x: Option<f32>,
        y: Option<f32>,
    ) -> bool {
        let (px, py) = match (x, y) {
            (Some(px), Some(py)) => {
                self.mcp_move_pointer(px, py, false);
                (px, py)
            }
            _ => {
                let c = self.ensure_mcp_cursor();
                (c.current_x, c.current_y)
            }
        };

        let cursor = self.ensure_mcp_cursor();
        cursor.set_down(true);

        let btn_idx = button.to_button_index();
        let mut rebuild = false;

        if button == strata_platform::MouseButton::Left {
            if self.check_scrollbar_thumb_press(px, py) {
                return true;
            }
            self.begin_window_drag_candidate(px, py);
            if self.dispatch_pointer_down_with_modifiers(
                px,
                py,
                strata_platform::Modifiers::default(),
            ) {
                rebuild = true;
            }
            if self.dispatch_mouse_click(px, py, btn_idx, 1) {
                rebuild = true;
            }
            if self.drain_edit_commands() {
                rebuild = true;
            }
        } else if button == strata_platform::MouseButton::Middle {
            if self.dispatch_pointer_down(px, py) {
                rebuild = true;
            }
            if self.dispatch_mouse_click(px, py, btn_idx, 1) {
                rebuild = true;
            }
        } else if button == strata_platform::MouseButton::Right {
            if self.dispatch_context_menu(px, py) {
                rebuild = true;
            }
            if self.drain_edit_commands() {
                rebuild = true;
            }
        }

        rebuild
    }

    /// Emulates AI mouse button release up event.
    pub fn mcp_mouse_up(
        &mut self,
        _button: strata_platform::MouseButton,
        x: Option<f32>,
        y: Option<f32>,
    ) -> bool {
        let (px, py) = match (x, y) {
            (Some(px), Some(py)) => {
                self.mcp_move_pointer(px, py, false);
                (px, py)
            }
            _ => {
                let c = self.ensure_mcp_cursor();
                (c.current_x, c.current_y)
            }
        };

        let cursor = self.ensure_mcp_cursor();
        cursor.set_down(false);

        let mut redraw = false;
        if self.scrollbar_drag.is_some() {
            self.scrollbar_drag = None;
            redraw = true;
        }
        self.pending_window_drag = None;
        let rebuild = self.dispatch_pointer_up(px, py);

        rebuild || redraw
    }

    /// Emulates AI mouse click (press down and release).
    pub fn mcp_mouse_click(
        &mut self,
        button: strata_platform::MouseButton,
        x: Option<f32>,
        y: Option<f32>,
        count: usize,
    ) -> bool {
        let (px, py) = match (x, y) {
            (Some(px), Some(py)) => {
                self.mcp_move_pointer(px, py, false);
                (px, py)
            }
            _ => {
                let c = self.ensure_mcp_cursor();
                (c.current_x, c.current_y)
            }
        };

        let mut rebuild = false;
        for _ in 0..count.max(1) {
            let down = self.mcp_mouse_down(button, Some(px), Some(py));
            let up = self.mcp_mouse_up(button, Some(px), Some(py));
            if down || up {
                rebuild = true;
            }
        }

        rebuild
    }

    /// Emulates AI mouse wheel scroll event.
    pub fn mcp_mouse_scroll(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        x: Option<f32>,
        y: Option<f32>,
    ) -> bool {
        let (px, py) = match (x, y) {
            (Some(px), Some(py)) => {
                self.mcp_move_pointer(px, py, false);
                (px, py)
            }
            _ => {
                let c = self.ensure_mcp_cursor();
                (c.current_x, c.current_y)
            }
        };

        self.ensure_mcp_cursor().touch_activity();
        self.dispatch_smooth_scroll(px, py, delta_x, delta_y)
    }

    /// Emulates AI keyboard character input.
    pub fn mcp_keyboard_type(&mut self, text: &str) -> bool {
        self.ensure_mcp_cursor().touch_activity();
        let mut any_handled = false;
        for ch in text.chars() {
            let ev = strata_platform::PlatformEvent::ReceivedCharacter(ch);
            if self.handle_key_event(&ev) {
                any_handled = true;
            }
        }
        any_handled
    }

    /// Emulates AI key combination or non-character navigation key.
    pub fn mcp_keyboard_key(
        &mut self,
        key: strata_platform::KeyCode,
        modifiers: strata_platform::Modifiers,
    ) -> bool {
        self.ensure_mcp_cursor().touch_activity();
        let ev = strata_platform::PlatformEvent::KeyDown { key, modifiers };
        let handled = self.handle_key_event(&ev);
        let _ = self.handle_key_event(&strata_platform::PlatformEvent::KeyUp { key, modifiers });
        handled
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
        if let Some(path) = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, x, y) {
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
                let text = Self::extract_text_content(bp);
                return (tag_name, classes.to_vec(), layout.geometry.border_box, text, cursor_icon);
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
