//! Regression tests covering standard MouseUp click semantics, CSS transition stability
//! across hover/active/click and keyed/unkeyed list reorders, display_list_generation
//! synchronization for split-pane 3D viewports, and `text-align: center` inside full-width
//! flex-column children.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use strata::prelude::*;
use strata::runtime::AppRuntime;
use strata_dom::{Attribute, EventType, NodeBlueprint, Tag, View};
use strata_platform::{MouseButton, PlatformEvent};
use strata_render::compositor::DrawCommand;
use strata_style::types::Color;

#[test]
fn test_onclick_fires_on_mouse_up_not_mouse_down() {
    let down_count = Rc::new(Cell::new(0usize));
    let click_count = Rc::new(Cell::new(0usize));
    let dc = Rc::clone(&down_count);
    let cc = Rc::clone(&click_count);

    let mut rt = AppRuntime::new(
        rsx! {
            <div class="w-[400px] h-[200px]">
                <button
                    id="action-btn"
                    class="w-[120px] h-[40px]"
                    onmousedown={move |_| dc.set(dc.get() + 1)}
                    onclick={move |_| cc.set(cc.get() + 1)}
                >
                    "Run"
                </button>
            </div>
        },
        400.0,
        200.0,
        1.0,
    );

    // MouseDown must invoke onmousedown but NOT onclick.
    rt.step_event(&PlatformEvent::MouseDown {
        button: MouseButton::Left,
        x: 50.0,
        y: 20.0,
    });
    assert_eq!(down_count.get(), 1);
    assert_eq!(
        click_count.get(),
        0,
        "onclick must NOT fire on MouseDown before pointer release"
    );

    // Matching MouseUp on the same button within drag tolerance fires onclick exactly once.
    rt.step_event(&PlatformEvent::MouseUp {
        button: MouseButton::Left,
        x: 52.0,
        y: 21.0,
    });
    assert_eq!(
        click_count.get(),
        1,
        "onclick must fire on MouseUp when started and ended on the same target"
    );
}

#[test]
fn test_onclick_cancelled_on_drag_or_sibling_release_and_allows_child_wobble() {
    let a_clicks = Rc::new(Cell::new(0usize));
    let b_clicks = Rc::new(Cell::new(0usize));
    let ac = Rc::clone(&a_clicks);
    let bc = Rc::clone(&b_clicks);

    let mut rt = AppRuntime::new(
        rsx! {
            <div class="flex flex-row w-[400px] h-[200px]">
                <button
                    id="btn-a"
                    class="flex flex-row w-[100px] h-[40px]"
                    onclick={move |_| ac.set(ac.get() + 1)}
                >
                    <span class="w-[50px] h-[40px]">"Icon"</span>
                    <span class="w-[50px] h-[40px]">"Text"</span>
                </button>
                <button
                    id="btn-b"
                    class="w-[100px] h-[40px]"
                    onclick={move |_| bc.set(bc.get() + 1)}
                >
                    "Other"
                </button>
            </div>
        },
        400.0,
        200.0,
        1.0,
    );

    // 1. Press on Button A, drag 20px away, then return and release on Button A -> cancelled.
    rt.step_event(&PlatformEvent::MouseDown {
        button: MouseButton::Left,
        x: 20.0,
        y: 20.0,
    });
    rt.step_event(&PlatformEvent::MouseMove { x: 45.0, y: 20.0 });
    rt.step_event(&PlatformEvent::MouseUp {
        button: MouseButton::Left,
        x: 20.0,
        y: 20.0,
    });
    assert_eq!(
        a_clicks.get(),
        0,
        "Dragging beyond click tolerance must suppress onclick even if pointer returns"
    );

    // 2. Press on Button A edge (x=98), slide 4px onto adjacent Button B (x=102) and release -> neither clicks.
    rt.step_event(&PlatformEvent::MouseDown {
        button: MouseButton::Left,
        x: 98.0,
        y: 20.0,
    });
    rt.step_event(&PlatformEvent::MouseUp {
        button: MouseButton::Left,
        x: 102.0,
        y: 20.0,
    });
    assert_eq!(a_clicks.get(), 0, "Sliding off Button A must not click Button A");
    assert_eq!(b_clicks.get(), 0, "Releasing on Button B without pressing on it must not click Button B");

    // 3. Press on child span 1 (x=49) inside Button A, wobble 2px onto child span 2 (x=51) -> Button A clicks!
    rt.step_event(&PlatformEvent::MouseDown {
        button: MouseButton::Left,
        x: 49.0,
        y: 20.0,
    });
    rt.step_event(&PlatformEvent::MouseUp {
        button: MouseButton::Left,
        x: 51.0,
        y: 20.0,
    });
    assert_eq!(
        a_clicks.get(),
        1,
        "Small wobble between children inside the same clickable button must still trigger onclick"
    );
}

