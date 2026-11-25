use strata::prelude::*;
use strata_dom::View;
use strata_style::selector::PseudoState;
use strata_style::tailwind::TailwindEngine;
use strata_style::types::Color;

#[test]
fn test_tailwind_engine_pseudo_state_resolution() {
    let classes = "bg-white text-black hover:bg-neutral-100 focus:ring-2 active:bg-neutral-200 disabled:opacity-50 dark:bg-black";

    // 1. Inactive base state
    let state_default = PseudoState::default();
    let style_default = TailwindEngine::resolve_classes_with_pseudo_state(classes, &state_default);
    assert_eq!(
        style_default.background_color,
        Some(Color::new(255, 255, 255, 255))
    );

    // 2. Hovered state
    let state_hover = PseudoState {
        is_hovered: true,
        ..Default::default()
    };
    let style_hover = TailwindEngine::resolve_classes_with_pseudo_state(classes, &state_hover);
    assert_eq!(
        style_hover.background_color,
        Some(Color::new(245, 245, 245, 255))
    );

    // 3. Active state
    let state_active = PseudoState {
        is_hovered: true,
        is_active: true,
        ..Default::default()
    };
    let style_active = TailwindEngine::resolve_classes_with_pseudo_state(classes, &state_active);
    assert_eq!(
        style_active.background_color,
        Some(Color::new(229, 229, 229, 255))
    );

    // 4. Disabled state
    let state_disabled = PseudoState {
        is_disabled: true,
        ..Default::default()
    };
    let style_disabled =
        TailwindEngine::resolve_classes_with_pseudo_state(classes, &state_disabled);
    assert_eq!(style_disabled.opacity, 0.5);

    // 5. Dark mode state
    let state_dark = PseudoState {
        is_dark: true,
        ..Default::default()
    };
    let style_dark = TailwindEngine::resolve_classes_with_pseudo_state(classes, &state_dark);
    assert_eq!(style_dark.background_color, Some(Color::new(0, 0, 0, 255)));

    // 6. group-hover state (`hidden group-hover:flex`)
    let tip_classes = "hidden group-hover:flex";
    let style_tip_off = TailwindEngine::resolve_classes_with_pseudo_state(tip_classes, &state_default);
    assert_eq!(style_tip_off.display, strata_style::types::Display::None);
    let state_gh = PseudoState {
        is_group_hovered: true,
        ..Default::default()
    };
    let style_tip_on = TailwindEngine::resolve_classes_with_pseudo_state(tip_classes, &state_gh);
    assert_eq!(style_tip_on.display, strata_style::types::Display::Flex);
}

#[component]
fn InteractiveStateApp() -> View {
    rsx! {
        <div class="flex flex-col w-[400px] h-[300px] p-4 bg-white dark:bg-black">
            <button class="w-[120px] h-[40px] bg-neutral-900 hover:bg-neutral-800 active:bg-neutral-700 focus:bg-blue-600 dark:bg-zinc-800 text-white">
                "Interactive Button"
            </button>
            <button disabled class="w-[120px] h-[40px] bg-emerald-500 disabled:bg-gray-400 disabled:opacity-50 text-white">
                "Disabled Button"
            </button>
        </div>
    }
}

#[test]
fn test_app_runtime_dynamic_pseudo_classes() {
    let view = InteractiveStateApp();
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    let neutral_900 = Color::new(23, 23, 23, 255);
    let neutral_800 = Color::new(38, 38, 38, 255);
    let neutral_700 = Color::new(64, 64, 64, 255);
    let blue_600 = Color::new(37, 99, 235, 255);
    let zinc_800 = Color::new(39, 39, 42, 255);
    let gray_400 = Color::new(156, 163, 175, 255);

    // Initial check on button 1: bg-neutral-900
    let btn1 = &runtime.layout_root.children[0];
    assert_eq!(btn1.style.background_color, Some(neutral_900));

    // Initial check on button 2 (disabled): disabled:bg-gray-400 and disabled:opacity-50
    let btn2 = &runtime.layout_root.children[1];
    assert_eq!(btn2.style.background_color, Some(gray_400));
    assert_eq!(btn2.style.opacity, 0.5);

    // 1. Hover button 1 at (50.0, 30.0)
    let dirty = runtime.update_hover(50.0, 30.0);
    assert!(dirty);
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(neutral_800),
        "Hovered button must resolve hover:bg-neutral-800"
    );

    // 2. Press mouse down while hovered -> active state
    let ev_down = strata_platform::PlatformEvent::MouseDown {
        button: strata_platform::MouseButton::Left,
        x: 50.0,
        y: 30.0,
    };
    runtime.step_event(&ev_down);
    assert!(runtime.is_mouse_down);
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(neutral_700),
        "Active pressed button must resolve active:bg-neutral-700"
    );

    // 3. Release mouse up -> back to hover state
    let ev_up = strata_platform::PlatformEvent::MouseUp {
        button: strata_platform::MouseButton::Left,
        x: 50.0,
        y: 30.0,
    };
    runtime.step_event(&ev_up);
    assert!(!runtime.is_mouse_down);
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(neutral_800),
        "Released button must revert to hover:bg-neutral-800"
    );

    // 4. Focus button 1
    runtime.clear_hover();
    runtime.set_focus_path(Some(vec![0]));
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(blue_600),
        "Focused button must resolve focus:bg-blue-600"
    );

    // 5. Dark mode toggle
    runtime.set_focus_path(None);
    runtime.set_dark_mode(true);
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(zinc_800),
        "Dark mode button must resolve dark:bg-zinc-800"
    );
    assert_eq!(
        runtime.layout_root.style.background_color,
        Some(Color::new(0, 0, 0, 255)),
        "Dark mode container must resolve dark:bg-black"
    );
}

