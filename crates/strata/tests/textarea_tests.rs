use strata::layout::Rect;
use strata::prelude::*;
use strata::render::compositor::DrawCommand;
use strata_platform::{KeyCode, Modifiers, PlatformEvent};

#[component]
fn TextareaSnippet() -> View {
    let text_sig = use_signal("alpha beta\ngamma delta".to_string());
    let current = text_sig.get();
    let onchange_sig = text_sig.clone();

    rsx! {
        <div class="p-4 flex flex-col gap-2 bg-neutral-950 w-[360px]">
            <textarea
                id="notes-box"
                rows="3"
                class="w-full p-2 text-sm text-white bg-neutral-900 border border-neutral-700 font-mono"
                placeholder="Type notes..."
                oninput={move |val: String| onchange_sig.set(val)}
            >
                {current}
            </textarea>
        </div>
    }
}

#[component]
fn ScrollingTextareaSnippet() -> View {
    rsx! {
        <div class="p-4 bg-neutral-950 w-[320px]">
            <textarea
                id="scroll-box"
                rows="2"
                class="w-full p-2 text-sm text-white bg-neutral-900 font-mono"
                value={"line1\nline2\nline3\nline4\nline5"}
            />
        </div>
    }
}

fn find_textarea_layout<'a>(runtime: &'a AppRuntime) -> &'a strata::layout::LayoutNode {
    let mut counter = 0;
    AppRuntime::find_input_layout(
        &runtime.root_blueprint,
        &runtime.layout_root,
        0,
        None,
        &mut counter,
    )
    .expect("textarea layout node must exist")
}

fn find_caret_rect(runtime: &AppRuntime) -> Option<Rect> {
    runtime.display_list.commands.iter().rev().find_map(|cmd| {
        if let DrawCommand::DrawRect { bounds, .. } = cmd {
            if bounds.width <= 2.5 && bounds.height >= 8.0 {
                return Some(*bounds);
            }
        }
        None
    })
}

fn selection_rects(runtime: &AppRuntime) -> Vec<Rect> {
    runtime
        .display_list
        .commands
        .iter()
        .filter_map(|cmd| {
            if let DrawCommand::DrawRect { bounds, color } = cmd {
                if color.r == 59 && color.g == 130 && color.b == 246 && color.a == 120 {
                    return Some(*bounds);
                }
            }
            None
        })
        .collect()
}

fn draw_text_commands(runtime: &AppRuntime) -> Vec<(String, f32, f32, Option<f32>, Option<f32>)> {
    runtime
        .display_list
        .commands
        .iter()
        .filter_map(|cmd| {
            if let DrawCommand::DrawText {
                text,
                origin,
                bounds_width,
                bounds_height,
                ..
            } = cmd
            {
                Some((text.clone(), origin.0, origin.1, *bounds_width, *bounds_height))
            } else {
                None
            }
        })
        .collect()
}

fn send_key(runtime: &mut AppRuntime, key: KeyCode, shift: bool, ctrl: bool) {
    let _ = runtime.dispatch_key_event(&PlatformEvent::KeyDown {
        key,
        modifiers: Modifiers {
            shift,
            ctrl,
            alt: false,
            meta: false,
        },
    });
    runtime.compile_display_list();
}

#[test]
fn test_textarea_renders_text_sizes_from_rows_and_exposes_to_mcp() {
    let runtime = AppRuntime::new(TextareaSnippet(), 640.0, 480.0, 1.0);
    let layout = find_textarea_layout(&runtime);

    assert_eq!(layout.text.as_deref(), Some("alpha beta\ngamma delta"));
    assert_eq!(
        layout.style.white_space,
        strata::style::types::WhiteSpace::PreWrap
    );
    let expected_min_h = layout.style.font_size * layout.style.line_height * 3.0;
    assert!(
        layout.geometry.content_box.height >= expected_min_h - 1.0,
        "textarea height {} must accommodate 3 rows ({})",
        layout.geometry.content_box.height,
        expected_min_h
    );

    let texts = draw_text_commands(&runtime);
    assert!(
        texts.iter().any(|(t, _, _, bw, bh)| {
            t == "alpha beta\ngamma delta" && bw.is_some() && bh.is_none()
        }),
        "textarea DrawText must wrap at bounds_width and top-align (bounds_height == None): {:?}",
        texts
    );

    let center_x = layout.geometry.border_box.x + layout.geometry.border_box.width / 2.0;
    let center_y = layout.geometry.border_box.y + layout.geometry.border_box.height / 2.0;
    let (tag, _, _, text, _) = runtime.mcp_inspect_element(center_x, center_y);
    assert_eq!(tag, "textarea");
    assert_eq!(text, "alpha beta\ngamma delta");
}