#[test]
fn test_header_close_button_unmounts_on_mouse_up_without_stranding_capture() {
    let open = Rc::new(Cell::new(true));
    let open_check = Rc::clone(&open);
    let build = move || {
        let is_open = open.get();
        let oc = Rc::clone(&open);
        if is_open {
            rsx! {
                <div class="w-[400px] h-[200px]">
                    <div data-app-region="drag" class="flex flex-row w-[300px] h-[40px]">
                        <button
                            id="close-btn"
                            data-app-region="no-drag"
                            class="w-[40px] h-[40px]"
                            onclick={move |_| oc.set(false)}
                        >
                            "X"
                        </button>
                    </div>
                </div>
            }
        } else {
            rsx! {
                <div class="w-[400px] h-[200px]" />
            }
        }
    };

    let mut rt = AppRuntime::new(build(), 400.0, 200.0, 1.0);
    // MouseDown on close button must NOT unmount the floating header yet.
    rt.step_event(&PlatformEvent::MouseDown {
        button: MouseButton::Left,
        x: 20.0,
        y: 20.0,
    });
    assert!(open_check.get(), "Header must remain mounted until MouseUp");
    assert!(rt.is_mouse_down);

    // MouseUp completes the click and clears pointer down state before unmounting.
    rt.step_event(&PlatformEvent::MouseUp {
        button: MouseButton::Left,
        x: 20.0,
        y: 20.0,
    });
    assert!(!open_check.get(), "Close button onclick must fire on MouseUp");
    assert!(!rt.is_mouse_down, "is_mouse_down must be cleared on MouseUp");
    assert!(
        rt.pointer_capture_target.is_none(),
        "pointer capture must not be stranded after header unmount"
    );
}

#[test]
fn test_hovered_button_variant_change_on_click_does_not_flash_to_unhovered_color() {
    let build = |selected: bool| {
        if selected {
            rsx! {
                <div class="w-[300px] h-[100px]">
                    <button
                        id="variant-btn"
                        class="w-[120px] h-[40px] bg-[#2563eb] hover:bg-[#1d4ed8] transition-colors duration-200"
                    >
                        "Toggle"
                    </button>
                </div>
            }
        } else {
            rsx! {
                <div class="w-[300px] h-[100px]">
                    <button
                        id="variant-btn"
                        class="w-[120px] h-[40px] bg-[#1e293b] hover:bg-[#334155] transition-colors duration-200"
                    >
                        "Toggle"
                    </button>
                </div>
            }
        }
    };

    let mut rt = AppRuntime::new(build(false), 300.0, 100.0, 1.0);
    rt.freeze_clock_at_epoch();

    // Hover button via fast hover path and let the 200ms hover transition settle completely.
    assert!(rt.update_hover(50.0, 20.0));
    rt.advance_clock(Duration::from_millis(250));
    let _ = rt.poll_animations();
    assert!(
        !rt.has_active_animations(),
        "Hover transition should have settled after 250ms"
    );
    let settled_hover_bg = rt.layout_root.children[0].style.background_color.unwrap();
    assert_eq!(settled_hover_bg, Color::rgb(0x33, 0x41, 0x55));

    // Click changes variant class while still hovered: frame 0 of the new transition must
    // start from the settled hovered color (#334155), NOT flash back to the unhovered color (#1e293b)!
    rt.update_view(build(true));
    let frame0_bg = rt.layout_root.children[0].style.background_color.unwrap();
    assert_eq!(
        frame0_bg,
        Color::rgb(0x33, 0x41, 0x55),
        "Variant change after fast hover must start transitioning from settled hover color, got {:?}",
        frame0_bg
    );
}

#[test]
fn test_mid_flight_transition_reversal_samples_live_interpolated_color() {
    let build = |active: bool| {
        if active {
            rsx! {
                <div class="w-[300px] h-[100px]">
                    <div
                        id="fade-box"
                        class="w-[100px] h-[40px] bg-[#ff0000] transition-colors duration-300 ease-linear"
                    />
                </div>
            }
        } else {
            rsx! {
                <div class="w-[300px] h-[100px]">
                    <div
                        id="fade-box"
                        class="w-[100px] h-[40px] bg-[#0000ff] transition-colors duration-300 ease-linear"
                    />
                </div>
            }
        }
    };

    let mut rt = AppRuntime::new(build(false), 300.0, 100.0, 1.0);
    rt.freeze_clock_at_epoch();

    // Start blue -> red transition at t=0ms.
    rt.update_view(build(true));
    // Jump directly to t=150ms (midpoint) WITHOUT polling intermediate frames, then reverse to blue.
    rt.advance_clock(Duration::from_millis(150));
    rt.update_view(build(false));

    let reversed_start = rt.layout_root.children[0].style.background_color.unwrap();
    assert!(
        reversed_start.r >= 90 && reversed_start.r <= 170 && reversed_start.b >= 90 && reversed_start.b <= 170,
        "Mid-flight reversal at 150ms/300ms must sample live midpoint (~127,0,127) rather than snapping to start/end, got {:?}",
        reversed_start
    );
}

