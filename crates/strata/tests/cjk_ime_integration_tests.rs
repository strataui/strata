//! Integration tests for East Asian CJK Input Method Editor (IME) text input,
//! candidate caret window positioning, and surrounding text buffer synchronization.

use strata::prelude::*;
use strata_platform::{MockPlatformWindow, PlatformEvent, WindowConfig};

#[component]
fn ImeInputContainer() -> View {
    rsx! {
        <div class="p-8 bg-[#18181b] w-full h-full flex flex-col gap-4">
            <label class="text-sm font-sans text-neutral-300">{"International CJK Input Test"}</label>
            <input
                type="text"
                placeholder="Type Chinese, Japanese, or Korean..."
                class="w-80 h-10 px-3 bg-[#27272a] text-white border border-neutral-700 rounded text-base font-sans"
            />
        </div>
    }
}

fn input_content_box(runtime: &AppRuntime) -> strata::layout::Rect {
    let mut counter = 0;
    let layout = AppRuntime::find_input_layout(
        &runtime.root_blueprint,
        &runtime.layout_root,
        0,
        None,
        &mut counter,
    )
    .expect("input must exist in layout tree");
    layout.geometry.content_box
}

#[test]
fn test_cjk_ime_composition_and_candidate_caret_positioning() {
    let mut runtime = AppRuntime::new(ImeInputContainer(), 800.0, 600.0, 1.0);

    let mut mock_window = MockPlatformWindow::new(WindowConfig::default());
    assert_eq!(mock_window.ime_cursor_location, None);

    // 1. Initial state: unfocused
    assert!(runtime.active_focus.is_none());
    runtime.sync_ime_cursor_location(&mut mock_window);
    assert_eq!(mock_window.ime_cursor_location, None);

    // 2. Focus the input element via mouse click
    let box_geom = input_content_box(&runtime);
    let click_x = box_geom.x + 10.0;
    let click_y = box_geom.y + box_geom.height / 2.0;
    let clicked = runtime.dispatch_pointer_down(click_x, click_y);
    assert!(clicked, "clicking on input element must succeed");
    assert!(runtime.active_focus.is_some(), "input element must now be focused");

    // 3. Verify initial active caret rectangle calculation
    let initial_caret = runtime
        .active_caret_rect()
        .expect("focused input must have active caret geometry");
    assert!(initial_caret.width > 0.0);
    assert!(initial_caret.height > 0.0);
    assert!(initial_caret.x >= box_geom.x);

    // 4. Synchronize IME location to platform window
    runtime.sync_ime_cursor_location(&mut mock_window);
    let initial_loc = mock_window
        .ime_cursor_location
        .expect("platform window must receive candidate window positioning coordinates");
    assert_eq!(initial_loc.0, initial_caret.x.round() as i32);
    assert_eq!(initial_loc.1, initial_caret.y.round() as i32);

    // 5. Preedit stage: Candidate composition (e.g. typing Pinyin "nihao")
    let preedit_ev = PlatformEvent::ImePreedit {
        text: "nihao".to_string(),
        cursor: Some((5, 5)),
    };
    let redraw_needed = runtime.step_event(&preedit_ev);
    assert!(redraw_needed, "preedit updates must trigger view redraw");

    // 6. Commit stage: Chinese Hanzi commitment ("你好世界")
    let commit_zh = PlatformEvent::ImeCommit("你好世界".to_string());
    runtime.step_event(&commit_zh);

    let current_text = runtime
        .active_focus
        .as_ref()
        .expect("still focused")
        .state
        .text
        .get();
    assert_eq!(current_text, "你好世界");

    // 7. Verify caret advanced to the right of the committed Hanzi glyphs
    let hanzi_caret = runtime
        .active_caret_rect()
        .expect("caret must exist after Hanzi commit");
    assert!(
        hanzi_caret.x > initial_caret.x,
        "caret X ({}) must advance past initial X ({})",
        hanzi_caret.x,
        initial_caret.x
    );

    // Verify candidate window positioning tracks caret progression
    runtime.sync_ime_cursor_location(&mut mock_window);
    let hanzi_loc = mock_window
        .ime_cursor_location
        .expect("platform window must track updated caret");
    assert!(hanzi_loc.0 > initial_loc.0);
    assert_eq!(
        mock_window.ime_surrounding,
        Some(("你好世界".to_string(), 4))
    );

    // 8. Japanese Kanji commit ("日本語")
    let commit_ja = PlatformEvent::ImeCommit("日本語".to_string());
    runtime.step_event(&commit_ja);
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "你好世界日本語"
    );

    // 9. Korean Hangul commit ("한글")
    let commit_ko = PlatformEvent::ImeCommit("한글".to_string());
    runtime.step_event(&commit_ko);
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "你好世界日本語한글"
    );
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.cursor_index.get(),
        9 // 4 Hanzi + 3 Kanji + 2 Hangul
    );

    // 10. Blur / click outside resets IME candidate location
    runtime.dispatch_pointer_down(750.0, 550.0);
    assert!(runtime.active_focus.is_none());
    runtime.sync_ime_cursor_location(&mut mock_window);
    assert_eq!(mock_window.ime_cursor_location, None);
}
