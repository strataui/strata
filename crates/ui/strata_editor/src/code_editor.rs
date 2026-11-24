//! High-performance Code Editor controller with virtualized lines, syntax highlighting, and gutter.

use crate::buffer::LineBuffer;
use crate::cursor::{MultiCursor, Position, Selection};
use crate::diagnostics::{DiagnosticCollection, DiagnosticCounts, EditorDiagnostic};
use crate::history::HistoryStack;
use crate::ime::ImeComposition;
use crate::minimap::{minimap_y_to_scroll_top, MinimapConfig};
use crate::search::{FindReplaceState, SearchMatch};
use crate::syntax::{FoldRange, Language, SyntaxHighlighter, TokenType};
use crate::theme::EditorTheme;
use strata_dom::{Event, NodeBlueprint, Tag};

/// High-level code editor managing buffer state, multi-cursors, incremental syntax,
/// search/replace, folding, diagnostics, theming, and virtualized rendering.
#[derive(Debug, Clone)]
pub struct CodeEditor {
    buffer: LineBuffer,
    cursors: MultiCursor,
    history: HistoryStack,
    highlighter: SyntaxHighlighter,
    language: Language,
    folded_lines: Vec<usize>,
    cached_folds: Vec<FoldRange>,
    folds_valid: bool,
    ime: ImeComposition,
    tab_size: usize,
    wrap_column: Option<usize>,
    search: FindReplaceState,
    diagnostics: DiagnosticCollection,
    theme: EditorTheme,
    scroll_top: f32,
    scroll_left: f32,
    viewport_height: f32,
    line_height: f32,
    char_width: f32,
    read_only: bool,
    is_pointer_dragging: bool,
}

impl CodeEditor {
    /// Constructs a new code editor with initial source code and language syntax.
    pub fn new(content: &str, language: Language) -> Self {
        let buffer = LineBuffer::from_str(content);
        let mut highlighter = SyntaxHighlighter::with_language(language);
        highlighter.parse_buffer(&buffer);
        let mut diagnostics = DiagnosticCollection::new();
        diagnostics.set_syntax_diagnostics(highlighter.collect_syntax_diagnostics(&buffer));
        Self {
            buffer,
            cursors: MultiCursor::new(),
            history: HistoryStack::default(),
            highlighter,
            language,
            folded_lines: Vec::new(),
            cached_folds: Vec::new(),
            folds_valid: false,
            ime: ImeComposition::new(),
            tab_size: 4,
            wrap_column: None,
            search: FindReplaceState::new(),
            diagnostics,
            theme: EditorTheme::default(),
            scroll_top: 0.0,
            scroll_left: 0.0,
            viewport_height: 480.0,
            line_height: 22.0,
            char_width: 8.4,
            read_only: false,
            is_pointer_dragging: false,
        }
    }

    /// Synchronizes live syntax diagnostics, invalidates cached folds, and refreshes open search results.
    fn on_buffer_mutated(&mut self) {
        self.folds_valid = false;
        self.diagnostics
            .set_syntax_diagnostics(self.highlighter.collect_syntax_diagnostics(&self.buffer));
        if self.search.is_open && !self.search.query.is_empty() {
            self.search.recompute_matches(&self.buffer);
        }
    }

    /// Accesses the active syntax highlighter.
    pub fn highlighter(&self) -> &SyntaxHighlighter {
        &self.highlighter
    }

    /// Accesses the active syntax highlighter mutably.
    pub fn highlighter_mut(&mut self) -> &mut SyntaxHighlighter {
        &mut self.highlighter
    }

    /// Returns the active syntax language.
    pub fn language(&self) -> Language {
        self.language
    }

    /// Changes the syntax language and re-parses the AST directly from the rope buffer.
    pub fn set_language(&mut self, language: Language) {
        self.language = language;
        self.highlighter.set_language(language);
        self.highlighter.parse_buffer(&self.buffer);
        self.folded_lines.clear();
        self.on_buffer_mutated();
    }

    /// Re-parses the document incrementally with the active Tree-Sitter grammar via zero-copy rope slices.
    pub fn reparse(&mut self) {
        let total = self.buffer.line_count();
        self.highlighter
            .reparse_incremental_buffer(&self.buffer, 0, total);
        self.on_buffer_mutated();
    }

    /// Feeds one or more chronological `tree_sitter::InputEdit` records into the syntax
    /// highlighter and performs a localized incremental reparse directly from the rope.
    fn apply_syntax_edits_and_reparse(&mut self, edits: &[tree_sitter::InputEdit]) {
        if edits.is_empty() {
            return;
        }
        let mut dirty_start = usize::MAX;
        let mut dirty_end = 0usize;
        for edit in edits {
            self.highlighter.edit(edit);
            dirty_start = dirty_start.min(edit.start_position.row);
            let row_delta =
                edit.new_end_position.row as isize - edit.old_end_position.row as isize;
            let shifted_end = (dirty_end as isize + row_delta).max(0) as usize;
            dirty_end = shifted_end.max(edit.new_end_position.row.saturating_add(1));
        }
        if dirty_start == usize::MAX {
            dirty_start = 0;
        }
        self.highlighter
            .reparse_incremental_buffer(&self.buffer, dirty_start, dirty_end);
        self.on_buffer_mutated();
    }

    /// Accesses the underlying line buffer.
    pub fn buffer(&self) -> &LineBuffer {
        &self.buffer
    }

    /// Accesses the undo/redo history stack.
    pub fn history(&self) -> &HistoryStack {
        &self.history
    }

    /// Accesses the active Find & Replace state.
    pub fn search(&self) -> &FindReplaceState {
        &self.search
    }

    /// Mutably accesses the active Find & Replace state.
    pub fn search_mut(&mut self) -> &mut FindReplaceState {
        &mut self.search
    }

    /// Accesses the diagnostics collection (live syntax + external LSP diagnostics).
    pub fn diagnostics(&self) -> &DiagnosticCollection {
        &self.diagnostics
    }

    /// Replaces external/LSP diagnostics while preserving live syntax diagnostics.
    pub fn set_external_diagnostics(&mut self, diagnostics: Vec<EditorDiagnostic>) {
        self.diagnostics.set_external(diagnostics);
    }

    /// Clears external/LSP diagnostics.
    pub fn clear_external_diagnostics(&mut self) {
        self.diagnostics.clear_external();
    }

    /// Returns diagnostic severity counts for status bars and badges.
    pub fn diagnostic_counts(&self) -> DiagnosticCounts {
        self.diagnostics.counts()
    }

    /// Accesses the active CSS-variable editor theme.
    pub fn theme(&self) -> &EditorTheme {
        &self.theme
    }