#[test]
fn test_keyed_list_reorder_and_unkeyed_list_insert_avoid_cross_element_transition_flashes() {
    // 1. Keyed list reversal: items with distinct colors and `transition-colors` must keep
    //    their own identity across index swaps and NOT cross-animate each other's colors.
    let build_keyed = |reversed: bool| {
        let item_a = NodeBlueprint::element(
            Tag::Div,
            vec!["w-[100px] h-[30px] bg-[#ff0000] transition-colors duration-200".to_string()],
            vec![Attribute::property("key", "item-a")],
            Vec::new(),
        );
        let item_b = NodeBlueprint::element(
            Tag::Div,
            vec!["w-[100px] h-[30px] bg-[#00ff00] transition-colors duration-200".to_string()],
            vec![Attribute::property("key", "item-b")],
            Vec::new(),
        );
        let list = if reversed {
            vec![item_b, item_a]
        } else {
            vec![item_a, item_b]
        };
        View::new(NodeBlueprint::element(
            Tag::Div,
            vec!["flex flex-col w-[200px] h-[200px]".to_string()],
            Vec::new(),
            vec![NodeBlueprint::fragment(list)],
        ))
    };

    let mut rt_keyed = AppRuntime::new(build_keyed(false), 200.0, 200.0, 1.0);
    rt_keyed.freeze_clock_at_epoch();
    rt_keyed.update_view(build_keyed(true));
    assert!(
        !rt_keyed.has_active_animations(),
        "Reordering keyed list items must preserve each item's own identity and not cross-animate colors"
    );
    assert_eq!(
        rt_keyed.layout_root.children[0].style.background_color,
        Some(Color::rgb(0x00, 0xff, 0x00))
    );
    assert_eq!(
        rt_keyed.layout_root.children[1].style.background_color,
        Some(Color::rgb(0xff, 0x00, 0x00))
    );

    // 2. Unkeyed list prepend: inserting a new element at index 0 changes child count, so
    //    shifted unkeyed neighbors must NOT cross-animate from the previous occupant's color.
    let build_unkeyed = |prepended: bool| {
        let mut items = Vec::new();
        if prepended {
            items.push(NodeBlueprint::element(
                Tag::Div,
                vec!["w-[100px] h-[30px] bg-[#0000ff] transition-colors duration-200".to_string()],
                Vec::new(),
                Vec::new(),
            ));
        }
        items.push(NodeBlueprint::element(
            Tag::Div,
            vec!["w-[100px] h-[30px] bg-[#ff0000] transition-colors duration-200".to_string()],
            Vec::new(),
            Vec::new(),
        ));
        items.push(NodeBlueprint::element(
            Tag::Div,
            vec!["w-[100px] h-[30px] bg-[#00ff00] transition-colors duration-200".to_string()],
            Vec::new(),
            Vec::new(),
        ));
        View::new(NodeBlueprint::element(
            Tag::Div,
            vec!["flex flex-col w-[200px] h-[200px]".to_string()],
            Vec::new(),
            items,
        ))
    };

    let mut rt_unkeyed = AppRuntime::new(build_unkeyed(false), 200.0, 200.0, 1.0);
    rt_unkeyed.freeze_clock_at_epoch();
    rt_unkeyed.update_view(build_unkeyed(true));
    assert!(
        !rt_unkeyed.has_active_animations(),
        "Prepending to an unkeyed list must suppress spurious cross-element transitions on shifted indices"
    );
}

#[test]
fn test_active_style_clears_cleanly_when_mouse_up_triggers_reconciled_update() {
    let clicks = Rc::new(Cell::new(0usize));
    let build = |c: &Rc<Cell<usize>>| {
        let cc = Rc::clone(c);
        let label = format!("Count: {}", c.get());
        View::new(NodeBlueprint::element(
            Tag::Div,
            vec!["w-[300px] h-[100px]".to_string()],
            Vec::new(),
            vec![
                NodeBlueprint::element(
                    Tag::Button,
                    vec!["w-[120px] h-[40px] bg-[#1e293b] hover:bg-[#334155] active:bg-[#0f172a]".to_string()],
                    vec![
                        Attribute::property("id", "press-btn"),
                        Attribute::event_listener(EventType::Click, move |_| {
                            cc.set(cc.get() + 1);
                        }),
                    ],
                    vec![NodeBlueprint::text("Press")],
                ),
                NodeBlueprint::element(
                    Tag::Span,
                    vec!["w-[100px] h-[20px]".to_string()],
                    Vec::new(),
                    vec![NodeBlueprint::text(label)],
                ),
            ],
        ))
    };

    let mut rt = AppRuntime::new(build(&clicks), 300.0, 100.0, 1.0);
    rt.update_hover(50.0, 20.0);

    // MouseDown applies `:active` background (#0f172a).
    rt.step_event(&PlatformEvent::MouseDown {
        button: MouseButton::Left,
        x: 50.0,
        y: 20.0,
    });
    assert_eq!(
        rt.layout_root.children[0].style.background_color,
        Some(Color::rgb(0x0f, 0x17, 0x2a)),
        "MouseDown must apply active:bg-[#0f172a]"
    );

    // MouseUp triggers onclick -> incremental update_view while button classes stayed identical.
    rt.step_event(&PlatformEvent::MouseUp {
        button: MouseButton::Left,
        x: 50.0,
        y: 20.0,
    });
    assert_eq!(clicks.get(), 1);
    rt.update_view(build(&clicks));
    assert_eq!(
        rt.layout_root.children[0].style.background_color,
        Some(Color::rgb(0x33, 0x41, 0x55)),
        "Button must return to hover:bg-[#334155] after MouseUp update_view instead of sticking in :active"
    );
}

