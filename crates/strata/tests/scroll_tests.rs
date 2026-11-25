use strata::prelude::*;
use strata_platform::{KeyCode, Modifiers, PlatformEvent};

#[test]
fn test_scroll_container_offset_clamping_and_translation() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div id="scroll-root" class="flex flex-col w-[800px] h-[400px] overflow-y-auto bg-black">
                <div class="h-[300px] w-full bg-red-500" />
                <div class="h-[300px] w-full bg-blue-500" />
                <div class="h-[300px] w-full bg-green-500" />
            </div>
        },
        800.0,
        400.0,
        1.0,
    );

    // Initial state: total content is 900px, viewport is 400px -> max_scroll_y = 500px
    assert_eq!(runtime.active_scrollbars.len(), 1);
    let bar = &runtime.active_scrollbars[0];
    assert_eq!(bar.scroll_y, 0.0);
    assert!((bar.max_scroll_y - 500.0).abs() < 2.0);

    // First child initial Y should be 0.0
    let child0_y = runtime.layout_root.children[0].geometry.border_box.y;
    assert_eq!(child0_y, 0.0);

    // Scroll down 120px
    let scrolled = runtime.dispatch_scroll(400.0, 200.0, 0.0, 120.0);
    assert!(scrolled);
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 120.0);

    // First child translated up by 120px
    let child0_scrolled_y = runtime.layout_root.children[0].geometry.border_box.y;
    assert_eq!(child0_scrolled_y, -120.0);

    // Scroll way past bottom (e.g. +1000px) -> should clamp to max_scroll_y (500px)
    runtime.dispatch_scroll(400.0, 200.0, 0.0, 1000.0);
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 500.0);
    let child0_clamped_bottom_y = runtime.layout_root.children[0].geometry.border_box.y;
    assert_eq!(child0_clamped_bottom_y, -500.0);

    // Scroll way past top (e.g. -2000px) -> should clamp to 0.0
    runtime.dispatch_scroll(400.0, 200.0, 0.0, -2000.0);
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 0.0);
    let child0_clamped_top_y = runtime.layout_root.children[0].geometry.border_box.y;
    assert_eq!(child0_clamped_top_y, 0.0);
}

#[test]
fn test_keyboard_navigation_scrolling() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div id="keyboard-scroll" class="flex flex-col w-[600px] h-[300px] overflow-y-auto">
                <div class="h-[200px] w-full" />
                <div class="h-[200px] w-full" />
                <div class="h-[200px] w-full" />
                <div class="h-[200px] w-full" />
            </div>
        },
        600.0,
        300.0,
        1.0,
    );

    runtime.last_mouse_pos = (300.0, 150.0);

    // ArrowDown -> 40px
    let down_ev = PlatformEvent::KeyDown {
        key: KeyCode::ArrowDown,
        modifiers: Modifiers::default(),
    };
    assert!(runtime.handle_key_event(&down_ev));
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 40.0);

    // PageDown -> +400px (40 + 400 = 440px)
    let pagedown_ev = PlatformEvent::KeyDown {
        key: KeyCode::PageDown,
        modifiers: Modifiers::default(),
    };
    assert!(runtime.handle_key_event(&pagedown_ev));
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 440.0);

    // PageUp -> -400px (440 - 400 = 40px)
    let pageup_ev = PlatformEvent::KeyDown {
        key: KeyCode::PageUp,
        modifiers: Modifiers::default(),
    };
    assert!(runtime.handle_key_event(&pageup_ev));
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 40.0);

    // End -> bottom (500px)
    let end_ev = PlatformEvent::KeyDown {
        key: KeyCode::End,
        modifiers: Modifiers::default(),
    };
    assert!(runtime.handle_key_event(&end_ev));
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 500.0);

    // Home -> top (0px)
    let home_ev = PlatformEvent::KeyDown {
        key: KeyCode::Home,
        modifiers: Modifiers::default(),
    };
    assert!(runtime.handle_key_event(&home_ev));
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 0.0);
}

#[test]
fn test_scrollbar_thumb_press_and_drag() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div id="thumb-drag" class="flex flex-col w-[500px] h-[200px] overflow-y-auto">
                <div class="h-[1000px] w-full" />
            </div>
        },
        500.0,
        200.0,
        1.0,
    );

    assert_eq!(runtime.active_scrollbars.len(), 1);
    let thumb = runtime.active_scrollbars[0].thumb_bounds;

    // Press outside thumb -> returns false
    assert!(!runtime.check_scrollbar_thumb_press(10.0, 10.0));
    assert!(runtime.scrollbar_drag.is_none());

    // Press on thumb center
    let thumb_center_x = thumb.x + thumb.width / 2.0;
    let thumb_center_y = thumb.y + thumb.height / 2.0;
    assert!(runtime.check_scrollbar_thumb_press(thumb_center_x, thumb_center_y));
    assert!(runtime.scrollbar_drag.is_some());

    // Drag thumb downwards by 50px
    assert!(runtime.poll_scrollbar_drag(thumb_center_x, thumb_center_y + 50.0));
    assert!(runtime.active_scrollbars[0].scroll_y > 0.0);
}

