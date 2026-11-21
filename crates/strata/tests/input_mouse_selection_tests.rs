//! Mouse selection, caret positioning, and native editing tests for the exact search-box
//! blueprint used by the browser chrome (rounded container, leading icon, text input).

use strata::layout::Rect;
use strata::prelude::*;
use strata::render::compositor::DrawCommand;
use strata_platform::{Modifiers, PlatformEvent};

const SEARCH_VALUE: &str = "Search engines";

#[component]
fn SearchInputSnippet() -> View {
    let query_str = SEARCH_VALUE.to_string();
    rsx! {
        <div class="p-2.5 pb-1.5 bg-[#1f1f1d] border-b border-[#3a3a36]">
            <div class="w-full relative flex items-center bg-[#232321] border border-[#3a3a36] rounded-lg px-3 py-2 cursor-text">
                <svg class="w-3.5 h-3.5 text-[#98958c] mr-2.5 shrink-0" viewBox="0 0 24 24" fill="none">
                    <circle cx="11" cy="11" r="8" stroke="currentColor" stroke-width="2" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                </svg>
                <input
                    type="text"
                    value={query_str.clone()}
                    placeholder="Search engines, tools, circuits..."
                    class="w-full bg-transparent text-xs text-[#ece9e1] placeholder-[#98958c] outline-none font-sans"
                />
            </div>
        </div>
    }
}

#[derive(Clone, Copy)]
struct InputMetrics {
    content_x: f32,
    content_y: f32,
    content_w: f32,
    content_h: f32,
    font_size: f32,
    line_height: f32,
    is_mono: bool,
    font_family: strata_primitives::AtomId,
}

fn input_metrics(runtime: &AppRuntime) -> InputMetrics {
    let mut counter = 0;
    let layout = AppRuntime::find_input_layout(
        &runtime.root_blueprint,
        &runtime.layout_root,
        0,
        None,
        &mut counter,
    )
    .expect("search box input must exist in the layout tree");
    InputMetrics {
        content_x: layout.geometry.content_box.x,
        content_y: layout.geometry.content_box.y,
        content_w: layout.geometry.content_box.width,
        content_h: layout.geometry.content_box.height,
        font_size: layout.style.font_size,
        line_height: layout.style.line_height,
        is_mono: strata::layout::is_monospace_family(layout.style.font_family),
        font_family: layout.style.font_family,
    }
}

fn text_start(metrics: &InputMetrics, chars: usize, text: &str) -> f32 {
    let prefix: String = text.chars().take(chars).collect();
    metrics.content_x
        + strata::layout::measure_text_bounds_with_family(
            &prefix,
            metrics.font_size,
            metrics.line_height,
            None,
            metrics.is_mono,
            metrics.font_family,
        )
        .0
}

fn caret_rect(runtime: &AppRuntime) -> Option<Rect> {
    runtime
        .display_list
        .commands
        .iter()
        .rev()
        .find_map(|command| {
            if let DrawCommand::DrawRect { bounds, .. } = command {
                if bounds.width <= 2.0 && bounds.height >= 8.0 {
                    return Some(*bounds);
                }
            }
            None
        })
}

fn text_y(metrics: &InputMetrics) -> f32 {
    metrics.content_y + metrics.content_h / 2.0
}

#[test]
fn test_container_padding_click_focuses_input_and_draws_caret() {
    let mut runtime = AppRuntime::new(SearchInputSnippet(), 640.0, 480.0, 1.0);
    let metrics = input_metrics(&runtime);
    let click_x = metrics.content_x - 12.0;
    let click_y = text_y(&metrics);

    assert!(runtime.dispatch_pointer_down(click_x, click_y));
    assert!(
        runtime.active_focus.is_some(),
        "clicking the rounded container padding must focus the child input"
    );

    runtime.tick_frame();
    let caret = caret_rect(&runtime).expect("a focused input must draw an insertion caret");
    assert!(caret.height >= 8.0, "caret must span the text line height");

    assert!(runtime.dispatch_pointer_up(click_x, click_y));
    assert_eq!(
        runtime
            .active_focus
            .as_ref()
            .unwrap()
            .state
            .selection_anchor
            .get(),
        None,
        "pointer up must clear a collapsed anchor"
    );
}

#[test]
fn test_click_positions_caret_within_text() {
    let mut runtime = AppRuntime::new(SearchInputSnippet(), 640.0, 480.0, 1.0);
    let metrics = input_metrics(&runtime);
    let total = SEARCH_VALUE.chars().count();
    let midpoint = total / 2;
    let click_x = text_start(&metrics, midpoint, SEARCH_VALUE) + 1.0;
    let click_y = text_y(&metrics);

    runtime.dispatch_pointer_down(click_x, click_y);
    let cursor = runtime
        .active_focus
        .as_ref()
        .expect("click must focus the input")
        .state
        .cursor_index
        .get();
    assert!(
        cursor > 0 && cursor < total,
        "cursor index {cursor} must land inside the text"
    );

    runtime.tick_frame();
    let expected_x = text_start(&metrics, cursor, SEARCH_VALUE);
    let caret = caret_rect(&runtime).expect("a focused input must draw an insertion caret");
    assert!(
        (caret.x - expected_x).abs() <= 2.0,
        "caret x {} must match the measured prefix width {}",
        caret.x,
        expected_x
    );
}

