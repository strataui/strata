//! Stateful Select: open/close and value state live inside the component and survive
//! root re-invocation.

use strata::prelude::*;
use strata_dom::clear_persistent_signals;
use strata_platform::{MouseButton, PlatformEvent};
use strata_ui::inputs::{select_step_index, Select, SelectOption};

fn root() -> View {
    rsx! {
        <div class="w-80 p-4">
            <Select
                id="region"
                label="Region"
                options={vec![
                    SelectOption::new("us", "United States"),
                    SelectOption::new("eu", "Europe"),
                    SelectOption::new("ap", "Asia Pacific"),
                ]}
            />
        </div>
    }
}

fn click(runtime: &mut AppRuntime, x: f32, y: f32) {
    for event in [
        PlatformEvent::MouseDown {
            button: MouseButton::Left,
            x,
            y,
        },
        PlatformEvent::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        },
    ] {
        runtime.step_event(&event);
    }
}

fn has_listbox(runtime: &AppRuntime) -> bool {
    format!("{:?}", runtime.root_blueprint).contains("aria-selected")
}

#[test]
fn select_opens_closes_and_keeps_state_across_reinvocation() {
    clear_persistent_signals();
    let mut runtime = AppRuntime::new(root(), 400.0, 300.0, 1.0);
    runtime.tick_frame();
    assert!(!has_listbox(&runtime), "starts closed");

    // The trigger sits under the label inside the 16px padding.
    let (tx, ty) = (60.0, 16.0 + 14.0 + 20.0);
    click(&mut runtime, tx, ty);
    runtime.update_view(root());
    assert!(has_listbox(&runtime), "click on the trigger opens the list");

    // Re-invoking the root must not reset the open state.
    runtime.update_view(root());
    assert!(
        has_listbox(&runtime),
        "open state survives root re-invocation"
    );
    let rendered = format!("{:?}", runtime.root_blueprint);
    assert!(rendered.contains("Europe") && rendered.contains("Asia Pacific"));
    assert!(rendered.contains("combobox"));

    click(&mut runtime, tx, ty);
    runtime.update_view(root());
    assert!(!has_listbox(&runtime), "second click closes the list");
}

#[test]
fn select_step_index_clamps() {
    assert_eq!(select_step_index(3, 0, -1), 0);
    assert_eq!(select_step_index(3, 0, 1), 1);
    assert_eq!(select_step_index(3, 2, 1), 2);
    assert_eq!(select_step_index(0, 5, 1), 0);
}

fn send_key(runtime: &mut AppRuntime, key: strata_platform::KeyCode) {
    runtime.step_event(&PlatformEvent::KeyDown {
        key,
        modifiers: strata_platform::Modifiers::default(),
    });
    runtime.update_view(root());
}

fn find_rect_by_role<'a>(
    bp: &'a strata_dom::NodeBlueprint,
    layout: &'a strata_layout::LayoutNode,
    role: &str,
) -> Option<strata_layout::Rect> {
    if let strata_dom::NodeBlueprint::Element { attributes, .. } = bp {
        for attr in attributes {
            if let strata_dom::Attribute::Property { key, value } = attr {
                if key == "role" && value == role {
                    return Some(layout.geometry.border_box);
                }
            }
        }
    }
    let mut flat_bp = Vec::new();
    match bp {
        strata_dom::NodeBlueprint::Element { children, .. }
        | strata_dom::NodeBlueprint::Fragment(children) => {
            fn flatten<'x>(
                n: &'x strata_dom::NodeBlueprint,
                out: &mut Vec<&'x strata_dom::NodeBlueprint>,
            ) {
                if let strata_dom::NodeBlueprint::Fragment(c) = n {
                    for x in c {
                        flatten(x, out);
                    }
                } else {
                    out.push(n);
                }
            }
            for c in children {
                flatten(c, &mut flat_bp);
            }
        }
        _ => {}
    }
    for (child_bp, child_layout) in flat_bp.iter().zip(layout.children.iter()) {
        if let Some(r) = find_rect_by_role(child_bp, child_layout, role) {
            return Some(r);
        }
    }
    None
}

