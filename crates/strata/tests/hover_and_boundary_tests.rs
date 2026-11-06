use std::cell::Cell;
use std::rc::Rc;
use strata::prelude::*;
use strata_dom::View;

#[component]
fn HoverButtonApp() -> View {
    rsx! {
        <div class="flex flex-col w-[400px] h-[300px] bg-black p-4">
            <button class="w-[120px] h-[40px] bg-neutral-900 hover:bg-neutral-800 text-white">
                "Hover Me"
            </button>
        </div>
    }
}

#[test]
fn test_hover_style_activation_and_reversion() {
    let view = HoverButtonApp();
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);

    // Initial state: button at (16, 16) with w=120, h=40
    let button_layout = &runtime.layout_root.children[0];
    let neutral_900 = strata_style::types::Color::new(23, 23, 23, 255);
    let neutral_800 = strata_style::types::Color::new(38, 38, 38, 255);

    assert_eq!(button_layout.style.background_color, Some(neutral_900));
    assert_eq!(runtime.current_hover_path, None);

    // Hover over button center (50.0, 30.0)
    let dirty = runtime.update_hover(50.0, 30.0);
    assert!(
        dirty,
        "Hovering over button must report dirty frame rebuild"
    );
    assert!(runtime.current_hover_path.is_some());

    let hovered_button = &runtime.layout_root.children[0];
    assert_eq!(
        hovered_button.style.background_color,
        Some(neutral_800),
        "Hovered button must activate hover:bg-neutral-800"
    );

    // Moving within the same button must not trigger rebuild
    let dirty_same = runtime.update_hover(60.0, 32.0);
    assert!(
        !dirty_same,
        "Moving within same target must not trigger rebuild"
    );

    // Move outside the button (e.g. 350.0, 250.0)
    let dirty_leave = runtime.update_hover(350.0, 250.0);
    assert!(
        dirty_leave,
        "Leaving button must report dirty frame rebuild"
    );

    let unhovered_button = &runtime.layout_root.children[0];
    assert_eq!(
        unhovered_button.style.background_color,
        Some(neutral_900),
        "Unhovered button must revert to base bg-neutral-900"
    );
}

#[component]
fn ColorHoverLinkApp() -> View {
    rsx! {
        <div class="w-[400px] p-4">
            <a class="text-[#0000ff] hover:text-[#ff0000]">"Colored Link"</a>
        </div>
    }
}

#[test]
fn test_color_only_hover_skips_layout_and_repaints_fragments() {
    fn collect_fragments(
        node: &strata::layout::LayoutNode,
        out: &mut Vec<strata_style::types::Color>,
    ) {
        for line in &node.line_boxes {
            for frag in &line.text_fragments {
                out.push(frag.style.color);
            }
        }
        for child in &node.children {
            collect_fragments(child, out);
        }
    }

    let view = ColorHoverLinkApp();
    let mut runtime = AppRuntime::new(view, 400.0, 200.0, 1.0);
    let before = runtime.layout_compute_count();
    let mut fragment_colors_before = Vec::new();
    collect_fragments(&runtime.layout_root, &mut fragment_colors_before);

    let dirty = runtime.update_hover(20.0, 20.0);
    assert!(dirty, "Entering a hover target must request a repaint");
    assert_eq!(
        runtime.layout_compute_count(),
        before,
        "Paint-only hover must not re-run compute_layout"
    );

    let mut fragment_colors_after = Vec::new();
    collect_fragments(&runtime.layout_root, &mut fragment_colors_after);
    assert!(
        !fragment_colors_before.is_empty(),
        "Inline text must produce at least one fragment"
    );
    assert_ne!(
        fragment_colors_before, fragment_colors_after,
        "Inline fragments must pick up the hover text color without relayout"
    );
    assert_eq!(
        fragment_colors_after.first().copied(),
        Some(strata_style::types::Color::new(255, 0, 0, 255))
    );
}

#[component]
fn BoundaryEventsApp(
    enter_count: Rc<Cell<usize>>,
    leave_count: Rc<Cell<usize>>,
    over_count: Rc<Cell<usize>>,
    out_count: Rc<Cell<usize>>,
) -> View {
    let on_enter = {
        let c = enter_count.clone();
        move |_: strata_dom::Event| c.set(c.get() + 1)
    };
    let on_leave = {
        let c = leave_count.clone();
        move |_: strata_dom::Event| c.set(c.get() + 1)
    };
    let on_over = {
        let c = over_count.clone();
        move |_: strata_dom::Event| c.set(c.get() + 1)
    };
    let on_out = {
        let c = out_count.clone();
        move |_: strata_dom::Event| c.set(c.get() + 1)
    };

    rsx! {
        <div class="flex flex-col w-[500px] h-[400px] bg-black">
            <button
                class="w-[100px] h-[50px] bg-neutral-900 hover:bg-neutral-800"
                onmouseenter=on_enter
                onmouseleave=on_leave
                onmouseover=on_over
                onmouseout=on_out
            >
                "Action"
            </button>
        </div>
    }
}

