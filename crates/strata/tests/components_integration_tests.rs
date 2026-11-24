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

#[test]
fn test_inline_code_stays_inline_and_w_full_button_keeps_sibling_in_card() {
    use strata_ui::buttons::{Button, ButtonVariant};

    let view = rsx! {
        <div class="w-[320px] flex flex-col gap-3 p-3">
            <p id="copilot-para" class="w-full text-sm">
                {"See "}
                <code class="px-1 bg-zinc-800">{"groundspeed_mps"}</code>
                {" reading inside this bubble."}
            </p>
            <div id="ssh-card-actions" class="w-full flex flex-row gap-2">
                <Button class="w-full" variant={ButtonVariant::Primary}>{"Connect"}</Button>
                <Button variant={ButtonVariant::Secondary}>{"Reset"}</Button>
            </div>
        </div>
    };
    let mut rt = AppRuntime::new(view, 320.0, 240.0, 1.0);
    rt.tick_frame();

    let para_box = rt.query_element_rect("copilot-para").expect("copilot-para rect");
    let para_node = &rt.layout_root.children[0];
    assert!(
        !para_node.line_boxes.is_empty(),
        "paragraph with inline <code> must format via IFC line_boxes instead of splitting blocks"
    );
    let first_line_text: String = para_node.line_boxes[0]
        .text_fragments
        .iter()
        .map(|f| f.text.as_str())
        .collect();
    assert!(
        first_line_text.contains("See ") && first_line_text.contains("groundspeed_mps"),
        "inline <code> must remain on the same line as surrounding text, got {:?} (para={:?})",
        first_line_text,
        para_box
    );

    let row_box = rt
        .query_element_rect("ssh-card-actions")
        .expect("ssh-card-actions rect");
    let actions = &rt.layout_root.children[1];
    let sib_box = actions.children[1].geometry.border_box;
    assert!(
        sib_box.x + sib_box.width <= row_box.x + row_box.width + 0.5,
        "sibling Button next to w-full Button must stay inside row bounds {:?} (got {:?})",
        row_box,
        sib_box
    );
}

#[test]
fn test_flex1_min_w0_inline_span_wraps_against_shrink0_button_and_in_grid_cards() {
    use strata_ui::buttons::{Button, ButtonVariant};

    let view = rsx! {
        <div class="w-[360px] flex flex-col gap-4 p-2">
            <div id="action-row" class="w-full flex items-center justify-between gap-3">
                <div id="text-wrap" class="flex-1 min-w-0">
                    <span id="long-span" class="text-sm">
                        {"This inline span description inside a flex-1 min-w-0 container must wrap cleanly and never bleed underneath the right-aligned shrink-0 button."}
                    </span>
                </div>
                <Button class="shrink-0" variant={ButtonVariant::Primary}>{"Launch"}</Button>
            </div>
            <div id="hub-grid" class="w-full grid grid-cols-2 gap-3">
                <div id="hub-card-1" class="p-2 flex flex-col gap-1">
                    <span id="lock-desc" class="text-xs text-yellow-400">
                        {"Workspace locked by another session until active telemetry synchronization completes across all nodes."}
                    </span>
                </div>
                <div id="hub-card-2" class="p-2 flex flex-col gap-1">
                    <span class="text-xs">{"Adjacent card"}</span>
                </div>
            </div>
        </div>
    };
    let mut rt = AppRuntime::new(view, 360.0, 480.0, 1.0);
    rt.tick_frame();

    let row_box = rt.query_element_rect("action-row").expect("action-row rect");
    let wrap_box = rt.query_element_rect("text-wrap").expect("text-wrap rect");
    let span_box = rt.query_element_rect("long-span").expect("long-span rect");
    let btn_box = rt.layout_root.children[0].children[1].geometry.border_box;

    assert!(
        btn_box.x + btn_box.width <= row_box.x + row_box.width + 0.5,
        "shrink-0 button overflowed row: row={:?} btn={:?}",
        row_box,
        btn_box
    );
    assert!(
        span_box.x + span_box.width <= btn_box.x + 0.5,
        "inline <span> bled horizontally underneath shrink-0 button: span={:?} wrap={:?} btn={:?}",
        span_box,
        wrap_box,
        btn_box
    );
    assert!(
        span_box.height > 30.0,
        "inline <span> must wrap onto multiple lines inside flex-1 min-w-0: span={:?}",
        span_box
    );

    let card1_box = rt.query_element_rect("hub-card-1").expect("hub-card-1 rect");
    let card2_box = rt.query_element_rect("hub-card-2").expect("hub-card-2 rect");
    let lock_box = rt.query_element_rect("lock-desc").expect("lock-desc rect");
    assert!(
        lock_box.x + lock_box.width <= card1_box.x + card1_box.width + 0.5
            && lock_box.x + lock_box.width <= card2_box.x + 0.5,
        "grid card lock description <span> overflowed into adjacent card: lock={:?} card1={:?} card2={:?}",
        lock_box,
        card1_box,
        card2_box
    );
    assert!(
        lock_box.height > 30.0,
        "grid card lock description <span> must wrap vertically: lock={:?}",
        lock_box
    );
}