#[test]
fn test_drag_selects_first_five_characters() {
    let mut runtime = AppRuntime::new(SearchInputSnippet(), 640.0, 480.0, 1.0);
    let metrics = input_metrics(&runtime);
    let y = text_y(&metrics);
    let start_x = metrics.content_x + 0.5;
    let end_x = text_start(&metrics, 5, SEARCH_VALUE);

    runtime.dispatch_pointer_down(start_x, y);
    assert!(runtime.dispatch_pointer_move(end_x, y));
    assert!(runtime.dispatch_pointer_up(end_x, y));

    let state = &runtime.active_focus.as_ref().unwrap().state;
    assert_eq!(state.selection_range(), Some((0, 5)));
    assert_eq!(state.selected_text().as_deref(), Some("Searc"));
}

#[test]
fn test_double_click_selects_word() {
    let mut runtime = AppRuntime::new(SearchInputSnippet(), 640.0, 480.0, 1.0);
    let metrics = input_metrics(&runtime);
    let y = text_y(&metrics);
    let click_x = text_start(&metrics, 10, SEARCH_VALUE) + 1.0;

    runtime.dispatch_pointer_down(click_x, y);
    runtime.dispatch_pointer_down(click_x, y);

    let state = &runtime.active_focus.as_ref().unwrap().state;
    assert_eq!(state.selected_text().as_deref(), Some("engines"));
}

#[test]
fn test_triple_click_selects_line() {
    let mut runtime = AppRuntime::new(SearchInputSnippet(), 640.0, 480.0, 1.0);
    let metrics = input_metrics(&runtime);
    let y = text_y(&metrics);
    let click_x = text_start(&metrics, 2, SEARCH_VALUE) + 1.0;

    runtime.dispatch_pointer_down(click_x, y);
    runtime.dispatch_pointer_down(click_x, y);
    runtime.dispatch_pointer_down(click_x, y);

    let state = &runtime.active_focus.as_ref().unwrap().state;
    assert_eq!(
        state.selected_text().as_deref(),
        Some(SEARCH_VALUE),
        "triple click in a single-line input selects the whole line"
    );
}

#[test]
fn test_shift_click_extends_selection() {
    let mut runtime = AppRuntime::new(SearchInputSnippet(), 640.0, 480.0, 1.0);
    let metrics = input_metrics(&runtime);
    let y = text_y(&metrics);
    let x8 = text_start(&metrics, 8, SEARCH_VALUE) + 0.5;
    let x3 = text_start(&metrics, 3, SEARCH_VALUE) + 0.5;

    runtime.dispatch_pointer_down(x8, y);
    assert!(runtime.dispatch_pointer_down_with_modifiers(
        x3,
        y,
        Modifiers {
            shift: true,
            ..Default::default()
        }
    ));

    let state = &runtime.active_focus.as_ref().unwrap().state;
    assert_eq!(state.selection_range(), Some((3, 8)));
    assert_eq!(state.selected_text().as_deref(), Some("rch e"));
}

#[test]
fn test_typing_after_focus_edits_without_oninput_handler() {
    let mut runtime = AppRuntime::new(SearchInputSnippet(), 640.0, 480.0, 1.0);
    let metrics = input_metrics(&runtime);
    let y = text_y(&metrics);
    let end_x = metrics.content_x + metrics.content_w - 1.0;

    runtime.dispatch_pointer_down(end_x, y);
    for ch in "abc".chars() {
        runtime.step_event(&PlatformEvent::ReceivedCharacter(ch));
    }

    let text = runtime.active_focus.as_ref().unwrap().state.text.get();
    assert_eq!(text, format!("{}abc", SEARCH_VALUE));
}