#[test]
fn select_keyboard_navigation_opens_steps_selects_and_escapes() {
    clear_persistent_signals();
    let mut runtime = AppRuntime::new(root(), 400.0, 300.0, 1.0);
    runtime.tick_frame();

    // Click trigger to focus and open, then Escape to close while keeping focus.
    let (tx, ty) = (60.0, 16.0 + 14.0 + 20.0);
    click(&mut runtime, tx, ty);
    runtime.update_view(root());
    assert!(has_listbox(&runtime));

    send_key(&mut runtime, strata_platform::KeyCode::Escape);
    assert!(!has_listbox(&runtime), "Escape closes the open listbox");

    // ArrowDown on focused trigger opens the listbox.
    send_key(&mut runtime, strata_platform::KeyCode::ArrowDown);
    assert!(has_listbox(&runtime), "ArrowDown opens the listbox");

    // ArrowDown steps from index 0 ("United States") to index 1 ("Europe").
    send_key(&mut runtime, strata_platform::KeyCode::ArrowDown);
    // Enter selects "Europe" and closes the listbox.
    send_key(&mut runtime, strata_platform::KeyCode::Enter);
    assert!(!has_listbox(&runtime), "Enter closes after selection");

    let rendered = format!("{:?}", runtime.root_blueprint);
    assert!(
        rendered.contains("Europe"),
        "selected option Europe should be rendered in trigger"
    );
}

#[test]
fn select_portal_flips_above_trigger_near_bottom_viewport_edge() {
    clear_persistent_signals();
    let bottom_root = || {
        rsx! {
            <div class="w-80 h-[200px] flex flex-col justify-end p-2">
                <Select
                    id="bottom-sel"
                    options={vec![
                        SelectOption::new("a", "Alpha"),
                        SelectOption::new("b", "Beta"),
                        SelectOption::new("c", "Gamma"),
                        SelectOption::new("d", "Delta"),
                    ]}
                />
            </div>
        }
    };
    let mut runtime = AppRuntime::new(bottom_root(), 400.0, 200.0, 1.0);
    runtime.tick_frame();

    let trigger_rect = find_rect_by_role(&runtime.root_blueprint, &runtime.layout_root, "combobox")
        .expect("trigger combobox rect");
    click(
        &mut runtime,
        trigger_rect.x + trigger_rect.width * 0.5,
        trigger_rect.y + trigger_rect.height * 0.5,
    );
    runtime.update_view(bottom_root());

    let listbox_rect = find_rect_by_role(&runtime.root_blueprint, &runtime.layout_root, "listbox")
        .expect("listbox rect when open");
    assert!(
        listbox_rect.y + listbox_rect.height <= 200.0 + 1.0,
        "portal listbox should stay inside viewport bottom (rect={:?})",
        listbox_rect
    );
    assert!(
        listbox_rect.y < trigger_rect.y,
        "portal listbox should flip above trigger near bottom edge (trigger={:?}, listbox={:?})",
        trigger_rect,
        listbox_rect
    );
}

#[test]
fn select_portal_tracks_scrolled_container_position() {
    clear_persistent_signals();
    let scroll_root = || {
        rsx! {
            <div class="w-80 h-[200px] overflow-y-auto p-2">
                <div class="h-[140px]">"Spacer"</div>
                <Select
                    id="scroll-sel"
                    options={vec![
                        SelectOption::new("x", "X-Ray"),
                        SelectOption::new("y", "Yankee"),
                    ]}
                />
                <div class="h-[240px]">"Bottom"</div>
            </div>
        }
    };
    let mut runtime = AppRuntime::new(scroll_root(), 400.0, 200.0, 1.0);
    runtime.tick_frame();

    let trigger_before =
        find_rect_by_role(&runtime.root_blueprint, &runtime.layout_root, "combobox")
            .expect("trigger combobox rect before scroll");

    // Scroll container down (wheel negative delta_y scrolls content up / scroll_y increases).
    runtime.dispatch_scroll(50.0, 50.0, 0.0, 80.0);

    let trigger_after =
        find_rect_by_role(&runtime.root_blueprint, &runtime.layout_root, "combobox")
            .expect("trigger combobox rect after scroll");
    assert!(
        trigger_after.y < trigger_before.y,
        "scrolling down should move trigger upward (before={:?}, after={:?})",
        trigger_before,
        trigger_after
    );

    click(
        &mut runtime,
        trigger_after.x + 20.0,
        trigger_after.y + trigger_after.height * 0.5,
    );
    runtime.update_view(scroll_root());

    let listbox_rect = find_rect_by_role(&runtime.root_blueprint, &runtime.layout_root, "listbox")
        .expect("listbox rect when open");
    assert!(
        listbox_rect.y < 140.0,
        "portal listbox should reflect scroll offset (trigger={:?}, listbox={:?})",
        trigger_after,
        listbox_rect
    );
}

