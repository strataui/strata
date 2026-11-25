use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use strata::prelude::*;
use strata_dom::View;
use strata_platform::PlatformEvent;

#[component]
fn SpinningIconApp() -> View {
    rsx! {
        <div class="flex flex-col w-[400px] h-[300px] bg-black p-4">
            <div class="w-[40px] h-[40px] bg-blue-500 animate-spin">
                "Icon"
            </div>
        </div>
    }
}

#[component]
fn StaticApp() -> View {
    rsx! {
        <div class="flex flex-col w-[400px] h-[300px] bg-black p-4">
            <div class="w-[40px] h-[40px] bg-neutral-900 text-white">
                "Stationary"
            </div>
        </div>
    }
}

#[component]
fn TransitionApp(active: bool) -> View {
    let bg_class = if active {
        "w-[200px] h-[100px] bg-white transition-all duration-200"
    } else {
        "w-[100px] h-[100px] bg-black transition-all duration-200"
    };

    rsx! {
        <div class="flex flex-col w-[400px] h-[300px] bg-neutral-950 p-4">
            <div class={bg_class}>
                "Box"
            </div>
        </div>
    }
}

#[component]
fn LifecycleEventsApp(start_fired: Rc<Cell<bool>>, iteration_fired: Rc<Cell<bool>>) -> View {
    let on_start = {
        let flag = start_fired.clone();
        move |_ev: strata_dom::Event| {
            flag.set(true);
        }
    };

    let on_iteration = {
        let flag = iteration_fired.clone();
        move |_ev: strata_dom::Event| {
            flag.set(true);
        }
    };

    rsx! {
        <div class="flex flex-col w-[400px] h-[300px] bg-black p-4">
            <div
                class="w-[40px] h-[40px] bg-green-500 animate-spin"
                onanimationstart={on_start}
                onanimationiteration={on_iteration}
            >
                "Animated"
            </div>
        </div>
    }
}

#[test]
fn test_runtime_tailwind_animation_registration_and_stepping() {
    let view = SpinningIconApp();
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    // Initial state: animated child should have registered spin animation
    assert!(
        runtime.has_active_animations(),
        "Runtime must report active animations when animate-spin is present"
    );

    let child = &runtime.layout_root.children[0];
    assert!(
        child.style.has_transform(),
        "Spinning child must have transform property initialized"
    );

    // Sleep a few milliseconds and poll
    std::thread::sleep(Duration::from_millis(15));
    let redraw_needed = runtime.poll_animations();
    assert!(
        redraw_needed,
        "poll_animations must return true while keyframe animation is active"
    );

    // Platform event AnimationTick should return true
    let tick_dirty = runtime.step_event(&PlatformEvent::AnimationTick);
    assert!(
        tick_dirty,
        "PlatformEvent::AnimationTick must return true during active animations"
    );
}

#[test]
fn test_runtime_auto_sleep_when_stationary() {
    let view = StaticApp();
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    assert!(
        !runtime.has_active_animations(),
        "Runtime must not report active animations for static views"
    );

    // Polling animations on stationary view must return false immediately
    let redraw_needed = runtime.poll_animations();
    assert!(
        !redraw_needed,
        "poll_animations must return false when stationary (auto-sleep)"
    );

    // PlatformEvent::AnimationTick must return false, saving CPU power
    let tick_dirty = runtime.step_event(&PlatformEvent::AnimationTick);
    assert!(
        !tick_dirty,
        "PlatformEvent::AnimationTick must return false when stationary"
    );
}

#[test]
fn test_runtime_css_transition_on_state_update() {
    let view1 = TransitionApp(false);
    let mut runtime = AppRuntime::new(view1, 400.0, 300.0, 1.0);

    // Initially static before state update
    assert!(
        !runtime.has_active_animations(),
        "No transitions should be active on initial mount"
    );

    // Update view with active = true
    let view2 = TransitionApp(true);
    runtime.update_view(view2);

    // Transition should now be running
    assert!(
        runtime.has_active_animations(),
        "Transition must be active after state update with changing style"
    );

    // Advance clock
    std::thread::sleep(Duration::from_millis(30));
    let dirty = runtime.poll_animations();
    assert!(dirty, "Polling active transition must trigger redraw");

    let animated_box = &runtime.layout_root.children[0];
    if let strata_style::types::Dimension::Px(w) = animated_box.style.width {
        assert!(
            (100.0..=200.0).contains(&w),
            "Width should be smoothly interpolated between 100 and 200, got: {}",
            w
        );
    } else {
        panic!("Width must be Dimension::Px");
    }
}