#[test]
fn test_split_pane_resize_advances_display_list_generation_and_3d_viewport_bounds() {
    let view = rsx! {
        <div class="flex flex-col w-[800px] h-[600px]">
            <div id="top-pane" class="w-full bg-[#111827]" style="flex: 1 1 0px;" />
            <div id="bottom-pane" class="w-full" style="flex: 3 1 0px;">
                <Canvas3D class="w-full h-full">
                    <PerspectiveCamera
                        fov={60.0}
                        near={0.1}
                        far={1000.0}
                        position={[0.0, 5.0, 10.0]}
                        target={[0.0, 0.0, 0.0]}
                    />
                    <AmbientLight color="#ffffff" intensity={0.5} />
                </Canvas3D>
            </div>
        </div>
    };

    let mut rt = AppRuntime::new(view, 800.0, 600.0, 1.0);
    let gen0 = rt.display_list_generation();
    let vp0 = rt
        .display_list
        .commands
        .iter()
        .find_map(|c| match c {
            DrawCommand::DrawViewport3D { bounds, .. } => Some(*bounds),
            _ => None,
        })
        .expect("DrawViewport3D command must be present");
    assert!(
        (vp0.y - 150.0).abs() <= 1.0,
        "Initial 1:3 split places 3D pane at y=150, got {:?}",
        vp0
    );

    // Expand top pane (3:1 split) via in-place split ratio update.
    assert!(rt.update_split_ratios(&[("top-pane", 3.0), ("bottom-pane", 1.0)]));
    let gen1 = rt.display_list_generation();
    assert!(
        gen1 > gen0,
        "update_split_ratios / relayout_in_place must advance display_list_generation so overlay holes synchronize"
    );
    let vp1 = rt
        .display_list
        .commands
        .iter()
        .find_map(|c| match c {
            DrawCommand::DrawViewport3D { bounds, .. } => Some(*bounds),
            _ => None,
        })
        .expect("DrawViewport3D command must remain present");
    assert!(
        (vp1.y - 450.0).abs() <= 1.0 && (vp1.height - 150.0).abs() <= 1.0,
        "Updated 3:1 split must immediately update DrawViewport3D bounds to y=450, h=150, got {:?}",
        vp1
    );
}

#[test]
fn test_text_align_center_inside_full_width_flex_column_children() {
    let rt = AppRuntime::new(
        rsx! {
            <div class="flex flex-col items-center w-[400px] h-[300px]">
                <div id="empty-state" class="w-full text-center">"Empty State"</div>
                <div id="block-empty" class="block w-full text-center">"Block Empty"</div>
                <h2 id="heading" class="w-full text-center">"Centered Heading"</h2>
                <p id="subtitle" class="w-full text-center">"Subtitle text"</p>
                <div id="truncated" class="w-full text-center truncate">"Short Centered"</div>
                <div id="nowrap" class="w-full text-center whitespace-nowrap">"Nowrap Centered"</div>
            </div>
        },
        400.0,
        300.0,
        1.0,
    );

    let texts = [
        ("Empty State", "empty-state div"),
        ("Block Empty", "block-empty div"),
        ("Centered Heading", "heading h2"),
        ("Subtitle text", "subtitle p"),
        ("Short Centered", "truncated div"),
        ("Nowrap Centered", "nowrap div"),
    ];

    for (expected_text, label) in texts {
        let mut found_paint_x: Option<(f32, f32)> = None;
        for cmd in &rt.display_list.commands {
            match cmd {
                DrawCommand::DrawText {
                    origin: (x, _),
                    bounds_width,
                    bounds_height,
                    text_align,
                    text,
                    font_size,
                    line_height,
                    ..
                } if text == expected_text => {
                    let scale = (*font_size / 16.0).max(0.65);
                    let char_w = 8.0 * scale;
                    let (lines, _) = strata_render::compositor::layout_draw_text_lines(
                        text,
                        *bounds_width,
                        *bounds_height,
                        *font_size,
                        *line_height,
                        *text_align,
                        |_| char_w,
                    );
                    let (line_ox, line_str) = lines[0];
                    let painted_x = *x + line_ox;
                    let painted_w = line_str.chars().count() as f32 * char_w;
                    found_paint_x = Some((painted_x, painted_x + painted_w * 0.5));
                }
                DrawCommand::DrawTextFragments { fragments } => {
                    for frag in fragments {
                        if frag.text == expected_text {
                            found_paint_x =
                                Some((frag.bounds.x, frag.bounds.x + frag.bounds.width * 0.5));
                        }
                    }
                }
                _ => {}
            }
        }
        let (painted_x, cx) =
            found_paint_x.unwrap_or_else(|| panic!("Missing painted text for {label}"));
        assert!(
            painted_x > 80.0,
            "{label} painted left-aligned at x={painted_x} instead of centered inside 400px flex-column child"
        );
        assert!(
            (cx - 200.0).abs() <= 24.0,
            "{label} text horizontal midpoint {cx} should be centered near 200.0px (painted_x={painted_x})"
        );
    }
}

