use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use strata::prelude::*;
use strata_dom::View;
use strata_platform::{MouseButton, PlatformEvent};
use strata_render::DrawCommand;
use strata_style::types::Color;

#[component]
fn TailwindArbitraryVarApp() -> View {
    rsx! {
        <div
            class="flex flex-col w-[400px] h-[300px] p-4"
            style="--theme-bg-card: #1e293b; --theme-bg-card-hover: #334155; --theme-text: #f8fafc; --theme-border: #475569; --theme-shadow: 0 8px 24px rgba(14, 165, 233, 0.5);"
        >
            <div
                class="w-[200px] h-[60px] border bg-[var(--theme-bg-card)] hover:bg-[var(--theme-bg-card-hover)] text-[var(--theme-text)] border-[var(--theme-border)] shadow-[var(--theme-shadow)]"
            >
                "Arbitrary Var Card"
            </div>
            <div
                class="w-[200px] h-[40px] bg-[var(--missing-token,rgba(15,23,42,0.8))]"
            >
                "Fallback Card"
            </div>
        </div>
    }
}

#[test]
fn test_tailwind_arbitrary_var_classes_and_hover() {
    let mut runtime = AppRuntime::new(TailwindArbitraryVarApp(), 400.0, 300.0, 1.0);

    let card = &runtime.layout_root.children[0];
    assert_eq!(
        card.style.background_color,
        Some(Color::new(30, 41, 59, 255))
    );
    assert_eq!(card.style.color, Color::new(248, 250, 252, 255));
    assert_eq!(card.style.border_color, Some(Color::new(71, 85, 105, 255)));
    assert_eq!(card.style.box_shadow.len(), 1);
    assert!((card.style.box_shadow[0].blur_radius - 24.0).abs() < 0.01);

    let fallback_card = &runtime.layout_root.children[1];
    let fb_bg = fallback_card
        .style
        .background_color
        .expect("Fallback var() color must resolve");
    assert_eq!((fb_bg.r, fb_bg.g, fb_bg.b), (15, 23, 42));
    assert!((fb_bg.a as i32 - 204).abs() <= 1);

    // Hover over the first card: hover:bg-[var(--theme-bg-card-hover)] must activate
    let dirty = runtime.update_hover(60.0, 40.0);
    assert!(dirty);
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(Color::new(51, 65, 85, 255)),
        "hover:bg-[var(--theme-bg-card-hover)] must resolve on hover"
    );

    runtime.clear_hover();
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(Color::new(30, 41, 59, 255))
    );
}

#[component]
fn InlineStyleVsPseudoApp() -> View {
    rsx! {
        <div
            class="flex flex-col w-[400px] h-[300px] p-4 gap-2"
            style="--theme-bg-card: #1e293b; --theme-bg-card-hover: #334155;"
        >
            <button
                class="w-[180px] h-[48px] border hover:bg-[var(--theme-bg-card-hover)] hover:border-[#38bdf8] active:bg-[#0ea5e9] focus:bg-[#6366f1]"
                style="background-color: var(--theme-bg-card); border-color: #334155;"
            >
                "Card Button"
            </button>
            <button
                class="w-[180px] h-[48px] hover:bg-[#38bdf8]"
                style="background-color: #ef4444 !important;"
            >
                "Important Inline"
            </button>
        </div>
    }
}

#[test]
fn test_inline_style_does_not_overwrite_hover_active_focus_unless_important() {
    let mut runtime = AppRuntime::new(InlineStyleVsPseudoApp(), 400.0, 300.0, 1.0);

    // Base state uses inline style
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(Color::new(30, 41, 59, 255))
    );
    assert_eq!(
        runtime.layout_root.children[0].style.border_color,
        Some(Color::new(51, 65, 85, 255))
    );

    // Hover state overrides normal inline style
    assert!(runtime.update_hover(60.0, 40.0));
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(Color::new(51, 65, 85, 255)),
        "hover:bg-* must win over static inline style"
    );
    assert_eq!(
        runtime.layout_root.children[0].style.border_color,
        Some(Color::new(56, 189, 248, 255)),
        "hover:border-* must win over static inline border-color"
    );

    // Active state overrides hover and normal inline style
    runtime.step_event(&PlatformEvent::MouseDown {
        button: MouseButton::Left,
        x: 60.0,
        y: 40.0,
    });
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(Color::new(14, 165, 233, 255)),
        "active:bg-* must win over static inline style"
    );

    runtime.step_event(&PlatformEvent::MouseUp {
        button: MouseButton::Left,
        x: 60.0,
        y: 40.0,
    });
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(Color::new(51, 65, 85, 255))
    );

    // Focus state overrides normal inline style
    runtime.clear_hover();
    runtime.set_focus_path(Some(vec![0]));
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(Color::new(99, 102, 241, 255)),
        "focus:bg-* must win over static inline style"
    );
    runtime.set_focus_path(None);

    // Inline !important still wins over hover
    let btn2_center_y = runtime.layout_root.children[1].geometry.border_box.y + 20.0;
    runtime.update_hover(60.0, btn2_center_y);
    assert_eq!(
        runtime.layout_root.children[1].style.background_color,
        Some(Color::new(239, 68, 68, 255)),
        "Inline !important declaration must retain highest priority"
    );
}

