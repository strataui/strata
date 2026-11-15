//! Stateful Select: open/close and value state live inside the component and survive
//! root re-invocation.

use strata::prelude::*;
use strata_dom::clear_persistent_signals;
use strata_platform::{MouseButton, PlatformEvent};
use strata_ui::inputs::{select_step_index, Select, SelectOption};

fn root() -> View {
    rsx! {
        <div class="w-80 p-4">
            <Select
                id="region"
                label="Region"
                options={vec![
                    SelectOption::new("us", "United States"),
                    SelectOption::new("eu", "Europe"),
                    SelectOption::new("ap", "Asia Pacific"),
                ]}
            />
        </div>
    }
}

fn click(runtime: &mut AppRuntime, x: f32, y: f32) {
    for event in [
        PlatformEvent::MouseDown {
            button: MouseButton::Left,
            x,
            y,
        },
        PlatformEvent::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        },
    ] {
        runtime.step_event(&event);
    }
}

fn has_listbox(runtime: &AppRuntime) -> bool {
    format!("{:?}", runtime.root_blueprint).contains("aria-selected")
}

#[test]
fn select_opens_closes_and_keeps_state_across_reinvocation() {
    clear_persistent_signals();
    let mut runtime = AppRuntime::new(root(), 400.0, 300.0, 1.0);
    runtime.tick_frame();
    assert!(!has_listbox(&runtime), "starts closed");

    // The trigger sits under the label inside the 16px padding.
    let (tx, ty) = (60.0, 16.0 + 14.0 + 20.0);
    click(&mut runtime, tx, ty);
    runtime.update_view(root());
    assert!(has_listbox(&runtime), "click on the trigger opens the list");

    // Re-invoking the root must not reset the open state.
    runtime.update_view(root());
    assert!(
        has_listbox(&runtime),
        "open state survives root re-invocation"
    );
    let rendered = format!("{:?}", runtime.root_blueprint);
    assert!(rendered.contains("Europe") && rendered.contains("Asia Pacific"));
    assert!(rendered.contains("combobox"));

    click(&mut runtime, tx, ty);
    runtime.update_view(root());
    assert!(!has_listbox(&runtime), "second click closes the list");
}

#[test]
fn select_step_index_clamps() {
    assert_eq!(select_step_index(3, 0, -1), 0);
    assert_eq!(select_step_index(3, 0, 1), 1);
    assert_eq!(select_step_index(3, 2, 1), 2);
    assert_eq!(select_step_index(0, 5, 1), 0);
}