#[test]
fn test_tailwind_max_h_vh_inside_absolute_overflow_y_auto_flex_column() {
    let build_view = |item_count: usize| -> View {
        let mut items = Vec::with_capacity(item_count);
        for i in 0..item_count {
            items.push(NodeBlueprint::element(
                Tag::Div,
                vec!["w-full".into(), "h-[50px]".into(), "shrink-0".into()],
                vec![],
                vec![NodeBlueprint::text(format!("Row {i}"))],
            ));
        }
        View::new(NodeBlueprint::element(
            Tag::Div,
            vec!["w-full".into(), "h-full".into()],
            vec![],
            vec![NodeBlueprint::element(
                Tag::Div,
                vec!["relative".into(), "h-[36px]".into(), "w-[260px]".into()],
                vec![],
                vec![NodeBlueprint::element(
                    Tag::Div,
                    vec![
                        "absolute".into(),
                        "top-full".into(),
                        "right-0".into(),
                        "w-[260px]".into(),
                        "flex".into(),
                        "flex-col".into(),
                        "overflow-y-auto".into(),
                        "max-h-[76vh]".into(),
                    ],
                    vec![Attribute::Property {
                        key: "id".into(),
                        value: "dropdown".into(),
                    }],
                    items,
                )],
            )],
        ))
    };

    // 1. Short content (4 * 50 = 200px < 76vh = 684px at 900px height): preserves 200px height.
    let rt_short = AppRuntime::new(build_view(4), 1280.0, 900.0, 1.0);
    let rect_short = rt_short
        .query_element_rect("dropdown")
        .expect("dropdown element rect must exist");
    assert!(
        (rect_short.height - 200.0).abs() <= 1.0,
        "short max-h-[76vh] absolute flex-col should resolve to intrinsic 200px, got {}",
        rect_short.height
    );
    assert!(
        (rect_short.y - 36.0).abs() <= 1.0,
        "top-full should place dropdown directly below 36px relative container, got y={}",
        rect_short.y
    );

    // 2. Tall content (20 * 50 = 1000px > 76vh = 684px): clamps to 684px and registers scrollbar.
    let mut rt_tall = AppRuntime::new(build_view(20), 1280.0, 900.0, 1.0);
    let rect_tall = rt_tall
        .query_element_rect("dropdown")
        .expect("tall dropdown rect must exist");
    assert!(
        (rect_tall.height - 684.0).abs() <= 1.0,
        "tall max-h-[76vh] absolute flex-col must clamp to 684px (76% of 900px), got {}",
        rect_tall.height
    );

    // 3. Window resize to 500px tall updates 76vh clamp dynamically to 380px.
    rt_tall.step_event(&PlatformEvent::WindowResized {
        width: 1280,
        height: 500,
    });
    let rect_resized = rt_tall
        .query_element_rect("dropdown")
        .expect("resized dropdown rect must exist");
    assert!(
        (rect_resized.height - 380.0).abs() <= 1.0,
        "after viewport resize to 500px, max-h-[76vh] must update to 380px, got {}",
        rect_resized.height
    );
}