#[component]
fn MountedStyleTagApp(clicked: Rc<Cell<bool>>) -> View {
    let css = r#"
        :root {
            --theme-accent: #06b6d4;
            --theme-tab: #1e293b;
            --theme-tab-hover: #334155;
        }
        .btn-primary {
            background-color: var(--theme-accent);
            color: #ffffff;
            border-radius: 10px;
        }
        .tab.active {
            background-color: var(--theme-tab);
        }
        .tab.active:hover {
            background-color: var(--theme-tab-hover);
        }
        .fab {
            background-color: #0284c7;
        }
        .fab:hover {
            background-color: #38bdf8;
        }
        .resizer .resizer-line {
            background-color: #475569;
        }
        .resizer:hover .resizer-line {
            background-color: #22d3ee;
        }
    "#;

    let on_click = move |_ev: strata_dom::Event| {
        clicked.set(true);
    };

    rsx! {
        <div class="flex flex-col w-[500px] h-[400px]">
            <style>{css}</style>
            <button class="btn-primary w-[140px] h-[40px]" onclick={on_click}>
                "Primary Action"
            </button>
            <div class="tab active w-[140px] h-[36px]">
                "Active Tab"
            </div>
            <button class="fab w-[48px] h-[48px]" style="background-color: #0284c7;">
                "+"
            </button>
            <div class="resizer flex flex-row w-[16px] h-[80px]">
                <div class="resizer-line w-[2px] h-[80px]"></div>
            </div>
        </div>
    }
}

#[test]
fn test_mounted_style_element_cascade_and_ancestor_hover_descendant_selector() {
    let clicked = Rc::new(Cell::new(false));
    let mut runtime = AppRuntime::new(MountedStyleTagApp(clicked.clone()), 500.0, 400.0, 1.0);

    // Child 0 is the hidden <style> placeholder (Display::None), preserving 1:1 blueprint indices
    assert_eq!(
        runtime.layout_root.children[0].style.display,
        strata_style::Display::None
    );

    // Child 1: .btn-primary resolved :root --theme-accent (#06b6d4)
    let btn = &runtime.layout_root.children[1];
    assert_eq!(
        btn.style.background_color,
        Some(Color::new(6, 182, 212, 255))
    );
    assert_eq!(btn.style.color, Color::new(255, 255, 255, 255));
    assert!((btn.style.border_radius.top_left - 10.0).abs() < 0.01);

    // Clicking .btn-primary dispatches accurately despite preceding <style> node
    let btn_box = btn.geometry.border_box;
    assert!(runtime.dispatch_click(btn_box.x + 20.0, btn_box.y + 15.0));
    assert!(clicked.get(), "onclick handler after <style> must fire");

    // Child 2: .tab.active and .tab.active:hover
    let tab_box = runtime.layout_root.children[2].geometry.border_box;
    assert_eq!(
        runtime.layout_root.children[2].style.background_color,
        Some(Color::new(30, 41, 59, 255))
    );
    assert!(runtime.update_hover(tab_box.x + 20.0, tab_box.y + 12.0));
    assert_eq!(
        runtime.layout_root.children[2].style.background_color,
        Some(Color::new(51, 65, 85, 255)),
        ".tab.active:hover from <style> must apply on hover"
    );

    // Child 3: .fab:hover wins over base inline style="background-color: #0284c7;"
    let fab_box = runtime.layout_root.children[3].geometry.border_box;
    assert!(runtime.update_hover(fab_box.x + 12.0, fab_box.y + 12.0));
    assert_eq!(
        runtime.layout_root.children[3].style.background_color,
        Some(Color::new(56, 189, 248, 255)),
        ".fab:hover must apply over base inline background-color"
    );

    // Child 4: .resizer:hover .resizer-line (hovering the 16px .resizer outside the 2px .resizer-line!)
    let resizer_box = runtime.layout_root.children[4].geometry.border_box;
    runtime.clear_hover();
    assert_eq!(
        runtime.layout_root.children[4].children[0]
            .style
            .background_color,
        Some(Color::new(71, 85, 105, 255))
    );
    // Hover at x = resizer_box.x + 12.0 (inside 16px .resizer, outside 2px .resizer-line)
    assert!(runtime.update_hover(resizer_box.x + 12.0, resizer_box.y + 20.0));
    assert_eq!(
        runtime.layout_root.children[4].children[0]
            .style
            .background_color,
        Some(Color::new(34, 211, 238, 255)),
        ".resizer:hover .resizer-line must restyle descendant when ancestor .resizer is hovered"
    );

    runtime.clear_hover();
    assert_eq!(
        runtime.layout_root.children[4].children[0]
            .style
            .background_color,
        Some(Color::new(71, 85, 105, 255))
    );
}

