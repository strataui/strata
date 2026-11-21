//! End-to-end integration tests for the Top-Layer `<Portal>` component,
//! `create_portal` / `create_anchored_portal` / `create_dropdown_portal` helpers,
//! and CSS Anchor Positioning (`anchor-name` / `position-anchor` / `position-area`)
//! with live scroll tracking and automatic viewport collision flipping (`bottom-end <-> top-end`).

use std::cell::Cell;
use std::rc::Rc;
use strata::prelude::*;
use strata_dom::Event;

#[cfg(target_os = "linux")]
fn render_to_argb_pixels(runtime: &mut AppRuntime, w: i32, h: i32) -> Vec<u8> {
    runtime.compile_display_list();
    let mut surface =
        strata_platform::cairo::ImageSurface::create(strata_platform::cairo::Format::ARgb32, w, h)
            .expect("Cairo ARgb32 surface");
    {
        let cr = strata_platform::cairo::Context::new(&surface).expect("Cairo context");
        cr.set_source_rgb(0.0, 0.0, 0.0);
        let _ = cr.paint();
        strata_platform::render_display_list_to_cairo(&cr, &runtime.display_list);
    }
    surface.flush();
    let data = surface.data().expect("Cairo surface data");
    data.to_vec()
}

#[cfg(target_os = "linux")]
fn pixel_rgb(buf: &[u8], stride_w: usize, x: usize, y: usize) -> (u8, u8, u8) {
    let idx = (y * stride_w + x) * 4;
    (buf[idx + 2], buf[idx + 1], buf[idx])
}

fn find_node_by_element_id<'a>(
    node: &'a strata_layout::LayoutNode,
    target_id: &str,
) -> Option<&'a strata_layout::LayoutNode> {
    if node.element_id.as_deref() == Some(target_id) {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_node_by_element_id(child, target_id) {
            return Some(found);
        }
    }
    None
}