#[test]
fn test_textarea_enter_arrows_selection_and_mcp_live_text() {
    let mut runtime = AppRuntime::new(TextareaSnippet(), 640.0, 480.0, 1.0);
    let (bx, by, cx, cy, fs, lh) = {
        let l = find_textarea_layout(&runtime);
        (
            l.geometry.border_box.x,
            l.geometry.border_box.y,
            l.geometry.content_box.x,
            l.geometry.content_box.y,
            l.style.font_size,
            l.style.line_height,
        )
    };
    let line_step = fs * lh;

    // Click near start of line 2 ("gamma delta")
    let click_x = cx + 1.0;
    let click_y = cy + line_step + line_step * 0.5;
    runtime.dispatch_pointer_down_action(click_x, click_y);
    runtime.dispatch_pointer_up_action(click_x, click_y);
    runtime.compile_display_list();

    let active = runtime.active_focus.as_ref().expect("textarea focused");
    assert!(active.is_multiline);
    let initial_line2_pos = active.state.cursor_index.get();
    assert!(
        initial_line2_pos >= "alpha beta\n".chars().count(),
        "click on second visual row should land on line 2, got cursor={initial_line2_pos}"
    );

    // ArrowUp moves back up to line 1
    send_key(&mut runtime, KeyCode::ArrowUp, false, false);
    let after_up = runtime.active_focus.as_ref().unwrap().state.cursor_index.get();
    assert!(
        after_up < "alpha beta\n".chars().count(),
        "ArrowUp should move to line 1, got cursor={after_up}"
    );

    // Move to line end and press Enter to insert newline (not submit)
    send_key(&mut runtime, KeyCode::End, false, false);
    send_key(&mut runtime, KeyCode::Enter, false, false);
    let _ = runtime.dispatch_key_event(&PlatformEvent::ReceivedCharacter('X'));
    runtime.compile_display_list();

    let updated = runtime.active_focus.as_ref().unwrap().state.text.get();
    assert_eq!(updated, "alpha beta\nX\ngamma delta");

    // Drag-select from line 1 to line 3 produces multi-line highlight rects
    runtime.dispatch_pointer_down_action(cx + 2.0, cy + 4.0);
    runtime.dispatch_pointer_move_action(cx + 40.0, cy + line_step * 2.0 + 4.0);
    runtime.dispatch_pointer_up_action(cx + 40.0, cy + line_step * 2.0 + 4.0);
    runtime.compile_display_list();

    let sels = selection_rects(&runtime);
    assert!(
        sels.len() >= 2,
        "multiline drag should highlight multiple visual lines, got {:?}",
        sels
    );

    // Inspect element returns live edited text even before blur
    let (tag, _, _, live_text, _) = runtime.mcp_inspect_element(bx + 20.0, by + 20.0);
    assert_eq!(tag, "textarea");
    assert_eq!(live_text, "alpha beta\nX\ngamma delta");
}

