//! Text input layout synchronization, caret offset calculation, and caret rendering.

use super::*;

#[derive(Debug, Clone)]
pub(crate) struct VisualTextLine {
    pub start_char: usize,
    pub end_char: usize,
    pub next_start: usize,
    pub y: f32,
    pub ends_hard: bool,
}

impl AppRuntime {
    /// Resets the caret blink cycle so the insertion caret is immediately solid and visible.
    #[inline]
    pub fn reset_caret_blink(&mut self) {
        self.caret_blink_epoch = self.now();
        self.last_caret_blink_state = true;
    }

    /// Polls caret blink cycle at the current runtime clock and returns true if blink state toggled.
    #[inline]
    pub fn poll_caret_blink(&mut self) -> bool {
        let now = self.now();
        self.poll_caret_blink_at(now)
    }

    /// Polls caret blink cycle at an explicit monotonic `Instant` and returns true if blink state toggled.
    pub fn poll_caret_blink_at(&mut self, now: std::time::Instant) -> bool {
        if self.active_focus.is_none() {
            return false;
        }
        let elapsed_ms = now.saturating_duration_since(self.caret_blink_epoch).as_millis();
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

    pub(crate) fn find_layout_for_focus<'a>(
        root_blueprint: &NodeBlueprint,
        layout_root: &'a LayoutNode,
        target_idx: usize,
        target_id: Option<&str>,
        entity_id: Option<strata_primitives::EntityId>,
    ) -> Option<&'a LayoutNode> {
        if let Some(eid) = entity_id {
            if let Some(found) = layout_root.find_by_entity_id(eid) {
                return Some(found);
            }
        }
        let mut current_idx = 0;
        Self::find_input_layout(
            root_blueprint,
            layout_root,
            target_idx,
            target_id,
            &mut current_idx,
        )
    }

    pub(crate) fn find_layout_for_focus_mut<'a>(
        root_blueprint: &NodeBlueprint,
        layout_root: &'a mut LayoutNode,
        target_idx: usize,
        target_id: Option<&str>,
        entity_id: Option<strata_primitives::EntityId>,
    ) -> Option<&'a mut LayoutNode> {
        if let Some(eid) = entity_id {
            if layout_root.find_by_entity_id(eid).is_some() {
                return layout_root.find_by_entity_id_mut(eid);
            }
        }
        let mut current_idx = 0;
        Self::find_input_layout_mut(
            root_blueprint,
            layout_root,
            target_idx,
            target_id,
            &mut current_idx,
        )
    }

    pub(crate) fn find_layout_path_for_focus(
        root_blueprint: &NodeBlueprint,
        layout_root: &LayoutNode,
        target_idx: usize,
        target_id: Option<&str>,
        entity_id: Option<strata_primitives::EntityId>,
    ) -> Option<Vec<usize>> {
        fn find_by_entity_path(
            node: &LayoutNode,
            eid: strata_primitives::EntityId,
            path: &mut Vec<usize>,
            depth: usize,
        ) -> bool {
            if depth >= 256 {
                return false;
            }
            if node.entity_id == eid {
                return true;
            }
            for (i, child) in node.children.iter().enumerate() {
                path.push(i);
                if find_by_entity_path(child, eid, path, depth + 1) {
                    return true;
                }
                path.pop();
            }
            false
        }

        if let Some(eid) = entity_id {
            let mut path = Vec::new();
            if find_by_entity_path(layout_root, eid, &mut path, 0) {
                return Some(path);
            }
        }

        fn find_by_bp_path(
            blueprint: &NodeBlueprint,
            layout: &LayoutNode,
            target_idx: usize,
            target_id: Option<&str>,
            counter: &mut usize,
            path: &mut Vec<usize>,
            depth: usize,
        ) -> bool {
            if depth >= 256 {
                return false;
            }
            match blueprint {
                NodeBlueprint::Element {
                    tag,
                    attributes,
                    children,
                    ..
                } => {
                    if matches!(tag, Tag::Input | Tag::Textarea) {
                        let this_idx = *counter;
                        *counter += 1;
                        if AppRuntime::input_blueprint_matches_target(
                            attributes,
                            this_idx,
                            target_idx,
                            target_id,
                        ) {
                            return true;
                        }
                    }
                    let mut flat = Vec::new();
                    for c in children {
                        AppRuntime::flatten_blueprints(c, &mut flat);
                    }
                    for (i, (&child_bp, child_layout)) in
                        flat.iter().zip(layout.children.iter()).enumerate()
                    {
                        path.push(i);
                        if find_by_bp_path(
                            child_bp,
                            child_layout,
                            target_idx,
                            target_id,
                            counter,
                            path,
                            depth + 1,
                        ) {
                            return true;
                        }
                        path.pop();
                    }
                    false
                }
                NodeBlueprint::Fragment(children) => {
                    let mut flat = Vec::new();
                    for c in children {
                        AppRuntime::flatten_blueprints(c, &mut flat);
                    }
                    for (i, (&child_bp, child_layout)) in
                        flat.iter().zip(layout.children.iter()).enumerate()
                    {
                        path.push(i);
                        if find_by_bp_path(
                            child_bp,
                            child_layout,
                            target_idx,
                            target_id,
                            counter,
                            path,
                            depth + 1,
                        ) {
                            return true;
                        }
                        path.pop();
                    }
                    false
                }
                _ => false,
            }
        }

        let mut counter = 0;
        let mut path = Vec::new();
        if find_by_bp_path(
            root_blueprint,
            layout_root,
            target_idx,
            target_id,
            &mut counter,
            &mut path,
            0,
        ) {
            Some(path)
        } else {
            None
        }
    }

    /// Computes vertical distance between visual lines for multiline inputs/textareas,
    /// matching [`strata_render::compositor::effective_draw_text_line_height`] used by
    /// Pango Cairo, CPU rasterizer, and GPU text renderers.
    pub(crate) fn visual_line_step(
        font_size: f32,
        line_height: f32,
        _is_mono: bool,
        _font_family: strata_primitives::AtomId,
    ) -> f32 {
        strata_render::compositor::effective_draw_text_line_height(font_size, line_height).max(1.0)
    }

    #[inline]
    fn measure_char_slice_width(
        chars: &[char],
        start: usize,
        end: usize,
        font_size: f32,
        line_height: f32,
        is_mono: bool,
        font_family: strata_primitives::AtomId,
    ) -> f32 {
        if start >= end {
            return 0.0;
        }
        let s: String = chars[start..end].iter().collect();
        strata_layout::text::measure_text_bounds_with_family(
            &s,
            font_size,
            line_height,
            None,
            is_mono,
            font_family,
        )
        .0
    }

    /// Breaks `text` into wrapped visual lines on hard newlines and `max_width` word/char boundaries.
    pub(crate) fn layout_visual_lines(
        text: &str,
        max_width: f32,
        font_size: f32,
        line_height: f32,
        is_mono: bool,
        font_family: strata_primitives::AtomId,
    ) -> (Vec<VisualTextLine>, f32) {
        let line_step = Self::visual_line_step(font_size, line_height, is_mono, font_family);
        let chars: Vec<char> = text.chars().collect();
        let total = chars.len();
        let mut lines = Vec::new();
        let mut seg_start = 0usize;

        loop {
            let mut seg_end = seg_start;
            while seg_end < total && chars[seg_end] != '\n' {
                seg_end += 1;
            }
            let has_newline = seg_end < total;

            if seg_start == seg_end {
                let y = lines.len() as f32 * line_step;
                lines.push(VisualTextLine {
                    start_char: seg_start,
                    end_char: seg_end,
                    next_start: seg_end + usize::from(has_newline),
                    y,
                    ends_hard: true,
                });
            } else {
                let mut cur = seg_start;
                while cur < seg_end {
                    let full_w = Self::measure_char_slice_width(
                        &chars,
                        cur,
                        seg_end,
                        font_size,
                        line_height,
                        is_mono,
                        font_family,
                    );
                    if max_width <= 0.0 || full_w <= max_width {
                        let y = lines.len() as f32 * line_step;
                        lines.push(VisualTextLine {
                            start_char: cur,
                            end_char: seg_end,
                            next_start: seg_end + usize::from(has_newline),
                            y,
                            ends_hard: true,
                        });
                        cur = seg_end;
                    } else {
                        let mut lo = cur + 1;
                        let mut hi = seg_end;
                        let mut best = cur + 1;
                        while lo <= hi {
                            let mid = lo + (hi - lo) / 2;
                            let w = Self::measure_char_slice_width(
                                &chars,
                                cur,
                                mid,
                                font_size,
                                line_height,
                                is_mono,
                                font_family,
                            );
                            if w <= max_width {
                                best = mid;
                                lo = mid + 1;
                            } else {
                                if mid == 0 {
                                    break;
                                }
                                hi = mid - 1;
                            }
                        }

                        let ws_pos = if best < seg_end {
                            (cur..best).rfind(|&i| matches!(chars[i], ' ' | '\t'))
                        } else {
                            None
                        };
                        let (content_end, next_cur) = if let Some(ws) = ws_pos {
                            if ws > cur {
                                (ws, ws + 1)
                            } else {
                                (cur, cur + 1)
                            }
                        } else {
                            (best, best)
                        };
                        let y = lines.len() as f32 * line_step;
                        lines.push(VisualTextLine {
                            start_char: cur,
                            end_char: content_end,
                            next_start: next_cur,
                            y,
                            ends_hard: false,
                        });
                        cur = next_cur;
                    }
                }
            }

            if !has_newline {
                break;
            }
            seg_start = seg_end + 1;
        }

        (lines, line_step)
    }

    pub(crate) fn visual_line_index_for_cursor(lines: &[VisualTextLine], cursor_idx: usize) -> usize {
        lines
            .iter()
            .position(|l| {
                if l.ends_hard {
                    cursor_idx >= l.start_char && cursor_idx <= l.end_char
                } else {
                    cursor_idx >= l.start_char && cursor_idx < l.next_start
                }
            })
            .unwrap_or_else(|| lines.len().saturating_sub(1))
    }

    pub(crate) fn visual_cursor_location(
        chars: &[char],
        lines: &[VisualTextLine],
        cursor_idx: usize,
        font_size: f32,
        line_height: f32,
        is_mono: bool,
        font_family: strata_primitives::AtomId,
    ) -> (usize, f32) {
        let line_idx = Self::visual_line_index_for_cursor(lines, cursor_idx);
        let line = &lines[line_idx];
        let clamped = cursor_idx.clamp(line.start_char, line.end_char);
        let x = Self::measure_char_slice_width(
            chars,
            line.start_char,
            clamped,
            font_size,
            line_height,
            is_mono,
            font_family,
        );
        (line_idx, x)
    }

    #[inline]
    pub(crate) fn compute_multiline_scroll_y(
        cursor_line_y: f32,
        line_step: f32,
        total_h: f32,
        viewport_h: f32,
        prev_scroll_y: f32,
    ) -> f32 {
        if viewport_h <= 0.0 || total_h <= viewport_h {
            return 0.0;
        }
        let max_scroll = (total_h - viewport_h).max(0.0);
        let mut next = prev_scroll_y.clamp(0.0, max_scroll);
        if cursor_line_y < next {
            next = cursor_line_y;
        } else if cursor_line_y + line_step > next + viewport_h {
            next = cursor_line_y + line_step - viewport_h;
        }
        next.clamp(0.0, max_scroll)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn cursor_index_at_input_point(
        text: &str,
        is_multiline: bool,
        px: f32,
        py: f32,
        content_x: f32,
        content_y: f32,
        content_w: f32,
        scroll_x: f32,
        scroll_y: f32,
        font_size: f32,
        line_height: f32,
        is_mono: bool,
        font_family: strata_primitives::AtomId,
    ) -> usize {
        if !is_multiline {
            let rel_x = if px < content_x {
                (scroll_x + (px - content_x)).max(0.0)
            } else if content_w > 0.0 && px > content_x + content_w {
                100000.0
            } else {
                (px - content_x + scroll_x).max(0.0)
            };
            return Self::calculate_exact_char_index_at_offset(
                text,
                rel_x,
                font_size,
                line_height,
                is_mono,
                font_family,
            );
        }

        let (lines, line_step) =
            Self::layout_visual_lines(text, content_w, font_size, line_height, is_mono, font_family);
        if lines.is_empty() {
            return 0;
        }
        let rel_y = (py - content_y + scroll_y).max(0.0);
        let target_row = ((rel_y / line_step).floor() as isize)
            .clamp(0, lines.len().saturating_sub(1) as isize) as usize;
        let line = &lines[target_row];
        let rel_x = if px < content_x {
            0.0
        } else if content_w > 0.0 && px > content_x + content_w {
            100000.0
        } else {
            (px - content_x).max(0.0)
        };
        let chars: Vec<char> = text.chars().collect();
        let line_slice: String = chars[line.start_char..line.end_char].iter().collect();
        let col = Self::calculate_exact_char_index_at_offset(
            &line_slice,
            rel_x,
            font_size,
            line_height,
            is_mono,
            font_family,
        );
        line.start_char + col
    }

    pub(crate) fn move_active_input_vertical(
        &mut self,
        delta_lines: isize,
        page_jump: bool,
        shift: bool,
    ) {
        let Some(ref active) = self.active_focus else {
            return;
        };
        let layout_opt = Self::find_layout_for_focus(
            &self.root_blueprint,
            &self.layout_root,
            active.input_index,
            active.input_id.as_deref(),
            active.entity_id,
        );
        let text = active.state.text.get();
        if let Some(layout) = layout_opt {
            let content_box = layout.geometry.content_box;
            let font_size = layout.style.font_size;
            let line_height = layout.style.line_height;
            let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
            let font_family = layout.style.font_family;
            let (lines, line_step) = Self::layout_visual_lines(
                &text,
                content_box.width,
                font_size,
                line_height,
                is_mono,
                font_family,
            );
            if !lines.is_empty() {
                let chars: Vec<char> = text.chars().collect();
                let cur_idx = active.state.cursor_index.get().min(chars.len());
                let (cur_line, caret_x) = Self::visual_cursor_location(
                    &chars,
                    &lines,
                    cur_idx,
                    font_size,
                    line_height,
                    is_mono,
                    font_family,
                );
                let step = if page_jump {
                    let visible_lines = (content_box.height / line_step).floor().max(1.0) as isize;
                    delta_lines.signum() * visible_lines
                } else {
                    delta_lines
                };
                let target_line =
                    (cur_line as isize + step).clamp(0, lines.len().saturating_sub(1) as isize)
                        as usize;
                let target = &lines[target_line];
                let line_slice: String = chars[target.start_char..target.end_char].iter().collect();
                let new_col = Self::calculate_exact_char_index_at_offset(
                    &line_slice,
                    caret_x,
                    font_size,
                    line_height,
                    is_mono,
                    font_family,
                );
                let new_idx = target.start_char + new_col;
                if shift {
                    if active.state.selection_anchor.get().is_none() {
                        active.state.selection_anchor.set(Some(cur_idx));
                    }
                } else {
                    active.state.selection_anchor.set(None);
                }
                active.state.cursor_index.set(new_idx);
                return;
            }
        }
        if delta_lines < 0 {
            active.state.move_up(shift);
        } else {
            active.state.move_down(shift);
        }
    }

    pub(crate) fn move_active_input_line_edge(&mut self, to_end: bool, shift: bool) {
        let Some(ref active) = self.active_focus else {
            return;
        };
        let layout_opt = Self::find_layout_for_focus(
            &self.root_blueprint,
            &self.layout_root,
            active.input_index,
            active.input_id.as_deref(),
            active.entity_id,
        );
        if let Some(layout) = layout_opt {
            let text = active.state.text.get();
            let content_w = layout.geometry.content_box.width;
            let font_size = layout.style.font_size;
            let line_height = layout.style.line_height;
            let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
            let font_family = layout.style.font_family;
            let (lines, _) = Self::layout_visual_lines(
                &text,
                content_w,
                font_size,
                line_height,
                is_mono,
                font_family,
            );
            if !lines.is_empty() {
                let total = text.chars().count();
                let cur_idx = active.state.cursor_index.get().min(total);
                let line_idx = Self::visual_line_index_for_cursor(&lines, cur_idx);
                let new_idx = if to_end {
                    lines[line_idx].end_char
                } else {
                    lines[line_idx].start_char
                };
                if shift {
                    if active.state.selection_anchor.get().is_none() {
                        active.state.selection_anchor.set(Some(cur_idx));
                    }
                } else {
                    active.state.selection_anchor.set(None);
                }
                active.state.cursor_index.set(new_idx);
                return;
            }
        }
        if to_end {
            active.state.move_line_end(shift);
        } else {
            active.state.move_line_home(shift);
        }
    }

    /// Synchronizes the active focused text input's live buffer, clamped scroll offset,
    /// and styling into the layout tree without snapping a multiline `<textarea>` back to
    /// the caret (preserving manual wheel/scrollbar scrolling across paints and view updates).
    pub fn sync_active_input_layout(&mut self) {
        self.sync_active_input_layout_inner(false);
    }

    /// Synchronizes the active focused text input's live buffer and styling into the layout
    /// tree and scrolls the caret into view (used on typing, edits, and caret navigation).
    pub(crate) fn sync_active_input_layout_and_scroll_caret(&mut self) {
        self.sync_active_input_layout_inner(true);
    }

    fn sync_active_input_layout_inner(&mut self, scroll_caret_into_view: bool) {
        let Some(ref active) = self.active_focus else {
            return;
        };
        let target_idx = active.input_index;
        let target_id = active.input_id.clone();
        let target_entity_id = active.entity_id;
        let is_multiline = active.is_multiline;
        let is_password =
            !is_multiline && self.input_is_password(target_idx, target_id.as_deref(), target_entity_id);
        let text_val = active.state.text.get();
        let cursor_idx = active.state.cursor_index.get();
        let prev_scroll_x = active.state.scroll_x.get();
        let prev_scroll_y = active.state.scroll_y.get();
        let focus_path = if is_multiline {
            Self::find_layout_path_for_focus(
                &self.root_blueprint,
                &self.layout_root,
                target_idx,
                target_id.as_deref(),
                target_entity_id,
            )
        } else {
            None
        };

        let layout_match = Self::find_layout_for_focus_mut(
            &self.root_blueprint,
            &mut self.layout_root,
            target_idx,
            target_id.as_deref(),
            target_entity_id,
        );

        let mut next_scroll_x = 0.0f32;
        let mut next_scroll_y = prev_scroll_y.max(0.0);
        let mut multiline_max_scroll: Option<f32> = None;
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
                if is_multiline {
                    let viewport_h = layout.geometry.content_box.height;
                    if viewport_h > 0.0 {
                        let (lines, line_step) = Self::layout_visual_lines(
                            &display_val,
                            layout.geometry.content_box.width,
                            layout.style.font_size,
                            layout.style.line_height,
                            is_mono,
                            layout.style.font_family,
                        );
                        let total_h = lines.len().max(1) as f32 * line_step;
                        let max_scroll = (total_h - viewport_h).max(0.0);
                        multiline_max_scroll = Some(max_scroll);
                        next_scroll_y = if scroll_caret_into_view {
                            let line_idx = Self::visual_line_index_for_cursor(&lines, cursor_idx);
                            let cursor_y = lines.get(line_idx).map(|l| l.y).unwrap_or(0.0);
                            Self::compute_multiline_scroll_y(
                                cursor_y,
                                line_step,
                                total_h,
                                viewport_h,
                                prev_scroll_y,
                            )
                        } else {
                            prev_scroll_y.clamp(0.0, max_scroll)
                        };
                    } else {
                        next_scroll_y = prev_scroll_y.max(0.0);
                    }
                    layout.geometry.scroll_offset.x = 0.0;
                    layout.geometry.scroll_offset.y = next_scroll_y;
                } else {
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
                    layout.geometry.content_box.height =
                        th.max(layout.geometry.content_box.height);
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
                    next_scroll_y = 0.0;
                    layout.geometry.scroll_offset.x = next_scroll_x;
                    layout.geometry.scroll_offset.y = 0.0;
                }
            } else {
                next_scroll_y = 0.0;
                if is_multiline && layout.geometry.content_box.height > 0.0 {
                    multiline_max_scroll = Some(0.0);
                }
                layout.geometry.scroll_offset.x = 0.0;
                layout.geometry.scroll_offset.y = 0.0;
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
        } else if !is_multiline {
            next_scroll_y = 0.0;
        }

        if let Some(ref active) = self.active_focus {
            active.state.scroll_x.set(next_scroll_x);
            active.state.scroll_y.set(next_scroll_y);
        }

        if is_multiline {
            if let (Some(path), Some(max_scroll)) = (focus_path, multiline_max_scroll) {
                if scroll_caret_into_view
                    && (next_scroll_y - prev_scroll_y).abs() > 0.01
                    && self
                        .active_smooth_scroll
                        .as_ref()
                        .is_some_and(|t| t.container_path == path)
                {
                    self.active_smooth_scroll = None;
                }
                if let Some(ref id) = target_id {
                    if !id.is_empty() {
                        self.scroll_offsets_by_id
                            .insert(id.clone(), (0.0, next_scroll_y));
                        strata_dom::hooks::set_live_scroll_offset(id, 0.0, next_scroll_y);
                    }
                }
                self.scroll_offsets.insert(path.clone(), (0.0, next_scroll_y));
                if next_scroll_y > 0.0 {
                    self.applied_scroll_offsets
                        .insert(path.clone(), (0.0, next_scroll_y));
                } else {
                    self.applied_scroll_offsets.remove(&path);
                }

                // Keep the `<textarea>`'s interactive scrollbar synchronized when edits change
                // content height or caret navigation scrolls the textarea.
                let bar_idx = self.active_scrollbars.iter().position(|b| {
                    b.path == path
                        || target_id
                            .as_deref()
                            .is_some_and(|id| !id.is_empty() && b.is_id && b.id == id)
                });
                let needs_rebuild = match bar_idx {
                    Some(idx) => {
                        let bar = &mut self.active_scrollbars[idx];
                        if (bar.max_scroll_y - max_scroll).abs() > 0.5 {
                            true
                        } else {
                            let clamped = next_scroll_y.clamp(0.0, bar.max_scroll_y);
                            bar.scroll_y = clamped;
                            bar.thumb_bounds.y =
                                if bar.max_scroll_y > 0.0 && bar.available_travel > 0.0 {
                                    bar.track_bounds.y
                                        + (clamped / bar.max_scroll_y) * bar.available_travel
                                } else {
                                    bar.track_bounds.y
                                };
                            false
                        }
                    }
                    None => max_scroll > 0.0,
                };
                if needs_rebuild {
                    self.rebuild_active_scrollbars();
                }
            }
        }
        self.update_gtk_ime_location();
    }

    /// Calculates the exact character index from an x offset within a single line using active font metrics and character advances.
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

    /// Computes caret rectangle and optional scroll update / selection rects for the active input.
    fn compute_active_caret_and_selection(
        &self,
        active: &ActiveInput,
        layout: &LayoutNode,
        include_selection: bool,
    ) -> Option<(strata_layout::Rect, Vec<strata_layout::Rect>, f32, f32)> {
        let content_box = layout.geometry.content_box;
        if content_box.width <= 0.0 || content_box.height <= 0.0 {
            return None;
        }

        let font_size = layout.style.font_size;
        let line_height = layout.style.line_height;
        let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
        let font_family = layout.style.font_family;
        let is_password = !active.is_multiline
            && self.input_is_password(active.input_index, active.input_id.as_deref(), active.entity_id);
        let raw_text = active.state.text.get();
        let text = if is_password {
            Self::mask_password_text(&raw_text)
        } else {
            raw_text
        };
        let chars: Vec<char> = text.chars().collect();
        let total_chars = chars.len();
        let cursor_idx = active.state.cursor_index.get().min(total_chars);
        let caret_w = 2.0f32;

        if active.is_multiline {
            let (lines, line_step) = Self::layout_visual_lines(
                &text,
                content_box.width,
                font_size,
                line_height,
                is_mono,
                font_family,
            );
            let (line_idx, caret_rel_x) = Self::visual_cursor_location(
                &chars,
                &lines,
                cursor_idx,
                font_size,
                line_height,
                is_mono,
                font_family,
            );
            let line_y = lines.get(line_idx).map(|l| l.y).unwrap_or(0.0);
            let total_h = lines.len().max(1) as f32 * line_step;
            let max_scroll = (total_h - content_box.height).max(0.0);
            let scroll_y = active.state.scroll_y.get().clamp(0.0, max_scroll);
            let caret_h = line_step.min(content_box.height).max(12.0);
            let caret_x = (content_box.x + caret_rel_x)
                .clamp(content_box.x, content_box.x + (content_box.width - caret_w).max(0.0));
            let caret_y = content_box.y + line_y - scroll_y;
            let caret_rect = strata_layout::Rect::new(caret_x, caret_y, caret_w, caret_h);

            let mut sel_rects = Vec::new();
            if include_selection {
                if let Some((start, end)) = active.state.selection_range() {
                    let s = start.min(total_chars);
                    let e = end.min(total_chars);
                    if s < e {
                        let sel_h = line_step.min(content_box.height);
                        for line in &lines {
                            let line_span_end = line.next_start.max(line.end_char);
                            if e <= line.start_char && !(s == e) {
                                continue;
                            }
                            if s >= line_span_end && !(s == line.start_char && line.start_char == line.end_char) {
                                continue;
                            }
                            let seg_start = s.max(line.start_char).min(line.end_char);
                            let seg_end = e.min(line.end_char).max(seg_start);
                            let start_x = Self::measure_char_slice_width(
                                &chars,
                                line.start_char,
                                seg_start,
                                font_size,
                                line_height,
                                is_mono,
                                font_family,
                            );
                            let mut sel_w = Self::measure_char_slice_width(
                                &chars,
                                seg_start,
                                seg_end,
                                font_size,
                                line_height,
                                is_mono,
                                font_family,
                            );
                            if e > line.end_char {
                                sel_w += font_size * 0.3;
                            }
                            if sel_w > 0.0 {
                                sel_rects.push(strata_layout::Rect::new(
                                    content_box.x + start_x,
                                    content_box.y + line.y - scroll_y,
                                    sel_w,
                                    sel_h,
                                ));
                            }
                        }
                    }
                }
            }

            return Some((caret_rect, sel_rects, 0.0, scroll_y));
        }

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

        let caret_h = (font_size * 1.2).min(content_box.height).max(14.0);
        let caret_y = content_box.y + ((content_box.height - caret_h) / 2.0).max(0.0);
        let max_caret_x = content_box.x + (content_box.width - caret_w).max(0.0);
        let caret_x =
            (content_box.x + align_offset + cursor_w - scroll_x).clamp(content_box.x, max_caret_x);
        let caret_rect = strata_layout::Rect::new(caret_x, caret_y, caret_w, caret_h);

        let mut sel_rects = Vec::new();
        if include_selection {
            if let Some((start, end)) = active.state.selection_range() {
                let start_clamped = start.min(total_chars);
                let end_clamped = end.min(total_chars);
                if start_clamped < end_clamped {
                    let start_w = Self::measure_char_slice_width(
                        &chars,
                        0,
                        start_clamped,
                        font_size,
                        line_height,
                        is_mono,
                        font_family,
                    );
                    let sel_w = Self::measure_char_slice_width(
                        &chars,
                        start_clamped,
                        end_clamped,
                        font_size,
                        line_height,
                        is_mono,
                        font_family,
                    );
                    let sel_h = (font_size * 1.25).min(content_box.height);
                    let sel_y = content_box.y + ((content_box.height - sel_h) / 2.0).max(0.0);
                    let sel_x = content_box.x + align_offset + start_w - scroll_x;
                    sel_rects.push(strata_layout::Rect::new(sel_x, sel_y, sel_w, sel_h));
                }
            }
        }

        Some((caret_rect, sel_rects, scroll_x, 0.0))
    }

    /// Renders the visible insertion caret cursor and selection highlight for the currently focused text input.
    pub fn render_active_input_caret(&mut self) {
        let Some(ref active) = self.active_focus else {
            return;
        };

        let Some(layout) = Self::find_layout_for_focus(
            &self.root_blueprint,
            &self.layout_root,
            active.input_index,
            active.input_id.as_deref(),
            active.entity_id,
        ) else {
            return;
        };

        let content_box = layout.geometry.content_box;
        let text_color = layout.style.color;
        let mut caret_color = layout.style.caret_color.unwrap_or(text_color);
        if caret_color.a == 0 {
            caret_color = strata_style::types::Color::new(236, 233, 225, 255);
        }

        let Some((caret_rect, sel_rects, scroll_x, _scroll_y)) =
            self.compute_active_caret_and_selection(active, layout, true)
        else {
            return;
        };

        if !active.is_multiline {
            active.state.scroll_x.set(scroll_x);
        }

        let has_sel = active.state.has_selection();
        let elapsed_ms = self.elapsed_since(self.caret_blink_epoch).as_millis();
        let is_blink_on = (elapsed_ms / 530).is_multiple_of(2);
        let draw_caret = !has_sel && is_blink_on;

        if sel_rects.is_empty() && !draw_caret {
            return;
        }

        self.display_list
            .push(strata_render::compositor::DrawCommand::PushClip {
                bounds: content_box,
                radius: strata_style::types::BorderRadius::all(0.0),
            });

        for sel_bounds in sel_rects {
            self.display_list
                .push(strata_render::compositor::DrawCommand::DrawRect {
                    bounds: sel_bounds,
                    color: strata_style::types::Color::rgba(59, 130, 246, 120),
                });
        }

        if draw_caret {
            self.display_list
                .push(strata_render::compositor::DrawCommand::DrawRect {
                    bounds: caret_rect,
                    color: caret_color,
                });
        }

        self.display_list
            .push(strata_render::compositor::DrawCommand::PopClip);
    }

    /// Computes the exact bounding box of the active text caret if an input is currently focused.
    pub fn active_caret_rect(&self) -> Option<strata_layout::Rect> {
        let active = self.active_focus.as_ref()?;
        let layout = Self::find_layout_for_focus(
            &self.root_blueprint,
            &self.layout_root,
            active.input_index,
            active.input_id.as_deref(),
            active.entity_id,
        )?;
        self.compute_active_caret_and_selection(active, layout, false)
            .map(|(caret_rect, _, _, _)| caret_rect)
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