#[test]
fn test_scroll_multiple_frames() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div id="scroll-root" class="flex flex-col w-[800px] h-[400px] overflow-y-auto bg-black">
                <div class="h-[300px] w-full bg-red-500" />
                <div class="h-[300px] w-full bg-blue-500" />
                <div class="h-[300px] w-full bg-green-500" />
            </div>
        },
        800.0,
        400.0,
        1.0,
    );

    // Initial state
    assert_eq!(runtime.active_scrollbars[0].content_h, 900.0);
    assert_eq!(runtime.active_scrollbars[0].max_scroll_y, 500.0);
    let initial_thumb_h = runtime.active_scrollbars[0].thumb_bounds.height;

    // Scroll down 120px
    runtime.dispatch_scroll(400.0, 200.0, 0.0, 120.0);
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 120.0);
    assert_eq!(runtime.active_scrollbars[0].content_h, 900.0);
    assert_eq!(runtime.active_scrollbars[0].max_scroll_y, 500.0);
    assert_eq!(
        runtime.layout_root.children[0].geometry.border_box.y,
        -120.0
    );
    assert!((runtime.active_scrollbars[0].thumb_bounds.height - initial_thumb_h).abs() < 0.01);

    // Run 25 consecutive frame ticks, asserting absolute metric invariance
    for frame in 1..=25 {
        runtime.tick_frame();
        assert_eq!(
            runtime.active_scrollbars[0].scroll_y, 120.0,
            "Frame {frame}: scroll_y must remain 120.0"
        );
        assert_eq!(
            runtime.active_scrollbars[0].content_h, 900.0,
            "Frame {frame}: content_h must remain 900.0"
        );
        assert_eq!(
            runtime.active_scrollbars[0].max_scroll_y, 500.0,
            "Frame {frame}: max_scroll_y must remain 500.0"
        );
        assert_eq!(
            runtime.layout_root.children[0].geometry.border_box.y, -120.0,
            "Frame {frame}: child0 border_box.y must remain -120.0"
        );
        assert!(
            (runtime.active_scrollbars[0].thumb_bounds.height - initial_thumb_h).abs() < 0.01,
            "Frame {frame}: thumb height must remain constant"
        );
    }
}

#[test]
fn test_smooth_scroll_critically_damped_convergence() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div id="smooth-scroll" class="flex flex-col w-[800px] h-[400px] overflow-y-auto">
                <div class="h-[300px] w-full" />
                <div class="h-[300px] w-full" />
                <div class="h-[300px] w-full" />
            </div>
        },
        800.0,
        400.0,
        1.0,
    );

    let start_t = std::time::Instant::now();
    let initiated = runtime.dispatch_smooth_scroll(400.0, 200.0, 0.0, 120.0);
    assert!(initiated, "Smooth scroll must initiate successfully");
    assert!(
        runtime.active_smooth_scroll.is_some(),
        "Tracker must be active"
    );

    if let Some(ref mut tr) = runtime.active_smooth_scroll {
        tr.start_time = start_t;
        tr.last_event_time = start_t;
    }

    // Step through 20ms intervals
    let mut prev_y = 0.0f32;
    for step in 1..=15 {
        let tick_t = start_t + std::time::Duration::from_millis(step * 20);
        let updated = runtime.poll_smooth_scroll_at(tick_t);
        let cur_y = runtime.active_scrollbars[0].scroll_y;
        assert!(updated || cur_y >= 119.5);
        assert!(
            cur_y >= prev_y,
            "Scroll offset must be monotonically non-decreasing: cur={cur_y}, prev={prev_y}"
        );
        prev_y = cur_y;
    }

    // Step past settle duration (450ms)
    let settle_t = start_t + std::time::Duration::from_millis(450);
    runtime.poll_smooth_scroll_at(settle_t);

    assert_eq!(
        runtime.active_scrollbars[0].scroll_y, 120.0,
        "Must settle exactly at 120.0px target"
    );
    assert!(
        runtime.active_smooth_scroll.is_none(),
        "Tracker must be cleared on settle"
    );

    // Subsequent tick should report no update needed (idle at 0% CPU)
    let idle_updated =
        runtime.poll_smooth_scroll_at(settle_t + std::time::Duration::from_millis(50));
    assert!(!idle_updated, "Idle tick must return false to sleep CPU");
}

#[test]
fn test_smooth_scroll_flick_velocity_acceleration() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div id="flick-scroll" class="flex flex-col w-[800px] h-[400px] overflow-y-auto">
                <div class="h-[2000px] w-full" />
            </div>
        },
        800.0,
        400.0,
        1.0,
    );

    let t0 = std::time::Instant::now();
    runtime.dispatch_smooth_scroll(400.0, 200.0, 0.0, 110.0);
    if let Some(ref mut tr) = runtime.active_smooth_scroll {
        tr.start_time = t0;
        tr.last_event_time = t0;
    }

    // Second rapid flick 40ms later
    let t1 = t0 + std::time::Duration::from_millis(40);
    runtime.poll_smooth_scroll_at(t1);

    // Dispatch second impulse while moving
    runtime.dispatch_smooth_scroll(400.0, 200.0, 0.0, 110.0);
    let target = runtime.active_smooth_scroll.as_ref().unwrap().target_y;

    // With rapid flick acceleration (accel > 2.0), target must exceed simple 220px sum
    assert!(
        target > 220.0,
        "Rapid flick acceleration must accumulate dynamic momentum: target was {target}"
    );

    // Poll to settle
    let t_end = t1 + std::time::Duration::from_millis(500);
    runtime.poll_smooth_scroll_at(t_end);
    assert_eq!(runtime.active_scrollbars[0].scroll_y, target);
    assert!(runtime.active_smooth_scroll.is_none());
}

