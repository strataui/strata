//! High-performance reactive text input editing engine with full cursor navigation,
//! range selection, word jumping, clipboard integration, and undo/redo history.

use crate::reactive::Signal;

/// Input navigation key representation for text editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKey {
    Char(char),
    Left,
    Right,
    Home,
    End,
    Backspace,
    Delete,
    Enter,
    Escape,
    A,
    C,
    V,
    X,
    Z,
    Y,
    Other,
}

/// Modifier keys active during text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// Result of an input key handling operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKeyResult {
    /// No change occurred.
    Ignored,
    /// Text or cursor changed; UI repaint recommended.
    Handled,
    /// Selected text to be copied to system clipboard.
    CopyToClipboard(String),
    /// Enter was pressed (submit/commit).
    Submitted,
}

/// Reactive state model for a rich text input field.
#[derive(Clone)]
pub struct TextInputState {
    pub text: Signal<String>,
    pub cursor_index: Signal<usize>,
    pub selection_anchor: Signal<Option<usize>>,
    pub is_focused: Signal<bool>,
    undo_stack: Vec<(String, usize)>,
    redo_stack: Vec<(String, usize)>,
}

impl TextInputState {
    /// Creates a new `TextInputState` initialized with default empty text.
    pub fn new() -> Self {
        Self::with_text("")
    }

    /// Creates a new `TextInputState` with initial text and cursor placed at end.
    pub fn with_text(initial: &str) -> Self {
        let text = Signal::new(initial.to_string());
        let cursor_index = Signal::new(initial.chars().count());
        let selection_anchor = Signal::new(None);
        let is_focused = Signal::new(false);

        Self {
            text,
            cursor_index,
            selection_anchor,
            is_focused,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Returns the active selection character range as `(start, end)` where `start <= end`.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.selection_anchor.get() {
            let cursor = self.cursor_index.get();
            if anchor != cursor {
                return Some((anchor.min(cursor), anchor.max(cursor)));
            }
        }
        None
    }

    /// Checks if there is an active non-empty text selection.
    pub fn has_selection(&self) -> bool {
        self.selection_range().is_some()
    }

    /// Returns the currently selected substring, if any.
    pub fn selected_text(&self) -> Option<String> {
        self.selection_range().map(|(start, end)| {
            let text = self.text.get();
            text.chars().skip(start).take(end - start).collect()
        })
    }

    /// Selects the entire text buffer.
    pub fn select_all(&self) {
        let total_chars = self.text.get().chars().count();
        self.selection_anchor.set(Some(0));
        self.cursor_index.set(total_chars);
    }

    /// Clears any active selection.
    pub fn clear_selection(&self) {
        self.selection_anchor.set(None);
    }

    /// Sets the text value, resetting the cursor to the end and clearing undo stacks.
    pub fn set_text(&mut self, new_text: &str) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.text.set(new_text.to_string());
        self.cursor_index.set(new_text.chars().count());
        self.selection_anchor.set(None);
    }

    /// Inserts text at the current cursor position, replacing any active selection.
    pub fn insert_text(&mut self, inserted: &str) {
        self.push_undo_snapshot();
        let cur_text = self.text.get();
        let chars: Vec<char> = cur_text.chars().collect();

        if let Some((start, end)) = self.selection_range() {
            let mut new_chars = Vec::with_capacity(chars.len() + inserted.len());
            new_chars.extend_from_slice(&chars[..start]);
            new_chars.extend(inserted.chars());
            new_chars.extend_from_slice(&chars[end..]);

            let new_cursor = start + inserted.chars().count();
            self.text.set(new_chars.into_iter().collect());
            self.cursor_index.set(new_cursor);
            self.selection_anchor.set(None);
        } else {
            let cursor = self.cursor_index.get().min(chars.len());
            let mut new_chars = Vec::with_capacity(chars.len() + inserted.len());
            new_chars.extend_from_slice(&chars[..cursor]);
            new_chars.extend(inserted.chars());
            new_chars.extend_from_slice(&chars[cursor..]);

            let new_cursor = cursor + inserted.chars().count();
            self.text.set(new_chars.into_iter().collect());
            self.cursor_index.set(new_cursor);
        }
    }

    /// Deletes the character before the cursor, or the current selection.
    pub fn delete_backward(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }

        let cursor = self.cursor_index.get();
        if cursor == 0 {
            return;
        }