#[test]
fn portal_escapes_overflow_hidden_transform_and_higher_sibling_z_index() {
    let menu_clicks = Rc::new(Cell::new(0usize));
    let card2_clicks = Rc::new(Cell::new(0usize));

    let menu_cb = Rc::clone(&menu_clicks);
    let card2_cb = Rc::clone(&card2_clicks);

    // Card #1 has BOTH `overflow-hidden`, low `z-10`, AND a `transform` (which normally traps
    // even `position: fixed` in CSS!), while sibling Card #2 has `z-50`.
    // A `<Portal anchor="card1-trigger" placement="bottom-end" offset={6.0}>` inside Card #1
    // must escape all clips, transforms, and stacking contexts into the root Top Layer.
    let view = rsx! {
        <div class="flex flex-col w-[360px] h-[280px] bg-[#09090b]">
            <div
                class="relative z-10 flex flex-row items-center justify-end w-[360px] h-[52px] px-3 overflow-hidden bg-[#18181b]"
                style="transform: translateX(0px);"
            >
                <button
                    id="card1-trigger"
                    class="w-[36px] h-[28px] bg-[#3f3f46]"
                >
                    "..."
                </button>
                <Portal anchor="card1-trigger" placement="bottom-end" offset={6.0}>
                    <button
                        id="card1-portal-menu"
                        class="w-[160px] h-[110px] bg-[#ef4444] hover:bg-[#facc15]"
                        onclick={move |_e: Event| menu_cb.set(menu_cb.get() + 1)}
                    >
                        "Uninstall"
                    </button>
                </Portal>
            </div>
            <div
                id="card2-higher-z"
                class="relative z-50 w-[360px] h-[90px] bg-[#2563eb]"
                onclick={move |_e: Event| card2_cb.set(card2_cb.get() + 1)}
            >
                <span>"Card 2 (z-50)"</span>
            </div>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 360.0, 280.0, 1.0);

    let trig_box = find_node_by_element_id(&runtime.layout_root, "card1-trigger")
        .expect("trigger node")
        .geometry
        .border_box;
    let menu_box = find_node_by_element_id(&runtime.layout_root, "card1-portal-menu")
        .expect("portaled menu node")
        .geometry
        .border_box;

    // Right-aligned (`bottom-end`) to trigger with 6px vertical offset
    let expected_x = trig_box.x + trig_box.width - 160.0;
    let expected_y = trig_box.y + trig_box.height + 6.0;
    assert!(
        (menu_box.x - expected_x).abs() <= 1.0,
        "Expected bottom-end x={} near {}, got {}",
        expected_x,
        trig_box.x + trig_box.width,
        menu_box.x
    );
    assert!(
        (menu_box.y - expected_y).abs() <= 1.0,
        "Expected bottom-end y={}, got {}",
        expected_y,
        menu_box.y
    );

    // Sample point inside the portaled menu where it overlaps Card #2 (`y = 80` is well below
    // Card #1's `52px` overflow-hidden box and squarely inside Card #2's `z-50` box!).
    let probe_x = menu_box.x + 80.0;
    let probe_y = 80.0;
    assert!(
        menu_box.contains_point(probe_x, probe_y),
        "Probe ({}, {}) must lie inside menu {:?}",
        probe_x,
        probe_y,
        menu_box
    );

    // 1. Cairo pixel verification: Top-Layer portal paints bright red (#ef4444) over Card #2's z-50 blue (#2563eb)
    #[cfg(target_os = "linux")]
    {
        let pixels = render_to_argb_pixels(&mut runtime, 360, 280);
        let (r, g, b) = pixel_rgb(&pixels, 360, probe_x as usize, probe_y as usize);
        assert!(
            r > 200 && g < 100 && b < 100,
            "Expected portaled menu red (#ef4444) above Card #2 z-50 and outside Card #1 overflow-hidden, got RGB({}, {}, {})",
            r,
            g,
            b
        );
    }

    // 2. SpatialIndex + click dispatch route to the portaled menu, NOT Card #2
    let menu_entity = find_node_by_element_id(&runtime.layout_root, "card1-portal-menu")
        .unwrap()
        .entity_id;
    assert_eq!(
        runtime
            .spatial_index
            .query_entity_at_point(probe_x, probe_y),
        Some(menu_entity),
        "SpatialIndex must rank top-layer portal above z-50 sibling stacking contexts"
    );

    runtime.dispatch_mouse_click(probe_x, probe_y, 0, 1);
    assert_eq!(menu_clicks.get(), 1);
    assert_eq!(card2_clicks.get(), 0);

    // 3. `:hover` routing updates portaled menu background to #facc15 yellow
    assert!(runtime.update_hover(probe_x, probe_y));
    #[cfg(target_os = "linux")]
    {
        let pixels = render_to_argb_pixels(&mut runtime, 360, 280);
        let (r, g, b) = pixel_rgb(&pixels, 360, probe_x as usize, probe_y as usize);
        assert!(
            r > 220 && g > 180 && b < 60,
            "Expected hovered portaled menu yellow (#facc15), got RGB({}, {}, {})",
            r,
            g,
            b
        );
    }
}