#[test]
fn test_smooth_scroll_boundary_clamping() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div id="boundary-scroll" class="flex flex-col w-[800px] h-[400px] overflow-y-auto">
                <div class="h-[1000px] w-full" />
            </div>
        },
        800.0,
        400.0,
        1.0,
    );

    // Max scroll is 600px
    assert_eq!(runtime.active_scrollbars[0].max_scroll_y, 600.0);

    let t0 = std::time::Instant::now();
    runtime.dispatch_smooth_scroll(400.0, 200.0, 0.0, 5000.0);
    if let Some(ref mut tr) = runtime.active_smooth_scroll {
        tr.start_time = t0;
        tr.last_event_time = t0;
    }

    // Settle at bottom
    runtime.poll_smooth_scroll_at(t0 + std::time::Duration::from_millis(500));
    assert_eq!(
        runtime.active_scrollbars[0].scroll_y, 600.0,
        "Must clamp to max_scroll_y exactly"
    );

    // Scroll up way past top
    let t1 = t0 + std::time::Duration::from_millis(600);
    runtime.dispatch_smooth_scroll(400.0, 200.0, 0.0, -8000.0);
    if let Some(ref mut tr) = runtime.active_smooth_scroll {
        tr.start_time = t1;
        tr.last_event_time = t1;
    }

    // Settle at top
    runtime.poll_smooth_scroll_at(t1 + std::time::Duration::from_millis(500));
    assert_eq!(
        runtime.active_scrollbars[0].scroll_y, 0.0,
        "Must clamp to 0.0 exactly"
    );
}

#[test]
fn test_smooth_scroll_interrupted_by_manual_thumb_drag() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div id="interrupted-scroll" class="flex flex-col w-[800px] h-[400px] overflow-y-auto">
                <div class="h-[1200px] w-full" />
            </div>
        },
        800.0,
        400.0,
        1.0,
    );

    runtime.dispatch_smooth_scroll(400.0, 200.0, 0.0, 200.0);
    assert!(runtime.active_smooth_scroll.is_some());

    let thumb = runtime.active_scrollbars[0].thumb_bounds;
    let thumb_center_x = thumb.x + thumb.width / 2.0;
    let thumb_center_y = thumb.y + thumb.height / 2.0;

    // Pressing thumb cancels active kinetic animation
    assert!(runtime.check_scrollbar_thumb_press(thumb_center_x, thumb_center_y));
    assert!(
        runtime.active_smooth_scroll.is_none(),
        "Manual thumb press must immediately cancel kinetic scroll"
    );
    assert!(runtime.scrollbar_drag.is_some());
}

#[test]
fn test_scrollbar_right_edge_pixel_perfect_hit_testing() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div id="edge-scroll" class="flex flex-col w-[800px] h-[400px] overflow-y-auto">
                <div class="h-[2000px] w-full" />
            </div>
        },
        800.0,
        400.0,
        1.0,
    );

    assert_eq!(runtime.active_scrollbars.len(), 1);
    let bar = runtime.active_scrollbars[0].clone();

    // Verify track bounds and thumb bounds are flush with the container's right edge
    let expected_right = 800.0;
    assert_eq!(
        bar.track_bounds.x + bar.track_bounds.width,
        expected_right,
        "Track right edge must be exactly at 800.0px"
    );
    assert_eq!(
        bar.thumb_bounds.x + bar.thumb_bounds.width,
        expected_right,
        "Thumb right edge must be exactly at 800.0px"
    );
    assert_eq!(
        bar.track_bounds.y, 0.0,
        "Track top edge must be exactly at 0.0px"
    );
    assert_eq!(
        bar.track_bounds.height, 400.0,
        "Track height must match container height 400.0px"
    );

    let thumb_center_y = bar.thumb_bounds.y + bar.thumb_bounds.height / 2.0;

    // Slam mouse to the very edge right (x = 800.0): must pick the scrollbar thumb without moving left!
    assert!(
        runtime.check_scrollbar_thumb_press(800.0, thumb_center_y),
        "Must pick scrollbar at x = 800.0px (very edge right)"
    );
    assert!(runtime.scrollbar_drag.is_some());
    runtime.scrollbar_drag = None;

    // Subpixel right boundary (x = 799.5 and x = 799.0): must pick scrollbar
    assert!(runtime.check_scrollbar_thumb_press(799.5, thumb_center_y));
    assert!(runtime.scrollbar_drag.is_some());
    runtime.scrollbar_drag = None;

    assert!(runtime.check_scrollbar_thumb_press(799.0, thumb_center_y));
    assert!(runtime.scrollbar_drag.is_some());
    runtime.scrollbar_drag = None;

    // Clicking track gutter at the very right edge (x = 800.0, py = 350.0): must pick scrollbar and jump/drag
    assert!(
        runtime.check_scrollbar_thumb_press(800.0, 350.0),
        "Must pick scrollbar track at x = 800.0px"
    );
    assert!(runtime.scrollbar_drag.is_some());
    assert!(
        runtime.active_scrollbars[0].scroll_y > 0.0,
        "Clicking track must scroll towards click point"
    );
    runtime.scrollbar_drag = None;

    // Coordinates to the left of the scrollbar track must not pick scrollbar
    assert!(
        !runtime.check_scrollbar_thumb_press(780.0, thumb_center_y),
        "Coordinates left of track must return false"
    );
    assert!(runtime.scrollbar_drag.is_none());
}