#[test]
fn test_textarea_vertical_scroll_keeps_fixed_height_and_shifts_draw_origin() {
    let mut runtime = AppRuntime::new(ScrollingTextareaSnippet(), 640.0, 480.0, 1.0);
    let (cx, cy, ch) = {
        let l = find_textarea_layout(&runtime);
        (
            l.geometry.content_box.x,
            l.geometry.content_box.y,
            l.geometry.content_box.height,
        )
    };

    // Focus top line
    runtime.dispatch_pointer_down_action(cx + 2.0, cy + 4.0);
    runtime.dispatch_pointer_up_action(cx + 2.0, cy + 4.0);
    runtime.compile_display_list();
    assert_eq!(runtime.active_focus.as_ref().unwrap().state.scroll_y.get(), 0.0);

    // Jump to last line (line 5) in a 2-row textarea -> should scroll_y > 0
    for _ in 0..4 {
        send_key(&mut runtime, KeyCode::ArrowDown, false, false);
    }

    let scroll_y = runtime.active_focus.as_ref().unwrap().state.scroll_y.get();
    assert!(
        scroll_y > 0.0,
        "moving to line 5 inside 2-row textarea must scroll vertically, got scroll_y={scroll_y}"
    );

    let after_h = find_textarea_layout(&runtime).geometry.content_box.height;
    assert!(
        (after_h - ch).abs() < 0.01,
        "textarea content_box.height must remain constrained to rows height ({ch}), got {after_h}"
    );

    let texts = draw_text_commands(&runtime);
    assert!(
        texts.iter().any(|(t, _, oy, _, _)| t.starts_with("line1") && (*oy - (cy - scroll_y)).abs() < 0.5),
        "DrawText origin.y should be shifted by -scroll_y ({}), got {:?}",
        scroll_y,
        texts
    );

    let caret = find_caret_rect(&runtime).expect("caret rect should be drawn");
    assert!(
        caret.y >= cy - 1.0 && caret.y + caret.height <= cy + ch + 1.0,
        "scrolled caret {:?} must remain within content_box vertical bounds [{}, {}]",
        caret,
        cy,
        cy + ch
    );
}

#[component]
fn NestedScrollingTextareaSnippet() -> View {
    let long_text = (1..=14)
        .map(|i| format!("row {:02} payload text", i))
        .collect::<Vec<_>>()
        .join("\n");
    rsx! {
        <div
            id="outer-scroll"
            class="w-[320px] h-[240px] overflow-y-auto bg-[#0f172a] p-2"
        >
            <div class="w-full h-[140px] bg-[#1e293b]">
                <textarea
                    id="notes-area"
                    rows="3"
                    value={long_text}
                    class="w-[260px] h-[90px] bg-[#0b1220] text-[#f1f5f9] text-sm p-2"
                />
            </div>
            <div class="w-full h-[360px] bg-[#1e293b]" />
        </div>
    }
}

