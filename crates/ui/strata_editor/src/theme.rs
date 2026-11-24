//! CSS-variable-driven theming system (`--strata-editor-*`) for `<CodeEditorWidget>`.
//!
//! Every surface, gutter, selection, diagnostic, and syntax token color resolves through
//! CSS custom properties (`var(--strata-editor-*, <fallback>)`) so host applications
//! (such as Obsidian Dark / Daylight Light workspaces) can re-theme live without rebuilding
//! editor state.

use serde::{Deserialize, Serialize};
use strata_primitives::color::Rgba;

use crate::diagnostics::DiagnosticSeverity;
use crate::syntax::TokenType;

/// Complete theme definition for the Monaco-class code editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorTheme {
    pub name: String,
    pub is_dark: bool,

    // Surface CSS values (typically `var(--strata-editor-*, <fallback>)`)
    pub bg: String,
    pub fg: String,
    pub gutter_bg: String,
    pub gutter_fg: String,
    pub gutter_active_fg: String,
    pub line_highlight: String,
    pub selection_bg: String,
    pub caret: String,
    pub border: String,
    pub panel_bg: String,
    pub panel_border: String,
    pub find_match_bg: String,
    pub find_active_bg: String,
    pub bracket_match_bg: String,
    pub fold_badge_bg: String,
    pub fold_badge_fg: String,
    pub minimap_bg: String,
    pub minimap_slider: String,

    // Syntax token CSS values
    pub tok_keyword: String,
    pub tok_control: String,
    pub tok_type: String,
    pub tok_function: String,
    pub tok_macro: String,
    pub tok_identifier: String,
    pub tok_parameter: String,
    pub tok_property: String,
    pub tok_string: String,
    pub tok_string_escape: String,
    pub tok_number: String,
    pub tok_boolean: String,
    pub tok_comment: String,
    pub tok_doc_comment: String,
    pub tok_operator: String,
    pub tok_punctuation: String,
    pub tok_tag: String,
    pub tok_attribute: String,
    pub tok_namespace: String,
    pub tok_heading: String,
    pub tok_link: String,
    pub tok_variable: String,

    // Diagnostic CSS values
    pub diag_error: String,
    pub diag_warning: String,
    pub diag_info: String,
    pub diag_hint: String,
}

impl Default for EditorTheme {
    fn default() -> Self {
        Self::css_variables_dark()
    }
}

