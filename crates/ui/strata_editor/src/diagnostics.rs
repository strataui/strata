//! LSP-compatible Diagnostics model merging real-time syntax diagnostics with external validators.

use serde::{Deserialize, Serialize};

use crate::cursor::Position;
use crate::syntax::SyntaxDiagnostic;

/// Diagnostic severity levels aligned with the Language Server Protocol (LSP).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum DiagnosticSeverity {
    #[default]
    Hint = 1,
    Info = 2,
    Warning = 3,
    Error = 4,
}

impl DiagnosticSeverity {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::Warning => "Warning",
            Self::Info => "Info",
            Self::Hint => "Hint",
        }
    }
}

/// A single diagnostic annotation anchored to a document range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorDiagnostic {
    pub start: Position,
    pub end: Position,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub code: Option<String>,
    pub source: Option<String>,
}

impl EditorDiagnostic {
    pub fn new(
        start: Position,
        end: Position,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            start,
            end,
            severity,
            message: message.into(),
            code: None,
            source: None,
        }
    }

    pub fn error(start: Position, end: Position, message: impl Into<String>) -> Self {
        Self::new(start, end, DiagnosticSeverity::Error, message)
    }

    pub fn warning(start: Position, end: Position, message: impl Into<String>) -> Self {
        Self::new(start, end, DiagnosticSeverity::Warning, message)
    }

    pub fn info(start: Position, end: Position, message: impl Into<String>) -> Self {
        Self::new(start, end, DiagnosticSeverity::Info, message)
    }

    pub fn hint(start: Position, end: Position, message: impl Into<String>) -> Self {
        Self::new(start, end, DiagnosticSeverity::Hint, message)
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn from_syntax_diagnostic(diag: &SyntaxDiagnostic) -> Self {
        Self {
            start: diag.start,
            end: diag.end,
            severity: if diag.is_warning {
                DiagnosticSeverity::Warning
            } else {
                DiagnosticSeverity::Error
            },
            message: diag.message.clone(),
            code: None,
            source: Some("syntax".to_string()),
        }
    }

    /// Returns whether this diagnostic overlaps the specified 0-indexed logical line.
    #[inline]
    pub fn covers_line(&self, line: usize) -> bool {
        line >= self.start.line && line <= self.end.line
    }
}

/// Summary count of diagnostics by severity for status bars and badges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DiagnosticCounts {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub hints: usize,
}

impl DiagnosticCounts {
    pub const fn total(&self) -> usize {
        self.errors + self.warnings + self.infos + self.hints
    }
}

/// Container storing live syntax diagnostics alongside user/LSP-provided diagnostics.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticCollection {
    syntax_diagnostics: Vec<EditorDiagnostic>,
    external_diagnostics: Vec<EditorDiagnostic>,
    include_syntax_diagnostics: bool,
}

impl DiagnosticCollection {
    pub fn new() -> Self {
        Self {
            syntax_diagnostics: Vec::new(),
            external_diagnostics: Vec::new(),
            include_syntax_diagnostics: true,
        }
    }

    /// Enables or disables automatic inclusion of real-time syntax diagnostics.
    pub fn set_include_syntax_diagnostics(&mut self, enabled: bool) {
        self.include_syntax_diagnostics = enabled;
    }

    /// Replaces the live syntax diagnostics produced by `SyntaxHighlighter`.
    pub fn set_syntax_diagnostics(&mut self, diags: Vec<SyntaxDiagnostic>) {
        self.syntax_diagnostics = diags
            .iter()
            .map(EditorDiagnostic::from_syntax_diagnostic)
            .collect();
    }

    /// Replaces external/LSP diagnostics without touching live syntax diagnostics.
    pub fn set_external(&mut self, diagnostics: Vec<EditorDiagnostic>) {
        self.external_diagnostics = diagnostics;
    }

    /// Appends a single external diagnostic.
    pub fn push_external(&mut self, diagnostic: EditorDiagnostic) {
        self.external_diagnostics.push(diagnostic);
    }

    /// Clears external diagnostics.
    pub fn clear_external(&mut self) {
        self.external_diagnostics.clear();
    }

    /// Iterates over all active diagnostics (syntax + external).
    pub fn iter(&self) -> impl Iterator<Item = &EditorDiagnostic> {
        let syntax_iter = if self.include_syntax_diagnostics {
            self.syntax_diagnostics.as_slice()
        } else {
            &[]
        };
        syntax_iter.iter().chain(self.external_diagnostics.iter())
    }

    /// Collects all active diagnostics sorted by `(start, severity desc)`.
    pub fn all_sorted(&self) -> Vec<EditorDiagnostic> {
        let mut out: Vec<EditorDiagnostic> = self.iter().cloned().collect();
        out.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| b.severity.cmp(&a.severity)));
        out
    }

    /// Returns all diagnostics overlapping `line`.
    pub fn diagnostics_on_line(&self, line: usize) -> Vec<&EditorDiagnostic> {
        self.iter().filter(|d| d.covers_line(line)).collect()
    }

    /// Returns the highest severity present on `line`, if any.
    pub fn max_severity_on_line(&self, line: usize) -> Option<DiagnosticSeverity> {
        self.iter()
            .filter(|d| d.covers_line(line))
            .map(|d| d.severity)
            .max()
    }

    /// Counts errors, warnings, info, and hints across the document.
    pub fn counts(&self) -> DiagnosticCounts {
        let mut counts = DiagnosticCounts::default();
        for diag in self.iter() {
            match diag.severity {
                DiagnosticSeverity::Error => counts.errors += 1,
                DiagnosticSeverity::Warning => counts.warnings += 1,
                DiagnosticSeverity::Info => counts.infos += 1,
                DiagnosticSeverity::Hint => counts.hints += 1,
            }
        }
        counts
    }
}