#[test]
fn test_hit_test_input_does_not_focus_occluded_input_through_portal_or_modal() {
    use std::cell::Cell;
    use std::rc::Rc;

    let menu_clicks = Rc::new(Cell::new(0usize));
    let menu_clicks_cb = menu_clicks.clone();

    // Case A: Top-layer <Portal> dropdown item positioned directly over a background <input>.
    let portal_view = View::new(NodeBlueprint::element(
        Tag::Div,
        vec!["w-full".into(), "h-full".into(), "relative".into()],
        vec![],
        vec![
            NodeBlueprint::element(
                Tag::Input,
                vec!["w-[300px]".into(), "h-[36px]".into()],
                vec![
                    Attribute::Property {
                        key: "id".into(),
                        value: "bg-search".into(),
                    },
                    Attribute::Property {
                        key: "value".into(),
                        value: "behind".into(),
                    },
                ],
                vec![],
            ),
            NodeBlueprint::element(
                Tag::Div,
                vec![
                    "fixed".into(),
                    "top-0".into(),
                    "left-0".into(),
                    "w-[300px]".into(),
                    "h-[120px]".into(),
                    "bg-[#18181b]".into(),
                ],
                vec![Attribute::Property {
                    key: "data-strata-portal".into(),
                    value: "true".into(),
                }],
                vec![NodeBlueprint::element(
                    Tag::Div,
                    vec!["w-full".into(), "h-[40px]".into(), "bg-[#27272a]".into()],
                    vec![Attribute::EventListener {
                        event: EventType::Click,
                        handler: Rc::new(move |_| {
                            menu_clicks_cb.set(menu_clicks_cb.get() + 1);
                        }),
                    }],
                    vec![NodeBlueprint::text("Dropdown Action")],
                )],
            ),
        ],
    ));

    let mut rt = AppRuntime::new(portal_view, 800.0, 600.0, 1.0);
    assert!(
        AppRuntime::hit_test_input(&rt.root_blueprint, &rt.layout_root, 80.0, 18.0).is_none(),
        "hit_test_input must not pierce through a top-layer Portal menu to focus an occluded <input>"
    );
    rt.step_event(&PlatformEvent::MouseDown {
        x: 80.0,
        y: 18.0,
        button: MouseButton::Left,
    });
    rt.step_event(&PlatformEvent::MouseUp {
        x: 80.0,
        y: 18.0,
        button: MouseButton::Left,
    });
    assert!(
        rt.active_focus.is_none(),
        "Clicking a Portal menu item over a background <input> must not focus the background <input>"
    );
    assert_eq!(menu_clicks.get(), 1, "Portal menu click must still fire");

    // Case B: Higher-z modal backdrop over a background <input>, plus an interactive <input> INSIDE the modal.
    let modal_view = View::new(NodeBlueprint::element(
        Tag::Div,
        vec!["w-full".into(), "h-full".into(), "relative".into()],
        vec![],
        vec![
            NodeBlueprint::element(
                Tag::Input,
                vec!["w-[300px]".into(), "h-[36px]".into()],
                vec![
                    Attribute::Property {
                        key: "id".into(),
                        value: "bg-search".into(),
                    },
                    Attribute::Property {
                        key: "value".into(),
                        value: "behind".into(),
                    },
                ],
                vec![],
            ),
            NodeBlueprint::element(
                Tag::Div,
                vec![
                    "fixed".into(),
                    "inset-0".into(),
                    "z-50".into(),
                    "bg-black/50".into(),
                ],
                vec![],
                vec![NodeBlueprint::element(
                    Tag::Input,
                    vec![
                        "fixed".into(),
                        "top-[100px]".into(),
                        "left-[40px]".into(),
                        "w-[240px]".into(),
                        "h-[36px]".into(),
                    ],
                    vec![
                        Attribute::Property {
                            key: "id".into(),
                            value: "modal-input".into(),
                        },
                        Attribute::Property {
                            key: "value".into(),
                            value: "foreground".into(),
                        },
                    ],
                    vec![],
                )],
            ),
        ],
    ));
    let mut rt_modal = AppRuntime::new(modal_view, 800.0, 600.0, 1.0);
    // Click on backdrop directly over bg-search at (80, 18): must NOT focus bg-search.
    rt_modal.step_event(&PlatformEvent::MouseDown {
        x: 80.0,
        y: 18.0,
        button: MouseButton::Left,
    });
    assert!(
        rt_modal.active_focus.is_none(),
        "Modal backdrop must block focusing occluded background <input>"
    );
    // Click on modal-input at (100, 118): MUST focus modal-input.
    rt_modal.step_event(&PlatformEvent::MouseDown {
        x: 100.0,
        y: 118.0,
        button: MouseButton::Left,
    });
    assert_eq!(
        rt_modal
            .active_focus
            .as_ref()
            .and_then(|f| f.input_id.as_deref()),
        Some("modal-input"),
        "Foreground <input> inside modal must remain focusable"
    );

    // Case C: pointer-events-none overlay over <input> must still pass through to focus the <input>.
    let passthrough_view = View::new(NodeBlueprint::element(
        Tag::Div,
        vec!["w-full".into(), "h-full".into(), "relative".into()],
        vec![],
        vec![
            NodeBlueprint::element(
                Tag::Input,
                vec!["w-[300px]".into(), "h-[36px]".into()],
                vec![
                    Attribute::Property {
                        key: "id".into(),
                        value: "bg-search".into(),
                    },
                    Attribute::Property {
                        key: "value".into(),
                        value: "visible".into(),
                    },
                ],
                vec![],
            ),
            NodeBlueprint::element(
                Tag::Div,
                vec![
                    "fixed".into(),
                    "inset-0".into(),
                    "z-50".into(),
                    "pointer-events-none".into(),
                ],
                vec![],
                vec![],
            ),
        ],
    ));
    let mut rt_pass = AppRuntime::new(passthrough_view, 800.0, 600.0, 1.0);
    rt_pass.step_event(&PlatformEvent::MouseDown {
        x: 80.0,
        y: 18.0,
        button: MouseButton::Left,
    });
    assert_eq!(
        rt_pass
            .active_focus
            .as_ref()
            .and_then(|f| f.input_id.as_deref()),
        Some("bg-search"),
        "pointer-events-none overlay must not block focusing the visible <input> underneath"
    );
}

