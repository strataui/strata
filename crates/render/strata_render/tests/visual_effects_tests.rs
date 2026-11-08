//! Integration Tests: CSS Visual Effects, GPU Compositing Commands, Blend Modes, and Filter Pipelines.

#![allow(clippy::field_reassign_with_default)]

use strata_layout::{LayoutNode, Rect};
use strata_render::compositor::{Compositor, DrawCommand};
use strata_render::rasterizer::FrameBuffer;
use strata_style::effects::{BackdropFilter, ClipPath, FilterOp};
use strata_style::types::{BlendMode, Color, ComputedStyle, Isolation};

fn make_node(style: ComputedStyle, bounds: Rect) -> LayoutNode {
    let mut node = LayoutNode::new(style);
    node.geometry.border_box = bounds;
    node
}

#[test]
fn test_stacking_context_creation_for_visual_effects() {
    let mut style = ComputedStyle::default();
    let node = make_node(style.clone(), Rect::new(0.0, 0.0, 100.0, 100.0));
    assert!(!Compositor::establishes_stacking_context(&node, false));

    // 1. Blend mode != Normal
    style.mix_blend_mode = BlendMode::Multiply;
    let node_blend = make_node(style.clone(), Rect::new(0.0, 0.0, 100.0, 100.0));
    assert!(Compositor::establishes_stacking_context(&node_blend, false));
    style.mix_blend_mode = BlendMode::Normal;

    // 2. Isolation == Isolate
    style.isolation = Isolation::Isolate;
    let node_iso = make_node(style.clone(), Rect::new(0.0, 0.0, 100.0, 100.0));
    assert!(Compositor::establishes_stacking_context(&node_iso, false));
    style.isolation = Isolation::Auto;

    // 3. Filter active
    style.filter = Some(BackdropFilter::new(vec![FilterOp::Blur(5.0)]));
    let node_filter = make_node(style.clone(), Rect::new(0.0, 0.0, 100.0, 100.0));
    assert!(Compositor::establishes_stacking_context(&node_filter, false));
    style.filter = None;

    // 4. Clip-path active
    style.clip_path = Some(ClipPath::circle(50.0, 50.0, 50.0));
    let node_clip = make_node(style.clone(), Rect::new(0.0, 0.0, 100.0, 100.0));
    assert!(Compositor::establishes_stacking_context(&node_clip, false));
}

#[test]
fn test_display_list_command_emission_and_lifo_order() {
    let mut style = ComputedStyle::default();
    style.background_color = Some(Color::rgb(255, 0, 0));
    style.isolation = Isolation::Isolate;
    style.mix_blend_mode = BlendMode::Screen;
    style.filter = Some(BackdropFilter::new(vec![FilterOp::Brightness(1.5)]));
    style.clip_path = Some(ClipPath::inset(10.0, 10.0, 10.0, 10.0, 0.0));

    let bounds = Rect::new(0.0, 0.0, 200.0, 100.0);
    let root = make_node(style, bounds);

    let display_list = Compositor::build_display_list(&root);
    let cmds = display_list.commands();

    // Verify presence and order of pushed commands:
    // PushIsolation -> PushBlendMode -> PushFilter -> PushClipPath
    let push_iso_idx = cmds.iter().position(|c| matches!(c, DrawCommand::PushIsolation { isolate: true })).expect("PushIsolation must be emitted");
    let push_blend_idx = cmds.iter().position(|c| matches!(c, DrawCommand::PushBlendMode { mode: BlendMode::Screen })).expect("PushBlendMode must be emitted");
    let push_filter_idx = cmds.iter().position(|c| matches!(c, DrawCommand::PushFilter { .. })).expect("PushFilter must be emitted");
    let push_clip_idx = cmds.iter().position(|c| matches!(c, DrawCommand::PushClipPath { .. })).expect("PushClipPath must be emitted");

    assert!(push_iso_idx < push_blend_idx, "PushIsolation must precede PushBlendMode");
    assert!(push_blend_idx < push_filter_idx, "PushBlendMode must precede PushFilter");
    assert!(push_filter_idx < push_clip_idx, "PushFilter must precede PushClipPath");

    // Verify reverse (LIFO) popping order at the end:
    // PopClipPath -> PopFilter -> PopBlendMode -> PopIsolation
    let pop_clip_idx = cmds.iter().position(|c| matches!(c, DrawCommand::PopClipPath)).expect("PopClipPath must be emitted");
    let pop_filter_idx = cmds.iter().position(|c| matches!(c, DrawCommand::PopFilter)).expect("PopFilter must be emitted");
    let pop_blend_idx = cmds.iter().position(|c| matches!(c, DrawCommand::PopBlendMode)).expect("PopBlendMode must be emitted");
    let pop_iso_idx = cmds.iter().position(|c| matches!(c, DrawCommand::PopIsolation)).expect("PopIsolation must be emitted");

    assert!(pop_clip_idx < pop_filter_idx, "PopClipPath must precede PopFilter");
    assert!(pop_filter_idx < pop_blend_idx, "PopFilter must precede PopBlendMode");
    assert!(pop_blend_idx < pop_iso_idx, "PopBlendMode must precede PopIsolation");
}