#[test]
fn test_keyframe_animation_ticks_are_layout_free() {
    let view = SpinningIconApp();
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    assert!(
        runtime.has_active_animations(),
        "animate-spin must register an active keyframe animation"
    );

    let baseline = runtime.layout_compute_count();
    for _ in 0..8 {
        std::thread::sleep(Duration::from_millis(2));
        assert!(
            runtime.poll_animations(),
            "paint-only keyframe animation must request a redraw"
        );
    }
    assert_eq!(
        runtime.layout_compute_count(),
        baseline,
        "poll_animations must not recompute layout for transform/opacity animations"
    );

    let baseline = runtime.layout_compute_count();
    for _ in 0..8 {
        std::thread::sleep(Duration::from_millis(2));
        assert!(
            runtime.step_event(&PlatformEvent::AnimationTick),
            "AnimationTick must request a redraw while the spinner is active"
        );
    }
    assert_eq!(
        runtime.layout_compute_count(),
        baseline,
        "AnimationTick must not recompute layout for paint-only animations"
    );
}

#[test]
fn test_width_transition_still_recomputes_layout() {
    let view = TransitionApp(false);
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.update_view(TransitionApp(true));

    assert!(runtime.has_active_animations());
    let baseline = runtime.layout_compute_count();
    std::thread::sleep(Duration::from_millis(20));
    assert!(runtime.poll_animations());
    assert!(
        runtime.layout_compute_count() > baseline,
        "width transitions participate in box layout and must recompute layout"
    );
}

#[test]
fn test_runtime_synthetic_animation_events() {
    let start_fired = Rc::new(Cell::new(false));
    let iteration_fired = Rc::new(Cell::new(false));

    let view = LifecycleEventsApp(start_fired.clone(), iteration_fired.clone());
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    // Start event should have fired during sync
    assert!(
        start_fired.get(),
        "onanimationstart listener should have been invoked on initial sync"
    );

    // Poll frame to verify continuous operation
    let dirty = runtime.poll_animations();
    assert!(dirty);
}

#[test]
fn test_conditional_swap_does_not_trigger_spurious_transition() {
    // 1. Parent branch identity change (id="tab-a" -> id="tab-b") with unkeyed child at same path
    let tab_a = rsx! {
        <div>
            <div id="tab-a" class="p-6">
                <div class="p-6">"Tab A"</div>
            </div>
        </div>
    };
    let mut runtime = AppRuntime::new(tab_a, 400.0, 300.0, 1.0);
    assert!(!runtime.has_active_animations());

    let tab_b = rsx! {
        <div>
            <div id="tab-b">
                <div class="h-8 transition-all duration-150">"Card"</div>
            </div>
        </div>
    };
    runtime.update_view(tab_b);
    assert!(
        !runtime.has_active_animations(),
        "Swapping parent branch id must not trigger transition on newly mounted child"
    );
    let card = &runtime.layout_root.children[0].children[0];
    assert_eq!(card.style.padding.top, 0.0);
    assert!((card.geometry.border_box.height - 32.0).abs() < 0.5);

    // 2. Unkeyed same-tag swap where previous element had p-6 (no transition) and new has transition-all
    let view_p6 = rsx! {
        <div>
            <div class="p-6">"Old"</div>
        </div>
    };
    let mut runtime2 = AppRuntime::new(view_p6, 400.0, 300.0, 1.0);
    let view_card = rsx! {
        <div>
            <div class="h-8 transition-all duration-150">"Card"</div>
        </div>
    };
    runtime2.update_view(view_card);
    assert!(
        !runtime2.has_active_animations(),
        "Newly mounted transition-all element replacing non-transitioning element must not interpolate old padding"
    );
    let card2 = &runtime2.layout_root.children[0];
    assert_eq!(card2.style.padding.top, 0.0);
    assert!((card2.geometry.border_box.height - 32.0).abs() < 0.5);

    // 3. Keyed swap where BOTH branches have transition-all duration-150
    let keyed_a = rsx! {
        <div>
            <div key="panel-a" class="p-6 transition-all duration-150">"A"</div>
        </div>
    };
    let mut runtime3 = AppRuntime::new(keyed_a, 400.0, 300.0, 1.0);
    let keyed_b = rsx! {
        <div>
            <div key="panel-b" class="h-8 transition-all duration-150">"B"</div>
        </div>
    };
    runtime3.update_view(keyed_b);
    assert!(
        !runtime3.has_active_animations(),
        "Swapping elements with distinct keys must not inherit previous_node_styles"
    );

    // 4. Tag change (section -> div) where both have transition-all
    let tag_a = rsx! {
        <div>
            <section class="p-6 transition-all duration-150">"A"</section>
        </div>
    };
    let mut runtime4 = AppRuntime::new(tag_a, 400.0, 300.0, 1.0);
    let tag_b = rsx! {
        <div>
            <div class="h-8 transition-all duration-150">"B"</div>
        </div>
    };
    runtime4.update_view(tag_b);
    assert!(
        !runtime4.has_active_animations(),
        "Swapping elements with different tags must not inherit previous_node_styles"
    );
}

