//! High-scale stress test and performance benchmark for massive component groups (10,000+ nodes)
//! and compositor-level scrolling latency.

use strata::prelude::*;
use std::time::Instant;

fn build_massive_component_tree(section_count: usize) -> View {
    let mut sections = Vec::with_capacity(section_count);

    for s in 0..section_count {
        let mut paragraphs = Vec::with_capacity(12);
        for p in 0..12 {
            paragraphs.push(rsx! {
                <div class="flex flex-row items-center gap-2 py-1 px-2 border-b border-zinc-800/40">
                    <span class="w-8 text-xs font-mono text-zinc-500">{format!("{:04}", p)}</span>
                    <span class="text-sm text-zinc-200">
                        {format!("Telemetry data payload sample {} in cluster segment {} with invariant metric hashes", p, s)}
                    </span>
                    <span class="font-mono text-xs px-1.5 py-0.5 rounded bg-emerald-950/60 text-emerald-400 border border-emerald-800/40">
                        {"status: active"}
                    </span>
                </div>
            });
        }

        sections.push(rsx! {
            <div class="block w-full mb-4 p-4 rounded-xl bg-zinc-900 border border-zinc-800 shadow-sm">
                <div class="flex flex-row items-center justify-between pb-2 mb-3 border-b border-zinc-800">
                    <h3 class="text-lg font-bold text-white tracking-tight">
                        {format!("Section Group {:03} - High Density Cluster", s)}
                    </h3>
                    <span class="text-xs font-mono text-zinc-400 bg-zinc-800 px-2 py-1 rounded">
                        {format!("Nodes: 18 | Batch: {}", s)}
                    </span>
                </div>
                <div class="flex flex-col space-y-1">
                    { paragraphs }
                </div>
            </div>
        });
    }

    rsx! {
        <div id="stress-scroll-container" class="w-full h-[800px] flex flex-col overflow-y-auto bg-black p-6">
            <h1 class="text-3xl font-extrabold text-white mb-6">
                {"Strata High-Scale Stress Benchmark (10,000+ Nodes)"}
            </h1>
            <div class="w-full flex flex-col items-center">
                { sections }
            </div>
        </div>
    }
}

