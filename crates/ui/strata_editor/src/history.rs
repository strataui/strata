//! Transactional Undo/Redo engine with timestamp-based typing coalescing.

use crate::buffer::LineBuffer;
use crate::cursor::{Position, Selection};

/// An atomic text modification action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOperation {
    Insert {
        pos: Position,
        text: String,
    },
    Delete {
        start: Position,
        end: Position,
        text: String,
    },
}

/// A transactional unit of work containing one or more atomic operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorTransaction {
    pub operations: Vec<EditOperation>,
    pub before_cursors: Vec<Selection>,
    pub after_cursors: Vec<Selection>,
    pub timestamp_ms: u64,
}

impl EditorTransaction {
    pub fn new(
        operations: Vec<EditOperation>,
        before_cursors: Vec<Selection>,
        after_cursors: Vec<Selection>,
    ) -> Self {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);

        Self {
            operations,
            before_cursors,
            after_cursors,
            timestamp_ms,
        }
    }
}

/// Manages undo and redo history with automatic typing burst coalescing.
#[derive(Debug, Clone)]
pub struct HistoryStack {
    undo_stack: Vec<EditorTransaction>,
    redo_stack: Vec<EditorTransaction>,
    max_depth: usize,
}

impl Default for HistoryStack {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl HistoryStack {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Records an insertion with automatic coalescing if typed within 400ms at adjacent position.
    pub fn record_insert(
        &mut self,
        pos: Position,
        text: &str,
        before_cursors: Vec<Selection>,
        after_cursors: Vec<Selection>,
    ) {
        self.redo_stack.clear();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);

        // Attempt coalescing with previous insertion
        if let Some(last) = self.undo_stack.last_mut() {
            if now_ms.saturating_sub(last.timestamp_ms) < 400 && last.operations.len() == 1 {
                if let EditOperation::Insert { text: ref mut prev_text, .. } = last.operations[0] {
                    if !prev_text.contains('\n') && !text.contains('\n') {
                        prev_text.push_str(text);
                        last.after_cursors = after_cursors;
                        last.timestamp_ms = now_ms;
                        return;
                    }
                }
            }
        }

        let tx = EditorTransaction {
            operations: vec![EditOperation::Insert {
                pos,
                text: text.to_string(),
            }],
            before_cursors,
            after_cursors,
            timestamp_ms: now_ms,
        };

        self.push_transaction(tx);
    }

    /// Records a deletion operation.
    pub fn record_delete(
        &mut self,
        start: Position,
        end: Position,
        text: &str,
        before_cursors: Vec<Selection>,
        after_cursors: Vec<Selection>,
    ) {
        self.redo_stack.clear();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);

        let tx = EditorTransaction {
            operations: vec![EditOperation::Delete {
                start,
                end,
                text: text.to_string(),
            }],
            before_cursors,
            after_cursors,
            timestamp_ms: now_ms,
        };

        self.push_transaction(tx);
    }

    /// Pushes a transaction onto the undo stack, respecting max depth limit.
    pub fn push_transaction(&mut self, tx: EditorTransaction) {
        if self.undo_stack.len() >= self.max_depth {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(tx);
    }

    /// Rolls back the most recent transaction, reversing buffer changes and restoring cursors.
    pub fn undo(&mut self, buffer: &mut LineBuffer) -> Option<Vec<Selection>> {
        let tx = self.undo_stack.pop()?;

        // Apply operations in reverse order
        for op in tx.operations.iter().rev() {
            match op {
                EditOperation::Insert { pos, text } => {
                    // Undo insert by deleting the inserted text range
                    let lines: Vec<&str> = text.split('\n').collect();
                    let end_line = pos.line + lines.len() - 1;
                    let end_col = if lines.len() == 1 {
                        pos.col + lines[0].len()
                    } else {
                        lines[lines.len() - 1].len()
                    };
                    buffer.delete_range(pos.line, pos.col, end_line, end_col);
                }
                EditOperation::Delete { start, text, .. } => {
                    // Undo delete by re-inserting the deleted text
                    buffer.insert_str(start.line, start.col, text);
                }
            }
        }

        let restore_cursors = tx.before_cursors.clone();
        self.redo_stack.push(tx);
        Some(restore_cursors)
    }

    /// Replays the most recently undone transaction.
    pub fn redo(&mut self, buffer: &mut LineBuffer) -> Option<Vec<Selection>> {
        let tx = self.redo_stack.pop()?;

        for op in &tx.operations {
            match op {
                EditOperation::Insert { pos, text } => {
                    buffer.insert_str(pos.line, pos.col, text);
                }
                EditOperation::Delete { start, end, .. } => {
                    buffer.delete_range(start.line, start.col, end.line, end.col);
                }
            }
        }

        let restore_cursors = tx.after_cursors.clone();
        self.undo_stack.push(tx);
        Some(restore_cursors)
    }
}