#[test]
fn test_unmounted_transition_is_pruned_and_active_schedules_16ms_sleep() {
    let view1 = rsx! {
        <div>
            <div key="item-1" class="w-[200px] h-8 transition-all duration-200">"Item"</div>
        </div>
    };
    let mut runtime = AppRuntime::new(view1, 400.0, 300.0, 1.0);
    assert!(!runtime.has_active_animations());

    let view2 = rsx! {
        <div>
            <div key="item-1" class="w-[100px] h-8 transition-all duration-200">"Item"</div>
        </div>
    };
    runtime.update_view(view2);
    assert!(runtime.has_active_animations());
    assert_eq!(
        runtime.compute_idle_sleep(false),
        Duration::from_millis(16),
        "Active transition must schedule 16ms frame ticks"
    );

    // Unmount item-1 by swapping to item-2
    let view3 = rsx! {
        <div>
            <div key="item-2" class="w-[100px] h-8 transition-all duration-200">"Other"</div>
        </div>
    };
    runtime.update_view(view3);
    assert!(
        !runtime.has_active_animations(),
        "Unmounting an element with an in-flight transition must prune its transition"
    );
}

#[test]
fn test_transition_width_to_zero_and_rem_reflows_flex_siblings_smoothly() {
    #[component]
    fn DockApp(open: bool) -> View {
        let dock_cls = if open {
            "w-60 h-[200px] shrink-0 transition-[width] duration-500"
        } else {
            "w-0 h-[200px] shrink-0 overflow-hidden transition-[width] duration-500"
        };
        rsx! {
            <div class="flex flex-row w-[480px] h-[200px]">
                <div key="dock-panel" class={dock_cls}>"Dock"</div>
                <div key="main-pane" class="flex-1 h-[200px]">"Main"</div>
            </div>
        }
    }

    let mut runtime = AppRuntime::new(DockApp(true), 480.0, 200.0, 1.0);
    let dock_open_w = runtime.layout_root.children[0].geometry.border_box.width;
    let main_open_x = runtime.layout_root.children[1].geometry.border_box.x;
    assert!((dock_open_w - 240.0).abs() < 1.0, "w-60 = 240px");
    assert!((main_open_x - 240.0).abs() < 1.0);

    // Toggle w-60 -> w-0 (Rem(15.0) -> Px(0.0)) and tick once mid-transition.
    runtime.update_view(DockApp(false));
    assert!(
        runtime.has_active_animations(),
        "transition-[width] must start a width transition from w-60 to w-0"
    );

    std::thread::sleep(Duration::from_millis(45));
    assert!(runtime.poll_animations());
    let dock_mid_w = runtime.layout_root.children[0].geometry.border_box.width;
    let main_mid = runtime.layout_root.children[1].geometry.border_box;
    assert!(
        dock_mid_w > 10.0 && dock_mid_w < 235.0,
        "sidebar width must interpolate cleanly between 240 and 0, got {dock_mid_w}"
    );
    assert!(
        (main_mid.x - dock_mid_w).abs() < 1.0
            && (main_mid.width - (480.0 - dock_mid_w)).abs() < 1.0,
        "flex-1 sibling must reflow smoothly alongside transitioning sidebar (dock={dock_mid_w}, main={main_mid:?})"
    );

    // Finish transition and verify sibling settles to full 480px width.
    std::thread::sleep(Duration::from_millis(520));
    let _ = runtime.poll_animations();
    let dock_end_w = runtime.layout_root.children[0].geometry.border_box.width;
    let main_end = runtime.layout_root.children[1].geometry.border_box;
    assert!(dock_end_w < 0.5, "settled dock width must reach 0, got {dock_end_w}");
    assert!(
        (main_end.width - 480.0).abs() < 1.0,
        "settled main pane must expand to full 480px, got {:?}",
        main_end
    );
}

