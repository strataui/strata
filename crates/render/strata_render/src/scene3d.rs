//! Unified 3D Engine: High-level Three.js ergonomics paired seamlessly with low-level Vulkan/OpenGL hardware pipelines.

#![allow(unused_variables)]

use std::collections::BTreeMap;
use strata_style::Color;

/// 3D Spatial Transformation
#[derive(Debug, Clone, PartialEq)]
pub struct Transform3D {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for Transform3D {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

impl Transform3D {
    pub const fn from_position(position: [f32; 3]) -> Self {
        Self {
            position,
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

/// 3D Camera Abstractions
#[derive(Debug, Clone, PartialEq)]
pub enum Camera3D {
    Perspective {
        fov: f32,
        aspect: f32,
        near: f32,
        far: f32,
        position: [f32; 3],
        target: [f32; 3],
    },
    Orthographic {
        left: f32,
        right: f32,
        top: f32,
        bottom: f32,
        near: f32,
        far: f32,
        position: [f32; 3],
        target: [f32; 3],
    },
}

impl Default for Camera3D {
    fn default() -> Self {
        Self::Perspective {
            fov: 60.0,
            aspect: 16.0 / 9.0,
            near: 0.1,
            far: 1000.0,
            position: [0.0, 2.0, 5.0],
            target: [0.0, 0.0, 0.0],
        }
    }
}

/// 3D Lighting
#[derive(Debug, Clone, PartialEq)]
pub enum Light3D {
    Ambient {
        color: Color,
        intensity: f32,
    },
    Directional {
        position: [f32; 3],
        color: Color,
        intensity: f32,
        cast_shadow: bool,
    },
    Point {
        position: [f32; 3],
        color: Color,
        intensity: f32,
        distance: f32,
    },
}

/// 3D Geometry: Supports both parametric Three.js-style primitives and low-level custom vertex buffers.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry3D {
    Box {
        width: f32,
        height: f32,
        depth: f32,
    },
    Sphere {
        radius: f32,
        width_segments: u32,
        height_segments: u32,
    },
    Plane {
        width: f32,
        height: f32,
    },
    Cylinder {
        radius_top: f32,
        radius_bottom: f32,
        height: f32,
        radial_segments: u32,
    },
    Custom {
        vertices: Vec<f32>,
        indices: Vec<u32>,
    },
}

/// Low-Level Hardware Shader Source
#[derive(Debug, Clone, PartialEq)]
pub enum ShaderSource {
    SpirV(Vec<u32>),
    Glsl(String),
    Wgsl(String),
    Msl(String),
}

/// 3D Material: Unifies high-level PBR workflows with low-level custom hardware shaders.
#[derive(Debug, Clone, PartialEq)]
pub enum Material3D {
    /// High-Level Physically Based Rendering (PBR) Metallic-Roughness workflow
    Standard {
        color: Color,
        roughness: f32,
        metalness: f32,
        emissive: Option<Color>,
    },
    /// High-Level PBR with Clearcoat extensions
    Physical {
        color: Color,
        roughness: f32,
        metalness: f32,
        clearcoat: f32,
        transmission: f32,
    },
    /// High-Level Unlit flat material
    Basic {
        color: Color,
    },
    /// Low-Level Custom Hardware Shader Material
    Shader {
        vertex_shader: Option<ShaderSource>,
        fragment_shader: Option<ShaderSource>,
        glsl_fragment: Option<String>,
        uniforms: BTreeMap<String, f32>,
    },
}

impl Default for Material3D {
    fn default() -> Self {
        Self::Standard {
            color: Color::rgba(255, 255, 255, 255),
            roughness: 0.5,
            metalness: 0.0,
            emissive: None,
        }
    }
}

/// A 3D Mesh in the Scene Graph: Seamlessly pairs high-level or low-level geometry with materials.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh3D {
    pub geometry: Geometry3D,
    pub material: Material3D,
    pub transform: Transform3D,
}

impl Mesh3D {
    pub fn new(geometry: Geometry3D, material: Material3D) -> Self {
        Self {
            geometry,
            material,
            transform: Transform3D::default(),
        }
    }

    pub fn with_transform(mut self, transform: Transform3D) -> Self {
        self.transform = transform;
        self
    }
}

/// Top-Level 3D Scene representation embedded in a Strata Viewport.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Scene3D {
    pub camera: Camera3D,
    pub lights: Vec<Light3D>,
    pub meshes: Vec<Mesh3D>,
}

impl Scene3D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_mesh(&mut self, mesh: Mesh3D) {
        self.meshes.push(mesh);
    }

    pub fn add_light(&mut self, light: Light3D) {
        self.lights.push(light);
    }
}

// ---------------------------------------------------------------------------
// Declarative 3D RSX Viewport Components
// ---------------------------------------------------------------------------

use strata_dom::{NodeBlueprint, Tag, View};
use strata_macro::component;

#[component]
pub fn Canvas3D(class: Option<&'static str>, children: Option<Vec<NodeBlueprint>>) -> View {
    let classes = class
        .map(|c| c.split_whitespace().map(|s| s.to_string()).collect())
        .unwrap_or_default();
    View::new(NodeBlueprint::element(
        Tag::Canvas,
        classes,
        Vec::new(),
        children.unwrap_or_default(),
    ))
}

#[component]
pub fn PerspectiveCamera(
    fov: Option<f32>,
    aspect: Option<f32>,
    near: Option<f32>,
    far: Option<f32>,
    position: Option<[f32; 3]>,
    target: Option<[f32; 3]>,
) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("PerspectiveCamera"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

#[component]
pub fn AmbientLight(color: Option<&'static str>, intensity: Option<f32>) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("AmbientLight"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

#[component]
pub fn DirectionalLight(
    position: Option<[f32; 3]>,
    color: Option<&'static str>,
    intensity: Option<f32>,
    cast_shadow: Option<bool>,
) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("DirectionalLight"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

#[component]
pub fn Mesh(
    position: Option<[f32; 3]>,
    rotation: Option<[f32; 3]>,
    scale: Option<[f32; 3]>,
    children: Option<Vec<NodeBlueprint>>,
) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("Mesh"),
        Vec::new(),
        Vec::new(),
        children.unwrap_or_default(),
    ))
}

#[component]
pub fn BoxGeometry(width: Option<f32>, height: Option<f32>, depth: Option<f32>) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("BoxGeometry"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

#[component]
pub fn SphereGeometry(
    radius: Option<f32>,
    width_segments: Option<u32>,
    height_segments: Option<u32>,
) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("SphereGeometry"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

#[component]
pub fn StandardMaterial(
    color: Option<&'static str>,
    roughness: Option<f32>,
    metalness: Option<f32>,
) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("StandardMaterial"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

#[component]
pub fn ShaderMaterial(
    glsl_fragment: Option<&'static str>,
    uniform_time: Option<f32>,
    uniforms: Option<strata_js::JsValue>,
) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("ShaderMaterial"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}
