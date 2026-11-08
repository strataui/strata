//! Integration tests for GOAL-03: Real 3D Viewport in RSX and Display List Pipeline.

use strata::prelude::*;
use strata_render::compositor::DrawCommand;

#[test]
fn test_canvas3d_declarative_pipeline_in_runtime() {
    let view = rsx! {
        <div class="flex flex-col w-full h-full p-4 bg-slate-900">
            <h1 class="text-xl font-bold text-white mb-2">"3D Scene Test"</h1>
            <Canvas3D class="w-full h-96">
                <PerspectiveCamera
                    fov={60.0}
                    near={0.1}
                    far={1000.0}
                    position={[0.0, 5.0, 10.0]}
                    target={[0.0, 0.0, 0.0]}
                />
                <AmbientLight color="#ffffff" intensity={0.5} />
                <DirectionalLight
                    position={[1.0, 2.0, 1.0]}
                    color="#ffffff"
                    intensity={1.0}
                />
                <Mesh position={[0.0, 1.0, 0.0]}>
                    <BoxGeometry width={2.0} height={2.0} depth={2.0} />
                    <StandardMaterial color="#3b82f6" roughness={0.4} metalness={0.1} />
                </Mesh>
            </Canvas3D>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);
    runtime.tick_frame();

    // Verify display list contains a DrawViewport3D command
    let viewport_cmd = runtime
        .display_list
        .commands
        .iter()
        .find(|cmd| matches!(cmd, DrawCommand::DrawViewport3D { .. }));

    assert!(
        viewport_cmd.is_some(),
        "Expected display list to contain DrawViewport3D command from <Canvas3D>"
    );

    if let Some(DrawCommand::DrawViewport3D { bounds, scene }) = viewport_cmd {
        // Bounds should match computed layout (800 wide minus padding, h-96 is 384px)
        assert!(bounds.width > 700.0, "Expected width > 700, got {}", bounds.width);
        assert_eq!(bounds.height, 384.0, "Expected height 384px (h-96), got {}", bounds.height);

        // Verify camera
        match &scene.camera {
            Camera3D::Perspective {
                fov_horizontal_deg,
                near,
                far,
                position,
                target,
            } => {
                assert!((fov_horizontal_deg - 60.0).abs() < 1e-4);
                assert!((near - 0.1).abs() < 1e-4);
                assert!((far - 1000.0).abs() < 1e-4);
                assert_eq!(*position, [0.0, 5.0, 10.0]);
                assert_eq!(*target, [0.0, 0.0, 0.0]);
            }
            _ => panic!("Expected Perspective camera in scene"),
        }

        // Verify lights
        assert_eq!(scene.lights.len(), 2, "Expected 2 lights in compiled scene");
        let has_ambient = scene.lights.iter().any(|l| matches!(l, Light3D::Ambient { intensity, .. } if (*intensity - 0.5).abs() < 1e-4));
        assert!(has_ambient, "Expected ambient light with intensity 0.5");

        // Verify meshes
        assert_eq!(scene.meshes.len(), 1, "Expected 1 mesh in compiled scene");
        let mesh = &scene.meshes[0];
        assert_eq!(mesh.transform.position, [0.0, 1.0, 0.0]);
        match &mesh.geometry {
            Geometry3D::Box { width, height, depth } => {
                assert_eq!(*width, 2.0);
                assert_eq!(*height, 2.0);
                assert_eq!(*depth, 2.0);
            }
            _ => panic!("Expected Box geometry on mesh"),
        }
        match &mesh.material {
            Material3D::Standard { roughness, metalness, .. } => {
                assert!((roughness - 0.4).abs() < 1e-4);
                assert!((metalness - 0.1).abs() < 1e-4);
            }
            _ => panic!("Expected Standard material on mesh"),
        }
    }
}