#[test]
fn test_smooth_scroll_consecutive_impulses_preserve_exact_layout_offset() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div id="rapid-wheel" class="flex flex-col w-[440px] h-[400px] p-5 overflow-y-auto">
                <div class="h-[300px] w-full shrink-0" />
                <div class="h-[300px] w-full shrink-0" />
            </div>
        },
        440.0,
        400.0,
        1.0,
    );

    let initial_y0 = runtime.layout_root.children[0].geometry.border_box.y;
    let max_scroll = runtime.active_scrollbars[0].max_scroll_y;
    assert!(max_scroll > 200.0);

    // First wheel impulse
    assert!(runtime.dispatch_smooth_scroll(220.0, 200.0, 0.0, 110.0));
    // Simulate 25ms elapsed before the next wheel event arrives without an intervening AnimationTick
    if let Some(ref mut tr) = runtime.active_smooth_scroll {
        tr.start_time = std::time::Instant::now() - std::time::Duration::from_millis(25);
        tr.last_event_time = tr.start_time;
    }

    // Second and third consecutive wheel impulses while in-flight
    assert!(runtime.dispatch_smooth_scroll(220.0, 200.0, 0.0, 110.0));
    if let Some(ref mut tr) = runtime.active_smooth_scroll {
        tr.start_time = std::time::Instant::now() - std::time::Duration::from_millis(25);
        tr.last_event_time = tr.start_time;
    }
    assert!(runtime.dispatch_smooth_scroll(220.0, 200.0, 0.0, 500.0));

    // Settle completely
    let settle_t = std::time::Instant::now() + std::time::Duration::from_millis(600);
    runtime.poll_smooth_scroll_at(settle_t);

    assert!(
        (runtime.active_scrollbars[0].scroll_y - max_scroll).abs() < 0.01,
        "scroll_y must reach exact max_scroll_y"
    );
    let actual_y0 = runtime.layout_root.children[0].geometry.border_box.y;
    let expected_y0 = initial_y0 - max_scroll;
    assert!(
        (actual_y0 - expected_y0).abs() < 0.05,
        "layout child must scroll all the way to {expected_y0}, got {actual_y0} (lost {}px)",
        actual_y0 - expected_y0
    );
}

#[test]
fn test_onscroll_event_and_programmatic_scroll_to_bottom_and_scroll_into_view() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let observed_scroll = Rc::new(RefCell::new((0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64)));
    let observed_clone = Rc::clone(&observed_scroll);
    let on_scroll = move |e: Event| {
        *observed_clone.borrow_mut() = (
            e.scroll_top,
            e.scroll_height,
            e.client_height,
            e.delta_y,
        );
    };

    let mut runtime = AppRuntime::new(
        rsx! {
            <div
                id="log-scroller"
                class="flex flex-col w-[400px] h-[200px] overflow-y-auto bg-neutral-900"
                onscroll=on_scroll
            >
                <div id="item-0" class="h-[150px] w-full shrink-0" />
                <div id="item-1" class="h-[150px] w-full shrink-0" />
                <div id="item-2" class="h-[150px] w-full shrink-0" />
                <div id="item-3" class="h-[150px] w-full shrink-0" />
            </div>
        },
        400.0,
        200.0,
        1.0,
    );

    assert_eq!(runtime.scroll_offset_by_id("log-scroller"), (0.0, 0.0));
    assert_eq!(get_scroll_offset("log-scroller"), (0.0, 0.0));

    // 1. Wheel scroll fires onscroll with live scroll_top, scroll_height (600), client_height (200), delta_y (80)
    assert!(runtime.dispatch_scroll(100.0, 100.0, 0.0, 80.0));
    let (top, sh, ch, dy) = *observed_scroll.borrow();
    assert!((top - 80.0).abs() < 0.01);
    assert!((sh - 600.0).abs() < 0.01);
    assert!((ch - 200.0).abs() < 0.01);
    assert!((dy - 80.0).abs() < 0.01);
    assert_eq!(get_scroll_offset("log-scroller"), (0.0, 80.0));

    // 2. Programmatic scroll_to_bottom via global hook + flush
    scroll_to_bottom("log-scroller");
    assert!(runtime.flush_pending_scroll_commands());
    assert_eq!(runtime.scroll_offset_by_id("log-scroller"), (0.0, 400.0));
    assert_eq!(get_scroll_offset("log-scroller"), (0.0, 400.0));
    assert!((observed_scroll.borrow().0 - 400.0).abs() < 0.01);

    // 3. Programmatic scroll_to_top
    scroll_to_top("log-scroller");
    assert!(runtime.flush_pending_scroll_commands());
    assert_eq!(runtime.scroll_offset_by_id("log-scroller"), (0.0, 0.0));

    // 4. Programmatic scroll_into_view("item-2") (item-2 spans y = 300..450 in a 200px viewport -> scrolls to 250px)
    scroll_into_view("item-2");
    assert!(runtime.flush_pending_scroll_commands());
    assert!(
        (runtime.scroll_offset_by_id("log-scroller").1 - 250.0).abs() < 0.05,
        "scroll_into_view(item-2) should scroll to 250.0, got {}",
        runtime.scroll_offset_by_id("log-scroller").1
    );
}