    /// Updates the active editor theme.
    pub fn set_theme(&mut self, theme: EditorTheme) {
        self.theme = theme;
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn scroll_top(&self) -> f32 {
        self.scroll_top
    }

    pub fn set_scroll_top(&mut self, scroll_top: f32) {
        let max_scroll = self.max_scroll_top();
        self.scroll_top = scroll_top.clamp(0.0, max_scroll);
    }

    pub fn scroll_left(&self) -> f32 {
        self.scroll_left
    }

    pub fn set_scroll_left(&mut self, scroll_left: f32) {
        self.scroll_left = scroll_left.max(0.0);
    }

    pub fn viewport_height(&self) -> f32 {
        self.viewport_height
    }

    pub fn set_viewport_height(&mut self, height: f32) {
        self.viewport_height = height.max(40.0);
        let max_scroll = self.max_scroll_top();
        self.scroll_top = self.scroll_top.clamp(0.0, max_scroll);
    }

    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    pub fn set_line_height(&mut self, line_height: f32) {
        self.line_height = line_height.max(12.0);
    }

    pub fn char_width(&self) -> f32 {
        self.char_width
    }

    pub fn set_char_width(&mut self, char_width: f32) {
        self.char_width = char_width.max(4.0);
    }

    pub fn max_scroll_top(&self) -> f32 {
        let visible_rows = self.visible_logical_lines().len().max(1) as f32;
        (visible_rows * self.line_height - self.viewport_height).max(0.0)
    }

    /// Accesses the active cursors.
    pub fn cursors(&self) -> &MultiCursor {
        &self.cursors
    }

    /// Accesses the active cursors mutably.
    pub fn cursors_mut(&mut self) -> &mut MultiCursor {
        &mut self.cursors
    }

    /// Returns the active IME composition state.
    pub fn ime(&self) -> &ImeComposition {
        &self.ime
    }

    /// Mutably accesses the IME composition state.
    pub fn ime_mut(&mut self) -> &mut ImeComposition {
        &mut self.ime
    }

    /// Returns the configured tab size (in spaces).
    pub fn tab_size(&self) -> usize {
        self.tab_size
    }

    /// Configures tab size.
    pub fn set_tab_size(&mut self, size: usize) {
        self.tab_size = size;
    }

    /// Returns the soft-wrap column limit, if enabled.
    pub fn wrap_column(&self) -> Option<usize> {
        self.wrap_column
    }

    /// Configures soft-wrap column limit.
    pub fn set_wrap_column(&mut self, wrap: Option<usize>) {
        self.wrap_column = wrap;
    }

    /// Total number of lines in the buffer.
    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    /// Serializes entire buffer content to a String.
    pub fn text(&self) -> String {
        self.buffer.to_string()
    }

    /// Replaces entire buffer content and resets history.
    pub fn set_text(&mut self, text: &str) {
        self.buffer = LineBuffer::from_str(text);
        self.cursors.set_single(Position::ZERO);
        self.history = HistoryStack::default();
        self.folded_lines.clear();
        self.scroll_top = 0.0;
        self.scroll_left = 0.0;
        self.highlighter.parse_buffer(&self.buffer);
        self.on_buffer_mutated();
    }

    /// Inserts a character at all active cursors (or handles Enter with auto-indentation).
    pub fn insert_char(&mut self, ch: char) {
        if ch == '\n' {
            self.insert_newline();
        } else {
            self.insert_text(&ch.to_string());
        }
    }

    /// Inserts a newline with automatic indentation calculated from the syntax engine.
    pub fn insert_newline(&mut self) {
        self.cursors.normalize();
        let before_all = self.cursors.all().to_vec();
        let descending = self.cursors.selections_descending();

        let mut operations = Vec::with_capacity(descending.len() * 2);
        let mut syntax_edits = Vec::with_capacity(descending.len() * 2);
        let mut processed_carets: Vec<Position> = Vec::with_capacity(descending.len());

        for sel in descending {
            let (start, end) = sel.ordered_range();
            if start != end {
                let (del_edit, _) = self.buffer.compute_input_edit(start, end, "");
                let deleted = self
                    .buffer
                    .delete_range(start.line, start.col, end.line, end.col);
                syntax_edits.push(del_edit);
                operations.push(crate::history::EditOperation::Delete {
                    start,
                    end,
                    text: deleted,
                });
            }

            let indent_spaces = self
                .highlighter
                .compute_auto_indent(&self.buffer, start.line);
            let indent_str = if indent_spaces > 0 {
                " ".repeat(indent_spaces)
            } else {
                String::new()
            };
            let inserted_text = format!("\n{indent_str}");
            let (ins_edit, _) = self.buffer.compute_input_edit(start, start, &inserted_text);

            self.buffer.split_line(start.line, start.col);
            let new_line = start.line + 1;
            if !indent_str.is_empty() {
                self.buffer.insert_str(new_line, 0, &indent_str);
            }
            syntax_edits.push(ins_edit);

            operations.push(crate::history::EditOperation::Insert {
                pos: start,
                text: inserted_text,
            });

            let lines_slice: Vec<&str> = vec!["", &indent_str];
            let my_new_pos = Position::new(new_line, indent_spaces);

            for caret in &mut processed_carets {
                *caret = crate::cursor::shift_position_after_edit(*caret, start, end, &lines_slice);
            }

            processed_carets.push(my_new_pos);
        }

        let new_cursors: Vec<Selection> =
            processed_carets.into_iter().map(Selection::point).collect();

        self.cursors = MultiCursor::from_selections(new_cursors);

        self.history
            .push_transaction(crate::history::EditorTransaction::new(
                operations,
                before_all,
                self.cursors.all().to_vec(),
            ));

        self.apply_syntax_edits_and_reparse(&syntax_edits);
    }

    /// Inserts text at all cursor positions atomically in descending document order.
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        self.cursors.normalize();
        let before_all = self.cursors.all().to_vec();
        let descending = self.cursors.selections_descending();

        let mut operations = Vec::with_capacity(descending.len());
        let mut syntax_edits = Vec::with_capacity(descending.len() * 2);
        let mut processed_carets: Vec<Position> = Vec::with_capacity(descending.len());

        let text_lines: Vec<&str> = text.split('\n').collect();

        for sel in descending {
            let (start, end) = sel.ordered_range();
            if start != end {
                let (del_edit, _) = self.buffer.compute_input_edit(start, end, "");
                let deleted = self
                    .buffer
                    .delete_range(start.line, start.col, end.line, end.col);
                syntax_edits.push(del_edit);
                operations.push(crate::history::EditOperation::Delete {
                    start,
                    end,
                    text: deleted,
                });
            }

            let (ins_edit, _) = self.buffer.compute_input_edit(start, start, text);
            self.buffer.insert_str(start.line, start.col, text);
            syntax_edits.push(ins_edit);
            operations.push(crate::history::EditOperation::Insert {
                pos: start,
                text: text.to_string(),
            });

            let my_new_pos = if text_lines.len() == 1 {
                Position::new(start.line, start.col + text_lines[0].chars().count())
            } else {
                Position::new(
                    start.line + text_lines.len() - 1,
                    text_lines.last().map_or(0, |l| l.chars().count()),
                )
            };

            for caret in &mut processed_carets {
                *caret = crate::cursor::shift_position_after_edit(*caret, start, end, &text_lines);
            }

            processed_carets.push(my_new_pos);
        }

        let new_cursors: Vec<Selection> =
            processed_carets.into_iter().map(Selection::point).collect();

        self.cursors = MultiCursor::from_selections(new_cursors);

        self.history
            .push_transaction(crate::history::EditorTransaction::new(
                operations,
                before_all,
                self.cursors.all().to_vec(),
            ));

        self.apply_syntax_edits_and_reparse(&syntax_edits);
    }

