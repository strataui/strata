use strata::dom::View;
use strata::gpu_path::{
    CurveComputeDispatcher, FillRule, LineCap, LineJoin, PathBuilder, StrokeOptions,
    FLAG_FILL, FLAG_STROKE,
};
use strata::{component, rsx};
use strata::render::compositor::DrawCommand;
use strata::style::types::Color;
use strata::runtime::AppRuntime;
use strata_layout::Rect;

#[component]
fn VectorPathDemo() -> View {
    rsx! {
        <div class="flex flex-col w-[800px] h-[600px] bg-neutral-950 p-6">
            <h1 class="text-white text-xl font-bold mb-4">"Direct Hardware Vector Path Telemetry"</h1>
            <svg class="w-[200px] h-[200px] text-emerald-500" viewBox="0 0 100 100">
                <path d="M 10 10 L 90 10 L 90 90 L 10 90 Z" fill="currentColor" />
            </svg>
        </div>
    }
}

#[test]
fn test_runtime_gpu_path_compilation_and_segments() {
    let mut runtime = AppRuntime::new(VectorPathDemo(), 800.0, 600.0, 1.0);
    runtime.tick_frame();

    let initial_count = runtime.gpu_path_segments().len();
    assert!(initial_count > 0, "Display list paths should compile to GPU path segments");
    assert_eq!(runtime.gpu_path_uniform_bytes().len(), 16);

    // Inject direct hardware path command
    let path = PathBuilder::from_svg("M 20 20 Q 100 200 180 20 Z").with_fill(FillRule::NonZero);
    runtime.display_list.push(DrawCommand::DrawGpuPath {
        bounds: Rect::new(20.0, 20.0, 160.0, 180.0),
        path,
        stroke: Some(
            StrokeOptions::new(3.0)
                .with_cap(LineCap::Round)
                .with_join(LineJoin::Round),
        ),
        fill_color: Some(Color::rgb(59, 130, 246)),
        stroke_color: Some(Color::rgb(29, 78, 216)),
        transform: None,
    });

    let count = runtime.gpu_path_pipeline.compile_frame(&runtime.display_list, None);
    assert!(count > initial_count, "Direct hardware path should expand segments");

    let uniforms_bytes = runtime.gpu_path_uniform_bytes();
    let curve_count = u32::from_ne_bytes(uniforms_bytes[12..16].try_into().unwrap());
    assert_eq!(curve_count, count as u32);
}

#[test]
fn test_runtime_direct_hardware_path_damage_scissoring() {
    let mut runtime = AppRuntime::new(VectorPathDemo(), 800.0, 600.0, 1.0);
    runtime.tick_frame();

    // Damage rect outside the SVG icon in top-left
    let damage = Rect::new(500.0, 400.0, 200.0, 150.0);
    let damaged_count = runtime.gpu_path_pipeline.compile_frame(&runtime.display_list, Some(damage));
    assert_eq!(damaged_count, 0, "Segments completely outside damage box should be culled");
}

#[test]
fn test_compute_pipeline_curve_flattening_simulation() {
    let mut dispatcher = CurveComputeDispatcher::new(0.25);
    let path = PathBuilder::from_svg("M 0 0 C 30 90 70 90 100 0 Z");
    let quads = path.to_quadratics(0.25);

    for q in quads {
        dispatcher.push_quadratic(&q, 2.0, FLAG_FILL | FLAG_STROKE, [0.2, 0.4, 0.8, 1.0], 1.0);
    }

    assert!(!dispatcher.is_empty());
    let flattened = dispatcher.dispatch_compute();
    assert_eq!(flattened.len(), dispatcher.len());
    assert_eq!(dispatcher.input_bytes().len(), dispatcher.len() * 64);
}
