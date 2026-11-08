//! Integration Tests: opacity groups, border styles, and outline emission.

#![allow(clippy::field_reassign_with_default)]

use strata_layout::{LayoutNode, Rect};
use strata_primitives::geom::EdgeInsets;
use strata_render::compositor::{Compositor, DrawCommand};
use strata_style::types::{BorderStyle, Color, ComputedStyle};

fn make_node(style: ComputedStyle, bounds: Rect) -> LayoutNode {
    let mut node = LayoutNode::new(style);
    node.geometry.border_box = bounds;
    node
}

#[test]
fn test_opacity_group_emitted_and_balanced() {
    let mut style = ComputedStyle::default();
    style.opacity = 0.5;
    style.background_color = Some(Color::rgb(255, 0, 0));
    let node = make_node(style, Rect::new(0.0, 0.0, 100.0, 100.0));

    let list = Compositor::build_display_list(&node);
    let cmds = list.commands();

    let push = cmds
        .iter()
        .position(
            |c| matches!(c, DrawCommand::PushOpacity { alpha } if (*alpha - 0.5).abs() < 1e-6),
        )
        .expect("PushOpacity must be emitted for opacity 0.5");
    let pop = cmds
        .iter()
        .position(|c| matches!(c, DrawCommand::PopOpacity))
        .expect("PopOpacity must be emitted");
    assert!(push < pop, "PushOpacity must precede PopOpacity");
    assert_eq!(
        cmds.iter()
            .filter(|c| matches!(c, DrawCommand::PushOpacity { .. }))
            .count(),
        1
    );
    assert_eq!(
        cmds.iter()
            .filter(|c| matches!(c, DrawCommand::PopOpacity))
            .count(),
        1
    );
    assert_eq!(DrawCommand::PushOpacity { alpha: 0.5 }.bounds(), None);
    assert!(!DrawCommand::PushOpacity { alpha: 0.5 }.is_clip());
    assert!(!DrawCommand::PopOpacity.is_clip());
}

#[test]
fn test_opacity_group_omitted_when_opaque() {
    let mut style = ComputedStyle::default();
    style.opacity = 1.0;
    style.background_color = Some(Color::rgb(255, 0, 0));
    let node = make_node(style, Rect::new(0.0, 0.0, 100.0, 100.0));

    let list = Compositor::build_display_list(&node);
    assert!(!list
        .commands()
        .iter()
        .any(|c| matches!(c, DrawCommand::PushOpacity { .. })));
}

#[test]
fn test_border_style_carried_on_draw_border() {
    let mut style = ComputedStyle::default();
    style.border_style = BorderStyle::Dashed;
    style.border_width = EdgeInsets::all(4.0);
    style.border_color = Some(Color::rgb(15, 23, 42));
    style.background_color = Some(Color::rgb(241, 245, 249));
    let node = make_node(style, Rect::new(0.0, 0.0, 240.0, 180.0));

    let list = Compositor::build_display_list(&node);
    let border = list
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::DrawBorder { style, .. } => Some(*style),
            _ => None,
        })
        .expect("DrawBorder must be emitted for a visible border");
    assert_eq!(border, BorderStyle::Dashed);
}

#[test]
fn test_border_style_none_suppresses_draw_border() {
    let mut style = ComputedStyle::default();
    style.border_style = BorderStyle::None;
    style.border_width = EdgeInsets::all(4.0);
    style.border_color = Some(Color::rgb(15, 23, 42));
    style.background_color = Some(Color::rgb(241, 245, 249));
    let node = make_node(style, Rect::new(0.0, 0.0, 240.0, 180.0));

    let list = Compositor::build_display_list(&node);
    assert!(!list
        .commands()
        .iter()
        .any(|c| matches!(c, DrawCommand::DrawBorder { .. })));
}

#[test]
fn test_outline_emitted_outside_border_box() {
    let mut style = ComputedStyle::default();
    style.outline_width = 2.0;
    style.outline_style = BorderStyle::Dotted;
    style.outline_color = Some(Color::rgb(220, 38, 38));
    style.outline_offset = 3.0;
    style.background_color = Some(Color::rgb(255, 255, 255));
    let node = make_node(style, Rect::new(50.0, 40.0, 100.0, 60.0));

    let list = Compositor::build_display_list(&node);
    let outline = list
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::DrawBorder {
                bounds,
                widths,
                style,
                ..
            } => Some((*bounds, *widths, *style)),
            _ => None,
        })
        .expect("outline DrawBorder must be emitted");
    assert_eq!(outline.0, Rect::new(47.0, 37.0, 106.0, 66.0));
    assert_eq!(outline.1, EdgeInsets::all(2.0));
    assert_eq!(outline.2, BorderStyle::Dotted);
}

