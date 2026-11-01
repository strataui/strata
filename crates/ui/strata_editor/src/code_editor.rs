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
}

impl CodeEditor {
    /// Constructs a new code editor with initial source code and language syntax.
    pub fn new(content: &str, language: Language) -> Self {
        Self {
            buffer: LineBuffer::from_str(content),
            cursors: MultiCursor::new(),
            history: HistoryStack::default(),
            highlighter: SyntaxHighlighter::new(),
            language,
            folded_lines: Vec::new(),
            ime: ImeComposition::new(),
            tab_size: 4,
        }
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
    }

    /// Inserts a character at all active cursors (or handles Enter with auto-indentation).
    pub fn insert_char(&mut self, ch: char) {
        if ch == '\n' {
            self.insert_newline();
            return;
        }

        let before = self.cursors.all().to_vec();
        let mut new_cursors = Vec::with_capacity(before.len());

        for sel in &before {
            let (start, end) = sel.ordered_range();
            if start != end {
                self.buffer.delete_range(start.line, start.col, end.line, end.col);
            }

            self.buffer.insert_char(start.line, start.col, ch);
            let next_pos = Position::new(start.line, start.col + 1);
            new_cursors.push(Selection::point(next_pos));

            self.history.record_insert(
                start,
                &ch.to_string(),
                vec![*sel],
                vec![Selection::point(next_pos)],
            );
        }

        self.cursors = MultiCursor::new();
        for c in new_cursors {
            self.cursors.add_selection(c);
        }
    }

    /// Inserts a newline with automatic indentation calculated from the syntax engine.
    pub fn insert_newline(&mut self) {
        let before = self.cursors.all().to_vec();
        let mut new_cursors = Vec::with_capacity(before.len());

        for sel in &before {
            let (start, end) = sel.ordered_range();
            if start != end {
                self.buffer.delete_range(start.line, start.col, end.line, end.col);
            }

            let indent_spaces = self.highlighter.compute_auto_indent(&self.buffer, start.line);
            self.buffer.split_line(start.line, start.col);

            let new_line = start.line + 1;
            if indent_spaces > 0 {
                let spaces = " ".repeat(indent_spaces);
                self.buffer.insert_str(new_line, 0, &spaces);
            }

            let next_pos = Position::new(new_line, indent_spaces);
            new_cursors.push(Selection::point(next_pos));
        }

        self.cursors = MultiCursor::new();
        for c in new_cursors {
            self.cursors.add_selection(c);
        }
    }

    /// Inserts text at all cursor positions.
    pub fn insert_text(&mut self, text: &str) {
        let before = self.cursors.all().to_vec();
        let mut new_cursors = Vec::with_capacity(before.len());

        for sel in &before {
            let (start, end) = sel.ordered_range();
            if start != end {
                self.buffer.delete_range(start.line, start.col, end.line, end.col);
            }

            self.buffer.insert_str(start.line, start.col, text);
            let lines: Vec<&str> = text.split('\n').collect();
            let next_line = start.line + lines.len() - 1;
            let next_col = if lines.len() == 1 {
                start.col + lines[0].len()
            } else {
                lines.last().unwrap().len()
            };

            let next_pos = Position::new(next_line, next_col);
            new_cursors.push(Selection::point(next_pos));
        }

        self.cursors = MultiCursor::new();
        for c in new_cursors {
            self.cursors.add_selection(c);
        }
    }

    /// Performs backspace deletion at all active cursors.
    pub fn delete_backwards(&mut self) {
        let before = self.cursors.all().to_vec();
        let mut new_cursors = Vec::with_capacity(before.len());

        for sel in &before {
            let (start, end) = sel.ordered_range();
            if start != end {
                let deleted = self.buffer.delete_range(start.line, start.col, end.line, end.col);
                self.history.record_delete(start, end, &deleted, vec![*sel], vec![Selection::point(start)]);
                new_cursors.push(Selection::point(start));
            } else if let Some((l, c)) = self.buffer.delete_backwards(start.line, start.col) {
                new_cursors.push(Selection::point(Position::new(l, c)));
            } else {
                new_cursors.push(*sel);
            }
        }

        self.cursors = MultiCursor::new();
        for c in new_cursors {
            self.cursors.add_selection(c);
        }
    }

    /// Undoes the last recorded transaction.
    pub fn undo(&mut self) {
        if let Some(restored) = self.history.undo(&mut self.buffer) {
            self.cursors = MultiCursor::new();
            for c in restored {
                self.cursors.add_selection(c);
            }
        }
    }

    /// Redoes the last undone transaction.
    pub fn redo(&mut self) {
        if let Some(restored) = self.history.redo(&mut self.buffer) {
            self.cursors = MultiCursor::new();
            for c in restored {
                self.cursors.add_selection(c);
            }
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
        let mut row_nodes = Vec::new();

        let visible_lines = self.buffer.slice_lines(0, self.buffer.line_count());
        let active_line = self.cursors.primary().focus.line;

        for (l_idx, line_str) in visible_lines {
            let line_num = l_idx + 1;
            let is_active = l_idx == active_line;

            let row_class = if is_active {
                "flex items-center font-mono text-sm leading-6 bg-slate-800/40 hover:bg-slate-800/60"
            } else {
                "flex items-center font-mono text-sm leading-6 hover:bg-slate-800/30"
            };

            // Gutter: line number
            let gutter_node = NodeBlueprint::element(
                Tag::Span,
                vec!["w-12 text-right pr-4 select-none text-slate-500 font-mono text-xs".to_string()],
                Vec::new(),
                vec![NodeBlueprint::text(line_num.to_string())],
            );

            // Tokens
            let tokens = self.highlighter.highlight_line(line_str, self.language);
            let mut line_token_nodes = Vec::new();

            if tokens.is_empty() {
                line_token_nodes.push(NodeBlueprint::text(line_str));
            } else {
                for token in tokens {
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
}