impl EditorTheme {
    /// Default dark theme referencing `--strata-editor-*` CSS variables with dark fallbacks.
    pub fn css_variables_dark() -> Self {
        Self {
            name: "strata-dark".to_string(),
            is_dark: true,
            bg: "var(--strata-editor-bg, #0d1016)".to_string(),
            fg: "var(--strata-editor-fg, #e2e8f0)".to_string(),
            gutter_bg: "var(--strata-editor-gutter-bg, #0b0e14)".to_string(),
            gutter_fg: "var(--strata-editor-gutter-fg, #64748b)".to_string(),
            gutter_active_fg: "var(--strata-editor-gutter-active-fg, #e2e8f0)".to_string(),
            line_highlight: "var(--strata-editor-line-highlight, rgba(148, 163, 184, 0.08))"
                .to_string(),
            selection_bg: "var(--strata-editor-selection-bg, rgba(56, 189, 248, 0.25))".to_string(),
            caret: "var(--strata-editor-caret, #38bdf8)".to_string(),
            border: "var(--strata-editor-border, #1e293b)".to_string(),
            panel_bg: "var(--strata-editor-panel-bg, #111827)".to_string(),
            panel_border: "var(--strata-editor-panel-border, #334155)".to_string(),
            find_match_bg: "var(--strata-editor-find-match-bg, rgba(251, 191, 36, 0.22))"
                .to_string(),
            find_active_bg: "var(--strata-editor-find-active-bg, rgba(245, 158, 11, 0.45))"
                .to_string(),
            bracket_match_bg: "var(--strata-editor-bracket-match-bg, rgba(99, 102, 241, 0.28))"
                .to_string(),
            fold_badge_bg: "var(--strata-editor-fold-badge-bg, rgba(51, 65, 85, 0.65))".to_string(),
            fold_badge_fg: "var(--strata-editor-fold-badge-fg, #cbd5e1)".to_string(),
            minimap_bg: "var(--strata-editor-minimap-bg, #090c11)".to_string(),
            minimap_slider: "var(--strata-editor-minimap-slider, rgba(148, 163, 184, 0.16))"
                .to_string(),
            tok_keyword: "var(--strata-editor-tok-keyword, #c084fc)".to_string(),
            tok_control: "var(--strata-editor-tok-control, #e879f9)".to_string(),
            tok_type: "var(--strata-editor-tok-type, #fbbf24)".to_string(),
            tok_function: "var(--strata-editor-tok-function, #60a5fa)".to_string(),
            tok_macro: "var(--strata-editor-tok-macro, #a5b4fc)".to_string(),
            tok_identifier: "var(--strata-editor-tok-identifier, #e2e8f0)".to_string(),
            tok_parameter: "var(--strata-editor-tok-parameter, #fed7aa)".to_string(),
            tok_property: "var(--strata-editor-tok-property, #7dd3fc)".to_string(),
            tok_string: "var(--strata-editor-tok-string, #34d399)".to_string(),
            tok_string_escape: "var(--strata-editor-tok-string-escape, #5eead4)".to_string(),
            tok_number: "var(--strata-editor-tok-number, #fb923c)".to_string(),
            tok_boolean: "var(--strata-editor-tok-boolean, #fcd34d)".to_string(),
            tok_comment: "var(--strata-editor-tok-comment, #64748b)".to_string(),
            tok_doc_comment: "var(--strata-editor-tok-doc-comment, #94a3b8)".to_string(),
            tok_operator: "var(--strata-editor-tok-operator, #fb7185)".to_string(),
            tok_punctuation: "var(--strata-editor-tok-punctuation, #94a3b8)".to_string(),
            tok_tag: "var(--strata-editor-tok-tag, #22d3ee)".to_string(),
            tok_attribute: "var(--strata-editor-tok-attribute, #7dd3fc)".to_string(),
            tok_namespace: "var(--strata-editor-tok-namespace, #fde68a)".to_string(),
            tok_heading: "var(--strata-editor-tok-heading, #67e8f9)".to_string(),
            tok_link: "var(--strata-editor-tok-link, #93c5fd)".to_string(),
            tok_variable: "var(--strata-editor-tok-variable, #c4b5fd)".to_string(),
            diag_error: "var(--strata-editor-diag-error, #f87171)".to_string(),
            diag_warning: "var(--strata-editor-diag-warning, #fbbf24)".to_string(),
            diag_info: "var(--strata-editor-diag-info, #38bdf8)".to_string(),
            diag_hint: "var(--strata-editor-diag-hint, #94a3b8)".to_string(),
        }
    }

