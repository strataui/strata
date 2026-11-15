use std::cell::RefCell;
use std::rc::Rc;
use strata::AppRuntime;
use strata_dom::{Event, View};
use strata_macro::rsx;

#[test]
fn test_cursor_warp_boundary_wrap_and_movement_delta_filtering() {
    let deltas: Rc<RefCell<Vec<(f64, f64)>>> = Rc::new(RefCell::new(Vec::new()));
    let deltas_cb = Rc::clone(&deltas);

    let view = rsx! {
        <div
            class="w-[800px] h-[400px]"
            onmousemove={move |ev: Event| {
                deltas_cb.borrow_mut().push((ev.movement_x, ev.movement_y));
                if ev.client_x >= 795.0 {
                    ev.warp_cursor(10.0, ev.client_y);
                }
            }}
        />
    };

    let mut rt = AppRuntime::new(view, 800.0, 400.0, 1.0);

    // 1. Initial move to (790, 200) -> movement (0, 0)
    rt.dispatch_pointer_move_action(790.0, 200.0);
    // 2. Move near right edge (798, 200) -> movement (8, 0), triggers warp to (10, 200)
    rt.dispatch_pointer_move_action(798.0, 200.0);
    // 3. Synthetic warp event at (10, 200) -> filtered, movement MUST be (0, 0)
    rt.dispatch_pointer_move_action(10.0, 200.0);
    // 4. Next real move to (14, 203) -> movement MUST be (4, 3), NOT (-784, 3)
    rt.dispatch_pointer_move_action(14.0, 203.0);

    let recorded = deltas.borrow();
    assert_eq!(recorded.len(), 4);
    assert_eq!(recorded[0], (0.0, 0.0));
    assert_eq!(recorded[1], (8.0, 0.0));
    assert_eq!(recorded[2], (0.0, 0.0));
    assert_eq!(recorded[3], (4.0, 3.0));
}

fn build_split_view(left_grow: f32, right_grow: f32) -> View {
    let left_style = format!("flex-grow: {}; flex-shrink: 1; flex-basis: 0px;", left_grow);
    let right_style = format!(
        "flex-grow: {}; flex-shrink: 1; flex-basis: 0px;",
        right_grow
    );
    rsx! {
        <div class="w-[1000px] h-[600px] flex">
            <div id="left-pane" style={left_style} class="h-full" />
            <div id="right-pane" style={right_style} class="h-full">
                <div id="unchanged-sibling" class="w-[100px] h-[100px]">
                    "Static Panel"
                </div>
            </div>
        </div>
    }
}

#[test]
fn test_isolated_split_ratio_update_and_incremental_reconcile() {
    let mut rt = AppRuntime::new(build_split_view(0.5, 0.5), 1000.0, 600.0, 1.0);
    let left_initial = rt.query_element_rect("left-pane").unwrap();
    let right_initial = rt.query_element_rect("right-pane").unwrap();
    assert!((left_initial.width - 500.0).abs() < 1.0);
    assert!((right_initial.width - 500.0).abs() < 1.0);

    // 1. Isolated layout-only split ratio update (no DOM rebuild).
    assert!(rt.update_split_ratios(&[("left-pane", 0.3), ("right-pane", 0.7)]));
    let left_after = rt.query_element_rect("left-pane").unwrap();
    let right_after = rt.query_element_rect("right-pane").unwrap();
    assert!((left_after.width - 300.0).abs() < 1.0);
    assert!((right_after.width - 700.0).abs() < 1.0);

    // 2. Incremental update_view when only pane flex ratios change preserves unchanged children.
    rt.update_view(build_split_view(0.25, 0.75));
    let left_reconciled = rt.query_element_rect("left-pane").unwrap();
    let right_reconciled = rt.query_element_rect("right-pane").unwrap();
    let sibling_reconciled = rt.query_element_rect("unchanged-sibling").unwrap();
    assert!((left_reconciled.width - 250.0).abs() < 1.0);
    assert!((right_reconciled.width - 750.0).abs() < 1.0);
    assert!((sibling_reconciled.width - 100.0).abs() < 1.0);
}