#[test]
fn test_collapsible_dock_transition_locks_inner_width_and_supports_auto_height() {
    use std::time::Duration;

    #[component]
    fn CollapsiblePanels(side_open: bool, bottom_open: bool) -> View {
        let side_cls = if side_open {
            "w-[260px] h-[220px] shrink-0 overflow-hidden transition-[width] duration-300 flex flex-col p-2"
        } else {
            "w-0 h-[220px] shrink-0 overflow-hidden transition-[width] duration-300 flex flex-col p-0"
        };
        let bottom_cls = if bottom_open {
            "w-[480px] h-auto overflow-hidden transition-[height] duration-300 flex flex-col"
        } else {
            "w-[480px] h-0 overflow-hidden transition-[height] duration-300 flex flex-col"
        };
        rsx! {
            <div class="flex flex-col w-[480px] h-[360px]">
                <div class="flex flex-row w-[480px] h-[220px]">
                    <div key="side-dock" class={side_cls}>
                        <div key="inner-card" class="w-full p-2">
                            <p key="inner-text" class="w-full text-sm">
                                {"Telemetry dock description should never squeeze to zero width mid-collapse"}
                            </p>
                        </div>
                    </div>
                    <div key="canvas-pane" class="flex-1 h-[220px]">{"Canvas"}</div>
                </div>
                <div key="bottom-dock" class={bottom_cls}>
                    <div class="w-full h-[96px] shrink-0">{"Bottom Strip"}</div>
                </div>
            </div>
        }
    }

    let mut runtime = AppRuntime::new(CollapsiblePanels(true, true), 480.0, 360.0, 1.0);
    let side_open_w = runtime.layout_root.children[0].children[0].geometry.border_box.width;
    let inner_open_w = runtime.layout_root.children[0].children[0].children[0]
        .geometry
        .border_box
        .width;
    let bottom_open_h = runtime.layout_root.children[1].geometry.border_box.height;
    assert!((side_open_w - 260.0).abs() < 1.0);
    assert!((inner_open_w - 244.0).abs() < 1.0);
    assert!((bottom_open_h - 96.0).abs() < 1.0);

    // 1. Collapse both side dock (260 -> 0) and auto-height bottom dock (Auto 96 -> 0).
    runtime.update_view(CollapsiblePanels(false, false));
    assert!(runtime.has_active_animations());

    std::thread::sleep(Duration::from_millis(45));
    assert!(runtime.poll_animations());

    let side_node = &runtime.layout_root.children[0].children[0];
    let side_w = side_node.geometry.border_box.width;
    let inner_card_w = side_node.children[0].geometry.border_box.width;
    let inner_text_h = side_node.children[0].children[0].geometry.border_box.height;
    let bottom_mid_h = runtime.layout_root.children[1].geometry.border_box.height;

    assert!(
        side_w > 10.0 && side_w < 255.0,
        "side dock outer width must animate smoothly, got {side_w}"
    );
    assert!(
        inner_card_w >= 240.0,
        "inner dock card must stay at expanded width (>=240px) instead of squeezing mid-collapse, got {inner_card_w}"
    );
    assert!(
        inner_text_h < 120.0,
        "inner dock text must not collapse into single-letter column wrapping (h={inner_text_h})"
    );
    assert!(
        bottom_mid_h > 2.0 && bottom_mid_h < 95.0,
        "bottom dock h-auto -> h-0 must smoothly interpolate height instead of snapping, got {bottom_mid_h}"
    );

    // Verify PushClip matches the animating side dock bounds so expanded inner content is clipped.
    runtime.compile_display_list();
    assert!(runtime.display_list.commands.iter().any(|cmd| matches!(
        cmd,
        strata::render::compositor::DrawCommand::PushClip { bounds, .. }
            if (bounds.width - side_w).abs() < 2.0
                && (bounds.height - 220.0).abs() < 2.0
    )));

    // 2. Finish collapse, then toggle back open (h-0 -> h-auto) and verify smooth expansion!
    std::thread::sleep(Duration::from_millis(320));
    let _ = runtime.poll_animations();
    assert!(runtime.layout_root.children[1].geometry.border_box.height < 0.5);

    runtime.update_view(CollapsiblePanels(true, true));
    assert!(runtime.has_active_animations());
    std::thread::sleep(Duration::from_millis(45));
    assert!(runtime.poll_animations());
    let bottom_reopen_h = runtime.layout_root.children[1].geometry.border_box.height;
    assert!(
        bottom_reopen_h > 2.0 && bottom_reopen_h < 95.0,
        "bottom dock h-0 -> h-auto must smoothly interpolate height upward, got {bottom_reopen_h}"
    );
}

