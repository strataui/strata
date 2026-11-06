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
    assert_eq!(runtime.layout_root.children[0].geometry.border_box.y, -120.0);
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
    assert!(runtime.active_smooth_scroll.is_some(), "Tracker must be active");

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
        assert!(cur_y >= prev_y, "Scroll offset must be monotonically non-decreasing: cur={cur_y}, prev={prev_y}");
        prev_y = cur_y;
    }

    // Step past settle duration (450ms)
    let settle_t = start_t + std::time::Duration::from_millis(450);
    runtime.poll_smooth_scroll_at(settle_t);

    assert_eq!(runtime.active_scrollbars[0].scroll_y, 120.0, "Must settle exactly at 120.0px target");
    assert!(runtime.active_smooth_scroll.is_none(), "Tracker must be cleared on settle");

    // Subsequent tick should report no update needed (idle at 0% CPU)
    let idle_updated = runtime.poll_smooth_scroll_at(settle_t + std::time::Duration::from_millis(50));
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
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 600.0, "Must clamp to max_scroll_y exactly");

    // Scroll up way past top
    let t1 = t0 + std::time::Duration::from_millis(600);
    runtime.dispatch_smooth_scroll(400.0, 200.0, 0.0, -8000.0);
    if let Some(ref mut tr) = runtime.active_smooth_scroll {
        tr.start_time = t1;
        tr.last_event_time = t1;
    }

    // Settle at top
    runtime.poll_smooth_scroll_at(t1 + std::time::Duration::from_millis(500));
    assert_eq!(runtime.active_scrollbars[0].scroll_y, 0.0, "Must clamp to 0.0 exactly");
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
    assert!(runtime.active_smooth_scroll.is_none(), "Manual thumb press must immediately cancel kinetic scroll");
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
    assert_eq!(bar.track_bounds.x + bar.track_bounds.width, expected_right, "Track right edge must be exactly at 800.0px");
    assert_eq!(bar.thumb_bounds.x + bar.thumb_bounds.width, expected_right, "Thumb right edge must be exactly at 800.0px");
    assert_eq!(bar.track_bounds.y, 0.0, "Track top edge must be exactly at 0.0px");
    assert_eq!(bar.track_bounds.height, 400.0, "Track height must match container height 400.0px");

    let thumb_center_y = bar.thumb_bounds.y + bar.thumb_bounds.height / 2.0;

    // Slam mouse to the very edge right (x = 800.0): must pick the scrollbar thumb without moving left!
    assert!(runtime.check_scrollbar_thumb_press(800.0, thumb_center_y), "Must pick scrollbar at x = 800.0px (very edge right)");
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
    assert!(runtime.check_scrollbar_thumb_press(800.0, 350.0), "Must pick scrollbar track at x = 800.0px");
    assert!(runtime.scrollbar_drag.is_some());
    assert!(runtime.active_scrollbars[0].scroll_y > 0.0, "Clicking track must scroll towards click point");
    runtime.scrollbar_drag = None;

    // Coordinates to the left of the scrollbar track must not pick scrollbar
    assert!(!runtime.check_scrollbar_thumb_press(780.0, thumb_center_y), "Coordinates left of track must return false");
    assert!(runtime.scrollbar_drag.is_none());
}

