//! Virtualized Line Buffer with O(1) line indexing and fine-grained line mutations.

/// Fast, indexed line-based text buffer optimized for code and document editing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineBuffer {
    lines: Vec<String>,
}

impl LineBuffer {
    /// Creates an empty buffer containing a single empty line.
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
        }
    }

    /// Constructs a LineBuffer by splitting an input string along linebreaks (`\n` or `\r\n`).
    pub fn from_str(text: &str) -> Self {
        let lines: Vec<String> = text
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();

        if lines.is_empty() {
            Self::new()
        } else {
            Self { lines }
        }
    }

    /// Total number of lines in the buffer.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Returns a reference to the line at the specified 0-based index.
    pub fn line(&self, idx: usize) -> Option<&str> {
        self.lines.get(idx).map(|s| s.as_str())
    }

    /// Returns the character length of the line at the specified index.
    pub fn line_len(&self, idx: usize) -> usize {
        self.lines.get(idx).map_or(0, |l| l.len())
    }

    /// Inserts a character at the specified line and column.
    pub fn insert_char(&mut self, line: usize, col: usize, ch: char) {
        if ch == '\n' {
            self.split_line(line, col);
            return;
        }

        if let Some(l) = self.lines.get_mut(line) {
            let clamped_col = col.min(l.len());
            l.insert(clamped_col, ch);
        }
    }

    /// Inserts a string at the specified line and column, automatically handling multiline breaks.
    pub fn insert_str(&mut self, line: usize, col: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        if !text.contains('\n') {
            if let Some(l) = self.lines.get_mut(line) {
                let clamped_col = col.min(l.len());
                l.insert_str(clamped_col, text);
            }
            return;
        }

        let parts: Vec<&str> = text.split('\n').collect();
        if let Some(l) = self.lines.get_mut(line) {
            let clamped_col = col.min(l.len());
            let remainder = l[clamped_col..].to_string();
            l.truncate(clamped_col);
            l.push_str(parts[0]);

            let mut insert_idx = line + 1;
            for part in &parts[1..parts.len() - 1] {
                self.lines.insert(insert_idx, part.to_string());
                insert_idx += 1;
            }

            let mut last_line = parts[parts.len() - 1].to_string();
            last_line.push_str(&remainder);
            self.lines.insert(insert_idx, last_line);
        }
    }

    /// Splits a line into two lines at the given column index.
    pub fn split_line(&mut self, line: usize, col: usize) {
        if line >= self.lines.len() {
            return;
        }

        let right = {
            let left = &mut self.lines[line];
            let clamped_col = col.min(left.len());
            let right = left[clamped_col..].to_string();
            left.truncate(clamped_col);
            right
        };

        self.lines.insert(line + 1, right);
    }

    /// Joins the specified line with the subsequent line.
    pub fn join_line_with_next(&mut self, line: usize) {
        if line + 1 >= self.lines.len() {
            return;
        }

        let next_line = self.lines.remove(line + 1);
        self.lines[line].push_str(&next_line);
    }

    /// Deletes a character before the specified position (backspace).
    pub fn delete_backwards(&mut self, line: usize, col: usize) -> Option<(usize, usize)> {
        if col > 0 {
            if let Some(l) = self.lines.get_mut(line) {
                let clamped_col = col.min(l.len());
                l.remove(clamped_col - 1);
                return Some((line, clamped_col - 1));
            }
        } else if line > 0 {
            let prev_line_len = self.line_len(line - 1);
            self.join_line_with_next(line - 1);
            return Some((line - 1, prev_line_len));
        }
        None
    }

    /// Deletes text within the given bounding range and returns the deleted substring.
    pub fn delete_range(
        &mut self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        if start_line > end_line || (start_line == end_line && start_col >= end_col) {
            return String::new();
        }

        if start_line == end_line {
            if let Some(line) = self.lines.get_mut(start_line) {
                let sc = start_col.min(line.len());
                let ec = end_col.min(line.len());
                let deleted = line[sc..ec].to_string();
                line.replace_range(sc..ec, "");
                return deleted;
            }
            return String::new();
        }

        let mut deleted = String::new();

        // 1. First line
        let remainder = if let Some(first) = self.lines.get_mut(start_line) {
            let sc = start_col.min(first.len());
            deleted.push_str(&first[sc..]);
            deleted.push('\n');
            first.truncate(sc);
            true
        } else {
            false
        };

        // 2. Middle lines
        let remove_count = end_line - start_line - 1;
        for _ in 0..remove_count {
            if start_line + 1 < self.lines.len() {
                let mid = self.lines.remove(start_line + 1);
                deleted.push_str(&mid);
                deleted.push('\n');
            }
        }

        // 3. Last line
        if remainder && start_line + 1 < self.lines.len() {
            let last = self.lines.remove(start_line + 1);
            let ec = end_col.min(last.len());
            deleted.push_str(&last[..ec]);
            self.lines[start_line].push_str(&last[ec..]);
        }

        deleted
    }

    /// Returns a slice of lines for virtual viewport rendering.
    pub fn slice_lines(&self, start: usize, count: usize) -> Vec<(usize, &str)> {
        let end = (start + count).min(self.lines.len());
        (start..end)
            .map(|idx| (idx, self.lines[idx].as_str()))
            .collect()
    }

    /// Serializes entire buffer back into a single newline-separated String.
    pub fn to_string(&self) -> String {
        self.lines.join("\n")
    }
}
