//! Regression tests for CSS 2.1 Appendix E stacking-context z-index hit-testing,
//! cursor/property/hover resolution, and SpatialIndex z-index point queries.

use std::cell::Cell;
use std::rc::Rc;
use strata::prelude::*;
use strata_dom::{Event, EventType};
use strata_platform::window::CursorIcon;

#[test]
fn test_card1_absolute_z50_dropdown_over_card2_dispatches_to_dropdown_and_spatial_index() {
    let menu_clicks = Rc::new(Cell::new(0usize));
    let card2_clicks = Rc::new(Cell::new(0usize));

    let menu_clicks_cb = Rc::clone(&menu_clicks);
    let card2_clicks_cb = Rc::clone(&card2_clicks);

    // Card #1 has `relative` (z-index: auto, does NOT create a stacking context)
    // and contains an `absolute right-2 top-10 z-50` dropdown wrapping a `w-48` panel
    // that extends downward over Card #2 (later in DOM order with `relative` / `z-index: auto`).
    let view = rsx! {
        <div class="flex flex-col w-[360px] h-[300px]">
            <div class="relative w-full h-[56px]">
                <span>"Card 1"</span>
                <div class="absolute right-2 top-10 z-50">
                    <div class="flex flex-col w-48 bg-[#18181b]">
                        <button
                            class="w-full h-[40px] cursor-crosshair"
                            data-menu-item="uninstall"
                            onclick={move |_ev: Event| {
                                menu_clicks_cb.set(menu_clicks_cb.get() + 1);
                            }}
                        >
                            "Uninstall"
                        </button>
                    </div>
                </div>
            </div>
            <div
                class="relative w-full h-[64px]"
                data-menu-item="card-2"
                onclick={move |_ev: Event| {
                    card2_clicks_cb.set(card2_clicks_cb.get() + 1);
                }}
            >
                <span>"Card 2"</span>
            </div>
        </div>
    };

    let runtime = AppRuntime::new(view, 360.0, 300.0, 1.0);

    // Verify Bug 2 + Bug 3 interaction:
    // Dropdown wrapper (`absolute right-2 top-10 z-50` with `width: auto` wrapping `w-48` = 192px)
    // must sit at x = 360 - 8 - 192 = 160..352, y = 40..80, directly overlapping Card #2 (y = 56..120).
    let card1_layout = &runtime.layout_root.children[0];
    let dropdown_layout = &card1_layout.children[1];
    let card2_layout = &runtime.layout_root.children[1];

    assert_eq!(dropdown_layout.geometry.border_box.width, 192.0);
    assert_eq!(dropdown_layout.geometry.border_box.height, 40.0);
    assert_eq!(dropdown_layout.geometry.border_box.x, 160.0);
    assert_eq!(dropdown_layout.geometry.border_box.y, 40.0);
    assert_eq!(card2_layout.geometry.border_box.y, 56.0);

    // Overlap point (240, 68) is inside BOTH Card #1's z-50 dropdown (160..352, 40..80)
    // AND Card #2 (0..360, 56..120).
    let (ox, oy) = (240.0, 68.0);
    assert!(dropdown_layout.geometry.border_box.contains_point(ox, oy));
    assert!(card2_layout.geometry.border_box.contains_point(ox, oy));

    // 1. Direct hit_test_click / dispatch_mouse_click must hit the z-50 dropdown button, NOT Card #2
    assert!(runtime.dispatch_mouse_click(ox, oy, 0, 1));
    assert_eq!(
        menu_clicks.get(),
        1,
        "Click on overlapping z-50 dropdown must trigger menu handler"
    );
    assert_eq!(
        card2_clicks.get(),
        0,
        "Card #2 beneath z-50 dropdown must not steal click"
    );

    // 2. Property, cursor, and hover node-path hit-testing must also resolve to the z-50 dropdown button
    let prop = AppRuntime::hit_test_property(
        &runtime.root_blueprint,
        &runtime.layout_root,
        ox,
        oy,
        "data-menu-item",
    );
    assert_eq!(prop.as_deref(), Some("uninstall"));

    let cur = AppRuntime::hit_test_cursor(&runtime.root_blueprint, &runtime.layout_root, ox, oy);
    assert_eq!(cur, CursorIcon::Crosshair);

    let hover_path =
        AppRuntime::hit_test_node_path(&runtime.root_blueprint, &runtime.layout_root, ox, oy);
    // Path [0, 1, 0, 0] is inside Card #1 -> Dropdown -> Panel -> Button (not Card #2 at [1, ...])
    assert_eq!(
        hover_path.as_deref().and_then(|p| p.first().copied()),
        Some(0)
    );

    // 3. SpatialIndex::query_event_at_point & query_entity_at_point must also hit the z-50 dropdown button
    let (_, spatial_handler, spatial_bounds) = runtime
        .spatial_index
        .query_event_at_point(ox, oy, EventType::Click)
        .expect("SpatialIndex should find Click listener at overlap");
    assert_eq!(spatial_bounds.x, 160.0);
    assert_eq!(spatial_bounds.width, 192.0);
    spatial_handler(Event::new(EventType::Click));
    assert_eq!(menu_clicks.get(), 2);
    assert_eq!(card2_clicks.get(), 0);

    let button_entity = dropdown_layout.children[0].children[0].entity_id;
    assert!(button_entity.is_valid());
    assert_eq!(
        runtime.spatial_index.query_entity_at_point(ox, oy),
        Some(button_entity)
    );

    // 4. Clicking on Card #2 outside the dropdown's horizontal bounds (x = 40, y = 68) still hits Card #2
    assert!(runtime.dispatch_mouse_click(40.0, 68.0, 0, 1));
    assert_eq!(card2_clicks.get(), 1);
}

