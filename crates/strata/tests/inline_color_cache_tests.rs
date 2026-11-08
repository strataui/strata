//! End-to-end regression for paint colors colliding in the inline layout cache.
//!
//! Two equally styled buttons whose only difference is text color must emit
//! their own colors through the production display-list path. The inline cache
//! used to key only on geometry-affecting fields, so the second run reused the
//! first run's cloned fragment style.

use strata::prelude::*;
use strata::render::compositor::{Compositor, DrawCommand};
use strata_style::types::Color;
use strata_web_reader::prelude::*;

const BLACK: Color = Color::new(0, 0, 0, 255);
const GREY: Color = Color::new(0xA3, 0xA3, 0xA3, 255);

fn painted_colors(
    first: &str,
    first_color: &str,
    second: &str,
    second_color: &str,
) -> Vec<(String, Color)> {
    let markup = format!(
        r#"<div class="flex flex-col">
            <button class="text-[11px] text-[{first_color}]"><span>{first}</span></button>
            <button class="text-[11px] text-[{second_color}]"><span>{second}</span></button>
        </div>"#
    );
    let nodes = HtmlParser::parse(&markup);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media = MediaContext::new(300.0, 80.0, true);
    let document = transpiler.transpile_full_document(&nodes, &media);
    let mut root = AppRuntime::build_layout_tree(&document);
    root.compute_layout(300.0, 80.0);
    let list = Compositor::build_display_list(&root);
    let mut painted = Vec::new();
    for command in list.commands() {
        if let DrawCommand::DrawTextFragments { fragments } = command {
            for fragment in fragments {
                painted.push((fragment.text.trim().to_string(), fragment.style.color));
            }
        }
    }
    painted
}

#[test]
fn inline_color_does_not_leak_between_equal_runs() {
    let painted = painted_colors("first", "#000000", "second", "#A3A3A3");
    assert_eq!(
        painted,
        vec![("first".to_string(), BLACK), ("second".to_string(), GREY),],
        "each run must paint with its own color"
    );
}

#[test]
fn identical_text_keeps_distinct_colors() {
    let painted = painted_colors("Diagnostics", "#000000", "Diagnostics", "#A3A3A3");
    assert_eq!(
        painted,
        vec![
            ("Diagnostics".to_string(), BLACK),
            ("Diagnostics".to_string(), GREY),
        ],
        "identical text with different colors must not share a cache entry"
    );
}

#[test]
fn inline_color_is_order_independent() {
    let painted = painted_colors("second", "#A3A3A3", "first", "#000000");
    assert_eq!(
        painted,
        vec![("second".to_string(), GREY), ("first".to_string(), BLACK),],
        "swapping the runs must swap the painted colors, not lock the first color"
    );
}
