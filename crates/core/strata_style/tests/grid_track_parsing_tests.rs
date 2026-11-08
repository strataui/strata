//! Integration tests for CSS `grid-template-*` track parsing and grid placement declarations.

use strata_style::parser::{parse_declarations, parse_grid_tracks};
use strata_style::prelude::*;

fn apply_css(style: &mut ComputedStyle, css: &str) {
    for decl in parse_declarations(css) {
        CascadeSolver::apply_declaration(style, &decl);
    }
}

#[test]
fn test_parse_explicit_pixel_columns() {
    let tracks = parse_grid_tracks("260px 420px 260px");
    assert_eq!(
        tracks,
        vec![
            GridTrack::Px(260.0),
            GridTrack::Px(420.0),
            GridTrack::Px(260.0)
        ]
    );
}

#[test]
fn test_parse_repeat_single_track() {
    let tracks = parse_grid_tracks("repeat(3, 1fr)");
    assert_eq!(
        tracks,
        vec![GridTrack::Fr(1.0), GridTrack::Fr(1.0), GridTrack::Fr(1.0)]
    );
}

#[test]
fn test_parse_repeat_multiple_tracks() {
    let tracks = parse_grid_tracks("repeat(2, 100px 1fr)");
    assert_eq!(
        tracks,
        vec![
            GridTrack::Px(100.0),
            GridTrack::Fr(1.0),
            GridTrack::Px(100.0),
            GridTrack::Fr(1.0),
        ]
    );
}

#[test]
fn test_parse_minmax_zero_to_fr() {
    let tracks = parse_grid_tracks("minmax(0, 1fr)");
    assert_eq!(
        tracks,
        vec![GridTrack::MinMax(
            GridTrackBreadth::Px(0.0),
            GridTrackBreadth::Fr(1.0)
        )]
    );
}

#[test]
fn test_parse_minmax_fixed_min_auto_max() {
    let tracks = parse_grid_tracks("minmax(120px, auto)");
    assert_eq!(
        tracks,
        vec![GridTrack::MinMax(
            GridTrackBreadth::Px(120.0),
            GridTrackBreadth::Auto
        )]
    );
}

#[test]
fn test_parse_strips_named_lines() {
    let tracks = parse_grid_tracks("[content-start] 100px [middle] 1fr [content-end]");
    assert_eq!(tracks, vec![GridTrack::Px(100.0), GridTrack::Fr(1.0)]);
}

#[test]
fn test_parse_auto_fill_expands_once() {
    let tracks = parse_grid_tracks("repeat(auto-fill, 80px)");
    assert_eq!(tracks, vec![GridTrack::Px(80.0)]);
}

#[test]
fn test_parse_fit_content_maps_to_max_content() {
    let tracks = parse_grid_tracks("fit-content(240px) auto min-content max-content");
    assert_eq!(
        tracks,
        vec![
            GridTrack::MaxContent,
            GridTrack::Auto,
            GridTrack::MinContent,
            GridTrack::MaxContent,
        ]
    );
}

#[test]
fn test_cascade_parses_grid_template_columns_and_rows() {
    let mut style = ComputedStyle::default();
    apply_css(
        &mut style,
        "grid-template-columns: 260px 420px 260px; grid-template-rows: 150px 150px;",
    );

    assert_eq!(
        style.grid_template_columns,
        vec![
            GridTrack::Px(260.0),
            GridTrack::Px(420.0),
            GridTrack::Px(260.0)
        ]
    );
    assert_eq!(
        style.grid_template_rows,
        vec![GridTrack::Px(150.0), GridTrack::Px(150.0)]
    );
}

#[test]
fn test_cascade_grid_column_span() {
    let mut style = ComputedStyle::default();
    apply_css(&mut style, "grid-column: span 2;");

    assert_eq!(style.grid_column_span, Some(2));
    assert_eq!(style.grid_column_start, None);
}

#[test]
fn test_cascade_grid_column_start_span_longhand() {
    let mut style = ComputedStyle::default();
    apply_css(&mut style, "grid-column-start: span 2;");

    assert_eq!(style.grid_column_span, Some(2));
    assert_eq!(style.grid_column_start, None);
}

#[test]
fn test_cascade_grid_column_start_line_longhand() {
    let mut style = ComputedStyle::default();
    apply_css(&mut style, "grid-column-start: 2;");

    assert_eq!(style.grid_column_start, Some(2));
    assert_eq!(style.grid_column_span, None);
}

#[test]
fn test_cascade_grid_column_shorthand_line_and_span() {
    let mut style = ComputedStyle::default();
    apply_css(&mut style, "grid-column: 2 / span 2;");

    assert_eq!(style.grid_column_start, Some(2));
    assert_eq!(style.grid_column_span, Some(2));
}

#[test]
fn test_cascade_grid_column_shorthand_line_range() {
    let mut style = ComputedStyle::default();
    apply_css(&mut style, "grid-column: 1 / 3;");

    assert_eq!(style.grid_column_start, Some(1));
    assert_eq!(style.grid_column_span, Some(2));
}

#[test]
fn test_cascade_grid_column_span_pair() {
    let mut style = ComputedStyle::default();
    apply_css(&mut style, "grid-column: span 2 / span 2;");

    assert_eq!(style.grid_column_span, Some(2));
    assert_eq!(style.grid_column_start, None);
}

#[test]
fn test_cascade_grid_row_shorthand_line_range() {
    let mut style = ComputedStyle::default();
    apply_css(&mut style, "grid-row: 1 / 3;");

    assert_eq!(style.grid_row_start, Some(1));
    assert_eq!(style.grid_row_span, Some(2));
}

#[test]
fn test_cascade_grid_row_span_shorthand() {
    let mut style = ComputedStyle::default();
    apply_css(&mut style, "grid-row: span 2;");

    assert_eq!(style.grid_row_span, Some(2));
    assert_eq!(style.grid_row_start, None);
}

#[test]
fn test_cascade_grid_auto_flow_accepts_row_and_column() {
    let mut row_style = ComputedStyle::default();
    apply_css(&mut row_style, "grid-auto-flow: row dense;");

    let mut column_style = ComputedStyle::default();
    apply_css(&mut column_style, "grid-auto-flow: column;");

    // Placement currently remains row-major; the declaration must not panic or
    // corrupt the rest of the style.
    assert_eq!(row_style.display, column_style.display);
}