    /// Performs backspace deletion at all active cursors atomically in descending document order.
    pub fn delete_backwards(&mut self) {
        self.cursors.normalize();
        let before_all = self.cursors.all().to_vec();
        let descending = self.cursors.selections_descending();

        let mut operations = Vec::with_capacity(descending.len());
        let mut syntax_edits = Vec::with_capacity(descending.len());
        let mut processed_carets: Vec<Position> = Vec::with_capacity(descending.len());

        for sel in descending {
            let (start, end) = sel.ordered_range();
            if start != end {
                let (del_edit, _) = self.buffer.compute_input_edit(start, end, "");
                let deleted = self
                    .buffer
                    .delete_range(start.line, start.col, end.line, end.col);
                syntax_edits.push(del_edit);
                operations.push(crate::history::EditOperation::Delete {
                    start,
                    end,
                    text: deleted,
                });

                for caret in &mut processed_carets {
                    *caret = crate::cursor::shift_position_after_edit(*caret, start, end, &[""]);
                }

                processed_carets.push(start);
            } else if start.col > 0 {
                let del_start = Position::new(start.line, start.col - 1);
                let (del_edit, _) = self.buffer.compute_input_edit(del_start, start, "");
                let deleted =
                    self.buffer
                        .delete_range(del_start.line, del_start.col, start.line, start.col);
                syntax_edits.push(del_edit);
                operations.push(crate::history::EditOperation::Delete {
                    start: del_start,
                    end: start,
                    text: deleted,
                });

                for caret in &mut processed_carets {
                    *caret =
                        crate::cursor::shift_position_after_edit(*caret, del_start, start, &[""]);
                }

                processed_carets.push(del_start);
            } else if start.line > 0 {
                let prev_line = start.line - 1;
                let prev_len = self.buffer.line_len(prev_line);
                let del_start = Position::new(prev_line, prev_len);
                let del_end = Position::new(start.line, 0);
                let (del_edit, _) = self.buffer.compute_input_edit(del_start, del_end, "");

                self.buffer.join_line_with_next(prev_line);
                syntax_edits.push(del_edit);
                operations.push(crate::history::EditOperation::Delete {
                    start: del_start,
                    end: del_end,
                    text: "\n".to_string(),
                });

                for caret in &mut processed_carets {
                    *caret = crate::cursor::shift_position_after_edit(
                        *caret,
                        del_start,
                        del_end,
                        &[""],
                    );
                }

                processed_carets.push(del_start);
            } else {
                processed_carets.push(start);
            }
        }

        let new_cursors: Vec<Selection> =
            processed_carets.into_iter().map(Selection::point).collect();

        self.cursors = MultiCursor::from_selections(new_cursors);

        if !operations.is_empty() {
            self.history
                .push_transaction(crate::history::EditorTransaction::new(
                    operations,
                    before_all,
                    self.cursors.all().to_vec(),
                ));
            self.apply_syntax_edits_and_reparse(&syntax_edits);
        }
    }

    /// Performs forward Delete key deletion at all active cursors atomically.
    pub fn delete_forward(&mut self) {
        self.cursors.normalize();
        let before_all = self.cursors.all().to_vec();
        let descending = self.cursors.selections_descending();

        let mut operations = Vec::with_capacity(descending.len());
        let mut syntax_edits = Vec::with_capacity(descending.len());
        let mut processed_carets: Vec<Position> = Vec::with_capacity(descending.len());

        for sel in descending {
            let (start, end) = sel.ordered_range();
            if start != end {
                let (del_edit, _) = self.buffer.compute_input_edit(start, end, "");
                let deleted = self
                    .buffer
                    .delete_range(start.line, start.col, end.line, end.col);
                syntax_edits.push(del_edit);
                operations.push(crate::history::EditOperation::Delete {
                    start,
                    end,
                    text: deleted,
                });
                for caret in &mut processed_carets {
                    *caret = crate::cursor::shift_position_after_edit(*caret, start, end, &[""]);
                }
                processed_carets.push(start);
            } else {
                let line_len = self.buffer.line_len(start.line);
                if start.col < line_len {
                    let del_end = Position::new(start.line, start.col + 1);
                    let (del_edit, _) = self.buffer.compute_input_edit(start, del_end, "");
                    let deleted =
                        self.buffer
                            .delete_range(start.line, start.col, del_end.line, del_end.col);
                    syntax_edits.push(del_edit);
                    operations.push(crate::history::EditOperation::Delete {
                        start,
                        end: del_end,
                        text: deleted,
                    });
                    for caret in &mut processed_carets {
                        *caret = crate::cursor::shift_position_after_edit(
                            *caret,
                            start,
                            del_end,
                            &[""],
                        );
                    }
                    processed_carets.push(start);
                } else if start.line + 1 < self.buffer.line_count() {
                    let del_end = Position::new(start.line + 1, 0);
                    let (del_edit, _) = self.buffer.compute_input_edit(start, del_end, "");
                    self.buffer.join_line_with_next(start.line);
                    syntax_edits.push(del_edit);
                    operations.push(crate::history::EditOperation::Delete {
                        start,
                        end: del_end,
                        text: "\n".to_string(),
                    });
                    for caret in &mut processed_carets {
                        *caret = crate::cursor::shift_position_after_edit(
                            *caret,
                            start,
                            del_end,
                            &[""],
                        );
                    }
                    processed_carets.push(start);
                } else {
                    processed_carets.push(start);
                }
            }
        }

        let new_cursors: Vec<Selection> =
            processed_carets.into_iter().map(Selection::point).collect();
        self.cursors = MultiCursor::from_selections(new_cursors);

        if !operations.is_empty() {
            self.history
                .push_transaction(crate::history::EditorTransaction::new(
                    operations,
                    before_all,
                    self.cursors.all().to_vec(),
                ));
            self.apply_syntax_edits_and_reparse(&syntax_edits);
        }
    }

    /// Replaces an arbitrary `[start, end)` range with `replacement` as a single undoable
    /// transaction and drives an incremental Tree-Sitter / line-state update.
    /// Returns the resulting end `Position` of `replacement`.
    pub fn replace_range(
        &mut self,
        start: Position,
        end: Position,
        replacement: &str,
    ) -> Position {
        let (s, e) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let before_all = self.cursors.all().to_vec();
        let mut operations = Vec::with_capacity(2);
        let mut syntax_edits = Vec::with_capacity(2);

        if s != e {
            let (del_edit, _) = self.buffer.compute_input_edit(s, e, "");
            let deleted = self.buffer.delete_range(s.line, s.col, e.line, e.col);
            syntax_edits.push(del_edit);
            operations.push(crate::history::EditOperation::Delete {
                start: s,
                end: e,
                text: deleted,
            });
        }

        let new_end = if !replacement.is_empty() {
            let (ins_edit, end_pos) = self.buffer.compute_input_edit(s, s, replacement);
            self.buffer.insert_str(s.line, s.col, replacement);
            syntax_edits.push(ins_edit);
            operations.push(crate::history::EditOperation::Insert {
                pos: s,
                text: replacement.to_string(),
            });
            end_pos
        } else {
            s
        };

        self.cursors.set_single(new_end);
        if !operations.is_empty() {
            self.history
                .push_transaction(crate::history::EditorTransaction::new(
                    operations,
                    before_all,
                    self.cursors.all().to_vec(),
                ));
            self.apply_syntax_edits_and_reparse(&syntax_edits);
        }
        new_end
    }

    /// Atomically replaces multiple disjoint `(start, end, replacement)` ranges in descending
    /// document order as a single undo transaction and a single incremental AST reparse.
    pub fn replace_ranges_atomic(&mut self, mut replacements: Vec<(Position, Position, String)>) {
        if replacements.is_empty() {
            return;
        }
        // Normalize and sort descending so earlier offsets remain valid as later ones change.
        for item in &mut replacements {
            if item.0 > item.1 {
                std::mem::swap(&mut item.0, &mut item.1);
            }
        }
        replacements.sort_by_key(|item| std::cmp::Reverse(item.0));

        let before_all = self.cursors.all().to_vec();
        let mut operations = Vec::with_capacity(replacements.len() * 2);
        let mut syntax_edits = Vec::with_capacity(replacements.len() * 2);
        let mut last_pos = Position::ZERO;

        for (s, e, repl) in replacements {
            if s != e {
                let (del_edit, _) = self.buffer.compute_input_edit(s, e, "");
                let deleted = self.buffer.delete_range(s.line, s.col, e.line, e.col);
                syntax_edits.push(del_edit);
                operations.push(crate::history::EditOperation::Delete {
                    start: s,
                    end: e,
                    text: deleted,
                });
            }
            if !repl.is_empty() {
                let (ins_edit, end_pos) = self.buffer.compute_input_edit(s, s, &repl);
                self.buffer.insert_str(s.line, s.col, &repl);
                syntax_edits.push(ins_edit);
                operations.push(crate::history::EditOperation::Insert { pos: s, text: repl });
                last_pos = end_pos;
            } else {
                last_pos = s;
            }
        }

        self.cursors.set_single(last_pos);
        if !operations.is_empty() {
            self.history
                .push_transaction(crate::history::EditorTransaction::new(
                    operations,
                    before_all,
                    self.cursors.all().to_vec(),
                ));
            self.apply_syntax_edits_and_reparse(&syntax_edits);
        }
    }