#[component]
fn StationaryHoverTransitionApp() -> View {
    rsx! {
        <div class="flex flex-col w-[300px] h-[200px]">
            <button class="w-[140px] h-[44px] bg-[#1e293b] hover:bg-[#38bdf8] transition-colors duration-500">
                "Hover Me"
            </button>
        </div>
    }
}

#[test]
fn test_stationary_hover_transition_tick_loop_advances_to_completion() {
    let mut runtime = AppRuntime::new(StationaryHoverTransitionApp(), 300.0, 200.0, 1.0);

    let start_c = Color::new(30, 41, 59, 255);
    let target_c = Color::new(56, 189, 248, 255);

    assert!(!runtime.has_active_transitions());
    assert!(!runtime.layout_root.has_active_transitions());
    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(start_c)
    );

    // Pointer enters button once and stops moving
    assert!(runtime.update_hover(50.0, 22.0));
    assert!(
        runtime.has_active_transitions(),
        "AppRuntime::has_active_transitions must be true immediately after hover starts transition"
    );
    assert!(
        runtime.layout_root.has_active_transitions(),
        "LayoutNode::has_active_transitions must propagate true up to root"
    );

    // Simulate stationary pointer: advance time ~120ms and fire AnimationTick without MouseMove
    std::thread::sleep(Duration::from_millis(120));
    assert!(
        runtime.step_event(&PlatformEvent::AnimationTick),
        "AnimationTick must request redraw while hover transition is interpolating"
    );

    let mid_c = runtime.layout_root.children[0]
        .style
        .background_color
        .expect("Mid-transition background color must exist");
    assert!(
        mid_c.g > start_c.g && mid_c.g < target_c.g,
        "Mid-transition color {:?} must be strictly interpolated between {:?} and {:?}",
        mid_c,
        start_c,
        target_c
    );

    // Advance past 500ms duration and verify completion + idle sleep
    std::thread::sleep(Duration::from_millis(460));
    let _ = runtime.step_event(&PlatformEvent::AnimationTick);

    assert_eq!(
        runtime.layout_root.children[0].style.background_color,
        Some(target_c),
        "Completed transition must settle on target hover color"
    );
    assert!(
        !runtime.has_active_transitions(),
        "has_active_transitions must return false after duration completes"
    );
    assert!(
        !runtime.layout_root.has_active_transitions(),
        "LayoutNode::has_active_transitions must clear once transition finishes"
    );
    assert!(
        !runtime.step_event(&PlatformEvent::AnimationTick),
        "Subsequent AnimationTick must return false so idle loop consumes 0 redraws"
    );
}

#[component]
fn SvgCurrentColorButtonApp() -> View {
    rsx! {
        <div class="flex flex-col w-[300px] h-[200px]">
            <button class="w-[120px] h-[48px] text-[#94a3b8] hover:text-[#38bdf8] transition-colors duration-500">
                <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M5 12h14" />
                    <circle cx="12" cy="12" r="4" fill="currentColor" stroke="none" />
                </svg>
            </button>
        </div>
    }
}

fn extract_svg_stroke_and_fill(
    list: &strata_render::DisplayList,
) -> (Option<Color>, Option<Color>) {
    let mut found_stroke = None;
    let mut found_fill = None;
    for cmd in list.commands() {
        if let DrawCommand::DrawSvgPath {
            stroke_color,
            fill_color,
            ..
        } = cmd
        {
            if let Some(sc) = stroke_color {
                if found_stroke.is_none() {
                    found_stroke = Some(*sc);
                }
            }
            if let Some(fc) = fill_color {
                if found_fill.is_none() {
                    found_fill = Some(*fc);
                }
            }
        }
    }
    (found_stroke, found_fill)
}