#[test]
fn reconciled_update_view_during_active_theme_transition_preserves_target_base_styles() {
    use strata_style::types::Color;

    fn render_theme_app(light: bool, hud_tick: u32) -> View {
        let css = if light {
            ".action-btn { background-color: rgb(255, 255, 255); color: rgb(30, 41, 59); }"
        } else {
            ".action-btn { background-color: rgb(30, 32, 37); color: rgb(229, 231, 235); }"
        };
        let tick_label = format!("FPS: {}", 60 + hud_tick);
        rsx! {
            <div class="w-[640px] h-[360px] flex flex-col gap-3 p-4">
                <style>{css}</style>
                <div class="flex flex-row gap-2">
                    <button id="action-btn" class="action-btn transition-all duration-500 px-3 py-1">
                        {"Action"}
                    </button>
                    <button id="other-btn" class="px-3 py-1 bg-slate-700 hover:bg-blue-600 text-white">
                        {"Hoverable"}
                    </button>
                </div>
                <div id="hud-fps" class="text-xs text-slate-400">{tick_label}</div>
            </div>
        }
    }

    fn find_by_id<'a>(
        node: &'a strata_layout::LayoutNode,
        id: &str,
    ) -> Option<&'a strata_layout::LayoutNode> {
        if node.element_id.as_deref() == Some(id) {
            return Some(node);
        }
        for child in &node.children {
            if let Some(found) = find_by_id(child, id) {
                return Some(found);
            }
        }
        None
    }

    let mut rt = AppRuntime::new(render_theme_app(false, 0), 640.0, 360.0, 1.0);
    rt.tick_frame();
    {
        let rtl = find_by_id(&rt.layout_root, "action-btn").expect("#action-btn must exist");
        assert_eq!(rtl.style.background_color, Some(Color::rgb(30, 32, 37)));
        assert_eq!(rtl.style.color, Color::rgb(229, 231, 235));
    }

    // 1. Switch theme obsidian-dark -> daylight-light (starts 150ms transition on #action-btn).
    rt.update_view(render_theme_app(true, 0));
    assert!(
        rt.has_active_animations(),
        "theme switch on transition-all duration-150 button must start an active transition"
    );

    // 2. At +20 ms, fire an unrelated reconciled update_view (e.g. status HUD 4Hz tick).
    std::thread::sleep(Duration::from_millis(20));
    let _ = rt.poll_animations();
    rt.update_view(render_theme_app(true, 1));
    assert!(
        rt.last_update_reconciled(),
        "unrelated HUD counter update must take the incremental reconcile path"
    );
    assert!(
        rt.has_active_animations(),
        "unrelated reconciled update_view must not cancel the in-flight theme transition"
    );

    // 3. Fire a second unrelated reconciled update_view at +40 ms as well.
    std::thread::sleep(Duration::from_millis(20));
    let _ = rt.poll_animations();
    rt.update_view(render_theme_app(true, 2));
    assert!(rt.last_update_reconciled());

    // 4. Wait past the 500 ms transition duration and poll animations to completion.
    std::thread::sleep(Duration::from_millis(520));
    let _ = rt.poll_animations();
    assert!(
        !rt.has_active_animations(),
        "500ms theme transition must complete cleanly after 560ms total elapsed time"
    );

    let rtl = find_by_id(&rt.layout_root, "action-btn").expect("#action-btn must still exist");
    assert_eq!(
        rtl.style.background_color,
        Some(Color::rgb(255, 255, 255)),
        "#action-btn background must settle to daylight-light white, not freeze at mid-transition dark"
    );
    assert_eq!(
        rtl.style.color,
        Color::rgb(30, 41, 59),
        "#action-btn text color must settle to daylight-light slate-800, not freeze at mid-transition dark"
    );
}