    /// Undoes the last recorded transaction with incremental AST synchronization.
    pub fn undo(&mut self) {
        if let Some((restored, edits)) = self.history.undo_with_edits(&mut self.buffer) {
            self.cursors = MultiCursor::from_selections(restored);
            self.apply_syntax_edits_and_reparse(&edits);
        }
    }

    /// Redoes the last undone transaction with incremental AST synchronization.
    pub fn redo(&mut self) {
        if let Some((restored, edits)) = self.history.redo_with_edits(&mut self.buffer) {
            self.cursors = MultiCursor::from_selections(restored);
            self.apply_syntax_edits_and_reparse(&edits);
        }
    }

    /// Moves all active cursors left by 1 character.
    pub fn move_left(&mut self, select: bool) {
        let before = self.cursors.all().to_vec();
        let mut new_cursors = Vec::with_capacity(before.len());

        for sel in &before {
            let focus = sel.focus;
            let new_focus = if focus.col > 0 {
                Position::new(focus.line, focus.col - 1)
            } else if focus.line > 0 {
                let prev_line = focus.line - 1;
                let prev_len = self.buffer.line_len(prev_line);
                Position::new(prev_line, prev_len)
            } else {
                Position::ZERO
            };

            let anchor = if select { sel.anchor } else { new_focus };
            new_cursors.push(Selection::range(anchor, new_focus));
        }

        self.cursors = MultiCursor::from_selections(new_cursors);
    }

    /// Moves all active cursors right by 1 character.
    pub fn move_right(&mut self, select: bool) {
        let before = self.cursors.all().to_vec();
        let mut new_cursors = Vec::with_capacity(before.len());

        for sel in &before {
            let focus = sel.focus;
            let cur_line_len = self.buffer.line_len(focus.line);

            let new_focus = if focus.col < cur_line_len {
                Position::new(focus.line, focus.col + 1)
            } else if focus.line + 1 < self.buffer.line_count() {
                Position::new(focus.line + 1, 0)
            } else {
                Position::new(focus.line, cur_line_len)
            };

            let anchor = if select { sel.anchor } else { new_focus };
            new_cursors.push(Selection::range(anchor, new_focus));
        }

        self.cursors = MultiCursor::from_selections(new_cursors);
    }

    /// Moves all active cursors up by 1 line.
    pub fn move_up(&mut self, select: bool) {
        let before = self.cursors.all().to_vec();
        let mut new_cursors = Vec::with_capacity(before.len());

        for sel in &before {
            let focus = sel.focus;
            let new_focus = if focus.line > 0 {
                let prev_line = focus.line - 1;
                let prev_len = self.buffer.line_len(prev_line);
                Position::new(prev_line, focus.col.min(prev_len))
            } else {
                Position::new(0, 0)
            };

            let anchor = if select { sel.anchor } else { new_focus };
            new_cursors.push(Selection::range(anchor, new_focus));
        }

        self.cursors = MultiCursor::from_selections(new_cursors);
    }

    /// Moves all active cursors down by 1 line.
    pub fn move_down(&mut self, select: bool) {
        let before = self.cursors.all().to_vec();
        let mut new_cursors = Vec::with_capacity(before.len());

        for sel in &before {
            let focus = sel.focus;
            let new_focus = if focus.line + 1 < self.buffer.line_count() {
                let next_line = focus.line + 1;
                let next_len = self.buffer.line_len(next_line);
                Position::new(next_line, focus.col.min(next_len))
            } else {
                let last_len = self.buffer.line_len(focus.line);
                Position::new(focus.line, last_len)
            };

            let anchor = if select { sel.anchor } else { new_focus };
            new_cursors.push(Selection::range(anchor, new_focus));
        }

        self.cursors = MultiCursor::from_selections(new_cursors);
    }

    /// Moves all active cursors to the start of their respective line.
    pub fn move_to_line_start(&mut self, select: bool) {
        let before = self.cursors.all().to_vec();
        let mut new_cursors = Vec::with_capacity(before.len());

        for sel in &before {
            let new_focus = Position::new(sel.focus.line, 0);
            let anchor = if select { sel.anchor } else { new_focus };
            new_cursors.push(Selection::range(anchor, new_focus));
        }

        self.cursors = MultiCursor::from_selections(new_cursors);
    }

    /// Moves all active cursors to the end of their respective line.
    pub fn move_to_line_end(&mut self, select: bool) {
        let before = self.cursors.all().to_vec();
        let mut new_cursors = Vec::with_capacity(before.len());

        for sel in &before {
            let line_len = self.buffer.line_len(sel.focus.line);
            let new_focus = Position::new(sel.focus.line, line_len);
            let anchor = if select { sel.anchor } else { new_focus };
            new_cursors.push(Selection::range(anchor, new_focus));
        }

        self.cursors = MultiCursor::from_selections(new_cursors);
    }

    /// Selects all text in the document.
    pub fn select_all(&mut self) {
        let total_lines = self.buffer.line_count();
        let last_line = total_lines.saturating_sub(1);
        let last_col = self.buffer.line_len(last_line);
        self.cursors.set_single(Position::ZERO);
        self.cursors.all_mut()[0] =
            Selection::range(Position::ZERO, Position::new(last_line, last_col));
    }

    /// Ensures structural fold ranges are computed and cached for the current buffer state.
    pub fn ensure_folds_cached(&mut self) -> &[FoldRange] {
        if !self.folds_valid {
            self.cached_folds = self.highlighter.compute_fold_ranges(&self.buffer);
            self.folds_valid = true;
            // Prune stale folded line indices that no longer correspond to a fold start.
            self.folded_lines.retain(|&line| {
                self.cached_folds.iter().any(|f| f.start_line == line)
            });
        }
        &self.cached_folds
    }

    /// Returns all structural fold ranges for the current document (using cache when valid).
    pub fn fold_ranges(&self) -> Vec<FoldRange> {
        if self.folds_valid {
            self.cached_folds.clone()
        } else {
            self.highlighter.compute_fold_ranges(&self.buffer)
        }
    }

    /// Toggles code folding on the specified line.
    pub fn toggle_fold(&mut self, line: usize) {
        if let Some(pos) = self.folded_lines.iter().position(|&l| l == line) {
            self.folded_lines.remove(pos);
        } else {
            self.folded_lines.push(line);
        }
    }

    /// Toggles folding at `line` only if `line` starts a foldable range; returns `true` if toggled.
    pub fn toggle_fold_at(&mut self, line: usize) -> bool {
        let is_fold_start = self
            .ensure_folds_cached()
            .iter()
            .any(|f| f.start_line == line && f.end_line > f.start_line);
        if !is_fold_start {
            return false;
        }
        self.toggle_fold(line);
        true
    }

    /// Collapses every foldable block in the document.
    pub fn fold_all(&mut self) {
        let starts: Vec<usize> = self
            .ensure_folds_cached()
            .iter()
            .filter(|f| f.end_line > f.start_line)
            .map(|f| f.start_line)
            .collect();
        self.folded_lines = starts;
        self.folded_lines.sort_unstable();
        self.folded_lines.dedup();
    }

    /// Expands all folded regions in the document.
    pub fn unfold_all(&mut self) {
        self.folded_lines.clear();
    }

    /// Checks if the line is currently folded.
    pub fn is_folded(&self, line: usize) -> bool {
        self.folded_lines.contains(&line)
    }

    /// If `line` is currently folded, returns the number of hidden lines beneath it.
    pub fn folded_hidden_count(&self, line: usize) -> Option<usize> {
        if !self.is_folded(line) {
            return None;
        }
        let folds = self.fold_ranges();
        folds
            .iter()
            .filter(|f| f.start_line == line && f.end_line > line)
            .map(|f| f.end_line - f.start_line)
            .max()
    }