#[test]
fn test_svg_text_and_stroke_dasharray_in_rasterizer_and_gpu_pipeline() {
    use strata_render::compositor::DrawCommand;

    let runtime = AppRuntime::new(
        rsx! {
            <div class="w-[200px] h-[100px] bg-black">
                <svg viewBox="0 0 200 100" class="w-[200px] h-[100px]">
                    <path
                        d="M 20 30 L 180 30"
                        stroke="#ff0000"
                        stroke-width="6"
                        stroke-dasharray="20,20"
                        stroke-linecap="butt"
                        fill="none"
                    />
                    <text x="100" y="75" font-size="16" text-anchor="middle" fill="#00ff00">
                        "ALT 120m"
                    </text>
                </svg>
            </div>
        },
        200.0,
        100.0,
        1.0,
    );

    let has_dashed_path = runtime.display_list.commands.iter().any(|cmd| {
        matches!(
            cmd,
            DrawCommand::DrawSvgPath { stroke_dasharray, .. }
                if stroke_dasharray.as_slice() == [20.0, 20.0]
        )
    });
    assert!(has_dashed_path, "<svg> must propagate stroke-dasharray");

    let has_svg_text = runtime.display_list.commands.iter().any(|cmd| {
        matches!(
            cmd,
            DrawCommand::DrawText { text, .. } if text == "ALT 120m"
        )
    });
    assert!(
        has_svg_text,
        "<svg> must parse <text> and emit DrawCommand::DrawText"
    );

    // Software rasterizer must leave gaps in the dashed line (x=30 is in first 20px dash [20..40], x=50 is in first 20px gap [40..60])
    let mut fb = strata_render::FrameBuffer::new(200, 100);
    fb.execute_display_list(&runtime.display_list);
    let dash_on = fb.get_pixel(30, 30);
    let dash_gap = fb.get_pixel(50, 30);
    assert_eq!(
        dash_on.r, 255,
        "Pixel inside first dash [20..40] must be painted red"
    );
    assert_eq!(
        dash_gap.r, 0,
        "Pixel inside first dash gap [40..60] must remain background black"
    );
}

#[test]
fn test_platform_event_file_drop_and_uri_list_parsing() {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    let parsed = strata_platform::parse_dropped_uri_list_to_paths(
        "# comment\r\nfile:///tmp/mission%20plan.json\r\nfile://localhost/var/data/track.gpx\r\n",
    );
    assert_eq!(
        parsed,
        vec![
            PathBuf::from("/tmp/mission plan.json"),
            PathBuf::from("/var/data/track.gpx"),
        ]
    );

    let dropped_files = Rc::new(RefCell::new(Vec::<PathBuf>::new()));
    let dropped_clone = Rc::clone(&dropped_files);
    let on_drop = move |e: Event| {
        *dropped_clone.borrow_mut() = e.files.clone();
    };

    let mut runtime = AppRuntime::new(
        rsx! {
            <div
                class="w-[300px] h-[200px] bg-neutral-900"
                ondrop=on_drop
            >
                "Drop Zone"
            </div>
        },
        300.0,
        200.0,
        1.0,
    );

    runtime.step_event(&PlatformEvent::MouseMove { x: 100.0, y: 100.0 });
    let redraw = runtime.step_event(&PlatformEvent::FileDrop(parsed.clone()));
    assert!(redraw);
    assert_eq!(*dropped_files.borrow(), parsed);
}

#[test]
fn test_use_click_outside_invoked_by_runtime_on_outside_pointer_down() {
    let is_open = Signal::new(true);
    let _handle = use_click_outside(is_open.clone(), {
        let is_open = is_open.clone();
        move || is_open.set(false)
    });
    let on_down = move |e: Event| {
        e.stop_propagation();
    };

    let mut runtime = AppRuntime::new(
        rsx! {
            <div class="w-[400px] h-[300px] bg-black">
                <div
                    class="w-[150px] h-[100px] bg-neutral-800"
                    onmousedown=on_down
                >
                    "Inside Popover"
                </div>
            </div>
        },
        400.0,
        300.0,
        1.0,
    );

    // 1. Clicking inside popover (50, 50) calls stop_propagation -> must NOT dismiss
    runtime.dispatch_pointer_down(50.0, 50.0);
    assert!(
        is_open.get(),
        "Clicking inside with stop_propagation must not trigger use_click_outside"
    );

    // 2. Clicking outside popover (300, 200) -> runtime automatically invokes use_click_outside
    runtime.dispatch_pointer_down(300.0, 200.0);
    assert!(
        !is_open.get(),
        "Clicking outside must automatically dismiss use_click_outside"
    );
}

#[test]
fn test_native_dom_drag_and_drop_lifecycle_with_data_transfer() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let l1 = Rc::clone(&log);
    let l2 = Rc::clone(&log);
    let l3 = Rc::clone(&log);
    let l4 = Rc::clone(&log);
    let l5 = Rc::clone(&log);
    let l6 = Rc::clone(&log);

    let on_drag_start = move |e: Event| {
        e.set_data("text/plain", "node-42");
        l1.borrow_mut().push("dragstart".to_string());
    };
    let on_drag_end = move |_e: Event| {
        l2.borrow_mut().push("dragend".to_string());
    };
    let on_drag_enter = move |_e: Event| {
        l3.borrow_mut().push("dragenter".to_string());
    };
    let on_drag_over = move |e: Event| {
        e.prevent_default();
        l4.borrow_mut().push("dragover".to_string());
    };
    let on_drag_leave = move |_e: Event| {
        l5.borrow_mut().push("dragleave".to_string());
    };
    let on_drop = move |e: Event| {
        let payload = e.get_data("text/plain").unwrap_or_default();
        l6.borrow_mut().push(format!("drop:{payload}"));
    };

    let mut runtime = AppRuntime::new(
        rsx! {
            <div class="flex flex-row w-[400px] h-[200px] bg-black">
                <div
                    class="w-[150px] h-[150px] bg-blue-600"
                    draggable="true"
                    ondragstart=on_drag_start
                    ondragend=on_drag_end
                >
                    "Draggable"
                </div>
                <div
                    class="w-[150px] h-[150px] bg-neutral-800"
                    ondragenter=on_drag_enter
                    ondragover=on_drag_over
                    ondragleave=on_drag_leave
                    ondrop=on_drop
                >
                    "Drop Target"
                </div>
            </div>
        },
        400.0,
        200.0,
        1.0,
    );

    // Press on draggable source (x=50, y=50)
    runtime.dispatch_pointer_down(50.0, 50.0);
    // Move past drag threshold still over source (x=60, y=50) -> fires dragstart
    runtime.dispatch_pointer_move(60.0, 50.0);
    // Move into drop target (x=220, y=50) -> fires dragenter + dragover
    runtime.dispatch_pointer_move(220.0, 50.0);
    // Release over drop target -> fires drop + dragend
    runtime.dispatch_pointer_up(220.0, 50.0);

    let events = log.borrow().clone();
    assert!(events.contains(&"dragstart".to_string()));
    assert!(events.contains(&"dragenter".to_string()));
    assert!(events.contains(&"dragover".to_string()));
    assert!(events.contains(&"drop:node-42".to_string()));
    assert_eq!(events.last().map(String::as_str), Some("dragend"));
}