#[test]
fn select_popover_expands_wider_than_narrow_trigger_and_stays_single_line() {
    clear_persistent_signals();
    let narrow_root = || {
        rsx! {
            <div class="w-[84px] h-[240px] p-1">
                <Select
                    id="narrow-sel"
                    options={vec![
                        SelectOption::new("3d", "Seamless 2.5D Globe"),
                        SelectOption::new("2d", "2D Flat Imagery Mode"),
                    ]}
                />
            </div>
        }
    };
    let mut runtime = AppRuntime::new(narrow_root(), 420.0, 240.0, 1.0);
    runtime.tick_frame();

    let trigger_rect = find_rect_by_role(&runtime.root_blueprint, &runtime.layout_root, "combobox")
        .expect("trigger combobox rect");
    assert!(trigger_rect.width <= 84.0);
    click(
        &mut runtime,
        trigger_rect.x + trigger_rect.width * 0.5,
        trigger_rect.y + trigger_rect.height * 0.5,
    );
    runtime.update_view(narrow_root());

    let listbox_rect = find_rect_by_role(&runtime.root_blueprint, &runtime.layout_root, "listbox")
        .expect("listbox rect when open");
    assert!(
        listbox_rect.width >= 160.0 && listbox_rect.width > trigger_rect.width + 50.0,
        "Select popover must expand wider than narrow trigger (trigger={:?}, listbox={:?})",
        trigger_rect,
        listbox_rect
    );
    assert!(
        listbox_rect.height < 96.0,
        "single-line options must not wrap excessively (listbox={:?})",
        listbox_rect
    );
}

#[test]
fn select_wide_anchor_stretches_listbox_and_aligns_overlay_scrollbar() {
    clear_persistent_signals();
    let options: Vec<SelectOption> = (0..16)
        .map(|i| SelectOption::new(format!("board-{i}"), format!("CubeOrangePlus-{i}")))
        .collect();
    let wide_root = move || {
        rsx! {
            <div class="w-[660px] h-[500px] p-3">
                <Select id="wide-board" label="Board" options={options.clone()} />
            </div>
        }
    };
    let mut runtime = AppRuntime::new(wide_root(), 720.0, 500.0, 1.0);
    runtime.tick_frame();

    let trigger_rect = find_rect_by_role(&runtime.root_blueprint, &runtime.layout_root, "combobox")
        .expect("trigger combobox rect");
    assert!(
        trigger_rect.width >= 620.0,
        "expected wide trigger, got {:?}",
        trigger_rect
    );

    click(
        &mut runtime,
        trigger_rect.x + trigger_rect.width * 0.5,
        trigger_rect.y + trigger_rect.height * 0.5,
    );
    runtime.update_view(wide_root());

    let listbox_rect = find_rect_by_role(&runtime.root_blueprint, &runtime.layout_root, "listbox")
        .expect("open listbox rect");
    assert!(
        (listbox_rect.width - trigger_rect.width).abs() < 4.0,
        "Select listbox must match wide anchor width (trigger={:?}, listbox={:?})",
        trigger_rect,
        listbox_rect
    );

    if let Some(bar) = runtime.active_scrollbars.iter().find(|b| b.is_top_layer) {
        assert!(
            bar.track_bounds.y >= listbox_rect.y - 0.5
                && bar.track_bounds.y + bar.track_bounds.height
                    <= listbox_rect.y + listbox_rect.height + 0.5
                && bar.thumb_bounds.y >= listbox_rect.y - 0.5
                && bar.thumb_bounds.x >= listbox_rect.x
                && bar.thumb_bounds.x + bar.thumb_bounds.width
                    <= listbox_rect.x + listbox_rect.width + 0.5,
            "Top-layer Select scrollbar track/thumb {:?} / {:?} must stay strictly inside listbox {:?}",
            bar.track_bounds,
            bar.thumb_bounds,
            listbox_rect
        );
    }
}


