//! Frame pipeline measurements and budgets.
//!
//! Run with `cargo test --release -p strata --test frame_budget_tests -- --ignored --nocapture`.
//! The tests are `#[ignore]`d because wall-clock budgets only make sense in release builds.
//! The always-on test at the bottom checks the structural claims (skip counters) that do not
//! depend on machine speed.

use std::time::{Duration, Instant};

use strata::prelude::*;
use strata_platform::{MouseButton, PlatformEvent};

const W: f32 = 1440.0;
const H: f32 = 900.0;
const ITERS: usize = 10;

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

fn measure(mut f: impl FnMut()) -> Duration {
    f();
    median(
        (0..ITERS)
            .map(|_| {
                let start = Instant::now();
                f();
                start.elapsed()
            })
            .collect(),
    )
}

fn grid_view(cells: usize) -> View {
    rsx! {
        <div class="flex flex-wrap w-full h-full">
            {(0..cells).map(|i| rsx! {
                <div class="w-8 h-8 m-0.5 bg-red-500 rounded">{i.to_string()}</div>
            })}
        </div>
    }
}

fn click(runtime: &mut AppRuntime, x: f32, y: f32) {
    runtime.step_event(&PlatformEvent::MouseDown {
        button: MouseButton::Left,
        x,
        y,
    });
    runtime.step_event(&PlatformEvent::MouseUp {
        button: MouseButton::Left,
        x,
        y,
    });
}

#[test]
#[ignore = "wall-clock budgets; run in release"]
fn frame_pipeline_report() {
    let empty = measure(|| {
        let _ = AppRuntime::new(rsx! { <div /> }, W, H, 1.0);
    });

    let mut runtime = AppRuntime::new(grid_view(300), W, H, 1.0);
    runtime.tick_frame();
    let commands = runtime.display_list.commands.len();

    let compile_unchanged = measure(|| {
        runtime.compile_display_list_if_dirty();
    });
    let compile_forced = measure(|| runtime.compile_display_list());
    let hover = measure(|| {
        runtime.step_event(&PlatformEvent::MouseMove { x: 100.0, y: 100.0 });
        runtime.compile_display_list_if_dirty();
    });
    let press = measure(|| click(&mut runtime, 100.0, 100.0));
    let rebuild = measure(|| runtime.rebuild_layout_tree());

    println!("commands in tree          : {commands}");
    println!("AppRuntime::new (empty)   : {empty:?}");
    println!("compile (unchanged, gated): {compile_unchanged:?}");
    println!("compile (forced)          : {compile_forced:?}");
    println!("move + compile            : {hover:?}");
    println!("press + release           : {press:?}");
    println!("rebuild_layout_tree       : {rebuild:?}");
    let profile = runtime.finish_frame_profile();
    println!("accumulated profile       : {profile:?}");

    assert!(
        compile_unchanged < Duration::from_millis(1),
        "unchanged compile budget exceeded: {compile_unchanged:?}"
    );
}

#[test]
fn unchanged_frames_do_no_compositor_work() {
    let mut runtime = AppRuntime::new(grid_view(50), 800.0, 600.0, 1.0);
    runtime.tick_frame();
    let before = runtime.compiles_skipped();
    for _ in 0..5 {
        assert!(!runtime.compile_display_list_if_dirty());
    }
    assert_eq!(runtime.compiles_skipped(), before + 5);
}