#[test]
fn test_unhovered_wheel_and_scroll_actions_do_not_fallback_to_largest_container() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div class="flex flex-row w-[800px] h-[200px] bg-black">
                <div id="nonscroll-chrome" class="w-[400px] h-[200px] bg-neutral-900" />
                <div id="large-side-panel" class="flex flex-col w-[400px] h-[200px] overflow-y-auto bg-neutral-800">
                    <div class="h-[800px] w-full shrink-0" />
                </div>
            </div>
        },
        800.0,
        200.0,
        1.0,
    );

    assert_eq!(runtime.active_scrollbars.len(), 1);
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 0.0);

    // Cursor is at (100, 100) over the non-scrollable chrome pane — NOT over `large-side-panel`.
    // None of the scroll entry points may fall back to scrolling `large-side-panel`.
    assert!(!runtime.dispatch_scroll(100.0, 100.0, 0.0, 120.0));
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 0.0);

    assert_eq!(
        runtime.dispatch_smooth_scroll_action(100.0, 100.0, 0.0, 120.0),
        (false, false)
    );
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 0.0);

    assert!(!runtime.scroll_to_top(100.0, 100.0));
    assert!(!runtime.scroll_to_bottom(100.0, 100.0));
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 0.0);

    // Moving the pointer inside `large-side-panel` (x=550, y=100) scrolls that container normally.
    assert!(runtime.dispatch_scroll(550.0, 100.0, 0.0, 120.0));
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 120.0);
}

#[test]
fn test_wheel_default_prevented_suppresses_container_scroll_and_horizontal_wheel_works() {
    use std::cell::Cell;
    use std::rc::Rc;
    use strata_dom::Event;

    let wheel_calls = Rc::new(Cell::new(0usize));
    let wheel_cb = Rc::clone(&wheel_calls);

    let mut runtime = AppRuntime::new(
        rsx! {
            <div
                id="prevented-scroller"
                class="flex flex-col w-[300px] h-[200px] overflow-y-auto"
                onwheel={move |e: Event| {
                    wheel_cb.set(wheel_cb.get() + 1);
                    e.prevent_default();
                }}
            >
                <div class="h-[800px] w-full shrink-0" />
            </div>
        },
        300.0,
        200.0,
        1.0,
    );

    runtime.last_mouse_pos = (150.0, 100.0);
    runtime.step_event(&PlatformEvent::Scroll {
        delta_x: 0.0,
        delta_y: 120.0,
        is_momentum: false,
    });
    assert_eq!(wheel_calls.get(), 1);
    assert_eq!(
        runtime.active_scrollbars[0].scroll_y, 0.0,
        "e.prevent_default() inside onwheel must suppress native container scroll"
    );

    // Horizontal wheel scrolling on an `overflow-x-auto` tab bar
    let mut tab_rt = AppRuntime::new(
        rsx! {
            <div id="tab-strip" class="flex flex-row w-[200px] h-[48px] overflow-x-auto">
                <div id="tab-0" class="w-[120px] h-[48px] shrink-0" />
                <div id="tab-1" class="w-[120px] h-[48px] shrink-0" />
                <div id="tab-2" class="w-[120px] h-[48px] shrink-0" />
            </div>
        },
        400.0,
        100.0,
        1.0,
    );
    let initial_x = tab_rt.layout_root.children[0].geometry.border_box.x;
    assert_eq!(initial_x, 0.0);
    assert!(tab_rt.dispatch_scroll(50.0, 24.0, 90.0, 0.0));
    let scrolled_x = tab_rt.layout_root.children[0].geometry.border_box.x;
    assert!(
        (scrolled_x + 90.0).abs() < 1.5,
        "expected tab-0 shifted left by 90px after horizontal wheel, got x={scrolled_x}"
    );
}