    /// Default light theme referencing `--strata-editor-*` CSS variables with crisp daylight fallbacks.
    pub fn css_variables_light() -> Self {
        Self {
            name: "strata-light".to_string(),
            is_dark: false,
            bg: "var(--strata-editor-bg, #ffffff)".to_string(),
            fg: "var(--strata-editor-fg, #0f172a)".to_string(),
            gutter_bg: "var(--strata-editor-gutter-bg, #f8fafc)".to_string(),
            gutter_fg: "var(--strata-editor-gutter-fg, #94a3b8)".to_string(),
            gutter_active_fg: "var(--strata-editor-gutter-active-fg, #0f172a)".to_string(),
            line_highlight: "var(--strata-editor-line-highlight, rgba(15, 23, 42, 0.05))"
                .to_string(),
            selection_bg: "var(--strata-editor-selection-bg, rgba(14, 165, 233, 0.22))".to_string(),
            caret: "var(--strata-editor-caret, #0284c7)".to_string(),
            border: "var(--strata-editor-border, #e2e8f0)".to_string(),
            panel_bg: "var(--strata-editor-panel-bg, #f1f5f9)".to_string(),
            panel_border: "var(--strata-editor-panel-border, #cbd5e1)".to_string(),
            find_match_bg: "var(--strata-editor-find-match-bg, rgba(245, 158, 11, 0.22))"
                .to_string(),
            find_active_bg: "var(--strata-editor-find-active-bg, rgba(217, 119, 6, 0.40))"
                .to_string(),
            bracket_match_bg: "var(--strata-editor-bracket-match-bg, rgba(99, 102, 241, 0.18))"
                .to_string(),
            fold_badge_bg: "var(--strata-editor-fold-badge-bg, #e2e8f0)".to_string(),
            fold_badge_fg: "var(--strata-editor-fold-badge-fg, #334155)".to_string(),
            minimap_bg: "var(--strata-editor-minimap-bg, #f8fafc)".to_string(),
            minimap_slider: "var(--strata-editor-minimap-slider, rgba(15, 23, 42, 0.10))"
                .to_string(),
            tok_keyword: "var(--strata-editor-tok-keyword, #7c3aed)".to_string(),
            tok_control: "var(--strata-editor-tok-control, #a21caf)".to_string(),
            tok_type: "var(--strata-editor-tok-type, #b45309)".to_string(),
            tok_function: "var(--strata-editor-tok-function, #1d4ed8)".to_string(),
            tok_macro: "var(--strata-editor-tok-macro, #4338ca)".to_string(),
            tok_identifier: "var(--strata-editor-tok-identifier, #0f172a)".to_string(),
            tok_parameter: "var(--strata-editor-tok-parameter, #c2410c)".to_string(),
            tok_property: "var(--strata-editor-tok-property, #0369a1)".to_string(),
            tok_string: "var(--strata-editor-tok-string, #047857)".to_string(),
            tok_string_escape: "var(--strata-editor-tok-string-escape, #0f766e)".to_string(),
            tok_number: "var(--strata-editor-tok-number, #c2410c)".to_string(),
            tok_boolean: "var(--strata-editor-tok-boolean, #b45309)".to_string(),
            tok_comment: "var(--strata-editor-tok-comment, #64748b)".to_string(),
            tok_doc_comment: "var(--strata-editor-tok-doc-comment, #475569)".to_string(),
            tok_operator: "var(--strata-editor-tok-operator, #be123c)".to_string(),
            tok_punctuation: "var(--strata-editor-tok-punctuation, #475569)".to_string(),
            tok_tag: "var(--strata-editor-tok-tag, #0e7490)".to_string(),
            tok_attribute: "var(--strata-editor-tok-attribute, #0369a1)".to_string(),
            tok_namespace: "var(--strata-editor-tok-namespace, #a16207)".to_string(),
            tok_heading: "var(--strata-editor-tok-heading, #0e7490)".to_string(),
            tok_link: "var(--strata-editor-tok-link, #2563eb)".to_string(),
            tok_variable: "var(--strata-editor-tok-variable, #6d28d9)".to_string(),
            diag_error: "var(--strata-editor-diag-error, #dc2626)".to_string(),
            diag_warning: "var(--strata-editor-diag-warning, #d97706)".to_string(),
            diag_info: "var(--strata-editor-diag-info, #0284c7)".to_string(),
            diag_hint: "var(--strata-editor-diag-hint, #64748b)".to_string(),
        }
    }

    /// Obsidian dark engineering theme preset.
    pub fn obsidian_dark() -> Self {
        let mut theme = Self::css_variables_dark();
        theme.name = "obsidian-dark".to_string();
        theme.bg = "var(--strata-editor-bg, #0a0d12)".to_string();
        theme.gutter_bg = "var(--strata-editor-gutter-bg, #080a0f)".to_string();
        theme.border = "var(--strata-editor-border, #1e2430)".to_string();
        theme.caret = "var(--strata-editor-caret, #22d3ee)".to_string();
        theme
    }

    /// Daylight light workstation theme preset.
    pub fn daylight_light() -> Self {
        let mut theme = Self::css_variables_light();
        theme.name = "daylight-light".to_string();
        theme
    }