#[test]
fn anchored_portal_live_scroll_tracking_and_automatic_viewport_collision_flipping() {
    let bottom_menu_clicks = Rc::new(Cell::new(0usize));
    let bottom_menu_cb = Rc::clone(&bottom_menu_clicks);

    // Viewport is 360x260.
    // Inside `<main class="overflow-y-auto">`:
    // - Spacer 200px puts `near-bottom-trigger` at y = 200..228 (only 32px left below in 260px viewport).
    // - A 140x90 menu with `placement="bottom-end" offset={6.0}` needs 96px, so it MUST auto-flip
    //   to `top-end` (`y = 200 - 90 - 6 = 104`).
    // - After scrolling `<main>` down by 110px, `near-bottom-trigger` moves to `y = 90..118`
    //   (now 142px available below!), so the portal tracks the scroll live AND auto-flips back
    //   to `bottom-end` (`y = 118 + 6 = 124`).
    // - Also includes `left-edge-trigger` near x = 10..42 with a 180px-wide `bottom-end` menu to
    //   verify viewport `shift` clamping (`x == 8.0` padding instead of `-138.0`).
    let view = rsx! {
        <main class="flex flex-col w-[360px] h-[260px] overflow-y-auto bg-[#09090b]">
            <div class="flex flex-row items-start w-[360px] h-[200px] shrink-0 pl-[10px] pt-[12px]">
                <button id="left-edge-trigger" class="w-[32px] h-[24px] bg-[#3f3f46]">"L"</button>
                <Portal anchor="left-edge-trigger" placement="bottom-end" offset={4.0} padding={8.0}>
                    <div id="left-clamped-menu" class="w-[180px] h-[48px] bg-[#10b981]">
                        "Clamped Menu"
                    </div>
                </Portal>
            </div>
            <div class="flex flex-row justify-end w-[360px] h-[40px] shrink-0 pr-[20px]">
                <button id="near-bottom-trigger" class="w-[40px] h-[28px] bg-[#3f3f46]">"..."</button>
                <Portal anchor="near-bottom-trigger" placement="bottom-end" offset={6.0}>
                    <button
                        id="flipping-menu"
                        class="w-[140px] h-[90px] bg-[#ef4444]"
                        onclick={move |_e: Event| bottom_menu_cb.set(bottom_menu_cb.get() + 1)}
                    >
                        "Action"
                    </button>
                </Portal>
            </div>
            <div class="w-[360px] h-[360px] shrink-0 bg-[#18181b]" />
        </main>
    };

    let mut runtime = AppRuntime::new(view, 360.0, 260.0, 1.0);

    // 1. Horizontal viewport shift check on `left-clamped-menu`
    let clamped_box = find_node_by_element_id(&runtime.layout_root, "left-clamped-menu")
        .expect("left-clamped-menu")
        .geometry
        .border_box;
    assert!(
        (clamped_box.x - 8.0).abs() <= 1.0,
        "Expected horizontal shift to clamp menu x to viewport padding 8.0, got {}",
        clamped_box.x
    );

    // 2. Initial collision flip check: `bottom-end` -> `top-end` near bottom of viewport
    let trig_initial = find_node_by_element_id(&runtime.layout_root, "near-bottom-trigger")
        .expect("near-bottom-trigger")
        .geometry
        .border_box;
    let menu_initial = find_node_by_element_id(&runtime.layout_root, "flipping-menu")
        .expect("flipping-menu")
        .geometry
        .border_box;

    let expected_flipped_y = trig_initial.y - 90.0 - 6.0;
    let expected_end_x = trig_initial.x + trig_initial.width - 140.0;
    assert!(
        (menu_initial.y - expected_flipped_y).abs() <= 1.0,
        "Expected near-bottom menu to flip upward to top-end y={}, got {} (trigger y={})",
        expected_flipped_y,
        menu_initial.y,
        trig_initial.y
    );
    assert!(
        (menu_initial.x - expected_end_x).abs() <= 1.0,
        "Expected right-aligned x={}, got {}",
        expected_end_x,
        menu_initial.x
    );

    // 3. Scroll `<main>` down by 110px: trigger moves up by 110px, giving ample room below,
    // so the anchored portal tracks the scroll live and flips back to `bottom-end`!
    let layout_count_before = runtime.layout_compute_count();
    assert!(runtime.dispatch_scroll(180.0, 130.0, 0.0, 110.0));
    assert_eq!(
        runtime.layout_compute_count(),
        layout_count_before,
        "Scroll tracking of anchored portals must run in O(portals) without recomputing layout"
    );

    let trig_scrolled = find_node_by_element_id(&runtime.layout_root, "near-bottom-trigger")
        .unwrap()
        .geometry
        .border_box;
    let menu_scrolled = find_node_by_element_id(&runtime.layout_root, "flipping-menu")
        .unwrap()
        .geometry
        .border_box;

    let expected_bottom_y = trig_scrolled.y + trig_scrolled.height + 6.0;
    assert!(
        (menu_scrolled.y - expected_bottom_y).abs() <= 1.0,
        "After scrolling trigger into middle of viewport, expected bottom-end y={}, got {}",
        expected_bottom_y,
        menu_scrolled.y
    );

    // Clicking the newly repositioned menu dispatches its click listener cleanly
    runtime.dispatch_mouse_click(menu_scrolled.x + 30.0, menu_scrolled.y + 30.0, 0, 1);
    assert_eq!(bottom_menu_clicks.get(), 1);
}

