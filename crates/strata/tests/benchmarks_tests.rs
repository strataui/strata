//! Integration tests verifying Strata's runtime performance, layout throughput,
//! signal propagation, DisplayList compilation, and memory footprint benchmarks
//! (consolidated from the retired `strata_benchmark` crate and `strata::benchmarks`).

use std::time::Instant;
use strata::runtime::AppRuntime;
use strata_dom::reactive::{use_effect, use_memo, Signal};
use strata_dom::{ClassList, NodeBlueprint, Tag, View};
use strata_layout::prelude::*;
use strata_primitives::geom::EdgeInsets;
use strata_primitives::Rgba;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_style::types::{ComputedStyle, Dimension, Display, FlexDirection, FlexWrap, GridTrack};

fn read_process_rss_bytes() -> usize {
    #[cfg(target_os = "linux")]
    {
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            if let Some(rss_pages) = statm.split_whitespace().nth(1) {
                if let Ok(pages) = rss_pages.parse::<usize>() {
                    return pages * 4096;
                }
            }
        }
    }
    0
}

#[test]
fn test_strata_engine_cold_start_layout_and_reconcile_benchmarks() {
    let start_cold = Instant::now();
    let root_view = View::new(NodeBlueprint::Element {
        tag: Tag::Div,
        classes: ClassList::Static(&["flex", "flex-col", "w-full", "h-full", "bg-neutral-900"]),
        attributes: vec![],
        children: (0..20)
            .map(|i| NodeBlueprint::Element {
                tag: Tag::Div,
                classes: ClassList::Static(&["flex", "h-12", "p-2", "rounded"]),
                attributes: vec![],
                children: vec![NodeBlueprint::Text(format!("Card Item {i}"))],
            })
            .collect(),
    });
    let mut rt = AppRuntime::new(root_view, 1280.0, 720.0, 1.0);
    rt.tick_frame();
    let cold_start_us = start_cold.elapsed().as_micros() as u64;

    // 10,000-node W3C Flexbox layout pass
    let flex_root_style = ComputedStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        ..ComputedStyle::default()
    };
    let mut children = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        let item = ComputedStyle {
            display: Display::Block,
            width: Dimension::Px(120.0 + (i % 20) as f32 * 5.0),
            height: Dimension::Px(40.0),
            margin: EdgeInsets::all(4.0),
            flex_grow: (i % 3) as f32,
            ..ComputedStyle::default()
        };
        children.push(LayoutNode::new(item));
    }
    let mut flex_root = LayoutNode::with_children(flex_root_style, children);
    let start_layout = Instant::now();
    flex_root.compute_layout(1920.0, 1080.0);
    let layout_10k_us = start_layout.elapsed().as_micros() as u64;
    assert_eq!(flex_root.children.len(), 10_000);

    // Frame reconcile & display-list tick
    let start_reconcile = Instant::now();
    rt.tick_frame();
    let frame_reconcile_us = start_reconcile.elapsed().as_micros() as u64;

    let max_cold_start = if cfg!(debug_assertions) {
        1_500_000
    } else {
        8_000
    };
    assert!(
        cold_start_us < max_cold_start,
        "Cold start took {cold_start_us} µs, exceeding {max_cold_start} µs threshold"
    );
    let max_layout_10k = if cfg!(debug_assertions) {
        1_500_000
    } else {
        50_000
    };
    assert!(
        layout_10k_us < max_layout_10k,
        "10k-node layout took {layout_10k_us} µs, exceeding {max_layout_10k} µs threshold"
    );
    let max_reconcile = if cfg!(debug_assertions) {
        150_000
    } else {
        2_000
    };
    assert!(
        frame_reconcile_us < max_reconcile,
        "Frame reconcile took {frame_reconcile_us} µs, exceeding {max_reconcile} µs threshold"
    );

    let rss_bytes = read_process_rss_bytes();
    assert!(
        rss_bytes < 128 * 1024 * 1024,
        "Process RSS memory was {rss_bytes} bytes, unexpectedly high"
    );
}

#[test]
fn test_strata_grid_display_list_and_signal_dag_benchmarks() {
    // 1,000-item CSS Grid Layout
    let grid_style = ComputedStyle {
        display: Display::Grid,
        grid_template_columns: vec![
            GridTrack::Fr(1.0),
            GridTrack::Fr(2.0),
            GridTrack::Px(200.0),
            GridTrack::Fr(1.0),
        ],
        ..ComputedStyle::default()
    };
    let grid_children: Vec<LayoutNode> = (0..1_000)
        .map(|_| {
            let s = ComputedStyle {
                display: Display::Block,
                height: Dimension::Px(60.0),
                ..ComputedStyle::default()
            };
            LayoutNode::new(s)
        })
        .collect();
    let mut grid_root = LayoutNode::with_children(grid_style, grid_children);
    let t_grid = Instant::now();
    grid_root.compute_layout(1920.0, 1080.0);
    assert!(t_grid.elapsed().as_millis() < 1_000);

    // 1,000-element DisplayList compilation
    let t_dl = Instant::now();
    let mut dl = DisplayList::new();
    for i in 0..1_000 {
        let x = (i % 20) as f32 * 60.0;
        let y = (i / 20) as f32 * 40.0;
        dl.push(DrawCommand::DrawRect {
            bounds: Rect::new(x, y, 50.0, 30.0),
            color: Rgba::new(50, 100, 200, 255),
        });
    }
    assert_eq!(dl.commands().len(), 1_000);
    assert!(t_dl.elapsed().as_millis() < 500);

    // Diamond Signal DAG propagation (10,000 updates)
    let a = Signal::new(0i64);
    let a_b = a.clone();
    let b = use_memo(move || a_b.get() + 1);
    let a_c = a.clone();
    let c = use_memo(move || a_c.get() * 2);
    let b_d = b.clone();
    let c_d = c.clone();
    let d = use_memo(move || b_d.get() + c_d.get());
    let observed = std::rc::Rc::new(std::cell::Cell::new(0i64));
    let obs_clone = observed.clone();
    let d_eff = d.clone();
    let _guard = use_effect(move || {
        obs_clone.set(d_eff.get());
    });

    let t_sig = Instant::now();
    for i in 1..=10_000i64 {
        a.set(i);
    }
    assert_eq!(d.get(), 30_001);
    assert_eq!(observed.get(), 30_001);
    assert!(t_sig.elapsed().as_millis() < 1_500);
}