#[test]
fn test_smooth_scroll_continuous_deltas_no_runaway_flick_and_zero_delta_skips_redraw() {
    let mut runtime = AppRuntime::new(
        rsx! {
            <div class="flex flex-row w-[800px] h-[400px]">
                <div id="left-scroll" class="flex flex-col w-[400px] h-[400px] overflow-y-auto">
                    <div id="row-0" class="h-[100px] w-full shrink-0" onclick={|_| {}} />
                    <div class="h-[2400px] w-full shrink-0" />
                </div>
                <div id="right-sibling" class="w-[400px] h-[400px]" onclick={|_| {}} />
            </div>
        },
        800.0,
        400.0,
        1.0,
    );

    let t0 = std::time::Instant::now();
    // Initial zero-delta tick at exact start time produces dy == 0 and must not request a spurious redraw.
    assert!(runtime.dispatch_smooth_scroll(200.0, 200.0, 0.0, 12.0));
    if let Some(ref mut tr) = runtime.active_smooth_scroll {
        tr.start_time = t0;
        tr.last_event_time = t0;
    }
    assert!(
        !runtime.poll_smooth_scroll_at(t0),
        "Zero-displacement tick must not trigger redraw"
    );
    assert!(
        runtime.active_smooth_scroll.is_some(),
        "Tracker must remain armed after zero-displacement tick"
    );

    // Simulate a 120Hz touchpad stream of small continuous deltas (12px every 10ms):
    // must accumulate linearly without 3.5x flick runaway.
    for i in 1..=10 {
        let ti = t0 + std::time::Duration::from_millis(i * 10);
        runtime.poll_smooth_scroll_at(ti);
        if let Some(ref mut tr) = runtime.active_smooth_scroll {
            tr.last_event_time = std::time::Instant::now() - std::time::Duration::from_millis(10);
        }
        runtime.dispatch_smooth_scroll(200.0, 200.0, 0.0, 12.0);
    }

    let tracker = runtime.active_smooth_scroll.as_ref().unwrap();
    let expected_sum = 11.0 * 12.0; // initial 12 + 10 steps * 12 = 132px
    assert!(
        (tracker.target_y - expected_sum).abs() < 0.1,
        "Continuous touchpad stream must not over-accelerate: expected {expected_sum}, got {}",
        tracker.target_y
    );

    // Settle and verify scrolled child bounds moved while sibling panel stays unaffected.
    runtime.poll_smooth_scroll_at(std::time::Instant::now() + std::time::Duration::from_millis(500));
    assert!((runtime.active_scrollbars[0].scroll_y - expected_sum).abs() < 0.05);
    let scrolled_y = runtime.layout_root.children[0].children[0].geometry.border_box.y;
    assert!((scrolled_y + expected_sum).abs() < 0.1);
    assert_eq!(runtime.layout_root.children[1].geometry.border_box.x, 400.0);

    // Direction reversal on discrete wheel must reset target without flick amplification.
    let mut rev_rt = AppRuntime::new(
        rsx! {
            <div id="rev-scroll" class="flex flex-col w-[400px] h-[400px] overflow-y-auto">
                <div class="h-[2000px] w-full shrink-0" />
            </div>
        },
        400.0,
        400.0,
        1.0,
    );
    rev_rt.dispatch_smooth_scroll(200.0, 200.0, 0.0, 200.0);
    if let Some(ref mut tr) = rev_rt.active_smooth_scroll {
        tr.last_event_time = std::time::Instant::now() - std::time::Duration::from_millis(40);
    }
    rev_rt.dispatch_smooth_scroll(200.0, 200.0, 0.0, -64.0);
    let rev_target = rev_rt.active_smooth_scroll.as_ref().unwrap().target_y;
    assert!(
        (rev_target - 136.0).abs() < 1.0,
        "Reverse wheel notch must apply 1:1 (-64px) without flick amplification, got {rev_target}"
    );

    // High-frequency 125Hz (8ms) continuous fast-swipe packets >= 48px must still stay 1:1.
    let mut hid_rt = AppRuntime::new(
        rsx! {
            <div id="hid-scroll" class="flex flex-col w-[400px] h-[400px] overflow-y-auto">
                <div class="h-[2000px] w-full shrink-0" />
            </div>
        },
        400.0,
        400.0,
        1.0,
    );
    hid_rt.dispatch_smooth_scroll(200.0, 200.0, 0.0, 55.0);
    if let Some(ref mut tr) = hid_rt.active_smooth_scroll {
        tr.last_event_time = std::time::Instant::now() - std::time::Duration::from_millis(8);
    }
    hid_rt.dispatch_smooth_scroll(200.0, 200.0, 0.0, 55.0);
    let hid_target = hid_rt.active_smooth_scroll.as_ref().unwrap().target_y;
    assert!(
        (hid_target - 110.0).abs() < 0.5,
        "8ms HID touchpad packet must accumulate 1:1 without flick amplification, got {hid_target}"
    );
}

#[test]
fn wheel_over_canvas3d_or_covering_modal_does_not_scroll_unrelated_dock() {
    // 1. Scrollable container holding both a Canvas3D viewport and normal scrollable content below it.
    let view = rsx! {
        <div class="relative w-[600px] h-[400px]">
            <div id="dock-scroll" class="w-[320px] h-[300px] flex flex-col overflow-y-auto">
                <Canvas3D class="w-full h-[180px] shrink-0" />
                <div id="dock-body" class="w-full h-[600px] shrink-0" />
            </div>
        </div>
    };
    let mut rt = AppRuntime::new(view, 600.0, 400.0, 1.0);
    rt.tick_frame();

    assert_eq!(rt.active_scrollbars.len(), 1);
    assert_eq!(rt.active_scrollbars[0].scroll_y, 0.0);

    // Wheel directly over Canvas3D must NOT scroll #dock-scroll behind/around it.
    let scrolled_canvas = rt.dispatch_scroll(100.0, 90.0, 0.0, 60.0);
    assert!(
        !scrolled_canvas,
        "wheel over Canvas3D must not fall back into #dock-scroll"
    );
    assert_eq!(rt.active_scrollbars[0].scroll_y, 0.0);

    // Wheel over #dock-body below Canvas3D DOES scroll #dock-scroll.
    let scrolled_body = rt.dispatch_scroll(100.0, 220.0, 0.0, 60.0);
    assert!(scrolled_body, "wheel over #dock-body must scroll #dock-scroll");
    assert!((rt.active_scrollbars[0].scroll_y - 60.0).abs() < 0.1);

    // 2. Non-scrollable overlay covering #dock-scroll blocks wheel fallthrough to underlying dock.
    let modal_view = rsx! {
        <div class="relative w-[600px] h-[400px]">
            <div id="under-dock" class="w-[320px] h-[300px] flex flex-col overflow-y-auto">
                <div class="w-full h-[700px] shrink-0" />
            </div>
            <div id="covering-panel" class="absolute left-0 top-0 w-[320px] h-[300px]" />
        </div>
    };
    let mut modal_rt = AppRuntime::new(modal_view, 600.0, 400.0, 1.0);
    modal_rt.tick_frame();
    let scrolled_under_modal = modal_rt.dispatch_scroll(100.0, 100.0, 0.0, 60.0);
    assert!(
        !scrolled_under_modal,
        "wheel over non-scrollable covering panel must not fall through to underlying dock"
    );
    assert_eq!(modal_rt.active_scrollbars[0].scroll_y, 0.0);
}