#[test]
fn test_textarea_scrollbar_wheel_scroll_parent_chaining_and_no_caret_snapback() {
    let mut runtime = AppRuntime::new(NestedScrollingTextareaSnippet(), 640.0, 480.0, 1.0);
    let ta_box = find_textarea_layout(&runtime).geometry.border_box;
    let cx = ta_box.x + ta_box.width * 0.5;
    let cy = ta_box.y + ta_box.height * 0.5;

    // 1. Overflowing <textarea> defaults to Overflow::Auto and emits an interactive scrollbar.
    let ta_bar = runtime
        .active_scrollbars
        .iter()
        .find(|b| b.id == "notes-area")
        .expect("overflowing <textarea> must emit an ActiveScrollbar");
    let max_ta_scroll = ta_bar.max_scroll_y;
    assert!(
        max_ta_scroll > 80.0,
        "14-line textarea in 90px box must have substantial max_scroll_y, got {max_ta_scroll}"
    );

    // 2. Focus line 1 (caret at top, scroll_y == 0) then wheel-scroll downward while hovered.
    runtime.dispatch_pointer_down_action(ta_box.x + 10.0, ta_box.y + 12.0);
    runtime.dispatch_pointer_up_action(ta_box.x + 10.0, ta_box.y + 12.0);
    runtime.compile_display_list();
    assert_eq!(runtime.active_focus.as_ref().unwrap().state.scroll_y.get(), 0.0);

    let (_, changed) = runtime.dispatch_smooth_scroll_action(cx, cy, 0.0, 80.0);
    assert!(changed, "wheel over focused textarea must initiate scroll");
    let settle_t = std::time::Instant::now() + std::time::Duration::from_millis(500);
    let _ = runtime.poll_smooth_scroll_at(settle_t);
    runtime.compile_display_list();

    let wheel_scrolled_y = runtime.active_focus.as_ref().unwrap().state.scroll_y.get();
    assert!(
        (wheel_scrolled_y - 80.0).abs() < 2.0,
        "wheel scroll must advance textarea scroll_y to ~80px, got {wheel_scrolled_y}"
    );
    assert!(
        (find_textarea_layout(&runtime).geometry.scroll_offset.y - wheel_scrolled_y).abs() < 0.5,
        "layout.geometry.scroll_offset.y must match active.state.scroll_y"
    );
    assert_eq!(
        runtime.scroll_offset_by_id("outer-scroll").1,
        0.0,
        "parent scroll container must NOT scroll while textarea absorbs wheel delta"
    );

    // 3. Per-frame caret blink/paint and unrelated update_view must NOT snap scroll_y back to caret!
    for _ in 0..4 {
        runtime.compile_display_list();
    }
    assert!(
        (runtime.active_focus.as_ref().unwrap().state.scroll_y.get() - wheel_scrolled_y).abs()
            < 0.5,
        "repeated compile_display_list / render_active_input_caret must NOT snap scroll_y back"
    );
    runtime.update_view(NestedScrollingTextareaSnippet());
    assert!(
        (runtime.active_focus.as_ref().unwrap().state.scroll_y.get() - wheel_scrolled_y).abs()
            < 0.5,
        "unrelated update_view must preserve manual wheel scroll_y on focused textarea"
    );
    assert!(
        (find_textarea_layout(&runtime).geometry.scroll_offset.y - wheel_scrolled_y).abs() < 0.5,
        "layout scroll_offset.y must remain preserved across update_view"
    );

    // 4. Wheel-scrolling all the way to the bottom and then scrolling further chains to #outer-scroll.
    let (_, changed_bot) = runtime.dispatch_smooth_scroll_action(cx, cy, 0.0, max_ta_scroll + 200.0);
    assert!(changed_bot);
    let _ = runtime.poll_smooth_scroll_at(std::time::Instant::now() + std::time::Duration::from_millis(500));
    let at_bottom = runtime.active_focus.as_ref().unwrap().state.scroll_y.get();
    assert!(
        (at_bottom - max_ta_scroll).abs() < 1.0,
        "textarea must reach bottom bound {max_ta_scroll}, got {at_bottom}"
    );

    let (_, chained) = runtime.dispatch_smooth_scroll_action(cx, cy, 0.0, 90.0);
    assert!(chained, "wheel past textarea bottom bound must chain to parent scroll container");
    let _ = runtime.poll_smooth_scroll_at(std::time::Instant::now() + std::time::Duration::from_millis(500));
    assert!(
        runtime.scroll_offset_by_id("outer-scroll").1 > 40.0,
        "parent #outer-scroll must scroll once textarea hits bottom bound, got {}",
        runtime.scroll_offset_by_id("outer-scroll").1
    );

    // 5. Typing/ArrowDown when caret is off-screen at line 1 DOES bring the caret back into view!
    send_key(&mut runtime, KeyCode::ArrowDown, false, false);
    let after_nav_y = runtime.active_focus.as_ref().unwrap().state.scroll_y.get();
    assert!(
        after_nav_y < 40.0,
        "ArrowDown from line 1 must scroll caret back into view, got scroll_y={after_nav_y}"
    );
    assert!(
        (runtime.scroll_offset_by_id("notes-area").1 - after_nav_y).abs() < 0.5,
        "scroll_offsets_by_id must stay synchronized after caret scroll-into-view"
    );

    // 6. Multiline DrawText DrawCommand::bounds() must span all 14 lines (never hardcoded 1 line).
    runtime.compile_display_list();
    let text_cmd = runtime
        .display_list
        .commands()
        .iter()
        .find(|cmd| {
            matches!(
                cmd,
                strata::render::compositor::DrawCommand::DrawText { text, .. }
                    if text.contains("row 14")
            )
        })
        .expect("multiline DrawText command present");
    let b = text_cmd.bounds().expect("DrawText bounds");
    assert!(
        b.height > 180.0,
        "14-line DrawText bounds.height must cover all lines (>180px), got {:?}",
        b
    );
}
