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