    /// Resolves the CSS `color` expression for a syntax [`TokenType`].
    pub fn token_css(&self, token_type: TokenType) -> &str {
        match token_type {
            TokenType::Keyword => &self.tok_keyword,
            TokenType::ControlFlow => &self.tok_control,
            TokenType::Type => &self.tok_type,
            TokenType::Function => &self.tok_function,
            TokenType::Macro => &self.tok_macro,
            TokenType::Identifier => &self.tok_identifier,
            TokenType::Parameter => &self.tok_parameter,
            TokenType::Property => &self.tok_property,
            TokenType::String => &self.tok_string,
            TokenType::StringEscape => &self.tok_string_escape,
            TokenType::Number => &self.tok_number,
            TokenType::Boolean => &self.tok_boolean,
            TokenType::Comment => &self.tok_comment,
            TokenType::DocComment => &self.tok_doc_comment,
            TokenType::Operator => &self.tok_operator,
            TokenType::Punctuation => &self.tok_punctuation,
            TokenType::Tag => &self.tok_tag,
            TokenType::Attribute => &self.tok_attribute,
            TokenType::Namespace => &self.tok_namespace,
            TokenType::Heading => &self.tok_heading,
            TokenType::Link => &self.tok_link,
            TokenType::Variable => &self.tok_variable,
        }
    }

    /// Resolves the CSS color expression for a diagnostic severity.
    pub fn diagnostic_css(&self, severity: DiagnosticSeverity) -> &str {
        match severity {
            DiagnosticSeverity::Error => &self.diag_error,
            DiagnosticSeverity::Warning => &self.diag_warning,
            DiagnosticSeverity::Info => &self.diag_info,
            DiagnosticSeverity::Hint => &self.diag_hint,
        }
    }

    /// Resolves an RGBA color for rasterizing a token bar inside the Canvas2D minimap.
    pub fn minimap_token_rgba(&self, token_type: TokenType) -> Rgba {
        if self.is_dark {
            match token_type {
                TokenType::Keyword => Rgba::new(192, 132, 252, 200),
                TokenType::ControlFlow => Rgba::new(232, 121, 249, 200),
                TokenType::Type | TokenType::Namespace => Rgba::new(251, 191, 36, 195),
                TokenType::Function | TokenType::Link => Rgba::new(96, 165, 250, 205),
                TokenType::Macro => Rgba::new(165, 180, 252, 195),
                TokenType::String | TokenType::StringEscape => Rgba::new(52, 211, 153, 195),
                TokenType::Number | TokenType::Boolean => Rgba::new(251, 146, 60, 195),
                TokenType::Comment | TokenType::DocComment => Rgba::new(100, 116, 139, 130),
                TokenType::Operator => Rgba::new(251, 113, 133, 180),
                TokenType::Punctuation => Rgba::new(148, 163, 184, 135),
                TokenType::Tag | TokenType::Heading => Rgba::new(34, 211, 238, 205),
                TokenType::Attribute | TokenType::Property => Rgba::new(125, 211, 252, 190),
                TokenType::Parameter | TokenType::Variable => Rgba::new(196, 181, 253, 185),
                TokenType::Identifier => Rgba::new(203, 213, 225, 165),
            }
        } else {
            match token_type {
                TokenType::Keyword => Rgba::new(124, 58, 237, 190),
                TokenType::ControlFlow => Rgba::new(162, 28, 175, 190),
                TokenType::Type | TokenType::Namespace => Rgba::new(180, 83, 9, 185),
                TokenType::Function | TokenType::Link => Rgba::new(29, 78, 216, 190),
                TokenType::Macro => Rgba::new(67, 56, 202, 185),
                TokenType::String | TokenType::StringEscape => Rgba::new(4, 120, 87, 185),
                TokenType::Number | TokenType::Boolean => Rgba::new(194, 65, 12, 185),
                TokenType::Comment | TokenType::DocComment => Rgba::new(100, 116, 139, 125),
                TokenType::Operator => Rgba::new(190, 18, 60, 170),
                TokenType::Punctuation => Rgba::new(71, 85, 105, 135),
                TokenType::Tag | TokenType::Heading => Rgba::new(14, 116, 144, 190),
                TokenType::Attribute | TokenType::Property => Rgba::new(3, 105, 161, 180),
                TokenType::Parameter | TokenType::Variable => Rgba::new(109, 40, 217, 175),
                TokenType::Identifier => Rgba::new(30, 41, 59, 155),
            }
        }
    }

