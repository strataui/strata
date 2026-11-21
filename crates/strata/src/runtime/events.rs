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
        self.dispatch_pointer_down_action_with_modifiers(
            px,
            py,
            strata_platform::Modifiers::default(),
        )
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
        self.dispatch_pointer_down_action_with_modifiers(px, py, modifiers)
            .is_handled()
    }

    /// Dispatches pointer down with modifier keys, returning whether the interaction only moved the caret
    /// in the already-active input, changed focus, or invoked a DOM listener.
    pub fn dispatch_pointer_down_action_with_modifiers(
        &mut self,
        px: f32,
        py: f32,
        modifiers: strata_platform::Modifiers,
    ) -> PointerDownAction {
        self.dispatch_pointer_down_action_with_button_and_modifiers(px, py, 0, modifiers)
    }

    /// Dispatches pointer down with explicit mouse button (`0` = Left, `1` = Middle, `2` = Right)
    /// and modifier keys, managing text input focus/caret on primary clicks and invoking DOM
    /// `PointerDown` / `MouseDown` listeners with accurate `button` / `buttons` metadata.
    pub fn dispatch_pointer_down_action_with_button_and_modifiers(
        &mut self,
        px: f32,
        py: f32,
        button: i16,
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
        if button == 0 {
            if let Some(target) =
                Self::hit_test_input(&self.root_blueprint, &self.layout_root, px, py)
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
                let is_password = self.input_is_password(
                    target.input_index,
                    target.input_id.as_deref(),
                    target.entity_id,
                );
                let measure_val = if is_password {
                    Self::mask_password_text(&text_val)
                } else {
                    text_val
                };
                let content_box_width = target.content_box_width;
                let rel_x = if px < target.content_box_x {
                    0.0
                } else if content_box_width > 0.0 && px > target.content_box_x + content_box_width
                {
                    100000.0
                } else {
                    px - target.content_box_x
                };
                let char_idx = Self::calculate_exact_char_index_at_offset(
                    &measure_val,
                    rel_x,
                    target.font_size,
                    target.line_height,
                    target.is_mono,
                    target.font_family,
                );

                let now = std::time::Instant::now();
                let click_count = if let Some((last_time, last_x, last_y, count)) =
                    self.last_click_info
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
                            if is_password {
                                active.state.select_all();
                            } else {
                                active.state.select_word_at_index(char_idx);
                            }
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
                    if is_password {
                        state.select_all();
                    } else {
                        state.select_word_at_index(char_idx);
                    }
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
                    entity_id: target.entity_id,
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
        }

        // Also dispatch PointerDown and MouseDown to DOM listeners
        let pre_open_outside_ids = strata_dom::hooks::snapshot_open_click_outside_ids();
        let mut inside_click_stopped = false;
        self.pointer_capture_target = None;
        let mut dom_handled = false;
        let mut captured_target: Option<(Vec<usize>, bool)> = None;
        if let Some((handler, target_box, eid)) = Self::hit_test_event_with_box_and_entity(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::PointerDown,
        ) {
            let target_box_copy = *target_box;
            let ev = self.create_mouse_event(
                strata_dom::EventType::PointerDown,
                px,
                py,
                button,
                1,
                Some(&target_box_copy),
            );
            let capture_req = ev.pointer_capture_requested.clone();
            let stopped_cell = ev.propagation_stopped.clone();
            let prevented_cell = ev.default_prevented.clone();
            handler(ev);
            dom_handled = true;
            if stopped_cell.get() || prevented_cell.get() {
                inside_click_stopped = true;
            }
            if let Some(target_path) = self.layout_root.find_path_by_entity_id(eid) {
                match capture_req.get() {
                    Some(true) => captured_target = Some((target_path, true)),
                    Some(false) => {}
                    None => captured_target = Some((target_path, false)),
                }
            }
        }
        if let Some((handler, target_box, eid)) = Self::hit_test_event_with_box_and_entity(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::MouseDown,
        ) {
            let target_box_copy = *target_box;
            let ev = self.create_mouse_event(
                strata_dom::EventType::MouseDown,
                px,
                py,
                button,
                1,
                Some(&target_box_copy),
            );
            let capture_req = ev.pointer_capture_requested.clone();
            let stopped_cell = ev.propagation_stopped.clone();
            let prevented_cell = ev.default_prevented.clone();
            handler(ev);
            dom_handled = true;
            if stopped_cell.get() || prevented_cell.get() {
                inside_click_stopped = true;
            }
            if let Some(target_path) = self.layout_root.find_path_by_entity_id(eid) {
                match capture_req.get() {
                    Some(true) => captured_target = Some((target_path, true)),
                    Some(false) => captured_target = None,
                    None if captured_target.is_none() => {
                        captured_target = Some((target_path, false));
                    }
                    None => {}
                }
            }
        }
        self.pointer_capture_target = captured_target;

        if button == 0 {
            self.active_dom_drag = self.find_draggable_path_at(px, py).map(|source_path| {
                ActiveDomDrag {
                    source_path,
                    start_pos: (px, py),
                    started: false,
                    last_over_path: None,
                    data_transfer: Default::default(),
                }
            });
        }

        if !pre_open_outside_ids.is_empty() && !inside_click_stopped {
            if strata_dom::hooks::dispatch_click_outside_for_ids(&pre_open_outside_ids) {
                dom_handled = true;
            }
        }

        let focus_changed = if button == 0 {
            let focus_path = self.find_focus_path_at(px, py);
            self.set_focus_path(focus_path)
        } else {
            false
        };

        if dom_handled {
            PointerDownAction::DomListenerInvoked
        } else if input_handled || focus_changed {
            PointerDownAction::FocusChanged
        } else {
            PointerDownAction::Ignored
        }
    }

    /// Dispatches pointer move at (px, py), returning detailed action result.
    pub fn dispatch_pointer_move_action(&mut self, px: f32, py: f32) -> PointerMoveAction {
        let mut selection_dragged = false;
        if self.drag_selection_active {
            let is_password = self
                .active_focus
                .as_ref()
                .is_some_and(|a| self.active_input_is_password(a));
            if let Some(ref mut active) = self.active_focus {
                let mut current_idx = 0;
                let target_idx = active.input_index;
                let target_id = active.input_id.as_deref();

                let layout_by_entity = if let Some(eid) = active.entity_id {
                    self.layout_root.find_by_entity_id(eid)
                } else {
                    None
                };
                let layout_opt = layout_by_entity.or_else(|| {
                    Self::find_input_layout(
                        &self.root_blueprint,
                        &self.layout_root,
                        target_idx,
                        target_id,
                        &mut current_idx,
                    )
                });

                if let Some(layout) = layout_opt {
                    let rel_x = (px - layout.geometry.content_box.x).max(0.0);
                    let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
                    let raw_text = active.state.text.get();
                    let measure_text = if is_password {
                        Self::mask_password_text(&raw_text)
                    } else {
                        raw_text
                    };
                    let char_idx = Self::calculate_exact_char_index_at_offset(
                        &measure_text,
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

        let (movement_x, movement_y) = if let Some((wx, wy)) = self.pending_cursor_warp {
            if (px - wx).abs() <= 2.5 && (py - wy).abs() <= 2.5 {
                self.pending_cursor_warp = None;
                self.last_pointer_pos = Some((px, py));
                (0.0, 0.0)
            } else {
                self.pending_cursor_warp = None;
                let delta = self
                    .last_pointer_pos
                    .map(|(lx, ly)| ((px - lx) as f64, (py - ly) as f64))
                    .unwrap_or((0.0, 0.0));
                self.last_pointer_pos = Some((px, py));
                delta
            }
        } else {
            let delta = self
                .last_pointer_pos
                .map(|(lx, ly)| ((px - lx) as f64, (py - ly) as f64))
                .unwrap_or((0.0, 0.0));
            self.last_pointer_pos = Some((px, py));
            delta
        };

        let mut dom_handled = false;
        let mut requested_warp: Option<(f64, f64)> = None;
        let capture = self.pointer_capture_target.clone();
        for ev_type in [
            strata_dom::EventType::PointerMove,
            strata_dom::EventType::MouseMove,
        ] {
            match capture {
                Some((ref cap_path, true)) => {
                    if let Some((handler, target_box, _)) = Self::find_event_at_or_above_path(
                        &self.root_blueprint,
                        &self.layout_root,
                        cap_path,
                        ev_type,
                    ) {
                        let target_box_copy = *target_box;
                        let mut ev =
                            self.create_mouse_event(ev_type, px, py, 0, 0, Some(&target_box_copy));
                        ev.movement_x = movement_x;
                        ev.movement_y = movement_y;
                        let capture_req = ev.pointer_capture_requested.clone();
                        let warp_req = ev.cursor_warp_requested.clone();
                        handler(ev);
                        dom_handled = true;
                        if capture_req.get() == Some(false) {
                            self.pointer_capture_target = None;
                        }
                        if let Some(pos) = warp_req.get() {
                            requested_warp = Some(pos);
                        }
                    }
                }
                Some((ref cap_path, false)) => {
                    let mut hit_eid = None;
                    if let Some((handler, target_box, eid)) =
                        Self::hit_test_event_with_box_and_entity(
                            &self.root_blueprint,
                            &self.layout_root,
                            px,
                            py,
                            ev_type,
                        )
                    {
                        hit_eid = Some(eid);
                        let mut ev =
                            self.create_mouse_event(ev_type, px, py, 0, 0, Some(target_box));
                        ev.movement_x = movement_x;
                        ev.movement_y = movement_y;
                        let warp_req = ev.cursor_warp_requested.clone();
                        handler(ev);
                        dom_handled = true;
                        if let Some(pos) = warp_req.get() {
                            requested_warp = Some(pos);
                        }
                    }
                    if let Some((handler, target_box, cap_eid)) = Self::find_event_at_or_above_path(
                        &self.root_blueprint,
                        &self.layout_root,
                        cap_path,
                        ev_type,
                    ) {
                        if hit_eid != Some(cap_eid) {
                            let target_box_copy = *target_box;
                            let mut ev = self.create_mouse_event(
                                ev_type,
                                px,
                                py,
                                0,
                                0,
                                Some(&target_box_copy),
                            );
                            ev.movement_x = movement_x;
                            ev.movement_y = movement_y;
                            let warp_req = ev.cursor_warp_requested.clone();
                            handler(ev);
                            dom_handled = true;
                            if let Some(pos) = warp_req.get() {
                                requested_warp = Some(pos);
                            }
                        }
                    }
                }
                None => {
                    if let Some((handler, target_box)) = Self::hit_test_event_with_box(
                        &self.root_blueprint,
                        &self.layout_root,
                        px,
                        py,
                        ev_type,
                    ) {
                        let mut ev =
                            self.create_mouse_event(ev_type, px, py, 0, 0, Some(target_box));
                        ev.movement_x = movement_x;
                        ev.movement_y = movement_y;
                        let warp_req = ev.cursor_warp_requested.clone();
                        handler(ev);
                        dom_handled = true;
                        if let Some(pos) = warp_req.get() {
                            requested_warp = Some(pos);
                        }
                    }
                }
            }
        }
        if let Some((wx, wy)) = requested_warp {
            self.warp_cursor(wx as f32, wy as f32);
        }

        if let Some(mut drag) = self.active_dom_drag.take() {
            let mut cancel_drag = false;
            if !drag.started {
                let (sx, sy) = drag.start_pos;
                if (px - sx).abs() > 3.0 || (py - sy).abs() > 3.0 {
                    drag.started = true;
                    let (invoked, prevented) = self.dispatch_drag_event_at_path(
                        &drag.source_path,
                        strata_dom::EventType::DragStart,
                        sx,
                        sy,
                        &drag.data_transfer,
                    );
                    if invoked {
                        dom_handled = true;
                    }
                    if prevented {
                        cancel_drag = true;
                    }
                }
            }

            if drag.started && !cancel_drag {
                let (drag_invoked, _) = self.dispatch_drag_event_at_path(
                    &drag.source_path,
                    strata_dom::EventType::Drag,
                    px,
                    py,
                    &drag.data_transfer,
                );
                if drag_invoked {
                    dom_handled = true;
                }

                let over_path = self.find_drop_target_path_at(px, py);
                if over_path != drag.last_over_path {
                    if let Some(ref old_path) = drag.last_over_path {
                        let (leave_invoked, _) = self.dispatch_drag_event_at_path(
                            old_path,
                            strata_dom::EventType::DragLeave,
                            px,
                            py,
                            &drag.data_transfer,
                        );
                        if leave_invoked {
                            dom_handled = true;
                        }
                    }
                    if let Some(ref new_path) = over_path {
                        let (enter_invoked, _) = self.dispatch_drag_event_at_path(
                            new_path,
                            strata_dom::EventType::DragEnter,
                            px,
                            py,
                            &drag.data_transfer,
                        );
                        if enter_invoked {
                            dom_handled = true;
                        }
                    }
                    drag.last_over_path = over_path.clone();
                }

                if let Some(ref cur_over) = over_path {
                    let (over_invoked, _) = self.dispatch_drag_event_at_path(
                        cur_over,
                        strata_dom::EventType::DragOver,
                        px,
                        py,
                        &drag.data_transfer,
                    );
                    if over_invoked {
                        dom_handled = true;
                    }
                }
            }

            if !cancel_drag {
                self.active_dom_drag = Some(drag);
            }
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

    /// Dispatches pointer up with primary button (`0`), returning detailed action result.
    pub fn dispatch_pointer_up_action(&mut self, px: f32, py: f32) -> PointerUpAction {
        self.dispatch_pointer_up_action_with_button(px, py, 0)
    }

    /// Dispatches pointer up with explicit mouse button (`0` = Left, `1` = Middle, `2` = Right).
    pub fn dispatch_pointer_up_action_with_button(
        &mut self,
        px: f32,
        py: f32,
        button: i16,
    ) -> PointerUpAction {
        let mut selection_ended = false;
        if button == 0 && self.drag_selection_active {
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

        let capture = self.pointer_capture_target.take();
        let mut dom_handled = false;
        for ev_type in [
            strata_dom::EventType::PointerUp,
            strata_dom::EventType::MouseUp,
        ] {
            match capture {
                Some((ref cap_path, true)) => {
                    if let Some((handler, target_box, _)) = Self::find_event_at_or_above_path(
                        &self.root_blueprint,
                        &self.layout_root,
                        cap_path,
                        ev_type,
                    ) {
                        let ev =
                            self.create_mouse_event(ev_type, px, py, button, 1, Some(target_box));
                        handler(ev);
                        dom_handled = true;
                    }
                }
                Some((ref cap_path, false)) => {
                    let mut hit_eid = None;
                    if let Some((handler, target_box, eid)) =
                        Self::hit_test_event_with_box_and_entity(
                            &self.root_blueprint,
                            &self.layout_root,
                            px,
                            py,
                            ev_type,
                        )
                    {
                        hit_eid = Some(eid);
                        let ev =
                            self.create_mouse_event(ev_type, px, py, button, 1, Some(target_box));
                        handler(ev);
                        dom_handled = true;
                    }
                    if let Some((handler, target_box, cap_eid)) = Self::find_event_at_or_above_path(
                        &self.root_blueprint,
                        &self.layout_root,
                        cap_path,
                        ev_type,
                    ) {
                        if hit_eid != Some(cap_eid) {
                            let ev = self.create_mouse_event(
                                ev_type,
                                px,
                                py,
                                button,
                                1,
                                Some(target_box),
                            );
                            handler(ev);
                            dom_handled = true;
                        }
                    }
                }
                None => {
                    if let Some((handler, target_box)) = Self::hit_test_event_with_box(
                        &self.root_blueprint,
                        &self.layout_root,
                        px,
                        py,
                        ev_type,
                    ) {
                        let ev =
                            self.create_mouse_event(ev_type, px, py, button, 1, Some(target_box));
                        handler(ev);
                        dom_handled = true;
                    }
                }
            }
        }

        if button == 0 {
            if let Some(drag) = self.active_dom_drag.take() {
                if drag.started {
                    let drop_path = self.find_drop_target_path_at(px, py).or(drag.last_over_path);
                    let mut drop_dispatched = false;
                    if let Some(ref target_path) = drop_path {
                        let (invoked, _) = self.dispatch_drag_event_at_path(
                            target_path,
                            strata_dom::EventType::Drop,
                            px,
                            py,
                            &drag.data_transfer,
                        );
                        if invoked {
                            drop_dispatched = true;
                            dom_handled = true;
                        }
                    }
                    if !drop_dispatched {
                        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
                            &self.root_blueprint,
                            &self.layout_root,
                            px,
                            py,
                            strata_dom::EventType::Drop,
                        ) {
                            let target_box_copy = *target_box;
                            let mut ev = self.create_mouse_event(
                                strata_dom::EventType::Drop,
                                px,
                                py,
                                0,
                                1,
                                Some(&target_box_copy),
                            );
                            ev.data_transfer = std::rc::Rc::clone(&drag.data_transfer);
                            ev.data = drag
                                .data_transfer
                                .borrow()
                                .get("text/plain")
                                .cloned()
                                .or_else(|| drag.data_transfer.borrow().values().next().cloned());
                            handler(ev);
                            dom_handled = true;
                        }
                    }

                    let (end_invoked, _) = self.dispatch_drag_event_at_path(
                        &drag.source_path,
                        strata_dom::EventType::DragEnd,
                        px,
                        py,
                        &drag.data_transfer,
                    );
                    if end_invoked {
                        dom_handled = true;
                    }
                }
            }
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

    /// Finds the deepest focusable element path at `(px, py)`.
    pub(crate) fn find_focus_path_at(&self, px: f32, py: f32) -> Option<Vec<usize>> {
        let hit_path = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)?;
        let mut cur = &self.root_blueprint;
        let is_focusable = |bp: &NodeBlueprint| -> bool {
            if let NodeBlueprint::Element {
                tag, attributes, ..
            } = bp
            {
                let tag_lower = Self::tag_as_str(tag).to_ascii_lowercase();
                if matches!(
                    tag_lower.as_str(),
                    "button" | "input" | "textarea" | "select" | "a"
                ) {
                    return true;
                }
                for attr in attributes {
                    match attr {
                        strata_dom::Attribute::EventListener { event, .. }
                            if *event == strata_dom::EventType::KeyDown
                                || *event == strata_dom::EventType::KeyUp =>
                        {
                            return true;
                        }
                        strata_dom::Attribute::Property { key, .. }
                            if key == "tabindex" || key == "role" =>
                        {
                            return true;
                        }
                        _ => {}
                    }
                }
            }
            false
        };
        let mut best: Option<Vec<usize>> = if is_focusable(cur) {
            Some(Vec::new())
        } else {
            None
        };
        let mut prefix = Vec::with_capacity(hit_path.len());
        for &idx in &hit_path {
            let mut flat = Vec::new();
            match cur {
                NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                }
                _ => break,
            }
            if idx >= flat.len() {
                break;
            }
            cur = flat[idx];
            prefix.push(idx);
            if is_focusable(cur) {
                best = Some(prefix.clone());
            }
        }
        best
    }

    fn keycode_to_dom_key(key: strata_platform::KeyCode, shift: bool) -> String {
        use strata_platform::KeyCode;
        match key {
            KeyCode::ArrowDown => "ArrowDown".to_string(),
            KeyCode::ArrowUp => "ArrowUp".to_string(),
            KeyCode::ArrowLeft => "ArrowLeft".to_string(),
            KeyCode::ArrowRight => "ArrowRight".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Escape => "Escape".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Space => " ".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Digit0 => "0".to_string(),
            KeyCode::Digit1 => "1".to_string(),
            KeyCode::Digit2 => "2".to_string(),
            KeyCode::Digit3 => "3".to_string(),
            KeyCode::Digit4 => "4".to_string(),
            KeyCode::Digit5 => "5".to_string(),
            KeyCode::Digit6 => "6".to_string(),
            KeyCode::Digit7 => "7".to_string(),
            KeyCode::Digit8 => "8".to_string(),
            KeyCode::Digit9 => "9".to_string(),
            other => {
                let dbg = format!("{:?}", other);
                if let Some(letter) = dbg.strip_prefix("Key") {
                    if shift {
                        letter.to_string()
                    } else {
                        letter.to_ascii_lowercase()
                    }
                } else {
                    dbg
                }
            }
        }
    }

    fn dispatch_dom_key_event(
        &self,
        event_type: strata_dom::EventType,
        key: strata_platform::KeyCode,
        modifiers: strata_platform::Modifiers,
    ) -> bool {
        let Some(ref path) = self.current_focus_path else {
            return false;
        };
        let mut chain: Vec<&NodeBlueprint> = Vec::with_capacity(path.len() + 1);
        let mut cur = &self.root_blueprint;
        chain.push(cur);
        for &idx in path {
            let mut flat = Vec::new();
            match cur {
                NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                }
                _ => break,
            }
            if idx >= flat.len() {
                break;
            }
            cur = flat[idx];
            chain.push(cur);
        }

        let dom_key = Self::keycode_to_dom_key(key, modifiers.shift);
        let mut ev = strata_dom::Event::new(event_type);
        ev.key = dom_key;
        ev.ctrl_key = modifiers.ctrl;
        ev.shift_key = modifiers.shift;
        ev.alt_key = modifiers.alt;
        ev.meta_key = modifiers.meta;

        let mut handled = false;
        for bp in chain.into_iter().rev() {
            if let NodeBlueprint::Element { attributes, .. } = bp {
                for attr in attributes {
                    if let strata_dom::Attribute::EventListener { event, handler } = attr {
                        if *event == event_type {
                            handler(ev.clone());
                            handled = true;
                            if ev.propagation_stopped.get() {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        handled
    }

    /// Dispatches keyboard events to the active focused input element, returning a fine-grained KeyDispatchResult.
    pub fn dispatch_key_event(&mut self, event: &PlatformEvent) -> KeyDispatchResult {
        if self.active_focus.is_none() {
            match event {
                PlatformEvent::KeyDown { key, modifiers } => {
                    if self.dispatch_dom_key_event(strata_dom::EventType::KeyDown, *key, *modifiers)
                    {
                        return KeyDispatchResult::NeedsRebuild;
                    }
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
                        strata_platform::KeyCode::Home => self.scroll_to_top(px, py),
                        strata_platform::KeyCode::End => self.scroll_to_bottom(px, py),
                        _ => false,
                    };
                    if scrolled {
                        return KeyDispatchResult::RedrawOnly;
                    }
                }
                PlatformEvent::KeyUp { key, modifiers } => {
                    if self.dispatch_dom_key_event(strata_dom::EventType::KeyUp, *key, *modifiers) {
                        return KeyDispatchResult::NeedsRebuild;
                    }
                }
                _ => {}
            }
            return KeyDispatchResult::Ignored;
        }

        match event {
            PlatformEvent::ImeCommit(text) => {
                if !text.is_empty() {
                    self.reset_caret_blink();
                    let active = self.active_focus.as_mut().unwrap();
                    active.state.insert_text(text);
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
            PlatformEvent::ImePreedit { .. } => KeyDispatchResult::RedrawOnly,
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
            ev.width = tb.width as f64;
            ev.height = tb.height as f64;
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

    /// Returns `true` when the active pointer capture target or element at `(px, py)` belongs to
    /// a retained `<Canvas3D>` viewport tracked by this runtime.
    pub fn target_is_retained_canvas3d(&self, px: f32, py: f32) -> bool {
        if self.tracked_retained_3d.is_empty() {
            return false;
        }
        let path_is_canvas = |path: &[usize]| -> bool {
            let mut current = &self.root_blueprint;
            if let NodeBlueprint::Element { tag, .. } = current {
                if matches!(tag, Tag::Canvas | Tag::Custom("Canvas3D")) {
                    return true;
                }
            }
            for &idx in path {
                let mut flat = Vec::new();
                match current {
                    NodeBlueprint::Element { children, .. }
                    | NodeBlueprint::Fragment(children) => {
                        for child in children {
                            Self::flatten_blueprints(child, &mut flat);
                        }
                    }
                    _ => return false,
                }
                if idx >= flat.len() {
                    return false;
                }
                current = flat[idx];
                if let NodeBlueprint::Element { tag, .. } = current {
                    if matches!(tag, Tag::Canvas | Tag::Custom("Canvas3D")) {
                        return true;
                    }
                }
            }
            false
        };
        if let Some((ref cap_path, _)) = self.pointer_capture_target {
            if path_is_canvas(cap_path) {
                return true;
            }
        }
        Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)
            .as_deref()
            .is_some_and(path_is_canvas)
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
        let is_drag = region.as_deref() == Some("drag")
            || self
                .csd_controller
                .hit_test(self.viewport_width, self.viewport_height, px, py)
                == strata_platform::CsdHitZone::Titlebar;

        if !is_drag {
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
                #[cfg(not(target_os = "linux"))]
                {
                    return true;
                }
            }
        }
        false
    }

    /// Finds the deepest ancestor path at `(px, py)` that is draggable (`draggable="true"` or has an `ondragstart` listener).
    pub(crate) fn find_draggable_path_at(&self, px: f32, py: f32) -> Option<Vec<usize>> {
        let hit_path = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)?;
        for len in (0..=hit_path.len()).rev() {
            let prefix = &hit_path[..len];
            let Some(NodeBlueprint::Element { attributes, .. }) =
                Self::find_blueprint_at_path(&self.root_blueprint, prefix)
            else {
                continue;
            };
            let mut explicit_draggable: Option<bool> = None;
            let mut has_drag_listener = false;
            for attr in attributes {
                match attr {
                    strata_dom::Attribute::Property { key, value } if key == "draggable" => {
                        explicit_draggable = Some(!value.eq_ignore_ascii_case("false"));
                    }
                    strata_dom::Attribute::EventListener { event, .. }
                        if matches!(
                            event,
                            strata_dom::EventType::DragStart | strata_dom::EventType::Drag
                        ) =>
                    {
                        has_drag_listener = true;
                    }
                    _ => {}
                }
            }
            if explicit_draggable == Some(true)
                || (explicit_draggable.is_none() && has_drag_listener)
            {
                return Some(prefix.to_vec());
            }
        }
        None
    }

    /// Resolves the drop-target element path at `(px, py)`, preferring the nearest ancestor registering
    /// a `DragEnter`, `DragOver`, `DragLeave`, or `Drop` listener so moving across child nodes inside a drop zone
    /// does not emit spurious `dragleave` / `dragenter` transitions.
    pub(crate) fn find_drop_target_path_at(&self, px: f32, py: f32) -> Option<Vec<usize>> {
        let hit_path = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)?;
        for len in (0..=hit_path.len()).rev() {
            let prefix = &hit_path[..len];
            let Some(NodeBlueprint::Element { attributes, .. }) =
                Self::find_blueprint_at_path(&self.root_blueprint, prefix)
            else {
                continue;
            };
            let is_drop_zone = attributes.iter().any(|attr| {
                matches!(
                    attr,
                    strata_dom::Attribute::EventListener {
                        event: strata_dom::EventType::DragEnter
                            | strata_dom::EventType::DragOver
                            | strata_dom::EventType::DragLeave
                            | strata_dom::EventType::Drop,
                        ..
                    }
                )
            });
            if is_drop_zone {
                return Some(prefix.to_vec());
            }
        }
        Some(hit_path)
    }

    /// Dispatches a drag-and-drop `EventType` (`DragStart`, `Drag`, `DragEnter`, `DragOver`, `DragLeave`, `Drop`, `DragEnd`)
    /// at or above `path`, returning `(listener_invoked, default_prevented)`.
    pub(crate) fn dispatch_drag_event_at_path(
        &self,
        path: &[usize],
        event_type: strata_dom::EventType,
        px: f32,
        py: f32,
        data_transfer: &std::rc::Rc<
            std::cell::RefCell<std::collections::HashMap<String, String>>,
        >,
    ) -> (bool, bool) {
        if let Some((handler, target_box, _)) = Self::find_event_at_or_above_path(
            &self.root_blueprint,
            &self.layout_root,
            path,
            event_type,
        ) {
            let target_box_copy = *target_box;
            let mut ev =
                self.create_mouse_event(event_type, px, py, 0, 1, Some(&target_box_copy));
            ev.data_transfer = std::rc::Rc::clone(data_transfer);
            ev.data = data_transfer
                .borrow()
                .get("text/plain")
                .cloned()
                .or_else(|| data_transfer.borrow().values().next().cloned());
            let prevented_cell = ev.default_prevented.clone();
            handler(ev);
            return (true, prevented_cell.get());
        }
        (false, false)
    }

    pub(crate) fn find_first_event_listener<'a>(
        blueprint: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        event_type: strata_dom::EventType,
    ) -> Option<(
        std::rc::Rc<dyn Fn(strata_dom::Event)>,
        &'a strata_layout::Rect,
    )> {
        if let NodeBlueprint::Element { attributes, .. } = blueprint {
            for attr in attributes {
                if let strata_dom::Attribute::EventListener { event, handler } = attr {
                    if *event == event_type {
                        return Some((handler.clone(), &layout.geometry.border_box));
                    }
                }
            }
        }
        for (_idx, child_bp, child_layout) in Self::paired_blueprint_children(blueprint, layout) {
            if let Some(found) =
                Self::find_first_event_listener(child_bp, child_layout, event_type)
            {
                return Some(found);
            }
        }
        None
    }

    /// Dispatches an OS file drop (`PlatformEvent::FileDrop`) to the drop target under `last_mouse_pos`
    /// (or the primary `ondrop` listener in the tree), populating `event.files` and `event.data`.
    pub fn dispatch_file_drop(&mut self, paths: Vec<std::path::PathBuf>) -> bool {
        if paths.is_empty() {
            return false;
        }
        let (px, py) = self.last_mouse_pos;
        let hit = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::Drop,
        )
        .or_else(|| {
            Self::find_first_event_listener(
                &self.root_blueprint,
                &self.layout_root,
                strata_dom::EventType::Drop,
            )
        });

        if let Some((handler, target_box)) = hit {
            let target_box_copy = *target_box;
            let mut ev = self.create_mouse_event(
                strata_dom::EventType::Drop,
                px,
                py,
                0,
                1,
                Some(&target_box_copy),
            );
            if let Some(first) = paths.first() {
                let first_str = first.to_string_lossy().into_owned();
                ev.data = Some(first_str.clone());
                ev.set_data("text/uri-list", first_str.clone());
                ev.set_data("text/plain", first_str);
            }
            ev.files = paths;
            handler(ev);
            return true;
        }
        false
    }
}