#[test]
fn create_dropdown_portal_match_width_and_css_anchor_positioning_work() {
    let dismissed = Rc::new(Cell::new(0usize));
    let item_selected = Rc::new(Cell::new(0usize));

    let dismiss_cb = Rc::clone(&dismissed);
    let select_cb = Rc::clone(&item_selected);

    let dropdown = create_dropdown_portal(
        "select-trigger",
        "bottom-start",
        4.0,
        move |_e: Event| dismiss_cb.set(dismiss_cb.get() + 1),
        rsx! {
            <button
                id="dropdown-option"
                class="w-[120px] h-[36px] bg-[#2563eb]"
                onclick={move |_e: Event| select_cb.set(select_cb.get() + 1)}
            >
                "Option A"
            </button>
        },
    );

    let view = rsx! {
        <div class="flex flex-col w-[400px] h-[300px] p-4 gap-4 bg-[#09090b]">
            <button id="select-trigger" class="w-[220px] h-[36px] bg-[#27272a]">
                "Choose item"
            </button>
            {dropdown}
            <Portal anchor="select-trigger" placement="top-start" offset={4.0} match_width={true}>
                <div id="matched-width-tooltip" class="w-full h-[28px] bg-[#10b981]">
                    "Same width as trigger"
                </div>
            </Portal>
            <button
                class="w-[80px] h-[30px] bg-[#3f3f46]"
                style="anchor-name: --css-anchor-btn;"
            >
                "CSS Anchor"
            </button>
            <div
                id="css-anchored-popover"
                class="w-[100px] h-[32px] bg-[#a855f7]"
                style="position-anchor: --css-anchor-btn; position-area: right-start;"
            >
                "Popover"
            </div>
        </div>
    };

    let runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    // 1. `match_width={true}` sizes the portal to `select-trigger`'s 220px width
    let trig_box = find_node_by_element_id(&runtime.layout_root, "select-trigger")
        .unwrap()
        .geometry
        .border_box;
    let matched_box = find_node_by_element_id(&runtime.layout_root, "matched-width-tooltip")
        .unwrap()
        .geometry
        .border_box;
    assert!(
        (matched_box.width - trig_box.width).abs() <= 1.0,
        "Expected match_width portal width {} to match trigger width {}",
        matched_box.width,
        trig_box.width
    );

    // 2. CSS `anchor-name: --css-anchor-btn` + `position-anchor: --css-anchor-btn; position-area: right-start`
    let css_pop = find_node_by_element_id(&runtime.layout_root, "css-anchored-popover")
        .unwrap()
        .geometry
        .border_box;
    assert!(
        css_pop.x > 90.0 && css_pop.y > 50.0,
        "Expected CSS position-anchor popover to position to the right of --css-anchor-btn, got {:?}",
        css_pop
    );

    // 3. Clicking inside `dropdown-option` hits the menu item (NOT the dismiss backdrop)
    let opt_box = find_node_by_element_id(&runtime.layout_root, "dropdown-option")
        .unwrap()
        .geometry
        .border_box;
    runtime.dispatch_mouse_click(opt_box.x + 20.0, opt_box.y + 15.0, 0, 1);
    assert_eq!(item_selected.get(), 1);
    assert_eq!(dismissed.get(), 0);

    // 4. Clicking outside on empty viewport space hits `create_dropdown_portal`'s top-layer backdrop
    runtime.dispatch_mouse_click(380.0, 280.0, 0, 1);
    assert_eq!(dismissed.get(), 1);
}