#[test]
fn test_hover_boundary_events_enter_and_leave() {
    let enter_count = Rc::new(Cell::new(0));
    let leave_count = Rc::new(Cell::new(0));
    let over_count = Rc::new(Cell::new(0));
    let out_count = Rc::new(Cell::new(0));

    let view = BoundaryEventsApp(
        enter_count.clone(),
        leave_count.clone(),
        over_count.clone(),
        out_count.clone(),
    );
    let mut runtime = AppRuntime::new(view, 500.0, 400.0, 1.0);

    assert_eq!(enter_count.get(), 0);
    assert_eq!(leave_count.get(), 0);
    assert_eq!(over_count.get(), 0);
    assert_eq!(out_count.get(), 0);

    // 1. Move pointer onto button (50.0, 25.0)
    runtime.update_hover(50.0, 25.0);
    assert_eq!(
        enter_count.get(),
        1,
        "mouseenter must fire once on entering button"
    );
    assert_eq!(
        over_count.get(),
        1,
        "mouseover must fire on entering button"
    );
    assert_eq!(leave_count.get(), 0);
    assert_eq!(out_count.get(), 0);

    // 2. Move pointer within button (60.0, 30.0) -> no redundant enter or leave
    runtime.update_hover(60.0, 30.0);
    assert_eq!(enter_count.get(), 1);
    assert_eq!(leave_count.get(), 0);

    // 3. Move pointer outside button (200.0, 200.0)
    runtime.update_hover(200.0, 200.0);
    assert_eq!(
        leave_count.get(),
        1,
        "mouseleave must fire once on leaving button"
    );
    assert_eq!(out_count.get(), 1, "mouseout must fire on leaving button");
    assert_eq!(enter_count.get(), 1);

    // 4. Move pointer back into button
    runtime.update_hover(50.0, 25.0);
    assert_eq!(
        enter_count.get(),
        2,
        "mouseenter must fire again on re-entering"
    );
    assert_eq!(over_count.get(), 2);
    assert_eq!(leave_count.get(), 1);

    // 5. Clear hover (window departure)
    let dirty = runtime.clear_hover();
    assert!(dirty);
    assert_eq!(
        leave_count.get(),
        2,
        "clear_hover must fire mouseleave on departure"
    );
    assert_eq!(out_count.get(), 2);
    assert_eq!(runtime.current_hover_path, None);
}

#[component]
fn AncestorHoverApp() -> View {
    rsx! {
        <div class="flex flex-row w-[500px] h-[300px] bg-black">
            <div class="flex flex-col w-[200px] h-[200px] bg-neutral-950 hover:bg-neutral-900 p-2">
                <button class="w-[100px] h-[50px] bg-neutral-800 hover:bg-neutral-700">
                    "Child Button"
                </button>
            </div>
            <div class="flex flex-col w-[200px] h-[200px] bg-neutral-950 hover:bg-neutral-900 p-2">
                <button class="w-[100px] h-[50px] bg-neutral-800 hover:bg-neutral-700">
                    "Sibling Button"
                </button>
            </div>
        </div>
    }
}

#[test]
fn test_ancestor_hover_propagation_and_sibling_isolation() {
    let view = AncestorHoverApp();
    let mut runtime = AppRuntime::new(view, 500.0, 300.0, 1.0);

    let neutral_950 = strata_style::types::Color::new(10, 10, 10, 255);
    let neutral_900 = strata_style::types::Color::new(23, 23, 23, 255);
    let neutral_800 = strata_style::types::Color::new(38, 38, 38, 255);
    let neutral_700 = strata_style::types::Color::new(64, 64, 64, 255);

    // Initial state: both containers and buttons are unhovered
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(neutral_950)
    );
    assert_eq!(
        runtime.layout_root.children[0].children[0]
            .style
            .background_color,
        Some(neutral_800)
    );
    assert_eq!(
        runtime.layout_root.children[1].style.background_color,
        Some(neutral_950)
    );
    assert_eq!(
        runtime.layout_root.children[1].children[0]
            .style
            .background_color,
        Some(neutral_800)
    );

    // Hover over child button inside container 0: position (50, 30)
    runtime.update_hover(50.0, 30.0);

    // Container 0 (ancestor of hovered button) must receive :hover
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(neutral_900),
        "Ancestor container must receive :hover when child is hovered"
    );
    // Button 0 (hover target) must receive :hover
    assert_eq!(
        runtime.layout_root.children[0].children[0]
            .style
            .background_color,
        Some(neutral_700),
        "Hovered button must receive :hover"
    );

    // Container 1 (sibling) must NOT receive :hover
    assert_eq!(
        runtime.layout_root.children[1].style.background_color,
        Some(neutral_950),
        "Sibling container must remain unhovered"
    );
    // Button 1 (sibling's child) must NOT receive :hover
    assert_eq!(
        runtime.layout_root.children[1].children[0]
            .style
            .background_color,
        Some(neutral_800),
        "Sibling button must remain unhovered"
    );
}
