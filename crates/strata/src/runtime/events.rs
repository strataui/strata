//! Platform event dispatching, pointer gestures, keyboard events, and window drag candidates.

use super::*;

impl AppRuntime {
    /// Returns the currently active platform mouse cursor icon.
    pub fn current_cursor(&self) -> strata_platform::window::CursorIcon {
        self.current_cursor
    }

    /// Dispatches pointer down at (px, py) coordinates with default modifiers.
    /// Returns the detailed action result (CaretMoved, FocusChanged, DomListenerInvoked, or Ignored).
    pub fn dispatch_pointer_down_action(&mut self, px: f32, py: f32) -> PointerDownAction {
        self.dispatch_pointer_down_action_with_modifiers(px, py, strata_platform::Modifiers::default())
    }

    /// Dispatches pointer down at (px, py) coordinates with no active modifier keys.
    /// Returns true if focus or selection state changed.
    pub fn dispatch_pointer_down(&mut self, px: f32, py: f32) -> bool {
        self.dispatch_pointer_down_action(px, py).is_handled()
    }

    /// Dispatches pointer down at (px, py) coordinates to manage native input element focus, multi-click,
    /// Shift+click range extension, and text selection.
    /// Returns true if focus or selection state changed.
    pub fn dispatch_pointer_down_with_modifiers(
        &mut self,
        px: f32,
        py: f32,
        modifiers: strata_platform::Modifiers,
    ) -> bool {
        self.dispatch_pointer_down_action_with_modifiers(px, py, modifiers).is_handled()
    }

