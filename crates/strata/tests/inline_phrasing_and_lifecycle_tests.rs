use strata::prelude::*;
use strata_dom::View;

#[component]
fn HoverBoxApp() -> View {
    rsx! {
        <div class="flex flex-col w-[600px] h-[400px] bg-black p-4">
            <div id="scroll-box" class="w-[300px] h-[150px] overflow-y-auto bg-neutral-900 mb-4">
                <div class="w-full h-[500px] p-2">
                    <p class="text-white">"Scrollable content with multiple long paragraphs."</p>
                </div>
            </div>
            <button id="hover-btn" class="w-[120px] h-[40px] bg-neutral-800 hover:bg-blue-600 text-white">
                "Button"
            </button>
            <div id="static-text-box" class="w-[300px] p-2 bg-neutral-950 mt-2">
                <p class="text-neutral-400">"Static text without any hover classes."</p>
            </div>
        </div>
    }
}

#[test]
fn test_scrollbar_drag_pointer_capture_isolates_hover() {
    let view = HoverBoxApp();
    let mut runtime = AppRuntime::new(view, 600.0, 400.0, 1.0);

    // Verify active scrollbar exists for scroll-box
    assert!(!runtime.active_scrollbars.is_empty(), "Active scrollbar must be discovered");
    let bar = runtime.active_scrollbars[0].clone();
    assert!(bar.max_scroll_y > 10.0, "Container must have overflow travel");

    // Press down on scrollbar thumb
    let thumb_center_x = bar.thumb_bounds.x + (bar.thumb_bounds.width / 2.0);
    let thumb_center_y = bar.thumb_bounds.y + (bar.thumb_bounds.height / 2.0);
    let pressed = runtime.check_scrollbar_thumb_press(thumb_center_x, thumb_center_y);
    assert!(pressed, "Must start scrollbar drag on thumb press");
    assert!(runtime.scrollbar_drag.is_some(), "Drag state must be active");

    // Hover path before drag
    let initial_hover = runtime.current_hover_path.clone();

    // While dragging thumb, move pointer across the button (which has hover:bg-blue-600)
    // In our event handler contract: when scrollbar_drag is active, poll_scrollbar_drag handles motion
    // and update_hover is completely bypassed.
    let drag_moved = runtime.poll_scrollbar_drag(thumb_center_x, thumb_center_y + 30.0);
    assert!(drag_moved, "Poll scrollbar drag must translate scroll offset");

    // Verify hover path remained unchanged (isolated by pointer capture)
    assert_eq!(
        runtime.current_hover_path, initial_hover,
        "Drag capture must prevent hover updates on underlying elements"
    );

    // Release drag
    runtime.scrollbar_drag = None;
    assert!(runtime.scrollbar_drag.is_none());
}

#[test]
fn test_static_element_hover_does_not_trigger_rebuild() {
    let view = HoverBoxApp();
    let mut runtime = AppRuntime::new(view, 600.0, 400.0, 1.0);

    // Move pointer onto static text element (inside static-text-box)
    // static-text-box has NO hover styles or boundary event listeners
    let dirty_static = runtime.update_hover(50.0, 230.0);
    assert!(
        !dirty_static,
        "Moving over static text without hover styles must NOT trigger layout rebuild"
    );

    // Now move pointer onto the interactive button (which has hover:bg-blue-600)
    let dirty_interactive = runtime.update_hover(50.0, 185.0);
    assert!(
        dirty_interactive,
        "Hovering element with hover: class must report dirty rebuild"
    );
}
