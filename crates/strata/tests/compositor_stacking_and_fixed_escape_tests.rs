//! Regression tests for:
//! 1. Bug 1: `strata_render::Compositor` hoisting descendant stacking contexts across
//!    non-stacking-context wrappers (`<section class="flex flex-col">`, `<div class="relative">`)
//!    so paint order and Cairo pixels match `hit_test.rs`.
//! 2. Bug 2: `overflow != Visible` NOT establishing an atomic stacking context, flex/grid items
//!    with `z-index` establishing a stacking context (CSS Flexbox §4.3 / Grid §5.4), and strict
//!    parity between `Compositor` and `AppRuntime`.
//! 3. Bug 3: `position: fixed` descendants inside `overflow-y-auto` scroll containers escaping
//!    ancestor `PushClip`, scroll translation, viewport culling, and hit-test clipping unless an
//!    ancestor has `transform`, `filter`, or `backdrop-filter`.

use std::cell::Cell;
use std::rc::Rc;
use strata::prelude::*;
use strata_dom::Event;
use strata_render::Compositor;
use strata_style::types::Display;

#[cfg(target_os = "linux")]
fn render_to_argb_pixels(runtime: &mut AppRuntime, w: i32, h: i32) -> Vec<u8> {
    runtime.compile_display_list();
    let mut surface =
        strata_platform::cairo::ImageSurface::create(strata_platform::cairo::Format::ARgb32, w, h)
            .expect("Cairo ARgb32 surface");
    {
        let cr = strata_platform::cairo::Context::new(&surface).expect("Cairo context");
        cr.set_source_rgb(0.0, 0.0, 0.0);
        let _ = cr.paint();
        strata_platform::render_display_list_to_cairo(&cr, &runtime.display_list);
    }
    surface.flush();
    let data = surface.data().expect("Cairo surface data");
    data.to_vec()
}

/// Reads `(r, g, b)` at `(x, y)` from a little-endian Cairo ARgb32 buffer (`[B, G, R, A]`).
#[cfg(target_os = "linux")]
fn pixel_rgb(buf: &[u8], stride_w: usize, x: usize, y: usize) -> (u8, u8, u8) {
    let idx = (y * stride_w + x) * 4;
    (buf[idx + 2], buf[idx + 1], buf[idx])
}

#[test]
fn bug1_compositor_hoists_card1_z50_dropdown_above_card2_and_section2() {
    let menu_clicks = Rc::new(Cell::new(0usize));
    let card2_clicks = Rc::new(Cell::new(0usize));
    let section2_clicks = Rc::new(Cell::new(0usize));

    let menu_cb = Rc::clone(&menu_clicks);
    let card2_cb = Rc::clone(&card2_clicks);
    let section2_cb = Rc::clone(&section2_clicks);

    // Mirrors Aerovex Toolbox structure:
    // Section #1 (<section class="flex flex-col">) -> Card #1 (<div class="relative">) ->
    //   Dropdown (<div class="absolute right-2 top-8 z-50">) painted in bright red (#ef4444),
    // followed by sibling Card #2 (Kortex, #2563eb blue) and Section #2 (Phonon, #10b981 green).
    let view = rsx! {
        <div class="flex flex-col w-[360px] h-[300px] bg-[#09090b]">
            <section class="flex flex-col w-full">
                <div class="relative w-full h-[48px] bg-[#18181b]">
                    <span>"Card 1"</span>
                    <div class="absolute right-2 top-8 z-50">
                        <button
                            class="w-[200px] h-[110px] bg-[#ef4444]"
                            onclick={move |_e: Event| menu_cb.set(menu_cb.get() + 1)}
                        >
                            "Uninstall"
                        </button>
                    </div>
                </div>
                <div
                    class="relative w-full h-[48px] bg-[#2563eb]"
                    onclick={move |_e: Event| card2_cb.set(card2_cb.get() + 1)}
                >
                    <span>"Card 2 (Kortex)"</span>
                </div>
            </section>
            <section class="flex flex-col w-full">
                <div
                    class="relative w-full h-[60px] bg-[#10b981]"
                    onclick={move |_e: Event| section2_cb.set(section2_cb.get() + 1)}
                >
                    <span>"Section 2 (Phonon)"</span>
                </div>
            </section>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 360.0, 300.0, 1.0);

    // Dropdown spans x = 360 - 8 - 200 = 152..352, y = 32..142.
    // Card #2 spans y = 48..96 (#2563eb = rgb(37, 99, 235)).
    // Section #2 spans y = 96..156 (#10b981 = rgb(16, 185, 129)).
    // Overlap points:
    // - (250, 70): overlaps Card #1's z-50 menu AND Card #2
    // - (250, 120): overlaps Card #1's z-50 menu AND Section #2
    assert!(runtime.dispatch_mouse_click(250.0, 70.0, 0, 1));
    assert_eq!(menu_clicks.get(), 1);
    assert_eq!(card2_clicks.get(), 0);

    assert!(runtime.dispatch_mouse_click(250.0, 120.0, 0, 1));
    assert_eq!(menu_clicks.get(), 2);
    assert_eq!(section2_clicks.get(), 0);

    #[cfg(target_os = "linux")]
    {
        let pixels = render_to_argb_pixels(&mut runtime, 360, 300);
        // Over Card #2 inside dropdown -> must be painted red (#ef4444 = 239, 68, 68), NOT blue!
        let p_over_card2 = pixel_rgb(&pixels, 360, 250, 70);
        assert!(
            p_over_card2.0 > 200 && p_over_card2.2 < 100,
            "Card #1 z-50 dropdown must paint ABOVE Card #2, got RGB {:?}",
            p_over_card2
        );
        // Over Section #2 inside dropdown -> must be painted red (#ef4444), NOT green!
        let p_over_sec2 = pixel_rgb(&pixels, 360, 250, 120);
        assert!(
            p_over_sec2.0 > 200 && p_over_sec2.1 < 100,
            "Card #1 z-50 dropdown must paint ABOVE Section #2, got RGB {:?}",
            p_over_sec2
        );
        // Left of dropdown at x=40 -> Card #2 blue and Section #2 green remain visible
        let p_card2_left = pixel_rgb(&pixels, 360, 40, 70);
        assert!(
            p_card2_left.2 > 200,
            "Card #2 visible outside menu, got {:?}",
            p_card2_left
        );
        let p_sec2_left = pixel_rgb(&pixels, 360, 40, 120);
        assert!(
            p_sec2_left.1 > 150,
            "Section #2 visible outside menu, got {:?}",
            p_sec2_left
        );
    }
}