#[test]
fn test_svg_current_color_inherits_and_interpolates_with_parent_transition() {
    let mut runtime = AppRuntime::new(SvgCurrentColorButtonApp(), 300.0, 200.0, 1.0);
    let base_c = Color::new(148, 163, 184, 255);
    let hover_c = Color::new(56, 189, 248, 255);

    runtime.compile_display_list();
    let (s0, f0) = extract_svg_stroke_and_fill(&runtime.display_list);
    assert_eq!(
        s0,
        Some(base_c),
        "Initial SVG stroke='currentColor' must match button text color"
    );
    assert_eq!(
        f0,
        Some(base_c),
        "Initial SVG fill='currentColor' must match button text color"
    );

    // Hover button and check mid-transition SVG colors
    assert!(runtime.update_hover(40.0, 24.0));
    std::thread::sleep(Duration::from_millis(120));
    assert!(runtime.poll_animations());
    runtime.compile_display_list();

    let btn_mid_c = runtime.layout_root.children[0].style.color;
    let (sm, fm) = extract_svg_stroke_and_fill(&runtime.display_list);
    assert_eq!(
        sm,
        Some(btn_mid_c),
        "Mid-transition SVG stroke='currentColor' must track parent's interpolated color"
    );
    assert_eq!(
        fm,
        Some(btn_mid_c),
        "Mid-transition SVG fill='currentColor' must track parent's interpolated color"
    );
    assert!(btn_mid_c.b > base_c.b && btn_mid_c.b < hover_c.b);

    // Settle transition
    std::thread::sleep(Duration::from_millis(460));
    let _ = runtime.poll_animations();
    runtime.compile_display_list();
    let (s1, f1) = extract_svg_stroke_and_fill(&runtime.display_list);
    assert_eq!(s1, Some(hover_c));
    assert_eq!(f1, Some(hover_c));
}

#[cfg(target_os = "linux")]
fn read_argb32_rgba(data: &[u8], stride: usize, x: usize, y: usize) -> (u8, u8, u8, u8) {
    let off = y * stride + x * 4;
    let b = data[off];
    let g = data[off + 1];
    let r = data[off + 2];
    let a = data[off + 3];
    (r, g, b, a)
}

#[cfg(target_os = "linux")]
#[component]
fn CairoBackdropShadowGlassApp() -> View {
    rsx! {
        <div class="relative w-[200px] h-[180px]">
            <div class="absolute left-0 top-0 w-[100px] h-[180px]" style="background-color: #000000;"></div>
            <div class="absolute left-[100px] top-0 w-[100px] h-[180px]" style="background-color: #ffffff;"></div>
            <div
                class="absolute left-[50px] top-[40px] w-[100px] h-[80px] rounded-[16px]"
                style="backdrop-filter: blur(12px); background-color: rgba(255, 255, 255, 0.0); box-shadow: 0 10px 18px rgba(255, 0, 0, 0.9);"
            ></div>
        </div>
    }
}

