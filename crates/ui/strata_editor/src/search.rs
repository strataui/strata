//! Find & Replace engine supporting literal, case-sensitive, whole-word, and regex search.

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};

use crate::buffer::LineBuffer;
use crate::code_editor::CodeEditor;
use crate::cursor::Position;

/// A single match range inside a document buffer (char-indexed columns).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMatch {
    pub start: Position,
    pub end: Position,
    pub text: String,
}

/// Interactive Find & Replace controller state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindReplaceState {
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub use_regex: bool,
    pub is_open: bool,
    pub show_replace: bool,
    pub active_match_index: usize,
    pub matches: Vec<SearchMatch>,
    pub regex_error: Option<String>,
}

impl FindReplaceState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_find(&mut self) {
        self.is_open = true;
    }

    pub fn open_replace(&mut self) {
        self.is_open = true;
        self.show_replace = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn set_query(&mut self, query: impl Into<String>, buffer: &LineBuffer) {
        self.query = query.into();
        self.recompute_matches(buffer);
    }

    pub fn set_replacement(&mut self, replacement: impl Into<String>) {
        self.replacement = replacement.into();
    }

    pub fn toggle_case_sensitive(&mut self, buffer: &LineBuffer) {
        self.case_sensitive = !self.case_sensitive;
        self.recompute_matches(buffer);
    }

    pub fn toggle_whole_word(&mut self, buffer: &LineBuffer) {
        self.whole_word = !self.whole_word;
        self.recompute_matches(buffer);
    }

    pub fn toggle_regex(&mut self, buffer: &LineBuffer) {
        self.use_regex = !self.use_regex;
        self.recompute_matches(buffer);
    }

    /// Recomputes all matches across `buffer` cleanly with Unicode char-column accuracy.
    pub fn recompute_matches(&mut self, buffer: &LineBuffer) {
        self.matches.clear();
        self.regex_error = None;

        if self.query.is_empty() {
            self.active_match_index = 0;
            return;
        }

        let pattern = if self.use_regex {
            if self.whole_word {
                format!(r"\b(?:{})\b", self.query)
            } else {
                self.query.clone()
            }
        } else {
            let escaped = regex::escape(&self.query);
            if self.whole_word {
                format!(r"\b{escaped}\b")
            } else {
                escaped
            }
        };

        let compiled = match RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .build()
        {
            Ok(re) => re,
            Err(err) => {
                self.regex_error = Some(err.to_string());
                self.active_match_index = 0;
                return;
            }
        };

        let line_count = buffer.line_count();
        for row in 0..line_count {
            let Some(line) = buffer.line(row) else {
                continue;
            };
            if line.is_empty() {
                continue;
            }

            // Build a lightweight byte->char column lookup only if the line has matches.
            let mut match_iter = compiled.find_iter(line).peekable();
            if match_iter.peek().is_none() {
                continue;
            }

            for m in match_iter {
                if m.start() == m.end() {
                    // Guard against zero-width regex matches flooding results.
                    continue;
                }
                let start_col = byte_to_char_col(line, m.start());
                let end_col = byte_to_char_col(line, m.end());
                self.matches.push(SearchMatch {
                    start: Position::new(row, start_col),
                    end: Position::new(row, end_col),
                    text: m.as_str().to_string(),
                });
            }
        }

        if self.matches.is_empty() {
            self.active_match_index = 0;
        } else if self.active_match_index >= self.matches.len() {
            self.active_match_index = 0;
        }
    }

    /// Returns the currently focused match, if any.
    pub fn active_match(&self) -> Option<&SearchMatch> {
        self.matches.get(self.active_match_index)
    }

    /// Advances to the next match (wrapping around to match 0).
    pub fn find_next(&mut self) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        self.active_match_index = (self.active_match_index + 1) % self.matches.len();
        self.matches.get(self.active_match_index).cloned()
    }

    /// Moves to the previous match (wrapping around to the last match).
    pub fn find_prev(&mut self) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        self.active_match_index = if self.active_match_index == 0 {
            self.matches.len() - 1
        } else {
            self.active_match_index - 1
        };
        self.matches.get(self.active_match_index).cloned()
    }

    /// Moves `active_match_index` to the first match starting at or after `pos`.
    pub fn select_nearest_after(&mut self, pos: Position) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        let idx = self
            .matches
            .partition_point(|m| m.start < pos)
            .min(self.matches.len() - 1);
        self.active_match_index = idx;
        self.matches.get(idx).cloned()
    }

    /// Binary-searches all matches on logical `line`, returning `(match_index, &SearchMatch)`.
    pub fn matches_on_line(&self, line: usize) -> Vec<(usize, &SearchMatch)> {
        if self.matches.is_empty() {
            return Vec::new();
        }
        let first = self.matches.partition_point(|m| m.end.line < line);
        let mut out = Vec::new();
        for (offset, m) in self.matches[first..].iter().enumerate() {
            if m.start.line > line {
                break;
            }
            out.push((first + offset, m));
        }
        out
    }

    /// Replaces the active match in `editor`, recomputes matches, and selects the next match.
    pub fn replace_current(&mut self, editor: &mut CodeEditor) -> bool {
        let Some(target) = self.active_match().cloned() else {
            return false;
        };
        let replacement = self.expand_replacement(&target.text);
        let new_end = editor.replace_range(target.start, target.end, &replacement);
        self.recompute_matches(editor.buffer());
        let _ = self.select_nearest_after(new_end);
        true
    }

    /// Replaces every match in `editor` as ONE atomic undo transaction and single incremental reparse.
    pub fn replace_all(&mut self, editor: &mut CodeEditor) -> usize {
        if self.matches.is_empty() {
            return 0;
        }
        let count = self.matches.len();
        let batch: Vec<(Position, Position, String)> = self
            .matches
            .iter()
            .map(|m| (m.start, m.end, self.expand_replacement(&m.text)))
            .collect();
        editor.replace_ranges_atomic(batch);
        self.recompute_matches(editor.buffer());
        count
    }

    /// Formats a Monaco-style status label like `"2 of 14"` or `"No results"`.
    pub fn status_label(&self) -> String {
        if self.query.is_empty() {
            return String::new();
        }
        if self.regex_error.is_some() {
            return "Invalid regex".to_string();
        }
        if self.matches.is_empty() {
            "No results".to_string()
        } else {
            format!("{} of {}", self.active_match_index + 1, self.matches.len())
        }
    }

    fn expand_replacement(&self, matched_text: &str) -> String {
        if !self.use_regex {
            return self.replacement.clone();
        }
        let pattern = if self.whole_word {
            format!(r"\b(?:{})\b", self.query)
        } else {
            self.query.clone()
        };
        if let Ok(re) = RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .build()
        {
            re.replace(matched_text, self.replacement.as_str())
                .into_owned()
        } else {
            self.replacement.clone()
        }
    }
}

#[inline]
fn byte_to_char_col(line: &str, byte_idx: usize) -> usize {
    let clamped = byte_idx.min(line.len());
    line[..clamped].chars().count()
}