#[test]
fn test_software_rasterizer_filter_effects() {
    let mut fb = FrameBuffer::new(50, 50);

    // 1. Test Brightness filter (0.5x on 200, 200, 200)
    fb.clear(Color::rgb(200, 200, 200));
    let bright_filter = BackdropFilter::new(vec![FilterOp::Brightness(0.5)]);
    fb.apply_filter(Rect::new(0.0, 0.0, 50.0, 50.0), &bright_filter);
    let p1 = fb.get_pixel(25, 25);
    let (r1, g1, b1, _) = p1.to_rgba8();
    assert_eq!(r1, 100);
    assert_eq!(g1, 100);
    assert_eq!(b1, 100);

    // 2. Test Invert filter (1.0 invert)
    fb.clear(Color::rgb(255, 0, 100));
    let invert_filter = BackdropFilter::new(vec![FilterOp::Invert(1.0)]);
    fb.apply_filter(Rect::new(0.0, 0.0, 50.0, 50.0), &invert_filter);
    let p2 = fb.get_pixel(10, 10);
    let (r2, g2, b2, _) = p2.to_rgba8();
    assert_eq!(r2, 0);
    assert_eq!(g2, 255);
    assert_eq!(b2, 155);

    // 3. Test Grayscale filter (1.0 full grayscale on pure red)
    fb.clear(Color::rgb(255, 0, 0));
    let gray_filter = BackdropFilter::new(vec![FilterOp::Grayscale(1.0)]);
    fb.apply_filter(Rect::new(0.0, 0.0, 50.0, 50.0), &gray_filter);
    let p3 = fb.get_pixel(10, 10);
    let (r3, g3, b3, _) = p3.to_rgba8();
    // 0.2126 * 255 = 54.213 -> 54
    assert!((r3 as i32 - 54).abs() <= 1);
    assert_eq!(r3, g3);
    assert_eq!(g3, b3);
}

#[test]
fn test_draw_command_metadata_and_clipping() {
    let bounds = Rect::new(10.0, 20.0, 100.0, 80.0);
    let filter = BackdropFilter::new(vec![FilterOp::Contrast(1.2)]);
    let clip_path = ClipPath::circle(40.0, 50.0, 40.0);

    let cmd_filter = DrawCommand::PushFilter { bounds, filter };
    assert_eq!(cmd_filter.bounds(), Some(bounds));
    assert!(!cmd_filter.is_clip());

    let cmd_clip_path = DrawCommand::PushClipPath { bounds, clip_path };
    assert_eq!(cmd_clip_path.bounds(), Some(bounds));
    assert!(cmd_clip_path.is_clip());

    assert!(DrawCommand::PopClipPath.is_clip());
    assert_eq!(DrawCommand::PopFilter.bounds(), None);
    assert_eq!(DrawCommand::PushBlendMode { mode: BlendMode::Multiply }.bounds(), None);
    assert_eq!(DrawCommand::PushIsolation { isolate: true }.bounds(), None);
}