#[test]
fn test_path_qualified_strata_dom_portal_renders_and_dispatches_clicks() {
    let modal_clicks = Rc::new(Cell::new(0usize));
    let anchored_clicks = Rc::new(Cell::new(0usize));

    let mc = modal_clicks.clone();
    let ac = anchored_clicks.clone();

    let view = rsx! {
        <div class="w-[400px] h-[300px] relative overflow-hidden">
            <div class="w-[160px] h-[60px] overflow-hidden">
                <button id="qual-trig" class="w-[100px] h-[32px]">{"Open"}</button>
                <strata_dom::Portal>
                    <button
                        id="qual-modal-btn"
                        style="position: fixed; left: 220px; top: 180px; width: 120px; height: 40px;"
                        onclick={move |_: Event| mc.set(mc.get() + 1)}
                    >
                        {"Modal Action"}
                    </button>
                </strata_dom::Portal>
                <::strata_dom::Portal anchor="qual-trig" placement="bottom-start" offset={4.0}>
                    <button
                        id="qual-anchored-btn"
                        class="w-[120px] h-[36px]"
                        onclick={move |_: Event| ac.set(ac.get() + 1)}
                    >
                        {"Anchored Action"}
                    </button>
                </::strata_dom::Portal>
            </div>
        </div>
    };
    let runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    // Unanchored `<strata_dom::Portal>` escapes the 160x60 overflow-hidden container
    runtime.dispatch_mouse_click(250.0, 200.0, 0, 1);
    assert_eq!(modal_clicks.get(), 1);

    // Anchored `<::strata_dom::Portal>` positions below `qual-trig` and receives clicks outside the 60px clip
    let anchored_box = find_node_by_element_id(&runtime.layout_root, "qual-anchored-btn")
        .expect("qual-anchored-btn in layout tree")
        .geometry
        .border_box;
    assert!(
        (anchored_box.y - 36.0).abs() <= 1.5,
        "Expected anchored ::strata_dom::Portal at y ~ 36, got {:?}",
        anchored_box
    );
    runtime.dispatch_mouse_click(anchored_box.x + 20.0, anchored_box.y + 30.0, 0, 1);
    assert_eq!(anchored_clicks.get(), 1);
}

#[test]
fn portal_scrollbar_uses_post_translation_bounds_clips_display_list_and_drags() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div class="w-[800px] h-[400px] p-5 bg-[#09090b]">
                <div class="w-[240px] h-[80px] ml-[400px] mt-[20px] overflow-hidden">
                    <button id="scroll-menu-trig" class="w-[120px] h-[30px] bg-[#2563eb]">
                        {"Open Scrolled Menu"}
                    </button>
                    <Portal anchor="scroll-menu-trig" placement="bottom-start" offset={6.0}>
                        <div
                            id="portal-scroll-list"
                            class="flex flex-col w-[180px] h-[120px] overflow-y-auto bg-[#18181b]"
                        >
                            <div class="h-[60px] w-full shrink-0" />
                            <div class="h-[60px] w-full shrink-0" />
                            <div class="h-[60px] w-full shrink-0" />
                            <div class="h-[60px] w-full shrink-0" />
                            <div class="h-[60px] w-full shrink-0" />
                            <div class="h-[60px] w-full shrink-0" />
                        </div>
                    </Portal>
                </div>
            </div>
        },
        800.0,
        400.0,
        1.0,
    );

    let list_box = find_node_by_element_id(&runtime.layout_root, "portal-scroll-list")
        .expect("portal-scroll-list must exist")
        .geometry
        .border_box;
    assert!(
        list_box.x > 400.0 && list_box.y >= 60.0,
        "expected portal list anchored past x=400, got {:?}",
        list_box
    );

    let bar = runtime
        .active_scrollbars
        .iter()
        .find(|b| (b.bounds.x - list_box.x).abs() < 2.0)
        .cloned()
        .expect("portal scrollbar must reflect post-translation portal coordinates");
    assert!(bar.is_top_layer);
    assert!(
        (bar.bounds.y - list_box.y).abs() < 2.0
            && (bar.thumb_bounds.x - list_box.x) > 100.0
            && (bar.clip_bounds.height - list_box.height).abs() < 2.0,
        "portal scrollbar track/thumb/clip {:?} / {:?} must align with translated list_box {:?}",
        bar.thumb_bounds,
        bar.clip_bounds,
        list_box
    );

    // Rendered scrollbar thumb must be clipped to `bar.clip_bounds`
    runtime.compile_display_list();
    let clip_idx = runtime
        .display_list
        .commands
        .iter()
        .rposition(|cmd| matches!(
            cmd,
            strata::render::compositor::DrawCommand::PushClip { bounds, .. }
                if (bounds.x - bar.clip_bounds.x).abs() < 1.0
                    && (bounds.y - bar.clip_bounds.y).abs() < 1.0
                    && (bounds.height - bar.clip_bounds.height).abs() < 1.0
        ))
        .expect("portal scrollbar must push clip_bounds before drawing thumb");
    assert!(
        matches!(
            runtime.display_list.commands.get(clip_idx + 1),
            Some(strata::render::compositor::DrawCommand::DrawRoundedRect { .. })
        ) && matches!(
            runtime.display_list.commands.get(clip_idx + 2),
            Some(strata::render::compositor::DrawCommand::PopClip)
        ),
        "portal scrollbar thumb DrawRoundedRect must be directly wrapped in PushClip/PopClip"
    );

    // Thumb press works at translated coordinates and dragging scrolls the portal list
    let tx = bar.thumb_bounds.x + bar.thumb_bounds.width * 0.5;
    let ty = bar.thumb_bounds.y + bar.thumb_bounds.height * 0.5;
    assert!(!runtime.check_scrollbar_thumb_press(10.0, 10.0));
    assert!(runtime.check_scrollbar_thumb_press(tx, ty));
    assert!(runtime.poll_scrollbar_drag(tx, ty + 40.0));
    let new_scroll = runtime
        .active_scrollbars
        .iter()
        .find(|b| (b.bounds.x - list_box.x).abs() < 2.0)
        .unwrap()
        .scroll_y;
    assert!(new_scroll > 0.0, "dragging portal thumb must scroll list");
}