#[test]
fn test_controlled_focused_input_syncs_programmatic_value_clear_while_focused() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let controlled_val = Rc::new(RefCell::new(String::new()));
    let submitted = Rc::new(RefCell::new(Vec::<String>::new()));

    let build_view = |val: &str,
                      controlled_val: &Rc<RefCell<String>>,
                      submitted: &Rc<RefCell<Vec<String>>>|
     -> View {
        let on_in_val = controlled_val.clone();
        let on_sub_val = controlled_val.clone();
        let on_sub_log = submitted.clone();
        View::new(NodeBlueprint::element(
            Tag::Div,
            vec!["w-full".into(), "h-full".into(), "flex".into(), "flex-col".into()],
            vec![],
            vec![
                NodeBlueprint::element(
                    Tag::Input,
                    vec!["w-[300px]".into(), "h-[36px]".into()],
                    vec![
                        Attribute::Property {
                            key: "id".into(),
                            value: "cmd-input".into(),
                        },
                        Attribute::Property {
                            key: "value".into(),
                            value: val.to_string(),
                        },
                        Attribute::Property {
                            key: "placeholder".into(),
                            value: "Enter command...".into(),
                        },
                        Attribute::EventListener {
                            event: EventType::Input,
                            handler: Rc::new(move |ev| {
                                *on_in_val.borrow_mut() = ev.value;
                            }),
                        },
                        Attribute::EventListener {
                            event: EventType::Submit,
                            handler: Rc::new(move |ev| {
                                on_sub_log.borrow_mut().push(ev.value);
                                on_sub_val.borrow_mut().clear();
                            }),
                        },
                    ],
                    vec![],
                ),
                NodeBlueprint::element(
                    Tag::Input,
                    vec!["w-[300px]".into(), "h-[36px]".into()],
                    vec![
                        Attribute::Property {
                            key: "id".into(),
                            value: "static-input".into(),
                        },
                        Attribute::Property {
                            key: "value".into(),
                            value: "static".into(),
                        },
                    ],
                    vec![],
                ),
            ],
        ))
    };

    let mut rt = AppRuntime::new(
        build_view("", &controlled_val, &submitted),
        800.0,
        400.0,
        1.0,
    );

    // Focus controlled input.
    rt.step_event(&PlatformEvent::MouseDown {
        x: 40.0,
        y: 18.0,
        button: MouseButton::Left,
    });
    assert_eq!(
        rt.active_focus.as_ref().and_then(|f| f.input_id.as_deref()),
        Some("cmd-input")
    );

    // Type "ping" with live controlled re-renders after each character.
    for ch in "ping".chars() {
        rt.step_event(&PlatformEvent::ReceivedCharacter(ch));
        let cur = controlled_val.borrow().clone();
        rt.update_view(build_view(&cur, &controlled_val, &submitted));
    }
    assert_eq!(
        rt.active_focus.as_ref().unwrap().state.text.get(),
        "ping"
    );
    assert_eq!(
        rt.active_focus.as_ref().unwrap().state.cursor_index.get(),
        4
    );

    // Submit on Enter: host handler clears `controlled_val` to "" while keeping focus.
    rt.step_event(&PlatformEvent::KeyDown {
        key: strata_platform::KeyCode::Enter,
        modifiers: strata_platform::Modifiers::default(),
    });
    assert_eq!(submitted.borrow().as_slice(), &["ping".to_string()]);
    assert_eq!(controlled_val.borrow().as_str(), "");

    let cur = controlled_val.borrow().clone();
    rt.update_view(build_view(&cur, &controlled_val, &submitted));

    let active = rt
        .active_focus
        .as_ref()
        .expect("Controlled input must stay focused after submit clear");
    assert_eq!(
        active.state.text.get(),
        "",
        "Focused controlled <input> must immediately sync programmatic value=\"\" clear"
    );
    assert_eq!(active.state.cursor_index.get(), 0);
    assert_eq!(active.state.selection_anchor.get(), None);
    assert_eq!(
        rt.layout_root.children[0].text.as_deref(),
        Some("Enter command..."),
        "Cleared focused input must render placeholder text rather than stale submitted text"
    );

    // Programmatic value replacement with selection clamping while still focused.
    *controlled_val.borrow_mut() = "abcdef".into();
    rt.update_view(build_view("abcdef", &controlled_val, &submitted));
    {
        let active = rt.active_focus.as_ref().unwrap();
        assert_eq!(active.state.text.get(), "abcdef");
        active.state.selection_anchor.set(Some(1));
        active.state.cursor_index.set(6);
    }
    *controlled_val.borrow_mut() = "ab".into();
    rt.update_view(build_view("ab", &controlled_val, &submitted));
    {
        let active = rt.active_focus.as_ref().unwrap();
        assert_eq!(active.state.text.get(), "ab");
        assert_eq!(active.state.cursor_index.get(), 2);
        assert_eq!(active.state.selection_anchor.get(), Some(1));
    }

    // Switching to an uncontrolled static value="static" input and typing must NOT be overwritten
    // by unrelated update_view calls where the static value prop did not change.
    rt.step_event(&PlatformEvent::MouseDown {
        x: 40.0,
        y: 54.0,
        button: MouseButton::Left,
    });
    assert_eq!(
        rt.active_focus.as_ref().and_then(|f| f.input_id.as_deref()),
        Some("static-input")
    );
    rt.step_event(&PlatformEvent::ReceivedCharacter('!'));
    let typed = rt.active_focus.as_ref().unwrap().state.text.get();
    assert!(typed.contains('!'));
    // Re-render with same static prop: user edits must be preserved!
    rt.update_view(build_view("", &controlled_val, &submitted));
    assert_eq!(
        rt.active_focus.as_ref().unwrap().state.text.get(),
        typed,
        "Uncontrolled static value prop must not overwrite live user input on unrelated update_view"
    );

    // Refocusing cmd-input (whose prop changed from "ab" -> "" while blurred) must sync to "".
    rt.step_event(&PlatformEvent::MouseDown {
        x: 40.0,
        y: 18.0,
        button: MouseButton::Left,
    });
    assert_eq!(
        rt.active_focus.as_ref().unwrap().state.text.get(),
        "",
        "Controlled input cleared while blurred must sync to \"\" upon refocus"
    );
}