#[test]
fn test_single_line_input_nowrap_clip_and_caret_follow_scroll_x() {
    const LONG_ENDPOINT: &str = "127.0.0.1:57600/api/v1/telemetry/stream";
    let mut runtime = AppRuntime::new(
        rsx! {
            <div class="w-[400px] h-[160px] p-4 bg-[#18181b]">
                <input
                    id="narrow-endpoint"
                    type="text"
                    value={LONG_ENDPOINT.to_string()}
                    class="w-[100px] text-xs outline-none bg-[#27272a] text-white"
                />
            </div>
        },
        400.0,
        160.0,
        1.0,
    );

    let mut counter = 0;
    let layout = AppRuntime::find_input_layout(
        &runtime.root_blueprint,
        &runtime.layout_root,
        0,
        None,
        &mut counter,
    )
    .expect("narrow-endpoint input must exist");
    assert_eq!(
        layout.style.white_space,
        strata::style::types::WhiteSpace::NoWrap,
        "single-line <input> must enforce WhiteSpace::NoWrap"
    );
    assert_eq!(
        layout.style.overflow_x,
        strata::style::types::Overflow::Hidden,
        "single-line <input> must enforce Overflow::Hidden horizontally"
    );
    assert_eq!(
        layout.style.overflow_y,
        strata::style::types::Overflow::Hidden,
        "single-line <input> must enforce Overflow::Hidden vertically"
    );
    let line_px = layout.style.font_size * layout.style.line_height;
    assert!(
        layout.geometry.content_box.height <= line_px * 1.5,
        "single-line <input> height {} must not wrap onto line 2 (line_px {})",
        layout.geometry.content_box.height,
        line_px
    );

    let metrics = input_metrics(&runtime);
    let y = text_y(&metrics);

    runtime.compile_display_list();
    let text_cmd_idx = runtime
        .display_list
        .commands
        .iter()
        .position(|cmd| matches!(cmd, DrawCommand::DrawText { text, .. } if text == LONG_ENDPOINT))
        .expect("input text command must be emitted in display list");
    match &runtime.display_list.commands[text_cmd_idx] {
        DrawCommand::DrawText { bounds_width, .. } => {
            assert_eq!(
                *bounds_width, None,
                "single-line <input> DrawText must use bounds_width: None to prevent wrapping"
            );
        }
        _ => unreachable!(),
    }
    assert!(
        text_cmd_idx > 0 && text_cmd_idx + 1 < runtime.display_list.commands.len(),
        "input DrawText must be bracketed by PushClip and PopClip"
    );
    assert!(
        matches!(
            runtime.display_list.commands[text_cmd_idx - 1],
            DrawCommand::PushClip { bounds, .. }
                if (bounds.x - metrics.content_x).abs() < 1.0
                    && (bounds.width - metrics.content_w).abs() < 1.0
        ),
        "expected PushClip matching input content_box before DrawText"
    );
    assert!(
        matches!(
            runtime.display_list.commands[text_cmd_idx + 1],
            DrawCommand::PopClip
        ),
        "expected PopClip after input DrawText"
    );

    // Focus and jump to End -> scroll_x must advance and keep caret inside visible box
    runtime.dispatch_pointer_down(metrics.content_x + 8.0, y);
    runtime.dispatch_pointer_up(metrics.content_x + 8.0, y);
    runtime.step_event(&PlatformEvent::KeyDown {
        key: strata_platform::KeyCode::End,
        modifiers: Modifiers::default(),
    });
    runtime.tick_frame();

    let scroll_after_end = runtime
        .active_focus
        .as_ref()
        .expect("focused input")
        .state
        .scroll_x
        .get();
    assert!(
        scroll_after_end > 5.0,
        "pressing End in overflowing single-line input must advance horizontal scroll_x (got {scroll_after_end})"
    );
    let end_caret = caret_rect(&runtime).expect("caret must render when focused at End");
    assert!(
        end_caret.x >= metrics.content_x - 1.5
            && end_caret.x <= metrics.content_x + metrics.content_w + 1.5,
        "scrolled caret x {} must remain inside content_box [{}..{}]",
        end_caret.x,
        metrics.content_x,
        metrics.content_x + metrics.content_w
    );

    // Appending a character keeps caret visible within the clip box
    runtime.step_event(&PlatformEvent::ReceivedCharacter('9'));
    runtime.tick_frame();
    let focus_after_type = runtime.active_focus.as_ref().unwrap();
    assert_eq!(focus_after_type.state.text.get(), format!("{LONG_ENDPOINT}9"));
    assert!(focus_after_type.state.scroll_x.get() >= scroll_after_end);
    let typed_caret = caret_rect(&runtime).expect("caret after typing");
    assert!(
        typed_caret.x >= metrics.content_x - 1.5
            && typed_caret.x <= metrics.content_x + metrics.content_w + 1.5,
        "caret after typing ({}) must remain visible inside input box [{}..{}]",
        typed_caret.x,
        metrics.content_x,
        metrics.content_x + metrics.content_w
    );

    // Clicking inside the horizontally scrolled box must hit-test into the scrolled tail
    let mid_click_x = metrics.content_x + metrics.content_w * 0.5;
    runtime.dispatch_pointer_down(mid_click_x, y);
    runtime.dispatch_pointer_up(mid_click_x, y);
    let scrolled_click_cursor = runtime
        .active_focus
        .as_ref()
        .unwrap()
        .state
        .cursor_index
        .get();
    assert!(
        scrolled_click_cursor > 10,
        "click at visible midpoint while scrolled must land in visible suffix, got cursor {scrolled_click_cursor}"
    );

    // Pressing Home resets horizontal scroll_x back to 0.0
    runtime.step_event(&PlatformEvent::KeyDown {
        key: strata_platform::KeyCode::Home,
        modifiers: Modifiers::default(),
    });
    runtime.tick_frame();
    let focus_home = runtime.active_focus.as_ref().unwrap();
    assert_eq!(focus_home.state.cursor_index.get(), 0);
    assert!(
        focus_home.state.scroll_x.get().abs() < 0.5,
        "pressing Home must scroll back to start, got scroll_x={}",
        focus_home.state.scroll_x.get()
    );

    // Clicking outside clears focus cleanly
    runtime.dispatch_pointer_down(350.0, 140.0);
    runtime.dispatch_pointer_up(350.0, 140.0);
    assert!(runtime.active_focus.is_none());
}

