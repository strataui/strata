//! Focus, text, caret and undo history must survive root re-invocation, including when the
//! tree above the input changes shape.

use strata::prelude::*;
use strata_platform::{MouseButton, PlatformEvent};

fn root(banner: bool, with_id: bool) -> View {
    let input = if with_id {
        rsx! { <input id="query" type="text" value="" class="w-full" /> }
    } else {
        rsx! { <input type="text" value="" class="w-full" /> }
    };
    rsx! {
        <div class="flex flex-col w-96 p-2">
            {if banner {
                rsx! { <div class="h-8 bg-amber-500">"Saved"</div> }
            } else {
                rsx! { <></> }
            }}
            <div class="h-10">"label"</div>
            {input}
        </div>
    }
}

fn type_text(runtime: &mut AppRuntime, text: &str) {
    for ch in text.chars() {
        runtime.step_event(&PlatformEvent::ReceivedCharacter(ch));
    }
}

fn run(with_id: bool) {
    let mut runtime = AppRuntime::new(root(false, with_id), 400.0, 300.0, 1.0);
    runtime.tick_frame();
    // The input sits below the 40px label inside 8px padding.
    runtime.step_event(&PlatformEvent::MouseDown {
        button: MouseButton::Left,
        x: 40.0,
        y: 8.0 + 40.0 + 10.0,
    });
    assert!(runtime.active_focus.is_some(), "click focuses the input");
    type_text(&mut runtime, "abc");

    // Re-invoke the root with a banner inserted above the input.
    runtime.update_view(root(true, with_id));
    let active = runtime
        .active_focus
        .as_ref()
        .expect("focus survives root re-invocation");
    assert_eq!(active.state.text.get(), "abc");
    assert_eq!(active.state.cursor_index.get(), 3);
    assert!(active.state.is_focused.get());

    type_text(&mut runtime, "d");
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "abcd"
    );

    // And back again.
    runtime.update_view(root(false, with_id));
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "abcd"
    );
}

#[test]
fn input_state_survives_reinvocation_without_id() {
    run(false);
}

#[test]
fn input_state_survives_reinvocation_with_id() {
    run(true);
}