#[test]
fn test_fixed_overlay_text_update_uses_isolated_boundary_relayout() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let build_scene = |hud_text: &str| -> View {
        let mut rows = Vec::with_capacity(64);
        for i in 0..64 {
            rows.push(NodeBlueprint::element(
                Tag::Div,
                vec!["w-full".into(), "h-[24px]".into()],
                vec![],
                vec![NodeBlueprint::text(format!("Telemetry Channel {i}"))],
            ));
        }
        View::new(NodeBlueprint::element(
            Tag::Div,
            vec!["w-full".into(), "h-full".into(), "relative".into()],
            vec![],
            vec![
                NodeBlueprint::element(
                    Tag::Div,
                    vec!["w-[320px]".into(), "flex".into(), "flex-col".into()],
                    vec![],
                    rows,
                ),
                NodeBlueprint::element(
                    Tag::Div,
                    vec![
                        "fixed".into(),
                        "bottom-4".into(),
                        "right-4".into(),
                        "px-3".into(),
                        "py-1.5".into(),
                    ],
                    vec![Attribute::Property {
                        key: "id".into(),
                        value: "hud-overlay".into(),
                    }],
                    vec![NodeBlueprint::element(
                        Tag::Span,
                        vec!["text-xs".into()],
                        vec![],
                        vec![NodeBlueprint::text(hud_text.to_string())],
                    )],
                ),
            ],
        ))
    };

    let mut rt = AppRuntime::new(build_scene("ALT 100m"), 1280.0, 720.0, 1.0);
    assert!(rt.layout_root.layout_cache.is_valid);
    assert!(rt.layout_root.children[0].layout_cache.is_valid);
    assert_eq!(rt.isolated_boundary_layouts, 0);
    let initial_hud_rect = rt
        .query_element_rect("hud-overlay")
        .expect("hud-overlay rect must exist");

    // 1. Update ONLY the position: fixed HUD overlay text via update_view.
    let next_label = "ALT 12450.5m | HDG 275.0 DEG";
    rt.update_view(build_scene(next_label));
    assert_eq!(
        rt.isolated_boundary_layouts, 1,
        "Updating text inside a position: fixed overlay must take the isolated boundary relayout path"
    );
    assert!(
        rt.layout_root.layout_cache.is_valid,
        "Root layout_cache must remain valid during isolated fixed overlay relayout"
    );
    assert!(
        rt.layout_root.children[0].layout_cache.is_valid,
        "Clean sibling subtree layout_cache must remain valid during isolated fixed overlay relayout"
    );
    let updated_hud_rect = rt
        .query_element_rect("hud-overlay")
        .expect("hud-overlay rect must still exist");
    assert!(
        updated_hud_rect.width > initial_hud_rect.width,
        "Wider HUD text must expand shrink-to-fit fixed overlay: {:?} vs {:?}",
        updated_hud_rect,
        initial_hud_rect
    );
    let has_new_text = rt.display_list.commands.iter().any(|cmd| match cmd {
        strata_render::compositor::DrawCommand::DrawText { text, .. } => text.contains(next_label),
        strata_render::compositor::DrawCommand::DrawTextFragments { fragments, .. } => {
            fragments.iter().any(|f| f.text.contains(next_label))
        }
        _ => false,
    });
    assert!(
        has_new_text,
        "Compiled display list must include the updated fixed HUD overlay text"
    );

    // 2. Also verify fine-grained sync_dynamic_text inside a position: fixed overlay uses isolated relayout.
    let dyn_text = Rc::new(RefCell::new("FPS 144".to_string()));
    let dyn_reader = dyn_text.clone();
    let dyn_view = View::new(NodeBlueprint::element(
        Tag::Div,
        vec!["w-full".into(), "h-full".into()],
        vec![],
        vec![
            NodeBlueprint::element(
                Tag::Div,
                vec!["w-[320px]".into(), "h-[400px]".into()],
                vec![],
                vec![NodeBlueprint::text("Main Content")],
            ),
            NodeBlueprint::element(
                Tag::Div,
                vec!["fixed".into(), "top-2".into(), "right-2".into()],
                vec![],
                vec![NodeBlueprint::DynamicText(Rc::new(move || {
                    dyn_reader.borrow().clone()
                }))],
            ),
        ],
    ));
    let mut rt_dyn = AppRuntime::new(dyn_view, 1280.0, 720.0, 1.0);
    assert!(rt_dyn.layout_root.layout_cache.is_valid);
    assert!(rt_dyn.layout_root.children[0].layout_cache.is_valid);
    *dyn_text.borrow_mut() = "FPS 143.9 (VSync)".to_string();
    assert!(rt_dyn.sync_dynamic_text());
    assert_eq!(
        rt_dyn.isolated_boundary_layouts, 1,
        "sync_dynamic_text inside a position: fixed overlay must take the isolated boundary relayout path"
    );
    assert!(rt_dyn.layout_root.layout_cache.is_valid);
    assert!(rt_dyn.layout_root.children[0].layout_cache.is_valid);
}
