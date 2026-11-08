//! Double-Buffered Terminal Screen with Delta ANSI Flushing and Synchronized Updates.

use crate::types::{AttrBits, CharCell, Color24};

/// Double-buffered terminal display surface.
pub struct TerminalScreen {
    cols: u16,
    rows: u16,
    front: Vec<CharCell>,
    back: Vec<CharCell>,
    dirty_lines: Vec<bool>,
}

impl TerminalScreen {
    /// Creates a new terminal screen with specified column and row dimensions.
    pub fn new(cols: u16, rows: u16) -> Self {
        let size = (cols as usize) * (rows as usize);
        Self {
            cols,
            rows,
            front: vec![CharCell::default(); size],
            back: vec![CharCell::default(); size],
            dirty_lines: vec![true; rows as usize],
        }
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Resizes the screen, clearing buffers and marking all lines dirty.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        let size = (cols as usize) * (rows as usize);
        self.front = vec![CharCell::default(); size];
        self.back = vec![CharCell::default(); size];
        self.dirty_lines = vec![true; rows as usize];
    }

    /// Clears the front buffer to spaces and marks all lines dirty.
    pub fn clear(&mut self) {
        for cell in &mut self.front {
            *cell = CharCell::default();
        }
        self.dirty_lines.fill(true);
    }

    /// Writes a single character to the front buffer with z-index ordering.
    pub fn put_char(
        &mut self,
        col: u16,
        row: u16,
        ch: char,
        fg: Color24,
        bg: Color24,
        style: u8,
        z_index: i16,
    ) {
        if col >= self.cols || row >= self.rows {
            return;
        }

        let idx = (row as usize) * (self.cols as usize) + (col as usize);
        let curr = &self.front[idx];

        if z_index >= curr.z_index {
            self.front[idx] = CharCell {
                ch,
                fg,
                bg,
                style,
                is_wide_continuation: false,
                z_index,
            };
            self.dirty_lines[row as usize] = true;
        }
    }

    /// Writes a text string horizontally starting at (col, row).
    pub fn put_str(
        &mut self,
        mut col: u16,
        row: u16,
        text: &str,
        fg: Color24,
        bg: Color24,
        style: u8,
        z_index: i16,
    ) {
        for ch in text.chars() {
            if col >= self.cols {
                break;
            }
            self.put_char(col, row, ch, fg, bg, style, z_index);
            col += 1;
        }
    }

    /// Retrieves an immutable reference to a cell in the front buffer.
    pub fn cell(&self, col: u16, row: u16) -> Option<&CharCell> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        let idx = (row as usize) * (self.cols as usize) + (col as usize);
        self.front.get(idx)
    }

    /// Flushes dirty lines, rendering them to an ANSI escape string and syncing back buffers.
    pub fn flush(&mut self) -> String {
        self.render_delta_ansi()
    }

    /// Renders minimal delta ANSI escape sequences comparing `front` to `back`.
    /// Emits synchronized frame protocols `\x1b[?2025h` (start) and `\x1b[?2025l` (end).
    pub fn render_delta_ansi(&mut self) -> String {
        let mut out = String::with_capacity(4096);

        // Begin synchronized update (DEC Mode 2025)
        out.push_str("\x1b[?2025h");

        let mut current_fg = Color24::Default;
        let mut current_bg = Color24::Default;
        let mut current_style = AttrBits::NONE;
        let mut cursor_row = u16::MAX;
        let mut cursor_col = u16::MAX;

        for r in 0..self.rows {
            if !self.dirty_lines[r as usize] {
                continue;
            }

            let row_start = (r as usize) * (self.cols as usize);

            for c in 0..self.cols {
                let idx = row_start + (c as usize);
                let f_cell = self.front[idx];
                let b_cell = self.back[idx];

                if f_cell == b_cell {
                    continue;
                }

                // Move cursor if discontinuous
                if cursor_row != r || cursor_col != c {
                    out.push_str(&format!("\x1b[{};{}H", r + 1, c + 1));
                    cursor_row = r;
                    cursor_col = c;
                }

                // Update text attributes if changed
                if f_cell.style != current_style {
                    out.push_str("\x1b[0m"); // Reset all
                    current_style = f_cell.style;
                    if (current_style & AttrBits::BOLD) != 0 {
                        out.push_str("\x1b[1m");
                    }
                    if (current_style & AttrBits::DIM) != 0 {
                        out.push_str("\x1b[2m");
                    }
                    if (current_style & AttrBits::ITALIC) != 0 {
                        out.push_str("\x1b[3m");
                    }
                    if (current_style & AttrBits::UNDERLINE) != 0 {
                        out.push_str("\x1b[4m");
                    }
                    if (current_style & AttrBits::REVERSE) != 0 {
                        out.push_str("\x1b[7m");
                    }
                    if (current_style & AttrBits::STRIKETHROUGH) != 0 {
                        out.push_str("\x1b[9m");
                    }
                    // Color state must be re-applied after \x1b[0m reset
                    current_fg = Color24::Default;
                    current_bg = Color24::Default;
                }

                // Update colors if changed
                if f_cell.fg != current_fg {
                    f_cell.fg.write_fg_ansi(&mut out);
                    current_fg = f_cell.fg;
                }
                if f_cell.bg != current_bg {
                    f_cell.bg.write_bg_ansi(&mut out);
                    current_bg = f_cell.bg;
                }

                // Write character
                out.push(f_cell.ch);
                cursor_col += 1;

                // Sync back buffer cell
                self.back[idx] = f_cell;
            }

            self.dirty_lines[r as usize] = false;
        }

        // Reset formatting and end synchronized update
        out.push_str("\x1b[0m\x1b[?2025l");
        out
    }

    /// Forces a full frame redraw by marking all lines dirty and resetting the back buffer.
    pub fn force_full_redraw(&mut self) {
        self.back.fill(CharCell {
                ch: '\0',
                fg: Color24::Default,
                bg: Color24::Default,
                style: 255,
                is_wide_continuation: false,
                z_index: -1,
            });
        self.dirty_lines.fill(true);
    }
}
