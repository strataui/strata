//! High-performance Code Editor controller with virtualized lines, syntax highlighting, and gutter.

use strata_dom::{NodeBlueprint, Tag};
use crate::buffer::LineBuffer;
use crate::cursor::{MultiCursor, Position, Selection};
use crate::history::HistoryStack;
use crate::ime::ImeComposition;
use crate::syntax::{Language, SyntaxHighlighter, TokenType};

/// High-level code editor managing buffer state, multi-cursors, and view rendering.
#[derive(Debug, Clone)]
pub struct CodeEditor {
    buffer: LineBuffer,
    cursors: MultiCursor,
    history: HistoryStack,
    highlighter: SyntaxHighlighter,
    language: Language,
    folded_lines: Vec<usize>,
    ime: ImeComposition,
    tab_size: usize,
    wrap_column: Option<usize>,
}

impl CodeEditor {
    /// Constructs a new code editor with initial source code and language syntax.
    pub fn new(content: &str, language: Language) -> Self {
        let mut highlighter = SyntaxHighlighter::with_language(language);
        highlighter.parse(content);
        Self {
            buffer: LineBuffer::from_str(content),
            cursors: MultiCursor::new(),
            history: HistoryStack::default(),
            highlighter,
            language,
            folded_lines: Vec::new(),
            ime: ImeComposition::new(),
            tab_size: 4,
            wrap_column: None,
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

    /// Changes the syntax language and re-parses the AST.
    pub fn set_language(&mut self, language: Language) {
        self.language = language;
        self.highlighter.set_language(language);
        self.highlighter.parse(&self.buffer.to_string());
    }

    /// Re-parses the document incrementally with the active Tree-Sitter grammar.
    pub fn reparse(&mut self) {
        self.highlighter.parse_incremental(&self.buffer.to_string());
    }

    /// Accesses the underlying line buffer.
    pub fn buffer(&self) -> &LineBuffer {
        &self.buffer
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
        self.highlighter.parse(text);
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
        let mut processed_carets: Vec<Position> = Vec::with_capacity(descending.len());

        for sel in descending {
            let (start, end) = sel.ordered_range();
            if start != end {
                let deleted = self.buffer.delete_range(start.line, start.col, end.line, end.col);
                operations.push(crate::history::EditOperation::Delete {
                    start,
                    end,
                    text: deleted,
                });
            }

            let indent_spaces = self.highlighter.compute_auto_indent(&self.buffer, start.line);
            self.buffer.split_line(start.line, start.col);

            let new_line = start.line + 1;
            let indent_str = if indent_spaces > 0 {
                let spaces = " ".repeat(indent_spaces);
                self.buffer.insert_str(new_line, 0, &spaces);
                spaces
            } else {
                String::new()
            };

            let inserted_text = format!("\n{indent_str}");
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

        let new_cursors: Vec<Selection> = processed_carets
            .into_iter()
            .map(Selection::point)
            .collect();

        self.cursors = MultiCursor::from_selections(new_cursors);

        self.history.push_transaction(crate::history::EditorTransaction::new(
            operations,
            before_all,
            self.cursors.all().to_vec(),
        ));

        self.reparse();
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
        let mut processed_carets: Vec<Position> = Vec::with_capacity(descending.len());

        let text_lines: Vec<&str> = text.split('\n').collect();

        for sel in descending {
            let (start, end) = sel.ordered_range();
            if start != end {
                let deleted = self.buffer.delete_range(start.line, start.col, end.line, end.col);
                operations.push(crate::history::EditOperation::Delete {
                    start,
                    end,
                    text: deleted,
                });
            }

            self.buffer.insert_str(start.line, start.col, text);
            operations.push(crate::history::EditOperation::Insert {
                pos: start,
                text: text.to_string(),
            });

            let my_new_pos = if text_lines.len() == 1 {
                Position::new(start.line, start.col + text_lines[0].len())
            } else {
                Position::new(
                    start.line + text_lines.len() - 1,
                    text_lines.last().map_or(0, |l| l.len()),
                )
            };

            for caret in &mut processed_carets {
                *caret = crate::cursor::shift_position_after_edit(*caret, start, end, &text_lines);
            }

            processed_carets.push(my_new_pos);
        }

        let new_cursors: Vec<Selection> = processed_carets
            .into_iter()
            .map(Selection::point)
            .collect();

        self.cursors = MultiCursor::from_selections(new_cursors);

        self.history.push_transaction(crate::history::EditorTransaction::new(
            operations,
            before_all,
            self.cursors.all().to_vec(),
        ));

        self.reparse();
    }

    /// Performs backspace deletion at all active cursors atomically in descending document order.
    pub fn delete_backwards(&mut self) {
        self.cursors.normalize();
        let before_all = self.cursors.all().to_vec();
        let descending = self.cursors.selections_descending();

        let mut operations = Vec::with_capacity(descending.len());
        let mut processed_carets: Vec<Position> = Vec::with_capacity(descending.len());

        for sel in descending {
            let (start, end) = sel.ordered_range();
            if start != end {
                let deleted = self.buffer.delete_range(start.line, start.col, end.line, end.col);
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
                let deleted = self.buffer.delete_range(del_start.line, del_start.col, start.line, start.col);
                operations.push(crate::history::EditOperation::Delete {
                    start: del_start,
                    end: start,
                    text: deleted,
                });

                for caret in &mut processed_carets {
                    *caret = crate::cursor::shift_position_after_edit(*caret, del_start, start, &[""]);
                }

                processed_carets.push(del_start);
            } else if start.line > 0 {
                let prev_line = start.line - 1;
                let prev_len = self.buffer.line_len(prev_line);
                let del_start = Position::new(prev_line, prev_len);

                self.buffer.join_line_with_next(prev_line);
                operations.push(crate::history::EditOperation::Delete {
                    start: del_start,
                    end: Position::new(start.line, 0),
                    text: "\n".to_string(),
                });

                for caret in &mut processed_carets {
                    *caret = crate::cursor::shift_position_after_edit(
                        *caret,
                        del_start,
                        Position::new(start.line, 0),
                        &[""],
                    );
                }

                processed_carets.push(del_start);
            } else {
                processed_carets.push(start);
            }
        }

        let new_cursors: Vec<Selection> = processed_carets
            .into_iter()
            .map(Selection::point)
            .collect();

        self.cursors = MultiCursor::from_selections(new_cursors);

        if !operations.is_empty() {
            self.history.push_transaction(crate::history::EditorTransaction::new(
                operations,
                before_all,
                self.cursors.all().to_vec(),
            ));
        }

        self.reparse();
    }

    /// Undoes the last recorded transaction.
    pub fn undo(&mut self) {
        if let Some(restored) = self.history.undo(&mut self.buffer) {
            self.cursors = MultiCursor::from_selections(restored);
            self.reparse();
        }
    }

    /// Redoes the last undone transaction.
    pub fn redo(&mut self) {
        if let Some(restored) = self.history.redo(&mut self.buffer) {
            self.cursors = MultiCursor::from_selections(restored);
            self.reparse();
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
        self.cursors.all_mut()[0] = Selection::range(Position::ZERO, Position::new(last_line, last_col));
    }

    /// Toggles code folding on the specified line.
    pub fn toggle_fold(&mut self, line: usize) {
        if let Some(pos) = self.folded_lines.iter().position(|&l| l == line) {
            self.folded_lines.remove(pos);
        } else {
            self.folded_lines.push(line);
        }
    }

    /// Checks if the line is currently folded.
    pub fn is_folded(&self, line: usize) -> bool {
        self.folded_lines.contains(&line)
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
                    vec!["w-12 text-right pr-4 select-none text-slate-500 font-mono text-xs".to_string()],
                    Vec::new(),
                    vec![NodeBlueprint::text(gutter_text)],
                );

                let tokens = self.highlighter.highlight_line_ast(l_idx, &vline.text);
                let mut line_token_nodes = Vec::new();

                if tokens.is_empty() {
                    line_token_nodes.push(NodeBlueprint::text(vline.text));
                } else {
                    let mut last_col = 0;
                    for token in tokens {
                        if token.start_col > last_col && token.start_col <= vline.text.len() {
                            line_token_nodes.push(NodeBlueprint::text(&vline.text[last_col..token.start_col]));
                        }

                        let color_class = match token.token_type {
                            TokenType::Keyword => "text-purple-400 font-semibold",
                            TokenType::Type => "text-amber-400 font-medium",
                            TokenType::Function => "text-blue-400",
                            TokenType::String => "text-emerald-400",
                            TokenType::Number => "text-orange-400",
                            TokenType::Comment => "text-slate-500 italic",
                            TokenType::Operator => "text-rose-400",
                            TokenType::Punctuation => "text-slate-400",
                            TokenType::Tag => "text-cyan-400",
                            TokenType::Attribute => "text-sky-300",
                            TokenType::Identifier => "text-slate-200",
                        };

                        line_token_nodes.push(NodeBlueprint::element(
                            Tag::Span,
                            vec![color_class.to_string()],
                            Vec::new(),
                            vec![NodeBlueprint::text(token.text)],
                        ));
                        last_col = token.end_col.min(vline.text.len());
                    }
                    if last_col < vline.text.len() {
                        line_token_nodes.push(NodeBlueprint::text(&vline.text[last_col..]));
                    }
                }

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

        NodeBlueprint::element(
            Tag::Div,
            vec![container_class],
            Vec::new(),
            row_nodes,
        )
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
                vec![format!("h-[{:.1}px] w-full pointer-events-none", top_spacer_height)],
                Vec::new(),
                Vec::new(),
            ));
        }

        let visible_lines = self.buffer.slice_lines(start_line, end_line.saturating_sub(start_line));
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
                let mut line_token_nodes = Vec::new();

                if tokens.is_empty() {
                    line_token_nodes.push(NodeBlueprint::text(vline.text));
                } else {
                    let mut last_col = 0;
                    for token in tokens {
                        if token.start_col > last_col && token.start_col <= vline.text.len() {
                            line_token_nodes.push(NodeBlueprint::text(&vline.text[last_col..token.start_col]));
                        }

                        let color_class = match token.token_type {
                            TokenType::Keyword => "text-purple-400 font-semibold",
                            TokenType::Type => "text-amber-400 font-medium",
                            TokenType::Function => "text-blue-400",
                            TokenType::String => "text-emerald-400",
                            TokenType::Number => "text-orange-400",
                            TokenType::Comment => "text-slate-500 italic",
                            TokenType::Operator => "text-rose-400",
                            TokenType::Punctuation => "text-slate-400",
                            TokenType::Tag => "text-cyan-400",
                            TokenType::Attribute => "text-sky-300",
                            TokenType::Identifier => "text-slate-200",
                        };

                        line_token_nodes.push(NodeBlueprint::element(
                            Tag::Span,
                            vec![color_class.to_string()],
                            Vec::new(),
                            vec![NodeBlueprint::text(token.text)],
                        ));
                        last_col = token.end_col.min(vline.text.len());
                    }
                    if last_col < vline.text.len() {
                        line_token_nodes.push(NodeBlueprint::text(&vline.text[last_col..]));
                    }
                }

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
                vec![format!("h-[{:.1}px] w-full pointer-events-none", bottom_spacer_height)],
                Vec::new(),
                Vec::new(),
            ));
        }

        let container_class = format!(
            "bg-slate-950 text-slate-200 border border-slate-800 rounded-lg p-4 font-mono overflow-y-auto {}",
            class_style
        );

        NodeBlueprint::element(
            Tag::Div,
            vec![container_class],
            Vec::new(),
            row_nodes,
        )
    }
}