#[test]
fn test_group_hover_tooltip_opens_and_closes_in_runtime() {
    let view = rsx! {
        <div class="flex flex-col w-[400px] h-[300px] p-4 bg-white">
            <div class="group relative inline-flex w-[140px] h-[40px]">
                <button class="w-[140px] h-[40px] bg-neutral-900 text-white">"Trigger"</button>
                <div class="hidden group-hover:flex absolute top-full left-0 px-2 py-1 bg-black text-white">
                    "Group Hover Tooltip"
                </div>
            </div>
        </div>
    };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    // Initially hidden (`display: none`)
    assert_eq!(
        runtime.layout_root.children[0].children[1].style.display,
        strata_style::types::Display::None
    );

    // Hovering inside the `.group` trigger reveals the `group-hover:flex` child.
    let changed = runtime.update_hover(50.0, 30.0);
    assert!(changed, "Hovering into .group must trigger restyle/layout");
    assert_eq!(
        runtime.layout_root.children[0].children[1].style.display,
        strata_style::types::Display::Flex,
        "group-hover:flex tooltip must open when ancestor .group is hovered"
    );
    let has_tip_text = runtime.display_list.commands().iter().any(|cmd| {
        matches!(
            cmd,
            strata_render::compositor::DrawCommand::DrawText { text, .. }
                if text.contains("Group Hover Tooltip")
        )
    });
    assert!(has_tip_text, "Opened group-hover tooltip must render text");

    // Moving outside `.group` hides it again.
    let left_changed = runtime.update_hover(350.0, 250.0);
    assert!(left_changed);
    assert_eq!(
        runtime.layout_root.children[0].children[1].style.display,
        strata_style::types::Display::None,
        "group-hover:flex tooltip must close when leaving .group"
    );
}

#[test]
fn test_native_hover_tooltip_for_title_and_alt_attributes() {
    let view = rsx! {
        <div class="flex flex-col w-[400px] h-[300px] p-4 gap-2 bg-white">
            <button title="Center on Target" class="w-[140px] h-[40px] bg-neutral-900 text-white">
                "Recenter"
            </button>
            <button alt="Toggle Terrain Wireframe" class="w-[140px] h-[40px] bg-neutral-800 text-white">
                "Wireframe"
            </button>
        </div>
    };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.hover_tooltip_delay = std::time::Duration::from_millis(5);

    let has_text = |rt: &AppRuntime, needle: &str| {
        rt.display_list.commands().iter().any(|cmd| {
            matches!(
                cmd,
                strata_render::compositor::DrawCommand::DrawText { text, .. } if text == needle
            )
        })
    };

    // Hover button 1 (`title="Center on Target"`): armed, not immediately visible.
    runtime.freeze_clock_at_epoch();
    runtime.step_event(&strata_platform::PlatformEvent::MouseMove { x: 50.0, y: 30.0 });
    assert_eq!(
        runtime.hover_tooltip_text.as_deref(),
        Some("Center on Target")
    );
    assert!(!runtime.hover_tooltip_visible);
    assert!(!has_text(&runtime, "Center on Target"));

    // After dwell delay, AnimationTick reveals the tooltip in the display list.
    runtime.advance_clock(std::time::Duration::from_millis(10));
    let woke = runtime.step_event(&strata_platform::PlatformEvent::AnimationTick);
    assert!(woke);
    assert!(runtime.hover_tooltip_visible);
    assert!(has_text(&runtime, "Center on Target"));

    // Clicking dismisses the tooltip immediately.
    let click_dirty = runtime.step_event(&strata_platform::PlatformEvent::MouseDown {
        button: strata_platform::MouseButton::Left,
        x: 50.0,
        y: 30.0,
    });
    assert!(click_dirty);
    assert!(!runtime.hover_tooltip_visible);
    assert!(!has_text(&runtime, "Center on Target"));
    runtime.step_event(&strata_platform::PlatformEvent::MouseUp {
        button: strata_platform::MouseButton::Left,
        x: 50.0,
        y: 30.0,
    });

    // Hover button 2 (`alt="Toggle Terrain Wireframe"` fallback on `<button>`).
    runtime.step_event(&strata_platform::PlatformEvent::MouseMove { x: 50.0, y: 80.0 });
    assert_eq!(
        runtime.hover_tooltip_text.as_deref(),
        Some("Toggle Terrain Wireframe")
    );
    runtime.advance_clock(std::time::Duration::from_millis(10));
    assert!(runtime.step_event(&strata_platform::PlatformEvent::AnimationTick));
    assert!(runtime.hover_tooltip_visible);
    assert!(has_text(&runtime, "Toggle Terrain Wireframe"));

    // CursorLeft clears the tooltip from the display list.
    assert!(runtime.step_event(&strata_platform::PlatformEvent::CursorLeft));
    assert!(!runtime.hover_tooltip_visible);
    assert!(!has_text(&runtime, "Toggle Terrain Wireframe"));
}