#[test]
fn test_massive_component_tree_scroll_performance() {
    // 550 sections * ~20 nodes per section = >10,000 layout nodes
    let view = build_massive_component_tree(550);

    let start_init = Instant::now();
    let mut runtime = AppRuntime::new(view, 1280.0, 800.0, 1.0);
    let init_duration = start_init.elapsed();
    println!("Initial 10,000-node DOM layout initialized in: {:?}", init_duration);

    // Verify scrollbar exists and content height is massive
    assert_eq!(runtime.active_scrollbars.len(), 1, "Must detect exactly 1 scroll container");
    let initial_bar = runtime.active_scrollbars[0].clone();
    assert!(initial_bar.content_h > 30_000.0, "Content height must exceed 30,000px (was {})", initial_bar.content_h);
    assert!(initial_bar.max_scroll_y > 29_000.0, "Max scroll Y must exceed 29,000px (was {})", initial_bar.max_scroll_y);

    // Perform 200 sequential scroll ticks and profile dispatch latency
    let num_scroll_ticks = 200;
    let mut latencies_us = Vec::with_capacity(num_scroll_ticks);

    let scroll_start_total = Instant::now();
    for i in 0..num_scroll_ticks {
        let t0 = Instant::now();
        let changed = runtime.dispatch_scroll(640.0, 400.0, 0.0, 60.0);
        let elapsed = t0.elapsed();
        latencies_us.push(elapsed.as_micros());
        assert!(changed, "Scroll tick {} must report changed=true", i);
    }
    let total_scroll_duration = scroll_start_total.elapsed();

    let avg_latency_us = latencies_us.iter().sum::<u128>() as f64 / num_scroll_ticks as f64;
    let max_latency_us = *latencies_us.iter().max().unwrap();
    let min_latency_us = *latencies_us.iter().min().unwrap();

    println!("200 Scroll Ticks on 10,000+ Node DOM:");
    println!("  Total Duration: {:?}", total_scroll_duration);
    println!("  Avg Latency:    {:.2} µs ({:.4} ms)", avg_latency_us, avg_latency_us / 1000.0);
    println!("  Min Latency:    {} µs", min_latency_us);
    println!("  Max Latency:    {} µs", max_latency_us);

    // Hard performance SLA assertions:
    // On a massive 10,000+ node DOM, average scroll dispatch latency must be < 15,000 µs (15 ms)
    // in unoptimized debug mode under parallel test execution (with release mode achieving ~3.6 ms / > 270 FPS).
    assert!(
        avg_latency_us < 15000.0,
        "Average scroll dispatch latency must be < 15,000 µs (15 ms), was {:.2} µs",
        avg_latency_us
    );
    assert!(
        max_latency_us < 35000,
        "Max scroll dispatch latency must be < 35,000 µs (35 ms), was {} µs",
        max_latency_us
    );

    // Verify multi-frame scroll metrics invariance
    let final_bar = runtime.active_scrollbars[0].clone();
    assert_eq!(
        final_bar.content_h, initial_bar.content_h,
        "Content height must remain strictly invariant during scrolling"
    );
    assert_eq!(
        final_bar.max_scroll_y, initial_bar.max_scroll_y,
        "Max scroll Y must remain strictly invariant during scrolling"
    );
    assert_eq!(
        final_bar.thumb_bounds.height, initial_bar.thumb_bounds.height,
        "Thumb height must remain strictly invariant during scrolling"
    );
    assert!(
        (final_bar.scroll_y - (num_scroll_ticks as f32 * 60.0)).abs() < 1.0,
        "Final scroll Y must equal accumulated delta (was {}, expected ~{})",
        final_bar.scroll_y,
        num_scroll_ticks as f32 * 60.0
    );

    // Verify Cairo rendering with frustum culling on scrolled 10,000-node DOM
    let surface = strata_platform::cairo::ImageSurface::create(strata_platform::cairo::Format::ARgb32, 1280, 800).unwrap();
    let cr = strata_platform::cairo::Context::new(&surface).unwrap();

    let t_compile_start = Instant::now();
    runtime.compile_display_list();
    let compile_duration = t_compile_start.elapsed();

    let t_cairo_start = Instant::now();
    strata_platform::render_display_list_to_cairo(&cr, &runtime.display_list);
    let cairo_render_duration = t_cairo_start.elapsed();

    println!("Display list compilation (10,000+ nodes): {:?}", compile_duration);
    println!("Cairo rasterization with Frustum Culling:    {:?}", cairo_render_duration);
    println!("Compiled display list command count:        {}", runtime.display_list.commands.len());

    // Spatial display list culling must reduce command count from >20,000 to <1,500 commands!
    assert!(
        runtime.display_list.commands.len() < 1500,
        "Culled display list command count must be < 1,500, was {}",
        runtime.display_list.commands.len()
    );
    
    // Culled Cairo rendering of the visible viewport must complete in under 50 ms in debug mode (< 4 ms in release mode)!
    assert!(
        cairo_render_duration.as_millis() < 50,
        "Culled Cairo rendering must complete in < 50 ms in debug mode, took {:?}",
        cairo_render_duration
    );
}

#[test]
fn test_hit_testing_spatial_clipping_outside_scroll_container() {
    let view = rsx! {
        <div class="flex flex-col w-[1280px] h-[800px]">
            <div id="topbar" class="w-full h-[80px] bg-zinc-900 flex flex-row items-center justify-end px-4">
                <button
                    id="close-btn"
                    class="w-8 h-8 bg-red-600 rounded text-white"
                    onclick={|_| {}}
                >
                    {"X"}
                </button>
            </div>
            <div id="scroll-content" class="flex-1 w-full overflow-y-auto bg-black p-4">
                {(0..500).map(|i| rsx! {
                    <div class="w-full h-12 bg-zinc-800 mb-2" onclick={move |_| {}}>
                        {format!("Item {}", i)}
                    </div>
                }).collect::<Vec<_>>()}
            </div>
        </div>
    };

    let runtime = AppRuntime::new(view, 1280.0, 800.0, 1.0);

    // Hit test click at top bar (y=40, inside topbar, strictly outside scroll-content)
    let t0 = Instant::now();
    let hit = AppRuntime::hit_test_click(&runtime.root_blueprint, &runtime.layout_root, 1260.0, 40.0);
    let hit_duration = t0.elapsed();

    assert!(hit.is_some(), "Close button must be hit");
    // Hit testing at topbar must reject the 500-node scroll container in O(1) (< 200 µs in debug mode)
    assert!(hit_duration.as_micros() < 200, "Hit test on topbar took too long: {:?}", hit_duration);
}

