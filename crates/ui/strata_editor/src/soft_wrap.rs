//! Soft Word-Wrapping and Visual-to-Logical Coordinate Translation for Strata Editor.

use crate::cursor::Position;

/// A visually wrapped slice of a logical line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualLine {
    /// 0-indexed logical line index in the underlying buffer.
    pub logical_line: usize,
    /// 0-indexed subline index within this logical line.
    pub visual_subline: usize,
    /// The wrapped display text for this visual line.
    pub text: String,
    /// Character offset start within the logical line (inclusive).
    pub char_start: usize,
    /// Character offset end within the logical line (exclusive).
    pub char_end: usize,
    /// Whether this visual line is a continuation line from soft wrapping.
    pub is_continuation: bool,
}

/// Computes visual soft-wrapped lines for a single logical line.
///
/// Features:
/// - Wraps at whitespace / word boundaries when possible.
/// - Hard breaks long words when they exceed `max_columns`.
/// - Preserves leading indentation on continuation lines.
pub fn wrap_logical_line(
    line: &str,
    logical_line_idx: usize,
    max_columns: usize,
    preserve_indent: bool,
) -> Vec<VisualLine> {
    if max_columns == 0 {
        return vec![VisualLine {
            logical_line: logical_line_idx,
            visual_subline: 0,
            text: line.to_string(),
            char_start: 0,
            char_end: line.chars().count(),
            is_continuation: false,
        }];
    }

    let chars: Vec<char> = line.chars().collect();
    let total_chars = chars.len();

    if total_chars <= max_columns {
        return vec![VisualLine {
            logical_line: logical_line_idx,
            visual_subline: 0,
            text: line.to_string(),
            char_start: 0,
            char_end: total_chars,
            is_continuation: false,
        }];
    }

    // Determine leading indentation
    let indent_len = chars.iter().take_while(|&&c| c == ' ' || c == '\t').count();
    let indent_str: String = if preserve_indent && indent_len > 0 && indent_len < max_columns {
        chars[..indent_len].iter().collect()
    } else {
        String::new()
    };

    let mut visual_lines = Vec::new();
    let mut current_start = 0;
    let mut subline_idx = 0;

    while current_start < total_chars {
        let is_continuation = subline_idx > 0;
        let effective_indent = if is_continuation { &indent_str } else { "" };
        let available_cols = max_columns.saturating_sub(effective_indent.chars().count()).max(1);

        let remaining = total_chars - current_start;
        if remaining <= available_cols {
            let slice: String = chars[current_start..total_chars].iter().collect();
            let text = format!("{}{}", effective_indent, slice);
            visual_lines.push(VisualLine {
                logical_line: logical_line_idx,
                visual_subline: subline_idx,
                text,
                char_start: current_start,
                char_end: total_chars,
                is_continuation,
            });
            break;
        }

        // Look for the last break opportunity (whitespace) within available_cols
        let window_end = current_start + available_cols;
        let mut break_point = None;
        for i in (current_start + 1..=window_end).rev() {
            if chars[i - 1].is_whitespace() {
                break_point = Some(i);
                break;
            }
        }

        let actual_end = match break_point {
            Some(bp) if bp > current_start => bp,
            _ => window_end, // Hard break if no whitespace found
        };

        let slice: String = chars[current_start..actual_end].iter().collect();
        let text = format!("{}{}", effective_indent, slice);
        visual_lines.push(VisualLine {
            logical_line: logical_line_idx,
            visual_subline: subline_idx,
            text,
            char_start: current_start,
            char_end: actual_end,
            is_continuation,
        });

        current_start = actual_end;
        subline_idx += 1;
    }

    visual_lines
}

/// Translates a logical document position `(line, col)` to visual coordinates `(visual_line_idx, visual_col)`.
pub fn logical_to_visual(
    pos: Position,
    all_visual_lines: &[VisualLine],
) -> Option<(usize, usize)> {
    for (v_idx, vl) in all_visual_lines.iter().enumerate() {
        if vl.logical_line == pos.line && pos.col >= vl.char_start && (pos.col < vl.char_end || (pos.col == vl.char_end && vl.visual_subline == all_visual_lines.iter().filter(|l| l.logical_line == pos.line).count().saturating_sub(1))) {
            let col_in_segment = pos.col.saturating_sub(vl.char_start);
            let visual_col = if vl.is_continuation {
                vl.text.chars().count().saturating_sub(vl.char_end - vl.char_start) + col_in_segment
            } else {
                col_in_segment
            };
            return Some((v_idx, visual_col));
        }
    }
    None
}

/// Translates visual coordinates `(visual_line_idx, visual_col)` back to logical document `Position`.
pub fn visual_to_logical(
    visual_line_idx: usize,
    visual_col: usize,
    all_visual_lines: &[VisualLine],
) -> Option<Position> {
    let vl = all_visual_lines.get(visual_line_idx)?;
    let indent_chars = if vl.is_continuation {
        vl.text.chars().count().saturating_sub(vl.char_end - vl.char_start)
    } else {
        0
    };
    let content_col = visual_col.saturating_sub(indent_chars);
    let logical_col = (vl.char_start + content_col).min(vl.char_end);
    Some(Position::new(vl.logical_line, logical_col))
}
