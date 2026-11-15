//! Regression tests for hot paths: scene sharing, click rebuilds, path batch
//! compilation and frame profiling.

use std::sync::Arc;

use strata::prelude::*;
use strata_platform::PlatformEvent;
use strata_render::compositor::DrawCommand;
use strata_ui::scene3d::SceneViewport;

fn scene_arc() -> Arc<Scene3D> {
    let mut scene = Scene3D::new();
    scene.add_mesh(Mesh3D::new(
        Geometry3D::Box {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        },
        Material3D::default(),
    ));
    Arc::new(scene)
}

fn viewport_scenes(runtime: &AppRuntime) -> Vec<Arc<Scene3D>> {
    runtime
        .display_list
        .commands
        .iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::DrawViewport3D { scene, .. } => Some(Arc::clone(scene)),
            _ => None,
        })
        .collect()
}

#[test]
fn draw_viewport_3d_shares_the_scene_arc_across_compiles() {
    let shared = scene_arc();
    let view = rsx! {
        <div class="w-full h-full">
            <SceneViewport scene={&shared} class="w-full h-full" />
        </div>
    };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.tick_frame();
    let first = viewport_scenes(&runtime);
    runtime.compile_display_list();
    runtime.compile_display_list();
    let second = viewport_scenes(&runtime);

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert!(
        Arc::ptr_eq(&first[0], &shared),
        "the command must hold the caller's Arc, not a deep copy"
    );
    assert!(Arc::ptr_eq(&first[0], &second[0]));
}

#[test]
fn click_without_handlers_does_not_rebuild_the_layout_tree() {
    let view = rsx! {
        <div class="w-full h-full">
            <div class="w-32 h-32 bg-red-500">"static"</div>
        </div>
    };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.tick_frame();
    runtime.finish_frame_profile();

    runtime.step_event(&PlatformEvent::MouseDown {
        button: strata_platform::MouseButton::Left,
        x: 10.0,
        y: 10.0,
    });
    runtime.step_event(&PlatformEvent::MouseUp {
        button: strata_platform::MouseButton::Left,
        x: 10.0,
        y: 10.0,
    });
    let profile = runtime.finish_frame_profile();
    assert!(
        profile.rebuild.is_zero(),
        "press and release on an inert element must not rebuild, spent {:?}",
        profile.rebuild
    );
}

#[test]
fn frame_profile_records_phases_and_invokes_the_hook() {
    let view = rsx! { <div class="w-full h-full">"hello"</div> };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let seen_hook = std::rc::Rc::clone(&seen);
    runtime.set_frame_profile_hook(Some(Box::new(move |_| {
        seen_hook.set(seen_hook.get() + 1);
    })));
    runtime.rebuild_layout_tree();
    runtime.tick_frame();
    let profile = runtime.frame_profile();
    assert!(!profile.layout.is_zero());
    assert!(!profile.compile.is_zero());
    let done = runtime.finish_frame_profile();
    assert_eq!(done, profile);
    assert_eq!(seen.get(), 1);
    assert!(runtime.frame_profile().total().is_zero());
}

#[test]
fn path_batch_is_skipped_when_the_frame_has_no_paths() {
    let view = rsx! { <div class="w-full h-full bg-slate-900" /> };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.tick_frame();
    let frames_before = runtime.gpu_path_pipeline.compiled_frames;
    runtime.compile_display_list();
    runtime.compile_display_list();
    assert_eq!(
        runtime.gpu_path_pipeline.compiled_frames, frames_before,
        "no path commands means no path compile"
    );
    assert!(runtime.gpu_path_segments().is_empty());
}

#[test]
fn dirty_gated_compile_skips_unchanged_frames_and_recompiles_on_change() {
    let view = rsx! { <div class="w-full h-full bg-slate-900">"static"</div> };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.tick_frame();
    assert!(!runtime.is_paint_dirty());

    let skipped = runtime.compiles_skipped();
    assert!(!runtime.compile_display_list_if_dirty());
    assert!(!runtime.compile_display_list_if_dirty());
    assert_eq!(runtime.compiles_skipped(), skipped + 2);

    runtime.mark_paint_dirty();
    assert!(runtime.compile_display_list_if_dirty());
    assert!(!runtime.is_paint_dirty());
}

#[test]
fn input_that_requests_a_redraw_marks_the_list_dirty() {
    let view = rsx! { <div class="w-full h-full">"x"</div> };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.tick_frame();
    assert!(!runtime.is_paint_dirty());
    let redraw = runtime.step_event(&PlatformEvent::MouseDown {
        button: strata_platform::MouseButton::Left,
        x: 5.0,
        y: 5.0,
    });
    assert!(redraw);
    assert!(runtime.is_paint_dirty());
}
