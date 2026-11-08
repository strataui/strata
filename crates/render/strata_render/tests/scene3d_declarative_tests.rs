//! Integration test suite for GOAL-03: Real 3D Viewport in RSX with Declarative Scene Graph Compilation.

use strata_macro::rsx;
use strata_render::scene3d::*;
use strata_style::Color;

#[test]
fn test_declarative_scene3d_perspective_camera_compilation() {
    let view = rsx! {
        <Canvas3D class="w-full h-96">
            <PerspectiveCamera
                fov={75.0}
                near={0.5}
                far={500.0}
                position={[1.0, 2.0, 3.0]}
                target={[0.0, 1.0, 0.0]}
            />
        </Canvas3D>
    };

    let scene = compile_scene3d_from_blueprint(&view.root);

    match scene.camera {
        Camera3D::Perspective {
            fov_horizontal_deg,
            near,
            far,
            position,
            target,
        } => {
            assert_eq!(fov_horizontal_deg, 75.0);
            assert_eq!(near, 0.5);
            assert_eq!(far, 500.0);
            assert_eq!(position, [1.0, 2.0, 3.0]);
            assert_eq!(target, [0.0, 1.0, 0.0]);
        }
        _ => panic!("Expected Perspective camera, got Orthographic"),
    }
}

#[test]
fn test_declarative_scene3d_lights_compilation() {
    let view = rsx! {
        <Canvas3D class="w-full h-96">
            <AmbientLight color="#ffffff" intensity={0.4} />
            <DirectionalLight
                position={[10.0, 20.0, 30.0]}
                color="#ffffaa"
                intensity={1.2}
                cast_shadow={true}
            />
            <PointLight
                position={[0.0, 5.0, 0.0]}
                color="#ff5555"
                intensity={2.0}
                distance={50.0}
            />
        </Canvas3D>
    };

    let scene = compile_scene3d_from_blueprint(&view.root);
    assert_eq!(scene.lights.len(), 3);

    match &scene.lights[0] {
        Light3D::Ambient { color, intensity } => {
            assert_eq!(*color, Color::WHITE);
            assert_eq!(*intensity, 0.4);
        }
        _ => panic!("Expected AmbientLight at index 0"),
    }

    match &scene.lights[1] {
        Light3D::Directional {
            position,
            color,
            intensity,
            cast_shadow,
        } => {
            assert_eq!(*position, [10.0, 20.0, 30.0]);
            assert_eq!(*color, Color::rgb(0xff, 0xff, 0xaa));
            assert_eq!(*intensity, 1.2);
            assert!(*cast_shadow);
        }
        _ => panic!("Expected DirectionalLight at index 1"),
    }

    match &scene.lights[2] {
        Light3D::Point {
            position,
            color,
            intensity,
            distance,
        } => {
            assert_eq!(*position, [0.0, 5.0, 0.0]);
            assert_eq!(*color, Color::rgb(0xff, 0x55, 0x55));
            assert_eq!(*intensity, 2.0);
            assert_eq!(*distance, 50.0);
        }
        _ => panic!("Expected PointLight at index 2"),
    }
}

#[test]
fn test_declarative_scene3d_mesh_box_standard_material_compilation() {
    let view = rsx! {
        <Canvas3D class="w-full h-96">
            <Mesh
                position={[2.0, 0.0, -5.0]}
                rotation={[0.0, 45.0, 0.0]}
                scale={[1.5, 1.5, 1.5]}
            >
                <BoxGeometry width={2.0} height={4.0} depth={6.0} />
                <StandardMaterial color="#3b82f6" roughness={0.25} metalness={0.8} />
            </Mesh>
        </Canvas3D>
    };

    let scene = compile_scene3d_from_blueprint(&view.root);
    assert_eq!(scene.meshes.len(), 1);

    let mesh = &scene.meshes[0];
    assert_eq!(mesh.transform.position, [2.0, 0.0, -5.0]);
    assert_eq!(mesh.transform.rotation, [0.0, 45.0, 0.0]);
    assert_eq!(mesh.transform.scale, [1.5, 1.5, 1.5]);

    match mesh.geometry {
        Geometry3D::Box { width, height, depth } => {
            assert_eq!(width, 2.0);
            assert_eq!(height, 4.0);
            assert_eq!(depth, 6.0);
        }
        _ => panic!("Expected BoxGeometry"),
    }

    match &mesh.material {
        Material3D::Standard {
            color,
            roughness,
            metalness,
            ..
        } => {
            assert_eq!(*color, Color::rgb(59, 130, 246));
            assert_eq!(*roughness, 0.25);
            assert_eq!(*metalness, 0.8);
        }
        _ => panic!("Expected StandardMaterial"),
    }
}

#[test]
fn test_declarative_scene3d_sphere_shader_material_compilation() {
    let view = rsx! {
        <Canvas3D class="w-full h-96">
            <Mesh position={[0.0, 1.0, 0.0]}>
                <SphereGeometry radius={3.5} width_segments={64} height_segments={32} />
                <ShaderMaterial glsl_fragment={"void main() { gl_FragColor = vec4(1.0); }"} uniform_time={4.2} />
            </Mesh>
        </Canvas3D>
    };

    let scene = compile_scene3d_from_blueprint(&view.root);
    assert_eq!(scene.meshes.len(), 1);

    let mesh = &scene.meshes[0];
    match mesh.geometry {
        Geometry3D::Sphere {
            radius,
            width_segments,
            height_segments,
        } => {
            assert_eq!(radius, 3.5);
            assert_eq!(width_segments, 64);
            assert_eq!(height_segments, 32);
        }
        _ => panic!("Expected SphereGeometry"),
    }

    match &mesh.material {
        Material3D::Shader {
            glsl_fragment,
            uniforms,
            ..
        } => {
            assert_eq!(
                glsl_fragment.as_deref(),
                Some("void main() { gl_FragColor = vec4(1.0); }")
            );
            assert_eq!(uniforms.get("u_time"), Some(&4.2));
        }
        _ => panic!("Expected ShaderMaterial"),
    }
}