#[test]
fn bug2_overflow_does_not_create_stacking_context_and_flex_grid_z_index_matches_spec() {
    let z50_clicks = Rc::new(Cell::new(0usize));
    let later_clicks = Rc::new(Cell::new(0usize));
    let z50_cb = Rc::clone(&z50_clicks);
    let later_cb = Rc::clone(&later_clicks);

    // Wrapper #1 has `overflow-y-auto` (Static, no z-index, no transform/opacity):
    // Per CSS 2.1 Appendix E / CSS Overflow Level 3, it clips Static/Relative children,
    // NOT an atomic stacking context! An `absolute` child inside it whose containing block
    // is the outer `relative` root escapes BOTH the clip and hoists `z-50` above Wrapper #2 (`z-10`).
    let view = rsx! {
        <div class="relative flex flex-col w-[360px] h-[300px]">
            <div class="w-full h-[80px] overflow-y-auto">
                <div
                    class="absolute left-10 top-10 w-[180px] h-[180px] z-50 bg-[#ef4444]"
                    onclick={move |_e: Event| z50_cb.set(z50_cb.get() + 1)}
                />
            </div>
            <div
                class="flex flex-row w-full h-[120px] z-10 bg-[#2563eb]"
                onclick={move |_e: Event| later_cb.set(later_cb.get() + 1)}
            />
        </div>
    };

    let mut runtime = AppRuntime::new(view, 360.0, 300.0, 1.0);

    let root = &runtime.layout_root;
    let overflow_box = &root.children[0];
    let z10_flex_item = &root.children[1];

    // 1. overflow-y-auto alone must NOT establish a stacking context in either Compositor or AppRuntime
    assert!(!Compositor::establishes_stacking_context(
        overflow_box,
        false
    ));
    assert!(!AppRuntime::establishes_stacking_context(
        overflow_box,
        false
    ));
    assert!(!overflow_box.establishes_stacking_context_in(Some(Display::Flex), false));

    // 2. A Static flex item with `z-10` MUST establish a stacking context when parent is Display::Flex
    // (CSS Flexbox §4.3), and Compositor + AppRuntime agree!
    assert!(z10_flex_item.establishes_stacking_context_in(Some(Display::Flex), false));
    assert!(AppRuntime::establishes_stacking_context_in(
        z10_flex_item,
        Some(Display::Flex),
        false
    ));
    assert!(!z10_flex_item.establishes_stacking_context_in(Some(Display::Block), false));

    // 3. Because overflow_box is NOT an atomic stacking context, its z-50 child hoists out and beats z-10 sibling!
    assert!(runtime.dispatch_mouse_click(100.0, 140.0, 0, 1));
    assert_eq!(z50_clicks.get(), 1);
    assert_eq!(later_clicks.get(), 0);

    #[cfg(target_os = "linux")]
    {
        let pixels = render_to_argb_pixels(&mut runtime, 360, 300);
        let p = pixel_rgb(&pixels, 360, 100, 140);
        assert!(
            p.0 > 200 && p.2 < 80,
            "Hoisted z-50 child inside non-SC overflow-y-auto must paint red above z-10 sibling, got {:?}",
            p
        );
    }
}