    /// Returns the ordered list of visible 0-indexed logical line numbers after skipping
    /// lines concealed inside collapsed fold regions.
    pub fn visible_logical_lines(&self) -> Vec<usize> {
        let total = self.buffer.line_count();
        if self.folded_lines.is_empty() {
            return (0..total).collect();
        }
        let folds = self.fold_ranges();
        let mut collapsed_spans: Vec<(usize, usize)> = folds
            .iter()
            .filter(|f| self.folded_lines.contains(&f.start_line) && f.end_line > f.start_line)
            .map(|f| (f.start_line + 1, f.end_line.min(total.saturating_sub(1))))
            .collect();
        if collapsed_spans.is_empty() {
            return (0..total).collect();
        }
        collapsed_spans.sort_unstable_by_key(|&(s, _)| s);

        let mut visible = Vec::with_capacity(total);
        let mut line = 0usize;
        let mut span_idx = 0usize;
        while line < total {
            while span_idx < collapsed_spans.len() && collapsed_spans[span_idx].1 < line {
                span_idx += 1;
            }
            if span_idx < collapsed_spans.len()
                && line >= collapsed_spans[span_idx].0
                && line <= collapsed_spans[span_idx].1
            {
                line = collapsed_spans[span_idx].1 + 1;
                continue;
            }
            visible.push(line);
            line += 1;
        }
        visible
    }

    /// Maps a 0-indexed logical line number to its visible display row index (or `None` if hidden inside a fold).
    pub fn logical_to_display_row(&self, logical_line: usize) -> Option<usize> {
        if self.folded_lines.is_empty() {
            return (logical_line < self.buffer.line_count()).then_some(logical_line);
        }
        let visible = self.visible_logical_lines();
        visible.binary_search(&logical_line).ok()
    }

    /// Maps a visible display row index back to its 0-indexed logical line number.
    pub fn display_row_to_logical(&self, display_row: usize) -> Option<usize> {
        if self.folded_lines.is_empty() {
            return (display_row < self.buffer.line_count()).then_some(display_row);
        }
        let visible = self.visible_logical_lines();
        visible.get(display_row).copied()
    }

    // -------------------------------------------------------------------------
    // Find & Replace Controller Delegation
    // -------------------------------------------------------------------------

    pub fn open_find(&mut self) {
        self.search.open_find();
        if !self.search.query.is_empty() {
            self.search.recompute_matches(&self.buffer);
        }
    }

    pub fn open_replace(&mut self) {
        self.search.open_replace();
        if !self.search.query.is_empty() {
            self.search.recompute_matches(&self.buffer);
        }
    }

    pub fn close_find(&mut self) {
        self.search.close();
    }

    pub fn set_find_query(&mut self, query: impl Into<String>) {
        self.search.set_query(query, &self.buffer);
        if let Some(m) = self.search.active_match().cloned() {
            self.select_search_match(&m);
        }
    }

    pub fn set_replace_text(&mut self, replacement: impl Into<String>) {
        self.search.set_replacement(replacement);
    }

    pub fn find_next(&mut self) -> Option<SearchMatch> {
        if self.search.matches.is_empty() && !self.search.query.is_empty() {
            self.search.recompute_matches(&self.buffer);
        }
        let m = self.search.find_next()?;
        self.select_search_match(&m);
        Some(m)
    }

    pub fn find_prev(&mut self) -> Option<SearchMatch> {
        if self.search.matches.is_empty() && !self.search.query.is_empty() {
            self.search.recompute_matches(&self.buffer);
        }
        let m = self.search.find_prev()?;
        self.select_search_match(&m);
        Some(m)
    }

    pub fn replace_current(&mut self) -> bool {
        if self.read_only {
            return false;
        }
        let mut search = std::mem::take(&mut self.search);
        let replaced = search.replace_current(self);
        self.search = search;
        if let Some(m) = self.search.active_match().cloned() {
            self.select_search_match(&m);
        }
        replaced
    }

    pub fn replace_all(&mut self) -> usize {
        if self.read_only {
            return 0;
        }
        let mut search = std::mem::take(&mut self.search);
        let count = search.replace_all(self);
        self.search = search;
        count
    }

    fn select_search_match(&mut self, m: &SearchMatch) {
        self.cursors.set_single(m.start);
        self.cursors.all_mut()[0] = Selection::range(m.start, m.end);
        self.ensure_line_visible(m.start.line);
    }

    // -------------------------------------------------------------------------
    // Monaco-Class Editing, Navigation, Scrolling & Input Event Handlers
    // -------------------------------------------------------------------------

