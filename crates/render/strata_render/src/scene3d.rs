//! Unified 3D Engine: High-level Three.js ergonomics paired seamlessly with low-level Vulkan/OpenGL hardware pipelines.

#![allow(unused_variables)]

use std::collections::BTreeMap;
use crate::overlay3d::Overlay3D;
use strata_style::Color;

pub use crate::texture3d::{
    SamplerDesc, TextureData, TextureDesc, TextureId, TextureStore, UvTransform,
};

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
    /// Custom vertex buffer with explicit texture coordinates.
    ///
    /// `vertices` is an eight-float stride: `x, y, z, nx, ny, nz, u, v`. Use this
    /// for tile quads, relief walls, and building prisms whose UVs are authored
    /// per corner rather than derived from the primitive generator.
    CustomTextured {
        vertices: Vec<f32>,
        indices: Vec<u32>,
    },
}

/// Textured surface material.
#[derive(Debug, Clone, PartialEq)]
pub struct TexturedMaterial3D {
    /// Texture sampled per fragment.
    pub texture: TextureId,
    /// Multiplied with the sampled RGBA before lighting.
    pub tint: Color,
    /// Sub-rect of the texture addressed by the mesh UVs.
    pub uv_transform: UvTransform,
    /// Whether the surface receives scene lighting. Tiles are usually unlit
    /// because the imagery already carries shading.
    pub lit: bool,
    /// How much of the sampled alpha to honour, 0..=1.
    pub opacity: f32,
}

impl TexturedMaterial3D {
    pub fn new(texture: TextureId) -> Self {
        Self {
            texture,
            tint: Color::rgba(255, 255, 255, 255),
            uv_transform: UvTransform::IDENTITY,
            lit: false,
            opacity: 1.0,
        }
    }

    pub fn with_uv_transform(mut self, transform: UvTransform) -> Self {
        self.uv_transform = transform;
        self
    }

    pub fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }
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
    Basic { color: Color },
    /// Low-Level Custom Hardware Shader Material
    Shader {
        vertex_shader: Option<ShaderSource>,
        fragment_shader: Option<ShaderSource>,
        glsl_fragment: Option<String>,
        uniforms: BTreeMap<String, f32>,
    },
    /// Textured surface sampled from a [`TextureId`].
    Textured(TexturedMaterial3D),
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

/// What the rasterizer fills before any geometry is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Background3D {
    /// Leave the existing framebuffer contents untouched.
    #[default]
    Transparent,
    /// A single colour.
    Solid(Color),
    /// A vertical gradient from `top` to `bottom` over the viewport.
    VerticalGradient { top: Color, bottom: Color },
}

impl Background3D {
    /// Returns the colour at a normalized vertical position `t` (0 = top).
    pub fn color_at(self, t: f32) -> Option<Color> {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Transparent => None,
            Self::Solid(color) => Some(color),
            Self::VerticalGradient { top, bottom } => {
                let mix = |a: f32, b: f32| -> u8 {
                    ((a * (1.0 - t) + b * t) * 255.0).round().clamp(0.0, 255.0) as u8
                };
                Some(Color::rgba(
                    mix(top.r(), bottom.r()),
                    mix(top.g(), bottom.g()),
                    mix(top.b(), bottom.b()),
                    mix(top.a(), bottom.a()),
                ))
            }
        }
    }
}

/// An emissive celestial body drawn as a billboard: the sun, the moon, or a
/// sensor anomaly marker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CelestialBody3D {
    /// Scene-space position of the body centre.
    pub position: [f64; 3],
    /// Apparent radius in scene units.
    pub radius: f64,
    pub color: Color,
    /// Soft edge width in pixels; 0 draws a hard disc.
    pub glow: f32,
    /// Whether the body participates in depth testing. The sun and stars
    /// usually do not.
    pub depth_test: bool,
}

/// A translucent spherical shell drawn over the surface: clouds, a night arc,
/// or a coverage wash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudShell3D {
    /// Scene-space centre of the shell.
    pub centre: [f64; 3],
    /// Radius of the shell in scene units.
    pub radius: f64,
    pub color: Color,
    /// Fraction of the shell that is cloud, 0..=1.
    pub coverage: f32,
    /// Shell opacity at full coverage, 0..=1.
    pub opacity: f32,
}

