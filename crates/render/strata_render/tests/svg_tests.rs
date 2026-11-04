//! Comprehensive Tests for Complete & Optimized SVG Vector Graphics Engine.

use strata_layout::{FillRule, LayoutGeometry, LayoutNode, Rect, StrokeLineCap, StrokeLineJoin, SvgItem};
use strata_render::compositor::{Compositor, DrawCommand};
use strata_style::types::{Color, ComputedStyle};

#[test]
fn test_svg_item_creation_and_attributes() {
    let mut item = SvgItem::new("M 0 0 L 100 100".to_string(), (100.0, 100.0));
    item.stroke_color = Some(Color::rgb(255, 0, 0));
    item.fill_color = Some(Color::rgb(0, 255, 0));
    item.stroke_width = 3.5;
    item.stroke_linecap = StrokeLineCap::Round;
    item.stroke_linejoin = StrokeLineJoin::Bevel;
    item.stroke_dasharray = vec![4.0, 2.0];
    item.fill_rule = FillRule::EvenOdd;

    assert_eq!(item.path_data, "M 0 0 L 100 100");
    assert_eq!(item.stroke_color, Some(Color::rgb(255, 0, 0)));
    assert_eq!(item.fill_color, Some(Color::rgb(0, 255, 0)));
    assert_eq!(item.stroke_width, 3.5);
    assert_eq!(item.stroke_linecap, StrokeLineCap::Round);
    assert_eq!(item.stroke_linejoin, StrokeLineJoin::Bevel);
    assert_eq!(item.stroke_dasharray, vec![4.0, 2.0]);
    assert_eq!(item.fill_rule, FillRule::EvenOdd);
    assert_eq!(item.view_box, (100.0, 100.0));
}

#[test]
fn test_svg_compositor_display_list_emission() {
    let style = ComputedStyle::default();
    let mut item = SvgItem::new("M 10 10 H 90 V 90 H 10 Z".to_string(), (100.0, 100.0));
    item.stroke_color = Some(Color::rgb(30, 41, 59));
    item.fill_color = Some(Color::rgb(59, 130, 246));
    item.stroke_width = 2.0;
    item.stroke_linecap = StrokeLineCap::Square;
    item.stroke_linejoin = StrokeLineJoin::Miter;
    item.stroke_dasharray = vec![8.0, 4.0];
    item.fill_rule = FillRule::NonZero;

    let mut node = LayoutNode::with_svg_items(style, vec![item]);
    node.geometry = LayoutGeometry {
        border_box: Rect::new(50.0, 50.0, 200.0, 200.0),
        padding_box: Rect::new(50.0, 50.0, 200.0, 200.0),
        content_box: Rect::new(50.0, 50.0, 200.0, 200.0),
        margin_box: Rect::new(50.0, 50.0, 200.0, 200.0),
    };

    let display_list = Compositor::build_display_list(&node);
    assert_eq!(display_list.commands.len(), 1);

    match &display_list.commands[0] {
        DrawCommand::DrawSvgPath {
            bounds,
            path_data,
            stroke_color,
            fill_color,
            stroke_width,
            stroke_linecap,
            stroke_linejoin,
            stroke_dasharray,
            fill_rule,
            view_box,
        } => {
            assert_eq!(*bounds, Rect::new(50.0, 50.0, 200.0, 200.0));
            assert_eq!(path_data, "M 10 10 H 90 V 90 H 10 Z");
            assert_eq!(*stroke_color, Some(Color::rgb(30, 41, 59)));
            assert_eq!(*fill_color, Some(Color::rgb(59, 130, 246)));
            assert_eq!(*stroke_width, 2.0);
            assert_eq!(*stroke_linecap, StrokeLineCap::Square);
            assert_eq!(*stroke_linejoin, StrokeLineJoin::Miter);
            assert_eq!(*stroke_dasharray, vec![8.0, 4.0]);
            assert_eq!(*fill_rule, FillRule::NonZero);
            assert_eq!(*view_box, (100.0, 100.0));
        }
        other => panic!("Expected DrawSvgPath, found {:?}", other),
    }
}

#[test]
fn test_svg_multi_item_complex_group_compositing() {
    let style = ComputedStyle::default();
    let mut path1 = SvgItem::new("M 0 0 L 50 50".to_string(), (200.0, 200.0));
    path1.stroke_color = Some(Color::rgb(239, 68, 68));

    let mut path2 = SvgItem::new("M 50 50 L 100 0".to_string(), (200.0, 200.0));
    path2.stroke_color = Some(Color::rgb(16, 185, 129));
    path2.stroke_linecap = StrokeLineCap::Round;

    let mut poly = SvgItem::new("M 0 100 L 50 150 L 100 100 Z".to_string(), (200.0, 200.0));
    poly.fill_color = Some(Color::rgb(245, 158, 11));
    poly.fill_rule = FillRule::EvenOdd;

    let mut node = LayoutNode::with_svg_items(style, vec![path1, path2, poly]);
    node.geometry = LayoutGeometry {
        border_box: Rect::new(0.0, 0.0, 200.0, 200.0),
        padding_box: Rect::new(0.0, 0.0, 200.0, 200.0),
        content_box: Rect::new(0.0, 0.0, 200.0, 200.0),
        margin_box: Rect::new(0.0, 0.0, 200.0, 200.0),
    };

    let display_list = Compositor::build_display_list(&node);
    assert_eq!(display_list.commands.len(), 3);

    match &display_list.commands[0] {
        DrawCommand::DrawSvgPath { stroke_color, .. } => {
            assert_eq!(*stroke_color, Some(Color::rgb(239, 68, 68)));
        }
        _ => panic!("Expected first path"),
    }
    match &display_list.commands[1] {
        DrawCommand::DrawSvgPath { stroke_linecap, .. } => {
            assert_eq!(*stroke_linecap, StrokeLineCap::Round);
        }
        _ => panic!("Expected second path"),
    }
    match &display_list.commands[2] {
        DrawCommand::DrawSvgPath { fill_color, fill_rule, .. } => {
            assert_eq!(*fill_color, Some(Color::rgb(245, 158, 11)));
            assert_eq!(*fill_rule, FillRule::EvenOdd);
        }
        _ => panic!("Expected third path"),
    }
}