#[test]
fn dropdown_portal_escapes_scrolled_tab_bar_and_handles_click_outside_cleanly() {
    let dismiss_count = Rc::new(Cell::new(0usize));
    let outside_hook_count = Rc::new(Cell::new(0usize));
    let dc = Rc::clone(&dismiss_count);
    let oc = Rc::clone(&outside_hook_count);

    let is_open = Signal::new(true);
    let _outside_handle = use_click_outside(is_open.clone(), move || {
        oc.set(oc.get() + 1);
    });

    #[component]
    fn ScrolledTabDropdownView(dismiss_count: Rc<Cell<usize>>) -> View {
        let dropdown = create_dropdown_portal(
            "scrolled-tab-btn",
            "bottom-start",
            4.0,
            move |_e: Event| dismiss_count.set(dismiss_count.get() + 1),
            rsx! {
                <div id="portal-panel-padding" class="p-4 w-[140px] h-[60px] bg-[#1e293b]">
                    <div id="portal-panel-inner" class="w-[60px] h-[20px] bg-[#38bdf8]" />
                </div>
            },
        );
        rsx! {
            <div class="w-[500px] h-[300px] bg-[#0f172a]">
                <div id="scrolled-tabs" class="flex flex-row w-[220px] h-[40px] overflow-x-auto">
                    <div class="w-[140px] h-[40px] shrink-0" />
                    <button id="scrolled-tab-btn" class="w-[100px] h-[32px] shrink-0 bg-[#334155]">
                        {"Menu"}
                    </button>
                    <div class="w-[140px] h-[40px] shrink-0" />
                </div>
                {dropdown}
            </div>
        }
    }

    let mut runtime = AppRuntime::new(ScrolledTabDropdownView(dc), 500.0, 300.0, 1.0);

    // Horizontally scroll the tab bar by 80px so `scrolled-tab-btn` moves left onto screen (x=60)
    assert!(runtime.scroll_to_by_id("scrolled-tabs", 80.0, 0.0));
    let trig_box = find_node_by_element_id(&runtime.layout_root, "scrolled-tab-btn")
        .expect("scrolled-tab-btn")
        .geometry
        .border_box;
    let panel_box = find_node_by_element_id(&runtime.layout_root, "portal-panel-padding")
        .expect("portal-panel-padding")
        .geometry
        .border_box;

    assert!(
        (panel_box.x - trig_box.x).abs() <= 2.0
            && (panel_box.y - (trig_box.y + trig_box.height + 4.0)).abs() <= 2.0
            && panel_box.y + panel_box.height > 40.0,
        "portal panel {:?} must track horizontally scrolled trigger {:?} and escape 40px tab bar clip",
        panel_box,
        trig_box
    );

    // Clicking non-clickable padding inside the portal content must NOT trigger backdrop dismiss or click_outside
    runtime.step_event(&strata_platform::PlatformEvent::MouseDown {
        button: strata_platform::MouseButton::Left,
        x: panel_box.x + 2.0,
        y: panel_box.y + 2.0,
    });
    runtime.step_event(&strata_platform::PlatformEvent::MouseUp {
        button: strata_platform::MouseButton::Left,
        x: panel_box.x + 2.0,
        y: panel_box.y + 2.0,
    });
    assert_eq!(dismiss_count.get(), 0);
    assert_eq!(outside_hook_count.get(), 0);

    // Clicking empty viewport outside the portal fires backdrop dismiss and use_click_outside cleanly
    runtime.step_event(&strata_platform::PlatformEvent::MouseDown {
        button: strata_platform::MouseButton::Left,
        x: 420.0,
        y: 220.0,
    });
    runtime.step_event(&strata_platform::PlatformEvent::MouseUp {
        button: strata_platform::MouseButton::Left,
        x: 420.0,
        y: 220.0,
    });
    assert_eq!(dismiss_count.get(), 1);
    assert_eq!(outside_hook_count.get(), 1);
}

