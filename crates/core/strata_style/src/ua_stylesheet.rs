//! Normative W3C HTML5 User-Agent Default Stylesheet (`html.css`).
//!
//! Implements W3C HTML Standard Section 15 ("Rendering") default element display modes,
//! typography scales, margins, table formatting, and phrasing rules.

use crate::parser::StyleSheet;
use std::sync::OnceLock;

/// Complete W3C HTML5 User-Agent default stylesheet per HTML Standard Section 15.
pub const W3C_UA_STYLESHEET_CSS: &str = r#"
/* Document & Canvas Defaults */
html, body {
    display: block;
    margin: 0;
}

/* Block-level Structural Elements */
article, aside, details, figcaption, figure, footer, header, hgroup, main, nav, section, summary,
div, p, h1, h2, h3, h4, h5, h6, ul, ol, li, dl, dt, dd, blockquote, hr, pre, address,
fieldset, legend {
    display: block;
}

/* Inline Phrasing Elements */
span, a, b, strong, i, em, mark, small, del, s, ins, sub, sup, code, kbd, samp, var,
cite, abbr, time, q, dfn, bdo, bdi {
    display: inline;
}

/* Relative Heading Typography & Margins */
h1 {
    font-size: 2.00em;
    margin-block-start: 0.67em;
    margin-block-end: 0.67em;
    font-weight: bold;
}

h2 {
    font-size: 1.50em;
    margin-block-start: 0.83em;
    margin-block-end: 0.83em;
    font-weight: bold;
}

h3 {
    font-size: 1.17em;
    margin-block-start: 1.00em;
    margin-block-end: 1.00em;
    font-weight: bold;
}

h4 {
    font-size: 1.00em;
    margin-block-start: 1.33em;
    margin-block-end: 1.33em;
    font-weight: bold;
}

h5 {
    font-size: 0.83em;
    margin-block-start: 1.67em;
    margin-block-end: 1.67em;
    font-weight: bold;
}

h6 {
    font-size: 0.67em;
    margin-block-start: 2.33em;
    margin-block-end: 2.33em;
    font-weight: bold;
}

/* Paragraphs, Blocks & Lists */
p {
    margin-block-start: 1.00em;
    margin-block-end: 1.00em;
}

ul, ol {
    margin-block-start: 1.00em;
    margin-block-end: 1.00em;
    padding-inline-start: 40px;
}

li {
    display: list-item;
}

dd {
    margin-inline-start: 40px;
}

blockquote {
    margin-block-start: 1.00em;
    margin-block-end: 1.00em;
    margin-inline-start: 40px;
    margin-inline-end: 40px;
}

hr {
    display: block;
    margin-block-start: 0.5em;
    margin-block-end: 0.5em;
    border-style: inset;
    border-width: 1px;
}

pre {
    font-family: monospace;
    white-space: pre;
    margin-block-start: 1.00em;
    margin-block-end: 1.00em;
}

code, kbd, samp, tt {
    font-family: monospace;
}

/* Text Formatting & Inline Phrasing */
b, strong {
    font-weight: bold;
}

i, em, cite, var, dfn, address {
    font-style: italic;
}

del, s, strike {
    text-decoration: line-through;
}

ins, u {
    text-decoration: underline;
}

mark {
    background-color: #ffff00;
    color: #000000;
}

small {
    font-size: 0.83em;
}

sub {
    vertical-align: sub;
    font-size: 0.83em;
}

sup {
    vertical-align: super;
    font-size: 0.83em;
}

/* Hyperlinks */
a {
    color: #3b82f6;
    text-decoration: underline;
    cursor: pointer;
}

/* Tabular Layout Primitives (CSS Tables Level 2) */
table {
    display: table;
    border-collapse: separate;
    border-spacing: 2px;
    box-sizing: border-box;
    vertical-align: middle;
}

thead {
    display: table-header-group;
    vertical-align: middle;
}

tbody {
    display: table-row-group;
    vertical-align: middle;
}

tfoot {
    display: table-footer-group;
    vertical-align: middle;
}

tr {
    display: table-row;
    vertical-align: inherit;
}

td, th {
    display: table-cell;
    vertical-align: inherit;
    padding: 1px;
}

th {
    font-weight: bold;
    text-align: center;
}

caption {
    display: table-caption;
    text-align: center;
}

/* Embedded Content */
img {
    display: inline-block;
}

/* Interactive & Hidden */
[hidden] {
    display: none !important;
}
"#;

static UA_STYLESHEET: OnceLock<StyleSheet> = OnceLock::new();

/// Returns the cached, statically parsed W3C User-Agent default stylesheet.
pub fn get_ua_stylesheet() -> &'static StyleSheet {
    UA_STYLESHEET.get_or_init(|| {
        StyleSheet::parse(W3C_UA_STYLESHEET_CSS)
            .expect("Normative W3C UA stylesheet must parse without errors")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ua_stylesheet_parsing_and_integrity() {
        let sheet = get_ua_stylesheet();
        assert!(!sheet.rules.is_empty(), "UA stylesheet must contain rules");

        // Verify key selectors are present
        let all_selectors: Vec<String> = sheet
            .rules
            .iter()
            .flat_map(|r| r.selectors.iter().map(|s| s.raw.clone()))
            .collect();

        assert!(all_selectors.iter().any(|s| s == "div"));
        assert!(all_selectors.iter().any(|s| s == "p"));
        assert!(all_selectors.iter().any(|s| s == "h1"));
        assert!(all_selectors.iter().any(|s| s == "table"));
        assert!(all_selectors.iter().any(|s| s == "tr"));
        assert!(all_selectors.iter().any(|s| s == "td"));
        assert!(all_selectors.iter().any(|s| s == "a"));
        assert!(all_selectors.iter().any(|s| s == "[hidden]"));
    }
}