#[test]
fn test_sibling_stacking_contexts_atomic_nesting_and_negative_z_index() {
    let hit_tag = Rc::new(Cell::new("none"));

    let t_a_inner = Rc::clone(&hit_tag);
    let t_b = Rc::clone(&hit_tag);
    let t_neg = Rc::clone(&hit_tag);
    let t_inflow = Rc::clone(&hit_tag);

    // Sibling A establishes an atomic stacking context at z-10, with a child at z-50.
    // Sibling B (later or earlier) establishes a stacking context at z-20 overlapping Sibling A.
    // Per CSS 2.1 Appendix E, Sibling B (z-20) paints & hit-tests above Sibling A's entire z-10 subtree!
    // Meanwhile at y = 200..260, an in-flow block is followed in DOM order by an `absolute z-[-10]` box;
    // the in-flow block (Layer 3) must win over the negative-z box (Layer 2).
    let view = rsx! {
        <div class="relative w-[300px] h-[300px]">
            <div class="absolute left-0 top-0 w-[120px] h-[120px] z-10">
                <button
                    class="absolute left-0 top-0 w-[120px] h-[120px] z-50"
                    onclick={move |_ev: Event| t_a_inner.set("a_inner_z50_inside_z10")}
                >
                    "Inner"
                </button>
            </div>
            <button
                class="absolute left-0 top-0 w-[120px] h-[120px] z-20"
                onclick={move |_ev: Event| t_b.set("sibling_b_z20")}
            >
                "Sibling B"
            </button>

            <button
                class="w-[120px] h-[60px] mt-[200px]"
                onclick={move |_ev: Event| t_inflow.set("in_flow_button")}
            >
                "In-Flow"
            </button>
            <div
                class="absolute left-0 top-[200px] w-[120px] h-[60px] -z-10"
                onclick={move |_ev: Event| t_neg.set("negative_z_bg")}
            >
                "Negative Z"
            </div>
        </div>
    };

    let runtime = AppRuntime::new(view, 300.0, 300.0, 1.0);

    // Overlap (50, 50): Sibling B (z-20) must beat Inner (z-50 trapped inside z-10 stacking context)
    assert!(runtime.dispatch_mouse_click(50.0, 50.0, 0, 1));
    assert_eq!(hit_tag.get(), "sibling_b_z20");

    let (_, sp_b, _) = runtime
        .spatial_index
        .query_event_at_point(50.0, 50.0, EventType::Click)
        .expect("SpatialIndex hit at (50, 50)");
    hit_tag.set("none");
    sp_b(Event::new(EventType::Click));
    assert_eq!(hit_tag.get(), "sibling_b_z20");

    // Overlap (50, 220): In-flow button (Layer 3) must beat -z-10 element (Layer 2) despite appearing earlier in DOM
    hit_tag.set("none");
    assert!(runtime.dispatch_mouse_click(50.0, 220.0, 0, 1));
    assert_eq!(hit_tag.get(), "in_flow_button");

    let (_, sp_inflow, _) = runtime
        .spatial_index
        .query_event_at_point(50.0, 220.0, EventType::Click)
        .expect("SpatialIndex hit at (50, 220)");
    hit_tag.set("none");
    sp_inflow(Event::new(EventType::Click));
    assert_eq!(hit_tag.get(), "in_flow_button");
}

