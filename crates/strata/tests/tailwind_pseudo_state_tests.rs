use strata::prelude::*;
use strata_dom::View;
use strata_style::selector::PseudoState;
use strata_style::tailwind::TailwindEngine;
use strata_style::types::Color;

#[test]
fn test_tailwind_engine_pseudo_state_resolution() {
    let classes = "bg-white text-black hover:bg-neutral-100 focus:ring-2 active:bg-neutral-200 disabled:opacity-50 dark:bg-black";

    // 1. Inactive base state
    let state_default = PseudoState {
        is_hovered: false,
        is_focused: false,
        is_active: false,
        is_disabled: false,
        is_dark: false,
    };
    let style_default = TailwindEngine::resolve_classes_with_pseudo_state(classes, &state_default);
    assert_eq!(style_default.background_color, Some(Color::new(255, 255, 255, 255)));

    // 2. Hovered state
    let state_hover = PseudoState {
        is_hovered: true,
        is_focused: false,
        is_active: false,
        is_disabled: false,
        is_dark: false,
    };
    let style_hover = TailwindEngine::resolve_classes_with_pseudo_state(classes, &state_hover);
    assert_eq!(style_hover.background_color, Some(Color::new(245, 245, 245, 255)));

    // 3. Active state
    let state_active = PseudoState {
        is_hovered: true,
        is_focused: false,
        is_active: true,
        is_disabled: false,
        is_dark: false,
    };
    let style_active = TailwindEngine::resolve_classes_with_pseudo_state(classes, &state_active);
    assert_eq!(style_active.background_color, Some(Color::new(229, 229, 229, 255)));

    // 4. Disabled state
    let state_disabled = PseudoState {
        is_hovered: false,
        is_focused: false,
        is_active: false,
        is_disabled: true,
        is_dark: false,
    };
    let style_disabled = TailwindEngine::resolve_classes_with_pseudo_state(classes, &state_disabled);
    assert_eq!(style_disabled.opacity, 0.5);

    // 5. Dark mode state
    let state_dark = PseudoState {
        is_hovered: false,
        is_focused: false,
        is_active: false,
        is_disabled: false,
        is_dark: true,
    };
    let style_dark = TailwindEngine::resolve_classes_with_pseudo_state(classes, &state_dark);
    assert_eq!(style_dark.background_color, Some(Color::new(0, 0, 0, 255)));
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