#[test]
fn background_floating_window_scrollbar_paints_and_hittests_below_foreground_window() {
    use strata::render::compositor::DrawCommand;

    let mut bg_rows = Vec::new();
    for i in 0..20 {
        let label = format!("Sequence log line #{i:02}");
        bg_rows.push(rsx! {
            <div class="h-[24px] shrink-0 px-2 text-xs text-slate-200">{label}</div>
        });
    }

    let view = rsx! {
        <div class="relative w-[800px] h-[600px] bg-slate-950">
            <div class="fixed left-[20px] top-[20px] w-[320px] h-[220px] z-[41] flex flex-col bg-slate-900">
                <div id="bg-term" class="flex-1 min-h-0 overflow-y-auto flex flex-col">
                    {bg_rows}
                </div>
            </div>
            <div class="fixed left-[180px] top-[80px] w-[320px] h-[180px] z-[42] flex flex-col bg-emerald-950">
                <div class="p-3 text-sm text-white">{"Terminal: Session #1"}</div>
            </div>
        </div>
    };

    let mut rt = AppRuntime::new(view, 800.0, 600.0, 1.0);
    rt.tick_frame();
    rt.compile_display_list();

    let bg_bar = rt
        .active_scrollbars
        .iter()
        .find(|b| b.id == "bg-term")
        .cloned()
        .expect("background floating window must have an active scrollbar");

    // 1. Paint order: background scrollbar thumb must be emitted strictly BEFORE the
    //    foreground (z=42) window's text.
    let bg_thumb_idx = rt
        .display_list
        .commands
        .iter()
        .position(|cmd| {
            matches!(
                cmd,
                DrawCommand::DrawRoundedRect { bounds, color, .. }
                    if (bounds.x - bg_bar.thumb_bounds.x).abs() < 3.0
                        && color.r == 255
                        && color.g == 255
                        && color.b == 255
            )
        })
        .expect("background scrollbar thumb DrawRoundedRect must be emitted");

    let fg_text_idx = rt
        .display_list
        .commands
        .iter()
        .position(|cmd| {
            matches!(
                cmd,
                DrawCommand::DrawText { text, .. } if text.contains("Session #1")
            )
        })
        .expect("foreground window text must be emitted");

    assert!(
        bg_thumb_idx < fg_text_idx,
        "background z=41 scrollbar ({bg_thumb_idx}) must paint before foreground z=42 window ({fg_text_idx})"
    );

    // 2. Occluded hover must NOT highlight/expand the background scrollbar thumb.
    let bar_center_x = bg_bar.track_bounds.x + bg_bar.track_bounds.width * 0.5;
    let occluded_y = 120.0; // Covered by foreground window (y=80..260, x=180..500)
    rt.last_mouse_pos = (bar_center_x, occluded_y);
    rt.compile_display_list();
    let occluded_alpha = rt
        .display_list
        .commands
        .iter()
        .find_map(|cmd| match cmd {
            DrawCommand::DrawRoundedRect { bounds, color, .. }
                if (bounds.x - bg_bar.thumb_bounds.x).abs() < 3.0
                    && color.r == 255
                    && color.g == 255
                    && color.b == 255 =>
            {
                Some(color.a)
            }
            _ => None,
        })
        .expect("background scrollbar thumb must still exist");
    assert_eq!(
        occluded_alpha, 60,
        "hovering foreground window over occluded background scrollbar must not activate hover alpha"
    );

    // 3. Clicking over the foreground window right above the background scrollbar track/thumb
    //    must NOT capture or scroll the background window's scrollbar.
    assert!(
        !rt.check_scrollbar_thumb_press(bar_center_x, occluded_y),
        "occluded background scrollbar must not capture clicks through higher z-index foreground window"
    );
    assert!(rt.scrollbar_drag.is_none());
    assert_eq!(rt.scroll_offset_by_id("bg-term").1, 0.0);

    // 4. Clicking the UNOCCLUDED top segment of the background window's scrollbar (y=40, above
    //    foreground window top y=80) still hits and initiates scrollbar drag normally.
    let unoccluded_y = 40.0;
    assert!(
        rt.check_scrollbar_thumb_press(bar_center_x, unoccluded_y),
        "unoccluded portion of background scrollbar must remain interactive"
    );
    assert!(rt.scrollbar_drag.is_some());
}