    /// Dispatches pointer down with modifier keys, returning whether the interaction only moved the caret
    /// in the already-active input, changed focus, or invoked a DOM listener.
    pub fn dispatch_pointer_down_action_with_modifiers(
        &mut self,
        px: f32,
        py: f32,
        modifiers: strata_platform::Modifiers,
    ) -> PointerDownAction {
        // Surfaces marked `data-keep-focus` (context menus, toolbars) never steal focus from the
        // active text input, so commands such as Cut/Paste act on the field the user was editing.
        if Self::hit_test_property(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            "data-keep-focus",
        )
        .is_some()
        {
            self.drag_selection_active = false;
            return PointerDownAction::Ignored;
        }

        let mut input_handled = false;
        if let Some(target) = Self::hit_test_input(&self.root_blueprint, &self.layout_root, px, py)
        {
            let target_key = target
                .input_id
                .clone()
                .unwrap_or_else(|| format!("input:{}", target.input_index));
            let text_val = if let Some(ref active) = self.active_focus {
                let current_key = active
                    .input_id
                    .clone()
                    .unwrap_or_else(|| format!("input:{}", active.input_index));
                if current_key == target_key {
                    active.state.text.get()
                } else {
                    target.current_value.clone()
                }
            } else {
                target.current_value.clone()
            };
            let content_box_width = target.content_box_width;
            let rel_x = if px < target.content_box_x {
                0.0
            } else if content_box_width > 0.0 && px > target.content_box_x + content_box_width {
                100000.0
            } else {
                px - target.content_box_x
            };
            let char_idx = Self::calculate_exact_char_index_at_offset(
                &text_val,
                rel_x,
                target.font_size,
                target.line_height,
                target.is_mono,
                target.font_family,
            );

            let now = std::time::Instant::now();
            let click_count = if let Some((last_time, last_x, last_y, count)) = self.last_click_info
            {
                if now.duration_since(last_time).as_millis() < 400
                    && (px - last_x).abs() < 5.0
                    && (py - last_y).abs() < 5.0
                {
                    count + 1
                } else {
                    1
                }
            } else {
                1
            };
            self.last_click_info = Some((now, px, py, click_count));
            self.reset_caret_blink();

            let target_key = target
                .input_id
                .clone()
                .unwrap_or_else(|| format!("input:{}", target.input_index));

            // If the same input is already active, handle multi-click or reposition caret
            if let Some(ref mut active) = self.active_focus {
                let current_key = active
                    .input_id
                    .clone()
                    .unwrap_or_else(|| format!("input:{}", active.input_index));
                if current_key == target_key {
                    if modifiers.shift {
                        // Shift+click extends the existing selection from its anchor to the clicked index.
                        active.state.update_selection_drag(char_idx);
                        self.drag_selection_active = true;
                    } else if click_count == 2 {
                        active.state.select_word_at_index(char_idx);
                        self.drag_selection_active = false;
                    } else if click_count == 3 {
                        active.state.select_line_at_index(char_idx);
                        self.drag_selection_active = false;
                    } else if click_count >= 4 {
                        active.state.select_all();
                        self.drag_selection_active = false;
                    } else {
                        active.state.cursor_index.set(char_idx);
                        active.state.selection_anchor.set(Some(char_idx));
                        self.drag_selection_active = true;
                    }
                    self.sync_active_input_layout();
                    return PointerDownAction::CaretMoved;
                }

                // Switching away from previous input: persist its undo/redo history in input_states
                self.input_states.insert(current_key, active.state.clone());
                if let Some(ref blur_cb) = active.on_blur {
                    blur_cb(strata_dom::Event::new(strata_dom::EventType::Blur));
                }
            }

            // Restore existing input state (with its private undo/redo history) or initialize new one
            let state = self.input_states.remove(&target_key).unwrap_or_else(|| {
                strata_dom::text_input::TextInputState::with_text(&target.current_value)
            });

            // Sync external value if state was empty
            if state.text.get().is_empty() && !target.current_value.is_empty() {
                state.text.set(target.current_value.clone());
            }

            if click_count == 2 {
                state.select_word_at_index(char_idx);
                self.drag_selection_active = false;
            } else if click_count == 3 {
                state.select_line_at_index(char_idx);
                self.drag_selection_active = false;
            } else if click_count >= 4 {
                state.select_all();
                self.drag_selection_active = false;
            } else {
                state.cursor_index.set(char_idx);
                state.selection_anchor.set(Some(char_idx));
                self.drag_selection_active = true;
            }
            state.is_focused.set(true);

            if let Some(ref focus_cb) = target.on_focus {
                focus_cb(strata_dom::Event::new(strata_dom::EventType::Focus));
            }

            self.active_focus = Some(ActiveInput {
                input_index: target.input_index,
                input_id: target.input_id,
                state,
                on_input: target.on_input,
                on_change: target.on_change,
                on_submit: target.on_submit,
                on_blur: target.on_blur,
            });
            self.sync_active_input_layout();
            input_handled = true;
        } else if let Some(active) = self.active_focus.take() {
            let key = active
                .input_id
                .clone()
                .unwrap_or_else(|| format!("input:{}", active.input_index));
            self.input_states.insert(key, active.state.clone());
            self.drag_selection_active = false;
            self.last_click_info = None;
            if let Some(ref blur_cb) = active.on_blur {
                blur_cb(strata_dom::Event::new(strata_dom::EventType::Blur));
            }
            self.sync_active_input_layout();
            input_handled = true;
        } else {
            self.drag_selection_active = false;
        }

        // Also dispatch PointerDown and MouseDown to DOM listeners
        let mut dom_handled = false;
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::PointerDown,
        ) {
            let ev = self.create_mouse_event(
                strata_dom::EventType::PointerDown,
                px,
                py,
                0,
                1,
                Some(target_box),
            );
            handler(ev);
            dom_handled = true;
        }
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::MouseDown,
        ) {
            let ev = self.create_mouse_event(
                strata_dom::EventType::MouseDown,
                px,
                py,
                0,
                1,
                Some(target_box),
            );
            handler(ev);
            dom_handled = true;
        }

        if dom_handled {
            PointerDownAction::DomListenerInvoked
        } else if input_handled {
            PointerDownAction::FocusChanged
        } else {
            PointerDownAction::Ignored
        }
    }

    /// Dispatches pointer move at (px, py), returning detailed action result.
    pub fn dispatch_pointer_move_action(&mut self, px: f32, py: f32) -> PointerMoveAction {
        let mut selection_dragged = false;
        if self.drag_selection_active {
            if let Some(ref mut active) = self.active_focus {
                let mut current_idx = 0;
                let target_idx = active.input_index;
                let target_id = active.input_id.as_deref();

                if let Some(layout) = Self::find_input_layout(
                    &self.root_blueprint,
                    &self.layout_root,
                    target_idx,
                    target_id,
                    &mut current_idx,
                ) {
                    let rel_x = (px - layout.geometry.content_box.x).max(0.0);
                    let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
                    let char_idx = Self::calculate_exact_char_index_at_offset(
                        &active.state.text.get(),
                        rel_x,
                        layout.style.font_size,
                        layout.style.line_height,
                        is_mono,
                        layout.style.font_family,
                    );
                    active.state.cursor_index.set(char_idx);
                    self.reset_caret_blink();
                    selection_dragged = true;
                }
            }
        }

        let mut dom_handled = false;
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::PointerMove,
        ) {
            let ev = self.create_mouse_event(
                strata_dom::EventType::PointerMove,
                px,
                py,
                0,
                0,
                Some(target_box),
            );
            handler(ev);
            dom_handled = true;
        }
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::MouseMove,
        ) {
            let ev = self.create_mouse_event(
                strata_dom::EventType::MouseMove,
                px,
                py,
                0,
                0,
                Some(target_box),
            );
            handler(ev);
            dom_handled = true;
        }

        if dom_handled {
            PointerMoveAction::DomListenerInvoked
        } else if selection_dragged {
            PointerMoveAction::SelectionDragged
        } else {
            PointerMoveAction::Ignored
        }
    }

    /// Dispatches pointer move at (px, py) to update mouse drag text selection and DOM move listeners.
    pub fn dispatch_pointer_move(&mut self, px: f32, py: f32) -> bool {
        self.dispatch_pointer_move_action(px, py).is_handled()
    }

    /// Dispatches pointer up, returning detailed action result.
    pub fn dispatch_pointer_up_action(&mut self, px: f32, py: f32) -> PointerUpAction {
        let mut selection_ended = false;
        if self.drag_selection_active {
            self.drag_selection_active = false;
            if let Some(ref mut active) = self.active_focus {
                if let Some(anchor) = active.state.selection_anchor.get() {
                    if anchor == active.state.cursor_index.get() {
                        active.state.selection_anchor.set(None);
                    }
                }
            }
            selection_ended = true;
        }

        let mut dom_handled = false;
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::PointerUp,
        ) {
            let ev = self.create_mouse_event(
                strata_dom::EventType::PointerUp,
                px,
                py,
                0,
                1,
                Some(target_box),
            );
            handler(ev);
            dom_handled = true;
        }
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::MouseUp,
        ) {
            let ev = self.create_mouse_event(
                strata_dom::EventType::MouseUp,
                px,
                py,
                0,
                1,
                Some(target_box),
            );
            handler(ev);
            dom_handled = true;
        }

        if dom_handled {
            PointerUpAction::DomListenerInvoked
        } else if selection_ended {
            PointerUpAction::SelectionEnded
        } else {
            PointerUpAction::Ignored
        }
    }

    /// Dispatches pointer up to finalize mouse drag text selection and notify DOM up listeners.
    pub fn dispatch_pointer_up(&mut self, px: f32, py: f32) -> bool {
        self.dispatch_pointer_up_action(px, py).is_handled()
    }

    /// Dispatches keyboard events to the active focused input element, returning a fine-grained KeyDispatchResult.
    pub fn dispatch_key_event(&mut self, event: &PlatformEvent) -> KeyDispatchResult {
        if self.active_focus.is_none() {
            if let PlatformEvent::KeyDown { key, .. } = event {
                let (px, py) = self.last_mouse_pos;
                let scrolled = match key {
                    strata_platform::KeyCode::ArrowDown => {
                        self.dispatch_scroll(px, py, 0.0, 40.0)
                    }
                    strata_platform::KeyCode::ArrowUp => {
                        self.dispatch_scroll(px, py, 0.0, -40.0)
                    }
                    strata_platform::KeyCode::PageDown => {
                        self.dispatch_scroll(px, py, 0.0, 400.0)
                    }
                    strata_platform::KeyCode::PageUp => {
                        self.dispatch_scroll(px, py, 0.0, -400.0)
                    }
                    strata_platform::KeyCode::Home => {
                        self.scroll_to_top(px, py)
                    }
                    strata_platform::KeyCode::End => {
                        self.scroll_to_bottom(px, py)
                    }
                    _ => false,
                };
                if scrolled {
                    return KeyDispatchResult::RedrawOnly;
                }
            }
            return KeyDispatchResult::Ignored;
        }

        match event {
            PlatformEvent::ReceivedCharacter(ch) => {
                if !ch.is_control() {
                    self.reset_caret_blink();
                    let active = self.active_focus.as_mut().unwrap();
                    active.state.insert_text(&ch.to_string());
                    let val = active.state.text.get();
                    if let Some(ref on_input) = active.on_input {
                        on_input(strata_dom::Event::with_value(
                            strata_dom::EventType::Input,
                            val,
                        ));
                    }
                    self.sync_active_input_layout();
                    KeyDispatchResult::NeedsRebuild
                } else {
                    KeyDispatchResult::Ignored
                }
            }
            PlatformEvent::KeyDown { key, modifiers } => {
                let mut text_changed = false;
                let mut nav_or_sel = false;
                let mut submit = false;
                let mut blur = false;

                let is_cmd = modifiers.ctrl || modifiers.meta;
                if is_cmd {
                    let cmd = match key {
                        strata_platform::KeyCode::KeyA => Some(EditCommand::SelectAll),
                        strata_platform::KeyCode::KeyC => Some(EditCommand::Copy),
                        strata_platform::KeyCode::KeyX => Some(EditCommand::Cut),
                        strata_platform::KeyCode::KeyV => Some(EditCommand::Paste),
                        strata_platform::KeyCode::KeyZ if modifiers.shift => {
                            Some(EditCommand::Redo)
                        }
                        strata_platform::KeyCode::KeyZ => Some(EditCommand::Undo),
                        strata_platform::KeyCode::KeyY => Some(EditCommand::Redo),
                        _ => None,
                    };
                    if let Some(cmd) = cmd {
                        let res = self.apply_edit_command(cmd);
                        self.sync_active_input_layout();
                        if res {
                            return match cmd {
                                EditCommand::SelectAll | EditCommand::Copy => {
                                    KeyDispatchResult::RedrawOnly
                                }
                                _ => KeyDispatchResult::NeedsRebuild,
                            };
                        } else {
                            return KeyDispatchResult::Ignored;
                        }
                    }
                }

                let active = self.active_focus.as_mut().unwrap();

                if !text_changed && !nav_or_sel {
                    match key {
                        strata_platform::KeyCode::Backspace => {
                            if is_cmd {
                                active.state.delete_word_backward();
                            } else {
                                active.state.delete_backward();
                            }
                            text_changed = true;
                        }
                        strata_platform::KeyCode::Delete => {
                            if is_cmd {
                                active.state.delete_word_forward();
                            } else {
                                active.state.delete_forward();
                            }
                            text_changed = true;
                        }
                        strata_platform::KeyCode::ArrowLeft => {
                            active.state.move_left(modifiers.shift, is_cmd);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::ArrowRight => {
                            active.state.move_right(modifiers.shift, is_cmd);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::Home => {
                            active.state.move_home(modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::End => {
                            active.state.move_end(modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::Enter => {
                            submit = true;
                        }
                        strata_platform::KeyCode::Escape => {
                            blur = true;
                        }
                        _ => {}
                    }
                }

                if text_changed || nav_or_sel {
                    self.caret_blink_epoch = std::time::Instant::now();
                }

                if text_changed {
                    let val = active.state.text.get();
                    if let Some(ref on_input) = active.on_input {
                        on_input(strata_dom::Event::with_value(
                            strata_dom::EventType::Input,
                            val,
                        ));
                    }
                    self.sync_active_input_layout();
                    return KeyDispatchResult::NeedsRebuild;
                }

                if nav_or_sel {
                    return KeyDispatchResult::RedrawOnly;
                }

                if submit {
                    let val = active.state.text.get();
                    if let Some(ref on_submit) = active.on_submit {
                        on_submit(strata_dom::Event::with_value(
                            strata_dom::EventType::Submit,
                            val.clone(),
                        ));
                    }
                    if let Some(ref on_change) = active.on_change {
                        on_change(strata_dom::Event::with_value(
                            strata_dom::EventType::Change,
                            val,
                        ));
                    }
                    return KeyDispatchResult::NeedsRebuild;
                }

                if blur {
                    if let Some(ref on_blur) = active.on_blur {
                        on_blur(strata_dom::Event::new(strata_dom::EventType::Blur));
                    }
                    self.active_focus = None;
                    return KeyDispatchResult::NeedsRebuild;
                }

                KeyDispatchResult::Ignored
            }
            _ => KeyDispatchResult::Ignored,
        }
    }

    /// Dispatches keyboard events to the active focused input element.
    /// Returns true if the key event was consumed by the focused input.
    pub fn handle_key_event(&mut self, event: &PlatformEvent) -> bool {
        self.dispatch_key_event(event).is_handled()
    }

    /// Creates a fully populated synthetic W3C DOM Event for mouse and pointer interactions,
    /// with client, page, screen, and offset coordinates, button identifiers, click detail,
    /// and keyboard modifier flags.
    pub fn create_mouse_event(
        &self,
        event_type: strata_dom::EventType,
        px: f32,
        py: f32,
        button: i16,
        detail: i32,
        target_box: Option<&strata_layout::Rect>,
    ) -> strata_dom::Event {
        let mut ev = strata_dom::Event::new(event_type);
        ev.client_x = px as f64;
        ev.client_y = py as f64;
        ev.page_x = px as f64;
        ev.page_y = py as f64;
        ev.screen_x = px as f64;
        ev.screen_y = py as f64;
        ev.button = button;
        ev.buttons = match button {
            0 => 1,
            1 => 4,
            2 => 2,
            _ => 0,
        };
        ev.detail = detail;
        if let Some(tb) = target_box {
            ev.offset_x = (px - tb.x) as f64;
            ev.offset_y = (py - tb.y) as f64;
        } else {
            ev.offset_x = px as f64;
            ev.offset_y = py as f64;
        }
        #[cfg(target_os = "linux")]
        {
            let mods = strata_platform::gtk_current_modifiers();
            ev.shift_key = mods.shift;
            ev.ctrl_key = mods.ctrl;
            ev.alt_key = mods.alt;
            ev.meta_key = mods.meta;
        }
        ev
    }

    /// Dispatches a pointer click event at (x, y) coordinates into the application blueprint tree.
    /// Returns true if an event listener was invoked.
    pub fn dispatch_click(&self, px: f32, py: f32) -> bool {
        self.dispatch_mouse_click(px, py, 0, 1)
    }

    /// Dispatches a mouse click event with specific button (0 = Left, 1 = Middle, 2 = Right) and detail.
    pub fn dispatch_mouse_click(&self, px: f32, py: f32, button: i16, detail: i32) -> bool {
        let ev_type = if button == 0 {
            strata_dom::EventType::Click
        } else {
            strata_dom::EventType::AuxClick
        };

        if let Some((handler, target_box)) =
            Self::hit_test_event_with_box(&self.root_blueprint, &self.layout_root, px, py, ev_type)
        {
            let ev = self.create_mouse_event(ev_type, px, py, button, detail, Some(target_box));
            handler(ev);
            true
        } else if button != 0 {
            // Fallback to Click listener if AuxClick had no explicit listener
            if let Some((handler, target_box)) = Self::hit_test_event_with_box(
                &self.root_blueprint,
                &self.layout_root,
                px,
                py,
                strata_dom::EventType::Click,
            ) {
                let ev = self.create_mouse_event(
                    strata_dom::EventType::Click,
                    px,
                    py,
                    button,
                    detail,
                    Some(target_box),
                );
                handler(ev);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Dispatches a standard W3C `dblclick` event at (x, y) coordinates to the nearest element with an `ondblclick` listener.
    pub fn dispatch_dblclick(&self, px: f32, py: f32) -> bool {
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::DblClick,
        ) {
            let ev = self.create_mouse_event(
                strata_dom::EventType::DblClick,
                px,
                py,
                0,
                2,
                Some(target_box),
            );
            handler(ev);
            true
        } else {
            false
        }
    }

    /// Dispatches a W3C `contextmenu` event at (x, y) to the nearest element that registered
    /// an `oncontextmenu` listener. Returns false (and leaves the platform default untouched)
    /// when nothing at that point listens, so a context menu never appears on arbitrary surfaces.
    pub fn dispatch_context_menu(&self, px: f32, py: f32) -> bool {
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::ContextMenu,
        ) {
            let ev = self.create_mouse_event(
                strata_dom::EventType::ContextMenu,
                px,
                py,
                2,
                1,
                Some(target_box),
            );
            handler(ev);
            true
        } else {
            false
        }
    }

    /// Records a possible window drag when a primary press lands on a `data-app-region="drag"`
    /// surface (the equivalent of `-webkit-app-region: drag`). Interactive children opt out with
    /// `data-app-region="no-drag"`. A quick second press toggles maximize, as native title bars do.
    pub fn begin_window_drag_candidate(&mut self, px: f32, py: f32) -> bool {
        let region = Self::hit_test_property(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            "data-app-region",
        );
        if region.as_deref() != Some("drag") {
            self.pending_window_drag = None;
            return false;
        }
        let now = std::time::Instant::now();
        if let Some((t, lx, ly)) = self.last_drag_click.take() {
            if now.duration_since(t).as_millis() < 400
                && (px - lx).abs() < 5.0
                && (py - ly).abs() < 5.0
            {
                // Cancel pending drag candidate on double-click gesture so movement doesn't interfere
                self.pending_window_drag = None;
                return true;
            }
        }
        self.last_drag_click = Some((now, px, py));
        self.pending_window_drag = Some((px, py));
        false
    }

    /// Starts a native window move once the pointer travelled past the drag threshold.
    /// Returns true when the window's maximized state changed (such as unmaximizing on drag).
    pub fn poll_window_drag(&mut self, px: f32, py: f32) -> bool {
        if let Some((sx, sy)) = self.pending_window_drag {
            if (px - sx).abs() > 4.0 || (py - sy).abs() > 4.0 {
                self.pending_window_drag = None;
                self.last_drag_click = None;
                #[cfg(target_os = "linux")]
                {
                    let was_max = strata_platform::gtk_is_maximized();
                    strata_platform::gtk_begin_window_drag(sx / self.viewport_width.max(1.0), sy);
                    let now_max = strata_platform::gtk_is_maximized();
                    return was_max != now_max;
                }
            }
        }
        false
    }

}
