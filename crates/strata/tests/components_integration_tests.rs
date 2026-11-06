//! Integration tests for scroll containers and frameless window chrome components.

use std::time::{Duration, Instant};

use strata::prelude::*;
use strata::render::compositor::DrawCommand;

fn tall_child(height_class: &str, label: &str) -> NodeBlueprint {
    let class = format!("{} w-full shrink-0 bg-blue-500", height_class);
    let view = rsx! {
        <div class={class}>
            <span>{label.to_string()}</span>
        </div>
    };
    view.root
}

fn thumb_of(runtime: &AppRuntime) -> Option<(strata::layout::Rect, strata::style::types::Color)> {
    runtime
        .display_list
        .commands
        .iter()
        .rev()
        .find_map(|command| {
            if let DrawCommand::DrawRoundedRect { bounds, color, .. } = command {
                if color.r == 255
                    && color.g == 255
                    && color.b == 255
                    && (color.a == 60 || color.a == 150)
                {
                    return Some((*bounds, *color));
                }
            }
            None
        })
}

#[test]
fn test_scroll_area_offset_preserved_across_rerender() {
    let children = vec![tall_child("h-[900px]", "tall content")];
    let view = strata_ui::scroll_area::ScrollArea(
        Some("scroll-preserve"),
        Some("h-[300px]"),
        Some(children),
        None,
    );
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    assert!(
        runtime
            .active_scrollbars
            .iter()
            .any(|bar| bar.max_scroll_y > 0.0),
        "tall content must create a scrollable container"
    );

    assert!(
        runtime.dispatch_scroll(200.0, 150.0, 0.0, 200.0),
        "wheel delta must move the scroll container"
    );
    runtime.rebuild_layout_tree();

    let stored = runtime
        .scroll_offsets_by_id
        .get("scroll-preserve")
        .copied()
        .unwrap_or((0.0, 0.0));
    assert!(
        stored.1 > 0.0,
        "scroll offset must be preserved by the stable container id across a re-render"
    );

    let bar = runtime
        .active_scrollbars
        .iter()
        .find(|bar| bar.id == "scroll-preserve")
        .expect("scrollbar state must survive a rebuild");
    assert!(bar.scroll_y > 0.0);
}

#[test]
fn test_nested_scroll_chaining_to_parent_at_inner_boundary() {
    let inner_children = vec![tall_child("h-[800px]", "inner content")];
    let inner = strata_ui::scroll_area::ScrollArea(
        Some("inner-scroll"),
        Some("h-[150px] shrink-0"),
        Some(inner_children),
        None,
    )
    .root;

    let outer_children = vec![inner, tall_child("h-[1000px]", "outer content")];
    let view = strata_ui::scroll_area::ScrollArea(
        Some("outer-scroll"),
        Some("h-[300px]"),
        Some(outer_children),
        None,
    );
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    let inner_bar = runtime
        .active_scrollbars
        .iter()
        .find(|bar| bar.id == "inner-scroll")
        .expect("inner scroll container must exist");
    let inner_max = inner_bar.max_scroll_y;
    assert!(inner_max > 0.0, "inner container must overflow");
    assert!(
        runtime
            .active_scrollbars
            .iter()
            .any(|bar| bar.id == "outer-scroll" && bar.max_scroll_y > 0.0),
        "outer container must overflow"
    );

    // Saturate the inner container.
    assert!(runtime.dispatch_smooth_scroll(100.0, 60.0, 0.0, 5000.0));
    runtime.poll_smooth_scroll_at(Instant::now() + Duration::from_secs(1));
    runtime.tick_frame();

    let inner_scrolled = runtime
        .scroll_offsets_by_id
        .get("inner-scroll")
        .copied()
        .map(|(_, y)| y)
        .unwrap_or(0.0);
    assert!(
        (inner_scrolled - inner_max).abs() < 1.0,
        "inner container should rest at its maximum offset, got {inner_scrolled} of {inner_max}"
    );

    // The same wheel position can no longer scroll the inner container, so the delta
    // must chain to the outer container.
    assert!(runtime.dispatch_smooth_scroll(100.0, 60.0, 0.0, 200.0));
    runtime.poll_smooth_scroll_at(Instant::now() + Duration::from_secs(1));
    runtime.tick_frame();

    let outer_scrolled = runtime
        .scroll_offsets_by_id
        .get("outer-scroll")
        .copied()
        .map(|(_, y)| y)
        .unwrap_or(0.0);
    assert!(
        outer_scrolled > 0.0,
        "wheel delta must chain to the outer container at the inner boundary, got {outer_scrolled}"
    );
}

#[test]
fn test_scrollbar_thumb_hover_expansion() {
    let children = vec![tall_child("h-[900px]", "tall content")];
    let view = strata_ui::scroll_area::ScrollArea(
        Some("scroll-hover"),
        Some("h-[200px]"),
        Some(children),
        None,
    );
    let mut runtime = AppRuntime::new(view, 300.0, 200.0, 1.0);

    runtime.compile_display_list();
    let (idle_bounds, idle_color) =
        thumb_of(&runtime).expect("an overflow container must draw a scrollbar thumb");
    assert_eq!(
        idle_color.a, 60,
        "idle scrollbar thumb should be translucent"
    );

    runtime.last_mouse_pos = (
        idle_bounds.x + idle_bounds.width / 2.0,
        idle_bounds.y + idle_bounds.height / 2.0,
    );
    runtime.compile_display_list();
    let (active_bounds, active_color) =
        thumb_of(&runtime).expect("hovered scrollbar thumb must still be drawn");

    assert!(
        active_color.a > idle_color.a,
        "hovered scrollbar thumb must be more opaque ({} vs {})",
        active_color.a,
        idle_color.a
    );
    assert!(
        active_bounds.width > idle_bounds.width,
        "hovered scrollbar thumb must expand ({} vs {})",
        active_bounds.width,
        idle_bounds.width
    );
}

#[test]
fn test_title_bar_drag_region_hit_testing() {
    let view = strata_ui::title_bar::TitleBar(
        Some("Hit Test Window".to_string()),
        Some(true),
        Some(true),
        None,
        None,
        None,
        None,
        None,
    );
    let runtime = AppRuntime::new(view, 400.0, 40.0, 1.0);

    assert_eq!(
        AppRuntime::hit_test_property(
            &runtime.root_blueprint,
            &runtime.layout_root,
            200.0,
            20.0,
            "data-app-region"
        )
        .as_deref(),
        Some("drag"),
        "the title bar body must resolve as a native drag region"
    );

    let no_drag = strata_ui::title_bar::TitleBar(
        Some("No Drag Window".to_string()),
        Some(false),
        Some(false),
        None,
        None,
        None,
        None,
        None,
    );
    let runtime = AppRuntime::new(no_drag, 400.0, 40.0, 1.0);
    assert_eq!(
        AppRuntime::hit_test_property(
            &runtime.root_blueprint,
            &runtime.layout_root,
            200.0,
            20.0,
            "data-app-region"
        )
        .as_deref(),
        Some("no-drag")
    );
}
