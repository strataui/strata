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
    assert_eq!(menu_clicks.get(), 1, "Click on overlapping z-50 dropdown must trigger menu handler");
    assert_eq!(card2_clicks.get(), 0, "Card #2 beneath z-50 dropdown must not steal click");

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
    assert_eq!(hover_path.as_deref().and_then(|p| p.first().copied()), Some(0));

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