    /// Toggles single-line comments (`//` or `#`) across all lines touched by active selections.
    pub fn toggle_line_comment(&mut self) {
        if self.read_only {
            return;
        }
        let Some(prefix) = self.language.line_comment_prefix() else {
            return;
        };
        let mut lines: Vec<usize> = Vec::new();
        for sel in self.cursors.all() {
            let (s, e) = sel.ordered_range();
            for line in s.line..=e.line {
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
        }
        if lines.is_empty() {
            return;
        }
        lines.sort_unstable();

        let all_commented = lines.iter().all(|&l| {
            self.buffer
                .line(l)
                .map(|txt| {
                    let t = txt.trim_start();
                    t.is_empty() || t.starts_with(prefix)
                })
                .unwrap_or(true)
        });

        let mut edits = Vec::new();
        let prefix_chars = prefix.chars().count();
        for line in lines {
            let Some(txt) = self.buffer.line(line) else {
                continue;
            };
            let indent = txt.chars().take_while(|c| *c == ' ' || *c == '\t').count();
            if all_commented {
                let after_indent: String = txt.chars().skip(indent).collect();
                if after_indent.starts_with(prefix) {
                    let remove_len = if after_indent[prefix.len()..].starts_with(' ') {
                        prefix_chars + 1
                    } else {
                        prefix_chars
                    };
                    edits.push((
                        Position::new(line, indent),
                        Position::new(line, indent + remove_len),
                        String::new(),
                    ));
                }
            } else {
                edits.push((
                    Position::new(line, indent),
                    Position::new(line, indent),
                    format!("{prefix} "),
                ));
            }
        }
        self.replace_ranges_atomic(edits);
    }

    /// Indents selected lines by `tab_size` spaces (or inserts spaces at collapsed carets).
    pub fn indent_selection(&mut self) {
        if self.read_only {
            return;
        }
        let spaces = " ".repeat(self.tab_size.max(1));
        let has_multiline = self
            .cursors
            .all()
            .iter()
            .any(|s| s.anchor.line != s.focus.line);
        if !has_multiline && self.cursors.all().iter().all(|s| s.is_collapsed()) {
            self.insert_text(&spaces);
            return;
        }

        let mut lines = Vec::new();
        for sel in self.cursors.all() {
            let (s, e) = sel.ordered_range();
            for line in s.line..=e.line {
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
        }
        let edits: Vec<(Position, Position, String)> = lines
            .into_iter()
            .map(|l| (Position::new(l, 0), Position::new(l, 0), spaces.clone()))
            .collect();
        self.replace_ranges_atomic(edits);
    }

    /// Outdents selected lines by removing up to `tab_size` leading spaces (or one leading `\t`).
    pub fn outdent_selection(&mut self) {
        if self.read_only {
            return;
        }
        let mut lines = Vec::new();
        for sel in self.cursors.all() {
            let (s, e) = sel.ordered_range();
            for line in s.line..=e.line {
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
        }
        let mut edits = Vec::new();
        for line in lines {
            let Some(txt) = self.buffer.line(line) else {
                continue;
            };
            if txt.starts_with('\t') {
                edits.push((Position::new(line, 0), Position::new(line, 1), String::new()));
            } else {
                let leading_spaces = txt.chars().take_while(|&c| c == ' ').count();
                let remove = leading_spaces.min(self.tab_size.max(1));
                if remove > 0 {
                    edits.push((
                        Position::new(line, 0),
                        Position::new(line, remove),
                        String::new(),
                    ));
                }
            }
        }
        self.replace_ranges_atomic(edits);
    }

    /// Moves all cursors left by one word boundary.
    pub fn move_word_left(&mut self, select: bool) {
        let before = self.cursors.all().to_vec();
        let mut next = Vec::with_capacity(before.len());
        for sel in &before {
            let focus = sel.focus;
            let new_focus = if focus.col == 0 {
                if focus.line > 0 {
                    let prev = focus.line - 1;
                    Position::new(prev, self.buffer.line_len(prev))
                } else {
                    Position::ZERO
                }
            } else if let Some(line_str) = self.buffer.line(focus.line) {
                let chars: Vec<char> = line_str.chars().collect();
                let mut col = focus.col.min(chars.len());
                while col > 0 && chars[col - 1].is_whitespace() {
                    col -= 1;
                }
                if col > 0 {
                    let is_word = is_word_char(chars[col - 1]);
                    while col > 0 && is_word_char(chars[col - 1]) == is_word && !chars[col - 1].is_whitespace() {
                        col -= 1;
                    }
                }
                Position::new(focus.line, col)
            } else {
                Position::new(focus.line, 0)
            };
            let anchor = if select { sel.anchor } else { new_focus };
            next.push(Selection::range(anchor, new_focus));
        }
        self.cursors = MultiCursor::from_selections(next);
    }

    /// Moves all cursors right by one word boundary.
    pub fn move_word_right(&mut self, select: bool) {
        let before = self.cursors.all().to_vec();
        let mut next = Vec::with_capacity(before.len());
        for sel in &before {
            let focus = sel.focus;
            let cur_len = self.buffer.line_len(focus.line);
            let new_focus = if focus.col >= cur_len {
                if focus.line + 1 < self.buffer.line_count() {
                    Position::new(focus.line + 1, 0)
                } else {
                    Position::new(focus.line, cur_len)
                }
            } else if let Some(line_str) = self.buffer.line(focus.line) {
                let chars: Vec<char> = line_str.chars().collect();
                let mut col = focus.col.min(chars.len());
                while col < chars.len() && chars[col].is_whitespace() {
                    col += 1;
                }
                if col < chars.len() {
                    let is_word = is_word_char(chars[col]);
                    while col < chars.len() && is_word_char(chars[col]) == is_word && !chars[col].is_whitespace() {
                        col += 1;
                    }
                }
                Position::new(focus.line, col)
            } else {
                Position::new(focus.line, cur_len)
            };
            let anchor = if select { sel.anchor } else { new_focus };
            next.push(Selection::range(anchor, new_focus));
        }
        self.cursors = MultiCursor::from_selections(next);
    }

    /// Moves cursors up by one viewport page.
    pub fn page_up(&mut self, select: bool) {
        let step = ((self.viewport_height / self.line_height.max(1.0)).floor() as usize).max(1);
        let before = self.cursors.all().to_vec();
        let mut next = Vec::with_capacity(before.len());
        for sel in &before {
            let target_line = sel.focus.line.saturating_sub(step);
            let target_col = sel.focus.col.min(self.buffer.line_len(target_line));
            let new_focus = Position::new(target_line, target_col);
            let anchor = if select { sel.anchor } else { new_focus };
            next.push(Selection::range(anchor, new_focus));
        }
        self.cursors = MultiCursor::from_selections(next);
        self.ensure_cursor_visible();
    }

    /// Moves cursors down by one viewport page.
    pub fn page_down(&mut self, select: bool) {
        let step = ((self.viewport_height / self.line_height.max(1.0)).floor() as usize).max(1);
        let max_line = self.buffer.line_count().saturating_sub(1);
        let before = self.cursors.all().to_vec();
        let mut next = Vec::with_capacity(before.len());
        for sel in &before {
            let target_line = (sel.focus.line + step).min(max_line);
            let target_col = sel.focus.col.min(self.buffer.line_len(target_line));
            let new_focus = Position::new(target_line, target_col);
            let anchor = if select { sel.anchor } else { new_focus };
            next.push(Selection::range(anchor, new_focus));
        }
        self.cursors = MultiCursor::from_selections(next);
        self.ensure_cursor_visible();
    }

    /// Selects the identifier/word surrounding `pos` (used on double-click).
    pub fn select_word_at(&mut self, pos: Position) {
        let line = pos.line.min(self.buffer.line_count().saturating_sub(1));
        let Some(line_str) = self.buffer.line(line) else {
            return;
        };
        let chars: Vec<char> = line_str.chars().collect();
        if chars.is_empty() {
            self.cursors.set_single(Position::new(line, 0));
            return;
        }
        let idx = pos.col.min(chars.len().saturating_sub(1));
        let target_is_word = is_word_char(chars[idx]);
        let mut start = idx;
        while start > 0 && is_word_char(chars[start - 1]) == target_is_word && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let mut end = idx + 1;
        while end < chars.len() && is_word_char(chars[end]) == target_is_word && !chars[end].is_whitespace() {
            end += 1;
        }
        self.cursors.set_single(Position::new(line, start));
        self.cursors.all_mut()[0] =
            Selection::range(Position::new(line, start), Position::new(line, end));
    }

    /// Selects the entire logical line (used on triple-click).
    pub fn select_line(&mut self, line: usize) {
        let clamped = line.min(self.buffer.line_count().saturating_sub(1));
        let len = self.buffer.line_len(clamped);
        self.cursors.set_single(Position::new(clamped, 0));
        self.cursors.all_mut()[0] =
            Selection::range(Position::new(clamped, 0), Position::new(clamped, len));
    }

    /// Scrolls the viewport vertically by `delta_y` pixels.
    pub fn scroll_by(&mut self, delta_y: f32) {
        self.set_scroll_top(self.scroll_top + delta_y);
    }

    /// Scrolls so that `logical_line` is positioned near the top of the viewport.
    pub fn scroll_to_line(&mut self, logical_line: usize) {
        if let Some(display_row) = self.logical_to_display_row(logical_line) {
            self.set_scroll_top(display_row as f32 * self.line_height);
        }
    }

    /// Adjusts `scroll_top` minimally so `logical_line` sits inside the visible viewport.
    pub fn ensure_line_visible(&mut self, logical_line: usize) {
        let Some(display_row) = self.logical_to_display_row(logical_line) else {
            return;
        };
        let row_top = display_row as f32 * self.line_height;
        let row_bottom = row_top + self.line_height;
        if row_top < self.scroll_top {
            self.set_scroll_top(row_top);
        } else if row_bottom > self.scroll_top + self.viewport_height {
            self.set_scroll_top(row_bottom - self.viewport_height);
        }
    }

    /// Scrolls minimally to keep the primary caret visible.
    pub fn ensure_cursor_visible(&mut self) {
        let line = self.cursors.primary().focus.line;
        self.ensure_line_visible(line);
    }

    /// Maps an editor-local pointer `(rel_x, rel_y)` to a document [`Position`].
    pub fn position_from_point(&self, rel_x: f32, rel_y: f32, gutter_width: f32) -> Position {
        let display_row = ((rel_y + self.scroll_top).max(0.0) / self.line_height.max(1.0)).floor() as usize;
        let visible = self.visible_logical_lines();
        let line = if visible.is_empty() {
            0
        } else {
            visible[display_row.min(visible.len() - 1)]
        };
        let code_x = (rel_x - gutter_width + self.scroll_left).max(0.0);
        let raw_col = (code_x / self.char_width.max(1.0)).round() as usize;
        let max_col = self.buffer.line_len(line);
        Position::new(line, raw_col.min(max_col))
    }

    /// Handles pointer press inside the code viewport (click, Shift+click, Alt+click multi-cursor,
    /// double-click word selection, triple-click line selection).
    pub fn handle_pointer_down(
        &mut self,
        rel_x: f32,
        rel_y: f32,
        detail: i32,
        alt_key: bool,
        shift_key: bool,
        gutter_width: f32,
    ) {
        let pos = self.position_from_point(rel_x, rel_y, gutter_width);
        self.is_pointer_dragging = true;
        if detail >= 3 {
            self.select_line(pos.line);
        } else if detail == 2 {
            self.select_word_at(pos);
        } else if alt_key {
            self.cursors.add_cursor(pos);
        } else if shift_key {
            let anchor = self.cursors.primary().anchor;
            self.cursors.set_single(anchor);
            self.cursors.all_mut()[0] = Selection::range(anchor, pos);
        } else {
            self.cursors.set_single(pos);
        }
    }

    /// Extends the active primary selection while dragging the pointer.
    pub fn handle_pointer_drag(&mut self, rel_x: f32, rel_y: f32, gutter_width: f32) {
        if !self.is_pointer_dragging {
            return;
        }
        let pos = self.position_from_point(rel_x, rel_y, gutter_width);
        let anchor = self.cursors.primary().anchor;
        self.cursors.set_single(anchor);
        self.cursors.all_mut()[0] = Selection::range(anchor, pos);
    }

    /// Ends a pointer drag gesture.
    pub fn handle_pointer_up(&mut self) {
        self.is_pointer_dragging = false;
    }

    /// Handles wheel scrolling over the editor viewport.
    pub fn handle_wheel(&mut self, delta_y: f32) {
        self.scroll_by(delta_y);
    }

    /// Handles click or drag inside the Canvas2D minimap to jump/scrub `scroll_top`.
    pub fn handle_minimap_pointer(
        &mut self,
        pointer_y: f32,
        minimap_height: f32,
        config: &MinimapConfig,
    ) {
        let target = minimap_y_to_scroll_top(
            pointer_y,
            minimap_height,
            self.visible_logical_lines().len().max(1),
            self.viewport_height,
            self.line_height,
            config,
        );
        self.set_scroll_top(target);
    }

    /// Handles a W3C-compatible Strata [`Event`] keydown with full Monaco keybindings.
    /// Returns `true` if the event was handled and mutated editor state.
    pub fn handle_key_event(&mut self, e: &Event) -> bool {
        let cmd = e.ctrl_key || e.meta_key;
        let key = e.key.as_str();

        if cmd {
            match key {
                "f" | "F" => {
                    self.open_find();
                    return true;
                }
                "h" | "H" => {
                    self.open_replace();
                    return true;
                }
                "a" | "A" => {
                    self.select_all();
                    return true;
                }
                "z" | "Z" => {
                    if !self.read_only {
                        if e.shift_key {
                            self.redo();
                        } else {
                            self.undo();
                        }
                        self.ensure_cursor_visible();
                    }
                    return true;
                }
                "y" | "Y" => {
                    if !self.read_only {
                        self.redo();
                        self.ensure_cursor_visible();
                    }
                    return true;
                }
                "/" => {
                    self.toggle_line_comment();
                    return true;
                }
                "[" if e.shift_key => {
                    let line = self.cursors.primary().focus.line;
                    self.toggle_fold_at(line);
                    return true;
                }
                "]" if e.shift_key => {
                    let line = self.cursors.primary().focus.line;
                    if self.is_folded(line) {
                        self.toggle_fold(line);
                    }
                    return true;
                }
                "ArrowLeft" => {
                    self.move_word_left(e.shift_key);
                    self.ensure_cursor_visible();
                    return true;
                }
                "ArrowRight" => {
                    self.move_word_right(e.shift_key);
                    self.ensure_cursor_visible();
                    return true;
                }
                "Home" => {
                    let anchor = if e.shift_key {
                        self.cursors.primary().anchor
                    } else {
                        Position::ZERO
                    };
                    self.cursors.set_single(Position::ZERO);
                    self.cursors.all_mut()[0] = Selection::range(anchor, Position::ZERO);
                    self.ensure_cursor_visible();
                    return true;
                }
                "End" => {
                    let last_line = self.buffer.line_count().saturating_sub(1);
                    let end = Position::new(last_line, self.buffer.line_len(last_line));
                    let anchor = if e.shift_key {
                        self.cursors.primary().anchor
                    } else {
                        end
                    };
                    self.cursors.set_single(end);
                    self.cursors.all_mut()[0] = Selection::range(anchor, end);
                    self.ensure_cursor_visible();
                    return true;
                }
                _ => return false,
            }
        }

        match key {
            "Escape" => {
                if self.search.is_open {
                    self.close_find();
                    return true;
                }
                if self.cursors.count() > 1 {
                    let p = self.cursors.primary().focus;
                    self.cursors.set_single(p);
                    return true;
                }
                false
            }
            "ArrowLeft" => {
                self.move_left(e.shift_key);
                self.ensure_cursor_visible();
                true
            }
            "ArrowRight" => {
                self.move_right(e.shift_key);
                self.ensure_cursor_visible();
                true
            }
            "ArrowUp" => {
                self.move_up(e.shift_key);
                self.ensure_cursor_visible();
                true
            }
            "ArrowDown" => {
                self.move_down(e.shift_key);
                self.ensure_cursor_visible();
                true
            }
            "Home" => {
                self.move_to_line_start(e.shift_key);
                self.ensure_cursor_visible();
                true
            }
            "End" => {
                self.move_to_line_end(e.shift_key);
                self.ensure_cursor_visible();
                true
            }
            "PageUp" => {
                self.page_up(e.shift_key);
                true
            }
            "PageDown" => {
                self.page_down(e.shift_key);
                true
            }
            "Enter" => {
                if !self.read_only {
                    self.insert_newline();
                    self.ensure_cursor_visible();
                }
                true
            }
            "Backspace" => {
                if !self.read_only {
                    self.delete_backwards();
                    self.ensure_cursor_visible();
                }
                true
            }
            "Delete" => {
                if !self.read_only {
                    self.delete_forward();
                    self.ensure_cursor_visible();
                }
                true
            }
            "Tab" => {
                if !self.read_only {
                    if e.shift_key {
                        self.outdent_selection();
                    } else {
                        self.indent_selection();
                    }
                }
                true
            }
            other if !e.alt_key && other.chars().count() == 1 => {
                if let Some(ch) = other.chars().next() {
                    if !ch.is_control() {
                        if !self.read_only {
                            self.insert_char(ch);
                            self.ensure_cursor_visible();
                        }
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Renders a complete code editor view blueprint with default styling.
    pub fn render(&self) -> NodeBlueprint {
        self.render_blueprint("")
    }

    /// Renders a complete code editor view blueprint with line numbering gutter and syntax tokens.
    pub fn render_blueprint(&self, class_style: &str) -> NodeBlueprint {
        if self.buffer.line_count() > 100 {
            return self.render_virtualized(0.0, 800.0, 24.0, class_style);
        }

        let mut row_nodes = Vec::new();

        let visible_lines = self.buffer.slice_lines(0, self.buffer.line_count());
        let active_line = self.cursors.primary().focus.line;

        for (l_idx, line_str) in visible_lines {
            let is_active = l_idx == active_line;

            let row_class = if is_active {
                "flex items-center font-mono text-sm leading-6 bg-slate-800/40 hover:bg-slate-800/60"
            } else {
                "flex items-center font-mono text-sm leading-6 hover:bg-slate-800/30"
            };

            let visual_sublines = if let Some(cols) = self.wrap_column {
                crate::soft_wrap::wrap_logical_line(line_str, l_idx, cols, true)
            } else {
                vec![crate::soft_wrap::VisualLine {
                    logical_line: l_idx,
                    visual_subline: 0,
                    text: line_str.to_string(),
                    char_start: 0,
                    char_end: line_str.chars().count(),
                    is_continuation: false,
                }]
            };

            for vline in visual_sublines {
                let gutter_text = if vline.is_continuation {
                    "↳".to_string()
                } else {
                    (l_idx + 1).to_string()
                };

                let gutter_node = NodeBlueprint::element(
                    Tag::Span,
                    vec![
                        "w-12 text-right pr-4 select-none text-slate-500 font-mono text-xs"
                            .to_string(),
                    ],
                    Vec::new(),
                    vec![NodeBlueprint::text(gutter_text)],
                );

                let tokens = self.highlighter.highlight_line_ast(l_idx, &vline.text);
                let line_token_nodes = Self::build_token_nodes(&vline.text, tokens);

                let code_node = NodeBlueprint::element(
                    Tag::Span,
                    vec!["flex-1 pl-2 whitespace-pre".to_string()],
                    Vec::new(),
                    line_token_nodes,
                );

                row_nodes.push(NodeBlueprint::element(
                    Tag::Div,
                    vec![row_class.to_string()],
                    Vec::new(),
                    vec![gutter_node, code_node],
                ));
            }
        }

        let container_class = format!(
            "bg-slate-950 text-slate-200 border border-slate-800 rounded-lg p-4 font-mono overflow-x-auto {}",
            class_style
        );

        NodeBlueprint::element(Tag::Div, vec![container_class], Vec::new(), row_nodes)
    }

    /// Maps a semantic [`TokenType`] to a Tailwind utility class string.
    pub fn token_type_class(token_type: TokenType) -> &'static str {
        match token_type {
            TokenType::Keyword => "text-purple-400 font-semibold",
            TokenType::ControlFlow => "text-fuchsia-400 font-semibold",
            TokenType::Type => "text-amber-400 font-medium",
            TokenType::Function => "text-blue-400",
            TokenType::Macro => "text-indigo-300 font-medium",
            TokenType::Identifier => "text-slate-200",
            TokenType::Parameter => "text-orange-200",
            TokenType::Property => "text-sky-300",
            TokenType::String => "text-emerald-400",
            TokenType::StringEscape => "text-teal-300 font-semibold",
            TokenType::Number => "text-orange-400",
            TokenType::Boolean => "text-amber-300 font-semibold",
            TokenType::Comment => "text-slate-500 italic",
            TokenType::DocComment => "text-slate-400 italic",
            TokenType::Operator => "text-rose-400",
            TokenType::Punctuation => "text-slate-400",
            TokenType::Tag => "text-cyan-400",
            TokenType::Attribute => "text-sky-300",
            TokenType::Namespace => "text-amber-200",
            TokenType::Heading => "text-cyan-300 font-bold",
            TokenType::Link => "text-blue-300 underline",
            TokenType::Variable => "text-violet-300",
        }
    }

    fn build_token_nodes(
        line_text: &str,
        tokens: Vec<crate::syntax::SyntaxToken>,
    ) -> Vec<NodeBlueprint> {
        let mut line_token_nodes = Vec::new();
        if tokens.is_empty() {
            line_token_nodes.push(NodeBlueprint::text(line_text));
            return line_token_nodes;
        }

        let mut last_col = 0usize;
        for token in tokens {
            let start_col = token.start_col.min(line_text.len());
            let end_col = token.end_col.min(line_text.len());
            if start_col > last_col
                && line_text.is_char_boundary(last_col)
                && line_text.is_char_boundary(start_col)
            {
                line_token_nodes.push(NodeBlueprint::text(&line_text[last_col..start_col]));
            }

            let color_class = Self::token_type_class(token.token_type);
            line_token_nodes.push(NodeBlueprint::element(
                Tag::Span,
                vec![color_class.to_string()],
                Vec::new(),
                vec![NodeBlueprint::text(token.text)],
            ));
            last_col = end_col.max(last_col);
        }
        if last_col < line_text.len() && line_text.is_char_boundary(last_col) {
            line_token_nodes.push(NodeBlueprint::text(&line_text[last_col..]));
        }
        line_token_nodes
    }

    /// Renders a virtualized viewport slice of the code editor.
    ///
    /// Only the lines currently visible in the scroll viewport (plus an overscan margin)
    /// are laid out and styled into DOM blueprint nodes. Spacer containers at the top
    /// and bottom maintain the exact natural scroll height of the entire document.
    pub fn render_virtualized(
        &self,
        scroll_top: f32,
        viewport_height: f32,
        line_height: f32,
        class_style: &str,
    ) -> NodeBlueprint {
        let total_lines = self.buffer.line_count();
        let effective_lh = line_height.max(1.0);

        let first_visible = (scroll_top / effective_lh).floor() as usize;
        let visible_count = (viewport_height / effective_lh).ceil() as usize;

        // Apply a 2-line overscan buffer for smooth scrolling
        let start_line = first_visible.saturating_sub(2).min(total_lines);
        let end_line = (first_visible + visible_count + 2).min(total_lines);

        let top_spacer_height = start_line as f32 * effective_lh;
        let bottom_spacer_height = (total_lines.saturating_sub(end_line)) as f32 * effective_lh;

        let mut row_nodes = Vec::new();

        // Top spacer for virtual scroll height preservation
        if top_spacer_height > 0.0 {
            row_nodes.push(NodeBlueprint::element(
                Tag::Div,
                vec![format!(
                    "h-[{:.1}px] w-full pointer-events-none",
                    top_spacer_height
                )],
                Vec::new(),
                Vec::new(),
            ));
        }

        let visible_lines = self
            .buffer
            .slice_lines(start_line, end_line.saturating_sub(start_line));
        let active_line = self.cursors.primary().focus.line;

        for (l_idx, line_str) in visible_lines {
            let is_active = l_idx == active_line;

            let row_class = if is_active {
                "flex items-center font-mono text-sm leading-6 bg-slate-800/40 hover:bg-slate-800/60"
            } else {
                "flex items-center font-mono text-sm leading-6 hover:bg-slate-800/30"
            };

            let visual_sublines = if let Some(cols) = self.wrap_column {
                crate::soft_wrap::wrap_logical_line(line_str, l_idx, cols, true)
            } else {
                vec![crate::soft_wrap::VisualLine {
                    logical_line: l_idx,
                    visual_subline: 0,
                    text: line_str.to_string(),
                    char_start: 0,
                    char_end: line_str.chars().count(),
                    is_continuation: false,
                }]
            };

            for vline in visual_sublines {
                let gutter_text = if vline.is_continuation {
                    "↳".to_string()
                } else {
                    (l_idx + 1).to_string()
                };

                let gutter_class = if vline.is_continuation {
                    "w-12 text-right pr-4 select-none text-slate-600 font-mono text-xs"
                } else {
                    "w-12 text-right pr-4 select-none text-slate-500 font-mono text-xs"
                };

                let gutter_node = NodeBlueprint::element(
                    Tag::Span,
                    vec![gutter_class.to_string()],
                    Vec::new(),
                    vec![NodeBlueprint::text(gutter_text)],
                );

                // Tokens
                let tokens = self.highlighter.highlight_line_ast(l_idx, &vline.text);
                let line_token_nodes = Self::build_token_nodes(&vline.text, tokens);

                let code_node = NodeBlueprint::element(
                    Tag::Span,
                    vec!["flex-1 pl-2 whitespace-pre".to_string()],
                    Vec::new(),
                    line_token_nodes,
                );

                row_nodes.push(NodeBlueprint::element(
                    Tag::Div,
                    vec![row_class.to_string()],
                    Vec::new(),
                    vec![gutter_node, code_node],
                ));
            }
        }

        // Bottom spacer for virtual scroll height preservation
        if bottom_spacer_height > 0.0 {
            row_nodes.push(NodeBlueprint::element(
                Tag::Div,
                vec![format!(
                    "h-[{:.1}px] w-full pointer-events-none",
                    bottom_spacer_height
                )],
                Vec::new(),
                Vec::new(),
            ));
        }

        let container_class = format!(
            "bg-slate-950 text-slate-200 border border-slate-800 rounded-lg p-4 font-mono overflow-y-auto {}",
            class_style
        );

        NodeBlueprint::element(Tag::Div, vec![container_class], Vec::new(), row_nodes)
    }
}

#[inline]
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