#[test]
fn test_state_commands_are_not_cullable() {
    assert!(DrawCommand::PushFilter {
        bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
        filter: strata_style::effects::BackdropFilter::blur(2.0),
    }
    .is_state_command());
    assert!(DrawCommand::PopFilter.is_state_command());
    assert!(DrawCommand::PushIsolation { isolate: true }.is_state_command());
    assert!(DrawCommand::PopIsolation.is_state_command());
    assert!(DrawCommand::PushBlendMode {
        mode: strata_style::types::BlendMode::Multiply,
    }
    .is_state_command());
    assert!(DrawCommand::PopBlendMode.is_state_command());
    assert!(DrawCommand::PushOpacity { alpha: 0.5 }.is_state_command());
    assert!(DrawCommand::PopOpacity.is_state_command());
    assert!(DrawCommand::PushTransform {
        matrix: strata_primitives::geom::Matrix4x4::IDENTITY,
    }
    .is_state_command());
    assert!(DrawCommand::PopTransform.is_state_command());
    assert!(DrawCommand::PushClip {
        bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
        radius: strata_style::types::BorderRadius::default(),
    }
    .is_state_command());
    assert!(DrawCommand::PopClip.is_state_command());

    // Paint commands are not stack commands.
    let fill = DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
        color: Color::rgb(0, 0, 0),
    };
    assert!(!fill.is_state_command());
    assert!(!fill.is_clip());
    // PushOpacity must remain a non-clip command (existing contract).
    assert!(!DrawCommand::PushOpacity { alpha: 0.5 }.is_clip());
    assert!(!DrawCommand::PushFilter {
        bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
        filter: strata_style::effects::BackdropFilter::blur(2.0),
    }
    .is_clip());
}

/// Returns the stack family name and depth delta (+1 push, -1 pop) for a command.
fn stack_family(cmd: &DrawCommand) -> Option<(&'static str, i32)> {
    match cmd {
        DrawCommand::PushClip { .. } => Some(("clip", 1)),
        DrawCommand::PopClip => Some(("clip", -1)),
        DrawCommand::PushBackdropFilter { .. } => Some(("backdrop", 1)),
        DrawCommand::PopBackdropFilter => Some(("backdrop", -1)),
        DrawCommand::PushFilter { .. } => Some(("filter", 1)),
        DrawCommand::PopFilter => Some(("filter", -1)),
        DrawCommand::PushClipPath { .. } => Some(("clip_path", 1)),
        DrawCommand::PopClipPath => Some(("clip_path", -1)),
        DrawCommand::PushBlendMode { .. } => Some(("blend", 1)),
        DrawCommand::PopBlendMode => Some(("blend", -1)),
        DrawCommand::PushIsolation { .. } => Some(("isolation", 1)),
        DrawCommand::PopIsolation => Some(("isolation", -1)),
        DrawCommand::PushOpacity { .. } => Some(("opacity", 1)),
        DrawCommand::PopOpacity => Some(("opacity", -1)),
        DrawCommand::PushTransform { .. } => Some(("transform", 1)),
        DrawCommand::PopTransform => Some(("transform", -1)),
        _ => None,
    }
}

#[test]
fn test_culled_child_stacking_context_emits_balanced_stack() {
    use strata_style::types::{Isolation, Overflow, Position};

    let mut root_style = ComputedStyle::default();
    root_style.background_color = Some(Color::rgb(255, 255, 255));
    let mut root = make_node(root_style, Rect::new(0.0, 0.0, 800.0, 600.0));

    // A child stacking context with overflow clipping, isolation, and a
    // filter, positioned entirely outside the viewport cull rect.
    let mut child_style = ComputedStyle::default();
    child_style.position = Position::Relative;
    child_style.overflow_x = Overflow::Hidden;
    child_style.overflow_y = Overflow::Hidden;
    child_style.isolation = Isolation::Isolate;
    child_style.filter = Some(strata_style::effects::BackdropFilter::blur(4.0));
    let child = make_node(child_style, Rect::new(0.0, 50_000.0, 200.0, 100.0));
    root.children.push(child);

    let list = Compositor::build_display_list_with_viewport(
        &root,
        Some(Rect::new(0.0, 0.0, 800.0, 600.0)),
    );
    let cmds = list.commands();

    let mut pushes = std::collections::BTreeMap::new();
    let mut pops = std::collections::BTreeMap::new();
    let mut depth: i32 = 0;
    for cmd in cmds {
        if let Some((family, delta)) = stack_family(cmd) {
            if delta > 0 {
                *pushes.entry(family).or_insert(0usize) += 1;
            } else {
                *pops.entry(family).or_insert(0usize) += 1;
            }
            depth += delta;
            assert!(depth >= 0, "stack depth went negative at {family}: {cmd:?}");
        }
    }

    assert_eq!(
        pushes, pops,
        "every push family must have a matching pop after viewport culling"
    );
    assert_eq!(depth, 0, "display list stack must balance to zero");
    assert!(
        cmds.iter()
            .all(|c| !matches!(c, DrawCommand::PushFilter { .. })),
        "culled child must not leak a PushFilter"
    );
}
