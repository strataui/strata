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