#[test]
fn global_keydown_hook_dispatches_on_runtime_keydown_and_respects_focused_input() {
    use strata_platform::{KeyCode, Modifiers, PlatformEvent};

    let escape_count = Rc::new(Cell::new(0usize));
    let shortcut_count = Rc::new(Cell::new(0usize));
    let ec = Rc::clone(&escape_count);
    let sc = Rc::clone(&shortcut_count);

    let _esc_handle = use_keyboard_shortcut("Escape", move || {
        ec.set(ec.get() + 1);
    });
    let _cmd_k_handle = use_keyboard_shortcut("Ctrl+K", move || {
        sc.set(sc.get() + 1);
    });

    #[component]
    fn GlobalKeyView() -> View {
        rsx! {
            <div class="w-[400px] h-[160px] p-4 bg-[#111827]">
                <input id="shortcut-input" type="text" value="" class="w-[200px] h-[32px]" />
            </div>
        }
    }

    let mut runtime = AppRuntime::new(GlobalKeyView(), 400.0, 160.0, 1.0);
    assert!(runtime.active_focus.is_none());

    // 1. With no element focused, global Escape & Ctrl+K shortcuts dispatch cleanly
    assert!(runtime.step_event(&PlatformEvent::KeyDown {
        key: KeyCode::Escape,
        modifiers: Modifiers::default(),
    }));
    assert_eq!(escape_count.get(), 1);

    assert!(runtime.step_event(&PlatformEvent::KeyDown {
        key: KeyCode::KeyK,
        modifiers: Modifiers {
            ctrl: true,
            ..Default::default()
        },
    }));
    assert_eq!(shortcut_count.get(), 1);

    // 2. Focus the input: printable typing still edits the input, and Ctrl+K global shortcut still fires
    let input_box = find_node_by_element_id(&runtime.layout_root, "shortcut-input")
        .expect("shortcut-input")
        .geometry
        .content_box;
    runtime.dispatch_pointer_down(input_box.x + 8.0, input_box.y + 8.0);
    runtime.dispatch_pointer_up(input_box.x + 8.0, input_box.y + 8.0);
    assert!(runtime.active_focus.is_some());

    runtime.step_event(&PlatformEvent::ReceivedCharacter('x'));
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "x"
    );

    runtime.step_event(&PlatformEvent::KeyDown {
        key: KeyCode::KeyK,
        modifiers: Modifiers {
            ctrl: true,
            ..Default::default()
        },
    });
    assert_eq!(shortcut_count.get(), 2);
}