    pub fn minimap_bg_rgba(&self) -> Rgba {
        if self.is_dark {
            Rgba::new(9, 12, 17, 255)
        } else {
            Rgba::new(248, 250, 252, 255)
        }
    }

    pub fn minimap_slider_rgba(&self) -> Rgba {
        if self.is_dark {
            Rgba::new(148, 163, 184, 42)
        } else {
            Rgba::new(15, 23, 42, 26)
        }
    }

    pub fn minimap_slider_border_rgba(&self) -> Rgba {
        if self.is_dark {
            Rgba::new(148, 163, 184, 96)
        } else {
            Rgba::new(15, 23, 42, 64)
        }
    }

    pub fn minimap_cursor_rgba(&self) -> Rgba {
        if self.is_dark {
            Rgba::new(56, 189, 248, 220)
        } else {
            Rgba::new(2, 132, 199, 220)
        }
    }

    pub fn minimap_search_rgba(&self) -> Rgba {
        Rgba::new(245, 158, 11, 225)
    }

    pub fn diagnostic_rgba(&self, severity: DiagnosticSeverity) -> Rgba {
        match severity {
            DiagnosticSeverity::Error => Rgba::new(248, 113, 113, 240),
            DiagnosticSeverity::Warning => Rgba::new(251, 191, 36, 235),
            DiagnosticSeverity::Info => Rgba::new(56, 189, 248, 225),
            DiagnosticSeverity::Hint => Rgba::new(148, 163, 184, 190),
        }
    }

    /// Exports a CSS custom property declaration block suitable for stylesheet injection.
    pub fn to_css_variables_block(&self) -> String {
        if self.is_dark {
            concat!(
                "--strata-editor-bg: #0d1016; ",
                "--strata-editor-fg: #e2e8f0; ",
                "--strata-editor-gutter-bg: #0b0e14; ",
                "--strata-editor-gutter-fg: #64748b; ",
                "--strata-editor-gutter-active-fg: #e2e8f0; ",
                "--strata-editor-line-highlight: rgba(148, 163, 184, 0.08); ",
                "--strata-editor-selection-bg: rgba(56, 189, 248, 0.25); ",
                "--strata-editor-caret: #38bdf8; ",
                "--strata-editor-border: #1e293b; ",
                "--strata-editor-tok-keyword: #c084fc; ",
                "--strata-editor-tok-type: #fbbf24; ",
                "--strata-editor-tok-function: #60a5fa; ",
                "--strata-editor-tok-string: #34d399; ",
                "--strata-editor-tok-number: #fb923c; ",
                "--strata-editor-tok-comment: #64748b; ",
                "--strata-editor-diag-error: #f87171; ",
                "--strata-editor-diag-warning: #fbbf24;"
            )
            .to_string()
        } else {
            concat!(
                "--strata-editor-bg: #ffffff; ",
                "--strata-editor-fg: #0f172a; ",
                "--strata-editor-gutter-bg: #f8fafc; ",
                "--strata-editor-gutter-fg: #94a3b8; ",
                "--strata-editor-gutter-active-fg: #0f172a; ",
                "--strata-editor-line-highlight: rgba(15, 23, 42, 0.05); ",
                "--strata-editor-selection-bg: rgba(14, 165, 233, 0.22); ",
                "--strata-editor-caret: #0284c7; ",
                "--strata-editor-border: #e2e8f0; ",
                "--strata-editor-tok-keyword: #7c3aed; ",
                "--strata-editor-tok-type: #b45309; ",
                "--strata-editor-tok-function: #1d4ed8; ",
                "--strata-editor-tok-string: #047857; ",
                "--strata-editor-tok-number: #c2410c; ",
                "--strata-editor-tok-comment: #64748b; ",
                "--strata-editor-diag-error: #dc2626; ",
                "--strata-editor-diag-warning: #d97706;"
            )
            .to_string()
        }
    }
}
