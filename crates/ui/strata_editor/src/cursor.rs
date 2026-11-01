//! Multi-cursor tracking, selection ranges, and collision normalization.

use serde::{Deserialize, Serialize};

/// Exact 2D position in text buffer coordinates (0-indexed line and column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub const ZERO: Self = Self { line: 0, col: 0 };

    pub const fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// A text selection representing an active cursor caret or highlighted range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Selection {
    /// Fixed anchor origin where selection started.
    pub anchor: Position,
    /// Active focus point where the caret currently sits.
    pub focus: Position,
}

impl Selection {
    /// Constructs a collapsed single-point cursor caret.
    pub const fn point(pos: Position) -> Self {
        Self {
            anchor: pos,
            focus: pos,
        }
    }

    /// Constructs a selection range between anchor and focus.
    pub const fn range(anchor: Position, focus: Position) -> Self {
        Self { anchor, focus }
    }

    /// Returns whether this selection is a collapsed single-point caret.
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.focus
    }

    /// Returns the ordered start and end positions `(start, end)` where `start <= end`.
    pub fn ordered_range(&self) -> (Position, Position) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    /// Returns whether this selection contains the given position.
    pub fn contains(&self, pos: Position) -> bool {
        let (start, end) = self.ordered_range();
        pos >= start && pos <= end
    }

    /// Checks if this selection overlaps or directly touches another selection.
    pub fn touches_or_overlaps(&self, other: &Selection) -> bool {
        let (s1, e1) = self.ordered_range();
        let (s2, e2) = other.ordered_range();
        !(e1 < s2 || e2 < s1)
    }

    /// Merges this selection with an overlapping selection.
    pub fn merge(&self, other: &Selection) -> Self {
        let (s1, e1) = self.ordered_range();
        let (s2, e2) = other.ordered_range();
        let min_pos = s1.min(s2);
        let max_pos = e1.max(e2);
        Self {
            anchor: min_pos,
            focus: max_pos,
        }
    }
}

/// Multi-cursor engine managing synchronized cursors across the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiCursor {
    selections: Vec<Selection>,
    primary_idx: usize,
}

impl Default for MultiCursor {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiCursor {
    /// Initializes multi-cursor engine with a single cursor at `(0, 0)`.
    pub fn new() -> Self {
        Self {
            selections: vec![Selection::point(Position::ZERO)],
            primary_idx: 0,
        }
    }

    /// Initializes with a single cursor at specified position.
    pub fn single(pos: Position) -> Self {
        Self {
            selections: vec![Selection::point(pos)],
            primary_idx: 0,
        }
    }

    /// Returns the primary active selection.
    pub fn primary(&self) -> &Selection {
        &self.selections[self.primary_idx]
    }

    /// Returns all active selections.
    pub fn all(&self) -> &[Selection] {
        &self.selections
    }

    /// Returns mutable slice of all selections.
    pub fn all_mut(&mut self) -> &mut [Selection] {
        &mut self.selections
    }

    /// Clears secondary cursors and sets single active caret.
    pub fn set_single(&mut self, pos: Position) {
        self.selections = vec![Selection::point(pos)];
        self.primary_idx = 0;
    }

    /// Appends a new cursor or selection range, automatically normalizing and merging.
    pub fn add_selection(&mut self, sel: Selection) {
        self.selections.push(sel);
        self.normalize();
    }

    /// Appends a new single-point cursor caret at `pos`.
    pub fn add_cursor(&mut self, pos: Position) {
        self.add_selection(Selection::point(pos));
    }

    /// Returns the number of active selections/cursors.
    pub fn count(&self) -> usize {
        self.selections.len()
    }

    /// Adds rectangular column block cursors between `start_line` and `end_line` at column `col`.
    pub fn add_column_cursors(&mut self, start_line: usize, end_line: usize, col: usize) {
        let min_l = start_line.min(end_line);
        let max_l = start_line.max(end_line);

        self.selections.clear();
        for l in min_l..=max_l {
            self.selections.push(Selection::point(Position::new(l, col)));
        }
        self.primary_idx = 0;
        self.normalize();
    }

    /// Normalizes selections: sorts them in document order and merges any overlapping carets.
    pub fn normalize(&mut self) {
        if self.selections.len() <= 1 {
            return;
        }

        self.selections.sort_by_key(|s| s.ordered_range().0);

        let mut merged: Vec<Selection> = Vec::with_capacity(self.selections.len());
        for sel in self.selections.drain(..) {
            if let Some(last) = merged.last_mut() {
                if last.touches_or_overlaps(&sel) {
                    *last = last.merge(&sel);
                    continue;
                }
            }
            merged.push(sel);
        }

        self.selections = merged;
        if self.primary_idx >= self.selections.len() {
            self.primary_idx = self.selections.len().saturating_sub(1);
        }
    }
}
