//! Tests for headless AppRuntime screenshot export and offscreen rendering.

use strata::prelude::*;
use strata::runtime::AppRuntime;

#[test]
fn test_app_runtime_render_to_framebuffer_and_export_png() {
    let view = rsx! {
        <div class="w-full h-full bg-slate-900 flex items-center justify-center">
            <div class="w-64 h-32 bg-blue-600 rounded-lg shadow-md p-4 text-white">
                <span class="text-xl font-bold">"Strata Screenshot Test"</span>
            </div>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);

    // 1. Offscreen FrameBuffer rasterization
    let fb = runtime.render_to_framebuffer();
    assert_eq!(fb.width, 800);
    assert_eq!(fb.height, 600);
    assert_eq!(fb.pixels.len(), 800 * 600 * 4);

    // 2. Headless PNG export
    let tmp_path = std::env::temp_dir().join(format!(
        "strata_runtime_screenshot_test_{}.png",
        std::process::id()
    ));
    let export_res = runtime.export_png(&tmp_path);
    assert!(
        export_res.is_ok(),
        "runtime.export_png must succeed: {:?}",
        export_res
    );

    assert!(tmp_path.exists());
    let file_bytes = std::fs::read(&tmp_path).expect("Must read exported screenshot");
    assert!(file_bytes.len() > 100);
    assert_eq!(
        &file_bytes[0..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
    );

    let _ = std::fs::remove_file(&tmp_path);
}