#[test]
fn bug3_position_fixed_inside_scroll_container_escapes_scroll_and_overflow_clip() {
    let backdrop_clicks = Rc::new(Cell::new(0usize));
    let modal_btn_clicks = Rc::new(Cell::new(0usize));
    let header_clicks = Rc::new(Cell::new(0usize));

    let backdrop_cb = Rc::clone(&backdrop_clicks);
    let modal_btn_cb = Rc::clone(&modal_btn_clicks);
    let header_cb = Rc::clone(&header_clicks);

    // Header sits at y = 0..60.
    // <main class="overflow-y-auto"> sits at y = 60..220 (height 160px) with 800px of scrollable content,
    // AND mounts a <div class="fixed inset-0 z-40"> modal backdrop + button at y = 250 (strictly below main's 220px bottom!).
    let view = rsx! {
        <div class="flex flex-col w-[360px] h-[300px] bg-[#09090b]">
            <header
                class="w-full h-[60px] bg-[#2563eb]"
                onclick={move |_e: Event| header_cb.set(header_cb.get() + 1)}
            >
                <span>"Top Header"</span>
            </header>
            <main class="w-full h-[160px] overflow-y-auto flex flex-col">
                <div class="w-full h-[400px] shrink-0 bg-[#18181b]">"Tall Scroll Block 1"</div>
                <div class="w-full h-[400px] shrink-0 bg-[#27272a]">"Tall Scroll Block 2"</div>
                <div
                    class="fixed inset-0 z-40 bg-[#ef4444]"
                    onclick={move |_e: Event| backdrop_cb.set(backdrop_cb.get() + 1)}
                >
                    <button
                        class="w-[160px] h-[40px] mt-[245px] ml-[20px] bg-[#eab308]"
                        onclick={move |_e: Event| modal_btn_cb.set(modal_btn_cb.get() + 1)}
                    >
                        "Confirm Outside Main"
                    </button>
                </div>
            </main>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 360.0, 300.0, 1.0);

    // Scroll <main> down by 150px!
    assert!(runtime.dispatch_scroll(180.0, 120.0, 0.0, 150.0));

    let main_node = &runtime.layout_root.children[1];
    let scrolled_block1 = &main_node.children[0];
    let fixed_modal = &main_node.children[2];
    let modal_btn = &fixed_modal.children[0];

    // Normal child scrolled up by -150px (from y=60 to y=-90)
    assert!(
        (scrolled_block1.geometry.border_box.y - (-90.0)).abs() < 0.5,
        "Normal child inside <main> must scroll by -150px, got {}",
        scrolled_block1.geometry.border_box.y
    );

    // Fixed modal and its child button MUST NOT scroll!
    assert!(
        fixed_modal.geometry.border_box.y.abs() < 0.5
            && (fixed_modal.geometry.border_box.height - 300.0).abs() < 0.5,
        "position: fixed backdrop must stay pinned to viewport 0..300 after scroll, got {:?}",
        fixed_modal.geometry.border_box
    );
    assert!(
        (modal_btn.geometry.border_box.y - 245.0).abs() < 0.5,
        "Static button inside position: fixed modal must stay at y=245, got {}",
        modal_btn.geometry.border_box.y
    );

    // Click at (30, 20) — strictly ABOVE <main> (over Header at y=0..60) — must hit fixed z-40 backdrop!
    assert!(runtime.dispatch_mouse_click(30.0, 20.0, 0, 1));
    assert_eq!(backdrop_clicks.get(), 1);
    assert_eq!(header_clicks.get(), 0);

    // Click at (60, 260) — strictly BELOW <main> (main ends at y=220!) — must hit modal button!
    assert!(runtime.dispatch_mouse_click(60.0, 260.0, 0, 1));
    assert_eq!(modal_btn_clicks.get(), 1);

    // Hover path at (60, 260) must also resolve to the modal button inside the fixed layer
    let hover_path =
        AppRuntime::hit_test_node_path(&runtime.root_blueprint, &runtime.layout_root, 60.0, 260.0);
    assert!(
        hover_path.is_some(),
        "Hover must hit button inside fixed modal outside <main> bounds"
    );

    #[cfg(target_os = "linux")]
    {
        let pixels = render_to_argb_pixels(&mut runtime, 360, 300);
        // Above <main> at (30, 20): fixed backdrop (#ef4444) must NOT be clipped to <main> (y=60..220)!
        let p_top = pixel_rgb(&pixels, 360, 30, 20);
        assert!(
            p_top.0 > 200 && p_top.2 < 100,
            "Fixed backdrop must paint over header at y=20 without <main> PushClip, got {:?}",
            p_top
        );
        // Below <main> at (60, 260): static child button (#eab308 = 234, 179, 8) inside fixed modal
        // must NOT be culled or clipped by <main>'s y=60..220 border_box!
        let p_btn = pixel_rgb(&pixels, 360, 60, 260);
        assert!(
            p_btn.0 > 200 && p_btn.1 > 150 && p_btn.2 < 50,
            "Button inside fixed modal at y=260 (> main bottom 220) must render yellow (#eab308), got {:?}",
            p_btn
        );
    }
}