#[test]
fn test_opaque_higher_z_floating_window_blocks_click_cursor_and_property_piercing() {
    use std::cell::Cell;
    use std::rc::Rc;

    let delete_clicks = Rc::new(Cell::new(0usize));
    let editor_mousedowns = Rc::new(Cell::new(0usize));
    let last_offset = Rc::new(Cell::new((-1.0f32, -1.0f32)));

    let dc = Rc::clone(&delete_clicks);
    let em = Rc::clone(&editor_mousedowns);
    let lo = Rc::clone(&last_offset);

    // Lower-z Explorer panel with destructive Delete File button directly underneath
    // an opaque z-40 FloatingWindow containing an editor viewport (binds MouseDown/MouseUp, NOT Click).
    let view = rsx! {
        <div class="relative w-[400px] h-[300px] bg-[#09090b]">
            <div class="w-[400px] h-[300px]">
                <button
                    id="delete-file-btn"
                    title="Delete File"
                    class="w-[240px] h-[200px] bg-[#dc2626] cursor-pointer"
                    onclick={move |_e: Event| dc.set(dc.get() + 1)}
                >
                    "Delete File"
                </button>
            </div>
            <div
                id="floating-script-editor"
                class="fixed left-[40px] top-[30px] w-[260px] h-[220px] z-40 bg-[#18181b]"
            >
                <div
                    id="editor-viewport"
                    class="w-[260px] h-[220px]"
                    onmousedown={move |e: Event| {
                        em.set(em.get() + 1);
                        lo.set((e.offset_x as f32, e.offset_y as f32));
                    }}
                    onmouseup={move |_e: Event| {}}
                >
                    <div class="w-full h-[24px]">
                        <span>"let x = 42;"</span>
                    </div>
                </div>
            </div>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    // Click at (100, 42) — inside both the floating editor viewport AND the hidden Delete button underneath.
    runtime.dispatch_pointer_down(100.0, 42.0);
    runtime.dispatch_pointer_up(100.0, 42.0);
    let click_handled = runtime.dispatch_mouse_click(100.0, 42.0, 0, 1);

    assert_eq!(
        editor_mousedowns.get(),
        1,
        "MouseDown inside floating editor must hit the editor viewport"
    );
    let (ox, oy) = last_offset.get();
    assert!(
        (ox - 60.0).abs() <= 1.0 && (oy - 12.0).abs() <= 1.0,
        "MouseDown offset_x/offset_y must stay relative to editor-viewport (60, 12), got ({}, {})",
        ox,
        oy
    );
    assert!(
        !click_handled && delete_clicks.get() == 0,
        "Click inside opaque z-40 floating window must NEVER pierce through to hidden lower-z Delete button"
    );
    assert!(
        AppRuntime::hit_test_click(&runtime.root_blueprint, &runtime.layout_root, 100.0, 42.0)
            .is_none(),
        "hit_test_click must return None over opaque higher-z overlay without a Click listener"
    );
    assert!(
        runtime
            .spatial_index
            .query_event_at_point(100.0, 42.0, EventType::Click)
            .is_none(),
        "SpatialIndex::query_event_at_point must NOT pierce opaque higher-z overlay"
    );
    assert_eq!(
        AppRuntime::hit_test_cursor(&runtime.root_blueprint, &runtime.layout_root, 100.0, 42.0),
        CursorIcon::Default,
        "hit_test_cursor must NOT leak pointer cursor from hidden lower-z button"
    );
    assert_eq!(
        AppRuntime::hit_test_property(
            &runtime.root_blueprint,
            &runtime.layout_root,
            100.0,
            42.0,
            "title"
        ),
        None,
        "hit_test_property must NOT pierce through opaque higher-z overlay"
    );
}

#[test]
fn test_opaque_higher_z_modal_scroll_does_not_bleed_wheel_to_lower_z_canvas3d() {
    let canvas_wheel_count = Rc::new(Cell::new(0usize));
    let cwc = Rc::clone(&canvas_wheel_count);

    let view = rsx! {
        <div class="relative w-[600px] h-[400px] bg-[#09090b]">
            <Canvas3D
                id="globe-canvas"
                class="w-[600px] h-[400px]"
                onwheel={move |e: Event| {
                    e.prevent_default();
                    cwc.set(cwc.get() + 1);
                }}
            />
            <div
                id="settings-modal-backdrop"
                class="fixed inset-0 z-50 bg-[#000000]/80 flex items-center justify-center"
            >
                <div
                    id="settings-scroll-pane"
                    class="w-[360px] h-[200px] overflow-y-auto bg-[#18181b]"
                >
                    <div class="w-full h-[900px]">
                        <span>"Long settings content"</span>
                    </div>
                </div>
            </div>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 600.0, 400.0, 1.0);
    runtime.freeze_clock_at_epoch();

    // Wheel in the center of the opaque z-50 SettingsModal (300, 200).
    let handled = runtime.dispatch_scroll(300.0, 200.0, 0.0, 120.0);
    assert!(
        handled,
        "Wheel event inside modal's overflow-y-auto pane should scroll the modal"
    );
    assert_eq!(
        canvas_wheel_count.get(),
        0,
        "Wheel event inside opaque z-50 modal must NEVER pierce through to lower-z Canvas3D onwheel"
    );
    assert!(
        runtime.scroll_offset_by_id("settings-scroll-pane").1 > 0.0
            || runtime.active_smooth_scroll.is_some(),
        "Modal overflow-y-auto pane must scroll instead of being locked by hidden Canvas3D"
    );
}

#[test]
fn test_pointer_events_none_overlay_still_passes_clicks_and_wheels_through() {
    use std::cell::Cell;
    use std::rc::Rc;

    let bg_clicks = Rc::new(Cell::new(0usize));
    let bg_wheels = Rc::new(Cell::new(0usize));
    let hud_clicks = Rc::new(Cell::new(0usize));

    let bc = Rc::clone(&bg_clicks);
    let bw = Rc::clone(&bg_wheels);
    let hc = Rc::clone(&hud_clicks);

    let view = rsx! {
        <div class="relative w-[400px] h-[300px]">
            <button
                id="bg-target"
                class="w-[400px] h-[300px] bg-[#1e293b]"
                onclick={move |_e: Event| bc.set(bc.get() + 1)}
                onwheel={move |e: Event| {
                    e.prevent_default();
                    bw.set(bw.get() + 1);
                }}
            >
                "Background"
            </button>
            <div class="fixed inset-0 z-50 pointer-events-none">
                <button
                    id="hud-btn"
                    class="w-[100px] h-[40px] pointer-events-auto bg-[#2563eb]"
                    onclick={move |_e: Event| hc.set(hc.get() + 1)}
                >
                    "HUD"
                </button>
            </div>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    // 1. Clicking/wheeling empty area of pointer-events-none overlay passes cleanly to bg-target
    assert!(runtime.dispatch_mouse_click(250.0, 200.0, 0, 1));
    assert_eq!(bg_clicks.get(), 1);
    assert_eq!(hud_clicks.get(), 0);
    runtime.dispatch_scroll(250.0, 200.0, 0.0, 60.0);
    assert_eq!(bg_wheels.get(), 1);

    // 2. Clicking pointer-events-auto child inside HUD hits hud-btn, NOT bg-target
    assert!(runtime.dispatch_mouse_click(40.0, 20.0, 0, 1));
    assert_eq!(hud_clicks.get(), 1);
    assert_eq!(bg_clicks.get(), 1);
}