impl CloudShell3D {
    /// Opacity after applying the coverage ramp.
    ///
    /// The reference ramps cloud opacity from clear to full as density rises;
    /// clamping here keeps every consumer on the same curve.
    pub fn effective_opacity(&self) -> f32 {
        (self.coverage.clamp(0.0, 1.0) * self.opacity.clamp(0.0, 1.0)).clamp(0.0, 1.0)
    }
}

/// Top-Level 3D Scene representation embedded in a Strata Viewport.
///
/// The background, celestial, cloud, and overlay state are private so adding
/// them does not break struct literals written by existing callers. Use the
/// `with_*` builders; the accessors below read them back.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Scene3D {
    pub camera: Camera3D,
    pub lights: Vec<Light3D>,
    pub meshes: Vec<Mesh3D>,
    background: Background3D,
    celestial: Vec<CelestialBody3D>,
    cloud_shells: Vec<CloudShell3D>,
    overlay: Option<Overlay3D>,
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

    /// Replaces the background fill used before geometry is drawn.
    pub fn with_background(mut self, background: Background3D) -> Self {
        self.background = background;
        self
    }

    /// Adds an emissive celestial body.
    pub fn with_celestial(mut self, body: CelestialBody3D) -> Self {
        self.celestial.push(body);
        self
    }

    /// Adds a translucent cloud shell.
    pub fn with_cloud_shell(mut self, shell: CloudShell3D) -> Self {
        self.cloud_shells.push(shell);
        self
    }

    /// Attaches the overlay pass drawn after geometry.
    pub fn with_overlay(mut self, overlay: Overlay3D) -> Self {
        self.overlay = Some(overlay);
        self
    }

    pub fn overlay(&self) -> Option<&Overlay3D> {
        self.overlay.as_ref()
    }

    pub fn background(&self) -> Background3D {
        self.background
    }

    pub fn celestial(&self) -> &[CelestialBody3D] {
        &self.celestial
    }

    pub fn cloud_shells(&self) -> &[CloudShell3D] {
        &self.cloud_shells
    }
}

/// Callback that rasterizes a text run into the framebuffer.
///
/// The 3D overlay pass needs text, but `strata_render` must not depend on the
/// platform text backend. Consumers install a rasterizer, exactly like the
/// layout crate installs a text measurer.
pub type TextRasterizer = crate::overlay3d::TextRasterizer;

thread_local! {
    static TEXT_RASTERIZER: std::cell::Cell<Option<TextRasterizer>> = const { std::cell::Cell::new(None) };
}

/// Installs the overlay text rasterizer for this thread.
pub fn register_overlay_text_rasterizer(rasterizer: TextRasterizer) {
    TEXT_RASTERIZER.with(|slot| slot.set(Some(rasterizer)));
}

/// Returns the installed overlay text rasterizer, if any.
pub fn overlay_text_rasterizer() -> Option<TextRasterizer> {
    TEXT_RASTERIZER.with(|slot| slot.get())
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

#[component]
pub fn OrthographicCamera(
    left: Option<f32>,
    right: Option<f32>,
    top: Option<f32>,
    bottom: Option<f32>,
    near: Option<f32>,
    far: Option<f32>,
    position: Option<[f32; 3]>,
    target: Option<[f32; 3]>,
) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("OrthographicCamera"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

#[component]
pub fn PointLight(
    position: Option<[f32; 3]>,
    color: Option<&'static str>,
    intensity: Option<f32>,
    distance: Option<f32>,
) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("PointLight"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

#[component]
pub fn PlaneGeometry(width: Option<f32>, height: Option<f32>) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("PlaneGeometry"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

#[component]
pub fn CylinderGeometry(
    radius_top: Option<f32>,
    radius_bottom: Option<f32>,
    height: Option<f32>,
    radial_segments: Option<u32>,
) -> View {
    View::new(NodeBlueprint::element(
        Tag::Custom("CylinderGeometry"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}