#[cfg(target_os = "linux")]
#[test]
fn test_cairo_backdrop_blur_and_non_clipped_rounded_box_shadow() {
    let mut runtime = AppRuntime::new(CairoBackdropShadowGlassApp(), 200.0, 180.0, 1.0);
    runtime.compile_display_list();

    let mut surface = strata_platform::cairo::ImageSurface::create(
        strata_platform::cairo::Format::ARgb32,
        200,
        180,
    )
    .expect("create ImageSurface");
    {
        let cr = strata_platform::cairo::Context::new(&surface).expect("create Context");
        strata_platform::render_display_list_to_cairo(&cr, &runtime.display_list);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().expect("read ImageSurface data");

    // 1. Outside the glass card (y = 15), the black/white split at x = 100 remains razor-sharp
    let (r_out_l, g_out_l, b_out_l, _) = read_argb32_rgba(&data, stride, 96, 15);
    let (r_out_r, g_out_r, b_out_r, _) = read_argb32_rgba(&data, stride, 104, 15);
    assert_eq!((r_out_l, g_out_l, b_out_l), (0, 0, 0));
    assert_eq!((r_out_r, g_out_r, b_out_r), (255, 255, 255));

    // 2. Inside the glass card (y = 80), pixels at x = 96 and x = 104 are blurred across the split
    let (r_in_l, _, _, _) = read_argb32_rgba(&data, stride, 96, 80);
    let (r_in_r, _, _, _) = read_argb32_rgba(&data, stride, 104, 80);
    assert!(
        r_in_l > 30 && r_in_l < 160,
        "Left of split inside backdrop-filter: blur(12px) must be blurred gray, got R={}",
        r_in_l
    );
    assert!(
        r_in_r > 95 && r_in_r < 225,
        "Right of split inside backdrop-filter: blur(12px) must be blurred gray, got R={}",
        r_in_r
    );

    // 3. Below the glass card (x = 100, y = 126), red outer box-shadow is NOT clipped by PushBackdropFilter
    let (r_sh, g_sh, b_sh, _) = read_argb32_rgba(&data, stride, 100, 126);
    assert!(
        r_sh > g_sh + 40 && r_sh > b_sh + 40,
        "Outer box-shadow below backdrop-filtered glass must not be clipped, got RGB=({}, {}, {})",
        r_sh,
        g_sh,
        b_sh
    );

    // 4. Inside the translucent glass card over white region far from split (x = 138, y = 80),
    // Even-Odd shadow knockout ensures the red drop shadow does NOT bleed into the interior!
    let (r_int, g_int, b_int, _) = read_argb32_rgba(&data, stride, 138, 80);
    assert!(
        r_int >= 250 && g_int >= 250 && b_int >= 250,
        "Even-Odd interior shadow knockout must keep translucent interior free of red shadow tint, got RGB=({}, {}, {})",
        r_int,
        g_int,
        b_int
    );
}

#[cfg(target_os = "linux")]
#[component]
fn CairoMultiStopGradientApp() -> View {
    rsx! {
        <div class="flex flex-col w-[240px] h-[220px]" style="background-color: #000000;">
            <div
                class="w-[200px] h-[80px] rounded-[16px]"
                style="background: linear-gradient(90deg, rgba(255, 0, 0, 1.0) 0%, rgba(0, 255, 0, 1.0) 50%, rgba(0, 0, 255, 1.0) 100%);"
            ></div>
            <div
                class="w-[100px] h-[100px]"
                style="background: radial-gradient(circle, rgba(255, 255, 0, 1.0) 0%, rgba(0, 128, 255, 1.0) 100%);"
            ></div>
        </div>
    }
}

#[cfg(target_os = "linux")]
#[test]
fn test_cairo_multi_stop_linear_and_radial_rgba_gradients() {
    let mut runtime = AppRuntime::new(CairoMultiStopGradientApp(), 240.0, 220.0, 1.0);
    runtime.compile_display_list();

    let mut surface = strata_platform::cairo::ImageSurface::create(
        strata_platform::cairo::Format::ARgb32,
        240,
        220,
    )
    .expect("create ImageSurface");
    {
        let cr = strata_platform::cairo::Context::new(&surface).expect("create Context");
        strata_platform::render_display_list_to_cairo(&cr, &runtime.display_list);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().expect("read ImageSurface data");

    // Linear gradient: left = red, middle = green, right = blue, exterior top-left corner = black
    let (r_corner, g_corner, b_corner, _) = read_argb32_rgba(&data, stride, 1, 1);
    assert_eq!(
        (r_corner, g_corner, b_corner),
        (0, 0, 0),
        "Rounded gradient corner exterior must stay un-painted"
    );

    let (rl, gl, bl, _) = read_argb32_rgba(&data, stride, 8, 40);
    assert!(
        rl > 220 && gl < 40 && bl < 40,
        "Start stop must be red: {:?}",
        (rl, gl, bl)
    );

    let (rm, gm, bm, _) = read_argb32_rgba(&data, stride, 100, 40);
    assert!(
        gm > 220 && rm < 40 && bm < 40,
        "50% stop must be green: {:?}",
        (rm, gm, bm)
    );

    let (rr, gr, br, _) = read_argb32_rgba(&data, stride, 192, 40);
    assert!(
        br > 220 && rr < 40 && gr < 40,
        "100% stop must be blue: {:?}",
        (rr, gr, br)
    );

    // Radial gradient at (0, 80, 100, 100): center (50, 130) = yellow, outer (95, 130) = blue
    let (rc, gc, bc, _) = read_argb32_rgba(&data, stride, 50, 130);
    assert!(
        rc > 220 && gc > 220 && bc < 40,
        "Radial gradient center must be yellow: {:?}",
        (rc, gc, bc)
    );

    let (re, _ge, be, _) = read_argb32_rgba(&data, stride, 95, 130);
    assert!(
        be > 200 && re < 60,
        "Radial gradient outer rim must transition to blue: {:?}",
        (re, _ge, be)
    );
}