        self.push_undo_snapshot();
        let cur_text = self.text.get();
        let mut chars: Vec<char> = cur_text.chars().collect();
        if cursor <= chars.len() {
            chars.remove(cursor - 1);
            self.cursor_index.set(cursor - 1);
            self.text.set(chars.into_iter().collect());
        }
    }

    /// Deletes the character after the cursor, or the current selection.
    pub fn delete_forward(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }

        let cursor = self.cursor_index.get();
        let cur_text = self.text.get();
        let mut chars: Vec<char> = cur_text.chars().collect();
        if cursor < chars.len() {
            self.push_undo_snapshot();
            chars.remove(cursor);
            self.text.set(chars.into_iter().collect());
        }
    }

    /// Deletes the word preceding the cursor.
    pub fn delete_word_backward(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }

        let cursor = self.cursor_index.get();
        if cursor == 0 {
            return;
        }

        let cur_text = self.text.get();
        let chars: Vec<char> = cur_text.chars().collect();
        let prev_boundary = find_prev_word_boundary(&chars, cursor);

        self.push_undo_snapshot();
        let mut new_chars = Vec::with_capacity(chars.len());
        new_chars.extend_from_slice(&chars[..prev_boundary]);
        new_chars.extend_from_slice(&chars[cursor..]);

        self.cursor_index.set(prev_boundary);
        self.text.set(new_chars.into_iter().collect());
    }

    /// Deletes the word following the cursor.
    pub fn delete_word_forward(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }

        let cursor = self.cursor_index.get();
        let cur_text = self.text.get();
        let chars: Vec<char> = cur_text.chars().collect();
        if cursor >= chars.len() {
            return;
        }

        let next_boundary = find_next_word_boundary(&chars, cursor);

        self.push_undo_snapshot();
        let mut new_chars = Vec::with_capacity(chars.len());
        new_chars.extend_from_slice(&chars[..cursor]);
        new_chars.extend_from_slice(&chars[next_boundary..]);

        self.text.set(new_chars.into_iter().collect());
    }

    /// Deletes the active selection range.
    fn delete_selection(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.push_undo_snapshot();
            let cur_text = self.text.get();
            let chars: Vec<char> = cur_text.chars().collect();

            let mut new_chars = Vec::with_capacity(chars.len());
            new_chars.extend_from_slice(&chars[..start]);
            new_chars.extend_from_slice(&chars[end..]);

            self.text.set(new_chars.into_iter().collect());
            self.cursor_index.set(start);
            self.selection_anchor.set(None);
        }
    }

    /// Moves the cursor left, optionally extending selection or jumping by word.
    pub fn move_left(&self, extend_selection: bool, by_word: bool) {
        let cur_text = self.text.get();
        let chars: Vec<char> = cur_text.chars().collect();
        let cursor = self.cursor_index.get();

        if !extend_selection && self.has_selection() {
            if let Some((start, _)) = self.selection_range() {
                self.selection_anchor.set(None);
                self.cursor_index.set(start);
                return;
            }
        }

        let new_cursor = if by_word {
            find_prev_word_boundary(&chars, cursor)
        } else {
            cursor.saturating_sub(1)
        };

        if extend_selection {
            if self.selection_anchor.get().is_none() {
                self.selection_anchor.set(Some(cursor));
            }
        } else {
            self.selection_anchor.set(None);
        }

        self.cursor_index.set(new_cursor);
    }

    /// Moves the cursor right, optionally extending selection or jumping by word.
    pub fn move_right(&self, extend_selection: bool, by_word: bool) {
        let cur_text = self.text.get();
        let chars: Vec<char> = cur_text.chars().collect();
        let cursor = self.cursor_index.get();

        if !extend_selection && self.has_selection() {
            if let Some((_, end)) = self.selection_range() {
                self.selection_anchor.set(None);
                self.cursor_index.set(end);
                return;
            }
        }

        let new_cursor = if by_word {
            find_next_word_boundary(&chars, cursor)
        } else {
            (cursor + 1).min(chars.len())
        };

        if extend_selection {
            if self.selection_anchor.get().is_none() {
                self.selection_anchor.set(Some(cursor));
            }
        } else {
            self.selection_anchor.set(None);
        }

        self.cursor_index.set(new_cursor);
    }

    /// Moves the cursor to the beginning of the line.
    pub fn move_home(&self, extend_selection: bool) {
        let cursor = self.cursor_index.get();
        if extend_selection {
            if self.selection_anchor.get().is_none() {
                self.selection_anchor.set(Some(cursor));
            }
        } else {
            self.selection_anchor.set(None);
        }
        self.cursor_index.set(0);
    }

    /// Moves the cursor to the end of the line.
    pub fn move_end(&self, extend_selection: bool) {
        let total = self.text.get().chars().count();
        let cursor = self.cursor_index.get();
        if extend_selection {
            if self.selection_anchor.get().is_none() {
                self.selection_anchor.set(Some(cursor));
            }
        } else {
            self.selection_anchor.set(None);
        }
        self.cursor_index.set(total);
    }

    /// Pushes current text and cursor position to the undo stack.
    fn push_undo_snapshot(&mut self) {
        self.undo_stack.push((self.text.get(), self.cursor_index.get()));
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Reverts to the previous state on the undo stack.
    pub fn undo(&mut self) -> bool {
        if let Some((prev_text, prev_cursor)) = self.undo_stack.pop() {
            self.redo_stack.push((self.text.get(), self.cursor_index.get()));
            self.text.set(prev_text);
            self.cursor_index.set(prev_cursor);
            self.selection_anchor.set(None);
            true
        } else {
            false
        }
    }

    /// Re-applies the next state on the redo stack.
    pub fn redo(&mut self) -> bool {
        if let Some((next_text, next_cursor)) = self.redo_stack.pop() {
            self.undo_stack.push((self.text.get(), self.cursor_index.get()));
            self.text.set(next_text);
            self.cursor_index.set(next_cursor);
            self.selection_anchor.set(None);
            true
        } else {
            false
        }
    }

    /// Processes an input keyboard event according to standard Chromium/OS input contracts.
    pub fn handle_key(
        &mut self,
        key: InputKey,
        modifiers: InputModifiers,
        clipboard_content: Option<&str>,
    ) -> InputKeyResult {
        // Handle Ctrl shortcuts
        if modifiers.ctrl {
            match key {
                InputKey::A => {
                    self.select_all();
                    return InputKeyResult::Handled;
                }
                InputKey::C => {
                    if let Some(sel) = self.selected_text() {
                        return InputKeyResult::CopyToClipboard(sel);
                    }
                    return InputKeyResult::Ignored;
                }
                InputKey::X => {
                    if let Some(sel) = self.selected_text() {
                        self.delete_selection();
                        return InputKeyResult::CopyToClipboard(sel);
                    }
                    return InputKeyResult::Ignored;
                }
                InputKey::V => {
                    if let Some(clip) = clipboard_content {
                        self.insert_text(clip);
                        return InputKeyResult::Handled;
                    }
                    return InputKeyResult::Ignored;
                }
                InputKey::Z => {
                    if modifiers.shift {
                        if self.redo() {
                            return InputKeyResult::Handled;
                        }
                    } else if self.undo() {
                        return InputKeyResult::Handled;
                    }
                    return InputKeyResult::Ignored;
                }
                InputKey::Y => {
                    if self.redo() {
                        return InputKeyResult::Handled;
                    }
                    return InputKeyResult::Ignored;
                }
                InputKey::Left => {
                    self.move_left(modifiers.shift, true);
                    return InputKeyResult::Handled;
                }
                InputKey::Right => {
                    self.move_right(modifiers.shift, true);
                    return InputKeyResult::Handled;
                }
                InputKey::Backspace => {
                    self.delete_word_backward();
                    return InputKeyResult::Handled;
                }
                InputKey::Delete => {
                    self.delete_word_forward();
                    return InputKeyResult::Handled;
                }
                _ => {}
            }
        }

        // Standard navigation and editing
        match key {
            InputKey::Left => {
                self.move_left(modifiers.shift, false);
                InputKeyResult::Handled
            }
            InputKey::Right => {
                self.move_right(modifiers.shift, false);
                InputKeyResult::Handled
            }
            InputKey::Home => {
                self.move_home(modifiers.shift);
                InputKeyResult::Handled
            }
            InputKey::End => {
                self.move_end(modifiers.shift);
                InputKeyResult::Handled
            }
            InputKey::Backspace => {
                self.delete_backward();
                InputKeyResult::Handled
            }
            InputKey::Delete => {
                self.delete_forward();
                InputKeyResult::Handled
            }
            InputKey::Enter => InputKeyResult::Submitted,
            InputKey::Escape => {
                self.clear_selection();
                InputKeyResult::Handled
            }
            InputKey::Char(c) => {
                let mut buf = [0u8; 4];
                self.insert_text(c.encode_utf8(&mut buf));
                InputKeyResult::Handled
            }
            _ => InputKeyResult::Ignored,
        }
    }
}

impl Default for TextInputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper locating the previous word boundary index before `cursor`.
fn find_prev_word_boundary(chars: &[char], cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let mut idx = cursor;

    // Skip trailing whitespace
    while idx > 0 && chars[idx - 1].is_whitespace() {
        idx -= 1;
    }
    // Skip word characters
    while idx > 0 && !chars[idx - 1].is_whitespace() {
        idx -= 1;
    }
    idx
}

/// Helper locating the next word boundary index after `cursor`.
fn find_next_word_boundary(chars: &[char], cursor: usize) -> usize {
    let len = chars.len();
    if cursor >= len {
        return len;
    }
    let mut idx = cursor;

    // Skip current word characters
    while idx < len && !chars[idx].is_whitespace() {
        idx += 1;
    }
    // Skip subsequent whitespace
    while idx < len && chars[idx].is_whitespace() {
        idx += 1;
    }
    idx
}
