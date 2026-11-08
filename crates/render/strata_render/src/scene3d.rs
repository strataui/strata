//! Unified 3D Engine: High-level Three.js ergonomics paired seamlessly with low-level Vulkan/OpenGL hardware pipelines.

#![allow(unused_variables)]

use crate::overlay3d::Overlay3D;
use std::collections::BTreeMap;
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
        /// Horizontal field of view in degrees. The projection matrix derives
        /// the vertical field of view from the viewport's aspect ratio.
        fov_horizontal_deg: f32,
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
            fov_horizontal_deg: 60.0,
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

/// One index range of a batched textured mesh, with its own material.
#[derive(Debug, Clone, PartialEq)]
pub struct TexturedDraw3D {
    pub material: TexturedMaterial3D,
    /// First index in the shared index buffer.
    pub index_start: u32,
    /// Number of indices, a multiple of three.
    pub index_count: u32,
}

impl TexturedDraw3D {
    pub fn new(material: TexturedMaterial3D, index_start: u32, index_count: u32) -> Self {
        Self {
            material,
            index_start,
            index_count,
        }
    }
}

/// A textured mesh with several texture draws sharing one vertex and index
/// buffer.
///
/// Terrain patches each need their own image, but setting up one mesh per patch
/// repeats the vertex transform for every patch. A batch transforms the shared
/// buffer once and switches textures per index range.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TexturedBatchMaterial3D {
    pub draws: Vec<TexturedDraw3D>,
}

impl TexturedBatchMaterial3D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, draw: TexturedDraw3D) -> &mut Self {
        self.draws.push(draw);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }
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
    /// Several textured index ranges sharing one vertex buffer.
    TexturedBatch(TexturedBatchMaterial3D),
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
    /// Camera up hint in world space. `None` uses the renderer's fallback.
    camera_up: Option<[f64; 3]>,
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

    /// Sets the camera's up direction. A downward-looking camera has no unique
    /// screen up from the world Y axis, so scenes that care about roll should
    /// declare it, for example the local east-north-up frame at a ground focus.
    pub fn with_camera_up(mut self, up: [f64; 3]) -> Self {
        self.camera_up = Some(up);
        self
    }

    pub fn camera_up(&self) -> Option<[f64; 3]> {
        self.camera_up
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

use strata_dom::{Attribute, NodeBlueprint, Tag, View};
use strata_macro::component;

#[component]
pub fn Canvas3D(class: Option<&'static str>, children: Option<Vec<NodeBlueprint>>) -> View {
    let classes: strata_dom::ClassList = class.map(strata_dom::ClassList::from).unwrap_or_default();
    View::new(NodeBlueprint::element(
        Tag::Canvas,
        classes,
        vec![Attribute::property("data-strata-canvas3d", "true")],
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
    let mut attrs = Vec::new();
    if let Some(f) = fov {
        attrs.push(Attribute::property("data-fov", f.to_string()));
    }
    if let Some(a) = aspect {
        attrs.push(Attribute::property("data-aspect", a.to_string()));
    }
    if let Some(n) = near {
        attrs.push(Attribute::property("data-near", n.to_string()));
    }
    if let Some(fa) = far {
        attrs.push(Attribute::property("data-far", fa.to_string()));
    }
    if let Some(p) = position {
        attrs.push(Attribute::property("data-position", format!("{},{},{}", p[0], p[1], p[2])));
    }
    if let Some(t) = target {
        attrs.push(Attribute::property("data-target", format!("{},{},{}", t[0], t[1], t[2])));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("PerspectiveCamera"),
        strata_dom::ClassList::default(),
        attrs,
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
    let mut attrs = Vec::new();
    if let Some(v) = left { attrs.push(Attribute::property("data-left", v.to_string())); }
    if let Some(v) = right { attrs.push(Attribute::property("data-right", v.to_string())); }
    if let Some(v) = top { attrs.push(Attribute::property("data-top", v.to_string())); }
    if let Some(v) = bottom { attrs.push(Attribute::property("data-bottom", v.to_string())); }
    if let Some(v) = near { attrs.push(Attribute::property("data-near", v.to_string())); }
    if let Some(v) = far { attrs.push(Attribute::property("data-far", v.to_string())); }
    if let Some(p) = position {
        attrs.push(Attribute::property("data-position", format!("{},{},{}", p[0], p[1], p[2])));
    }
    if let Some(t) = target {
        attrs.push(Attribute::property("data-target", format!("{},{},{}", t[0], t[1], t[2])));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("OrthographicCamera"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn AmbientLight(color: Option<&'static str>, intensity: Option<f32>) -> View {
    let mut attrs = Vec::new();
    if let Some(c) = color {
        attrs.push(Attribute::property("data-color", c));
    }
    if let Some(i) = intensity {
        attrs.push(Attribute::property("data-intensity", i.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("AmbientLight"),
        strata_dom::ClassList::default(),
        attrs,
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
    let mut attrs = Vec::new();
    if let Some(p) = position {
        attrs.push(Attribute::property("data-position", format!("{},{},{}", p[0], p[1], p[2])));
    }
    if let Some(c) = color {
        attrs.push(Attribute::property("data-color", c));
    }
    if let Some(i) = intensity {
        attrs.push(Attribute::property("data-intensity", i.to_string()));
    }
    if let Some(s) = cast_shadow {
        attrs.push(Attribute::property("data-cast-shadow", s.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("DirectionalLight"),
        strata_dom::ClassList::default(),
        attrs,
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
    let mut attrs = Vec::new();
    if let Some(p) = position {
        attrs.push(Attribute::property("data-position", format!("{},{},{}", p[0], p[1], p[2])));
    }
    if let Some(c) = color {
        attrs.push(Attribute::property("data-color", c));
    }
    if let Some(i) = intensity {
        attrs.push(Attribute::property("data-intensity", i.to_string()));
    }
    if let Some(d) = distance {
        attrs.push(Attribute::property("data-distance", d.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("PointLight"),
        strata_dom::ClassList::default(),
        attrs,
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
    let mut attrs = Vec::new();
    if let Some(p) = position {
        attrs.push(Attribute::property("data-position", format!("{},{},{}", p[0], p[1], p[2])));
    }
    if let Some(r) = rotation {
        attrs.push(Attribute::property("data-rotation", format!("{},{},{}", r[0], r[1], r[2])));
    }
    if let Some(s) = scale {
        attrs.push(Attribute::property("data-scale", format!("{},{},{}", s[0], s[1], s[2])));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("Mesh"),
        strata_dom::ClassList::default(),
        attrs,
        children.unwrap_or_default(),
    ))
}

#[component]
pub fn BoxGeometry(width: Option<f32>, height: Option<f32>, depth: Option<f32>) -> View {
    let mut attrs = Vec::new();
    if let Some(w) = width { attrs.push(Attribute::property("data-width", w.to_string())); }
    if let Some(h) = height { attrs.push(Attribute::property("data-height", h.to_string())); }
    if let Some(d) = depth { attrs.push(Attribute::property("data-depth", d.to_string())); }
    View::new(NodeBlueprint::element(
        Tag::Custom("BoxGeometry"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn SphereGeometry(
    radius: Option<f32>,
    width_segments: Option<u32>,
    height_segments: Option<u32>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(r) = radius { attrs.push(Attribute::property("data-radius", r.to_string())); }
    if let Some(w) = width_segments { attrs.push(Attribute::property("data-width-segments", w.to_string())); }
    if let Some(h) = height_segments { attrs.push(Attribute::property("data-height-segments", h.to_string())); }
    View::new(NodeBlueprint::element(
        Tag::Custom("SphereGeometry"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn PlaneGeometry(width: Option<f32>, height: Option<f32>) -> View {
    let mut attrs = Vec::new();
    if let Some(w) = width { attrs.push(Attribute::property("data-width", w.to_string())); }
    if let Some(h) = height { attrs.push(Attribute::property("data-height", h.to_string())); }
    View::new(NodeBlueprint::element(
        Tag::Custom("PlaneGeometry"),
        strata_dom::ClassList::default(),
        attrs,
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
    let mut attrs = Vec::new();
    if let Some(rt) = radius_top { attrs.push(Attribute::property("data-radius-top", rt.to_string())); }
    if let Some(rb) = radius_bottom { attrs.push(Attribute::property("data-radius-bottom", rb.to_string())); }
    if let Some(h) = height { attrs.push(Attribute::property("data-height", h.to_string())); }
    if let Some(rs) = radial_segments { attrs.push(Attribute::property("data-radial-segments", rs.to_string())); }
    View::new(NodeBlueprint::element(
        Tag::Custom("CylinderGeometry"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn StandardMaterial(
    color: Option<&'static str>,
    roughness: Option<f32>,
    metalness: Option<f32>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(c) = color { attrs.push(Attribute::property("data-color", c)); }
    if let Some(r) = roughness { attrs.push(Attribute::property("data-roughness", r.to_string())); }
    if let Some(m) = metalness { attrs.push(Attribute::property("data-metalness", m.to_string())); }
    View::new(NodeBlueprint::element(
        Tag::Custom("StandardMaterial"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn ShaderMaterial(
    glsl_fragment: Option<&'static str>,
    uniform_time: Option<f32>,
    uniforms: Option<strata_js::JsValue>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(g) = glsl_fragment { attrs.push(Attribute::property("data-glsl", g)); }
    if let Some(t) = uniform_time { attrs.push(Attribute::property("data-time", t.to_string())); }
    View::new(NodeBlueprint::element(
        Tag::Custom("ShaderMaterial"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

fn get_3d_attr<'a>(attrs: &'a [strata_dom::Attribute], key: &str) -> Option<&'a str> {
    attrs.iter().find_map(|attr| {
        if let strata_dom::Attribute::Property { key: k, value: v } = attr {
            if k == key {
                return Some(v.as_str());
            }
        }
        None
    })
}

fn parse_3d_vec3(s: &str) -> Option<[f32; 3]> {
    let mut parts = s.split(',');
    let x = parts.next()?.trim().parse::<f32>().ok()?;
    let y = parts.next()?.trim().parse::<f32>().ok()?;
    let z = parts.next()?.trim().parse::<f32>().ok()?;
    Some([x, y, z])
}

/// Compiles a declarative `<Canvas3D>` blueprint subtree into a populated [`Scene3D`] instance.
pub fn compile_scene3d_from_blueprint(canvas_node: &NodeBlueprint) -> Scene3D {
    let mut scene = Scene3D::new();

    let children = match canvas_node {
        NodeBlueprint::Element { children, .. } => children,
        _ => return scene,
    };

    for child in children {
        if let NodeBlueprint::Element { tag, attributes, children: mesh_children, .. } = child {
            match tag {
                Tag::Custom("PerspectiveCamera") => {
                    let fov = get_3d_attr(attributes, "data-fov").and_then(|v| v.parse().ok()).unwrap_or(60.0);
                    let near = get_3d_attr(attributes, "data-near").and_then(|v| v.parse().ok()).unwrap_or(0.1);
                    let far = get_3d_attr(attributes, "data-far").and_then(|v| v.parse().ok()).unwrap_or(1000.0);
                    let position = get_3d_attr(attributes, "data-position").and_then(parse_3d_vec3).unwrap_or([0.0, 2.0, 5.0]);
                    let target = get_3d_attr(attributes, "data-target").and_then(parse_3d_vec3).unwrap_or([0.0, 0.0, 0.0]);
                    scene.camera = Camera3D::Perspective {
                        fov_horizontal_deg: fov,
                        near,
                        far,
                        position,
                        target,
                    };
                }
                Tag::Custom("OrthographicCamera") => {
                    let left = get_3d_attr(attributes, "data-left").and_then(|v| v.parse().ok()).unwrap_or(-5.0);
                    let right = get_3d_attr(attributes, "data-right").and_then(|v| v.parse().ok()).unwrap_or(5.0);
                    let top = get_3d_attr(attributes, "data-top").and_then(|v| v.parse().ok()).unwrap_or(5.0);
                    let bottom = get_3d_attr(attributes, "data-bottom").and_then(|v| v.parse().ok()).unwrap_or(-5.0);
                    let near = get_3d_attr(attributes, "data-near").and_then(|v| v.parse().ok()).unwrap_or(0.1);
                    let far = get_3d_attr(attributes, "data-far").and_then(|v| v.parse().ok()).unwrap_or(1000.0);
                    let position = get_3d_attr(attributes, "data-position").and_then(parse_3d_vec3).unwrap_or([0.0, 2.0, 5.0]);
                    let target = get_3d_attr(attributes, "data-target").and_then(parse_3d_vec3).unwrap_or([0.0, 0.0, 0.0]);
                    scene.camera = Camera3D::Orthographic {
                        left,
                        right,
                        top,
                        bottom,
                        near,
                        far,
                        position,
                        target,
                    };
                }
                Tag::Custom("AmbientLight") => {
                    let color_str = get_3d_attr(attributes, "data-color").unwrap_or("#ffffff");
                    let color = strata_style::parse_color(color_str).unwrap_or(Color::WHITE);
                    let intensity = get_3d_attr(attributes, "data-intensity").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                    scene.add_light(Light3D::Ambient { color, intensity });
                }
                Tag::Custom("DirectionalLight") => {
                    let pos = get_3d_attr(attributes, "data-position").and_then(parse_3d_vec3).unwrap_or([5.0, 10.0, 7.0]);
                    let color_str = get_3d_attr(attributes, "data-color").unwrap_or("#ffffff");
                    let color = strata_style::parse_color(color_str).unwrap_or(Color::WHITE);
                    let intensity = get_3d_attr(attributes, "data-intensity").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                    let cast_shadow = get_3d_attr(attributes, "data-cast-shadow").and_then(|v| v.parse().ok()).unwrap_or(false);
                    scene.add_light(Light3D::Directional { position: pos, color, intensity, cast_shadow });
                }
                Tag::Custom("PointLight") => {
                    let pos = get_3d_attr(attributes, "data-position").and_then(parse_3d_vec3).unwrap_or([0.0, 2.0, 0.0]);
                    let color_str = get_3d_attr(attributes, "data-color").unwrap_or("#ffffff");
                    let color = strata_style::parse_color(color_str).unwrap_or(Color::WHITE);
                    let intensity = get_3d_attr(attributes, "data-intensity").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                    let distance = get_3d_attr(attributes, "data-distance").and_then(|v| v.parse().ok()).unwrap_or(10.0);
                    scene.add_light(Light3D::Point { position: pos, color, intensity, distance });
                }
                Tag::Custom("Mesh") => {
                    let position = get_3d_attr(attributes, "data-position").and_then(parse_3d_vec3).unwrap_or([0.0, 0.0, 0.0]);
                    let rotation = get_3d_attr(attributes, "data-rotation").and_then(parse_3d_vec3).unwrap_or([0.0, 0.0, 0.0]);
                    let scale = get_3d_attr(attributes, "data-scale").and_then(parse_3d_vec3).unwrap_or([1.0, 1.0, 1.0]);
                    let transform = Transform3D { position, rotation, scale };

                    let mut geometry = Geometry3D::Box { width: 1.0, height: 1.0, depth: 1.0 };
                    let mut material = Material3D::default();

                    for sub in mesh_children {
                        if let NodeBlueprint::Element { tag: sub_tag, attributes: sub_attrs, .. } = sub {
                            match sub_tag {
                                Tag::Custom("BoxGeometry") => {
                                    let width = get_3d_attr(sub_attrs, "data-width").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                                    let height = get_3d_attr(sub_attrs, "data-height").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                                    let depth = get_3d_attr(sub_attrs, "data-depth").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                                    geometry = Geometry3D::Box { width, height, depth };
                                }
                                Tag::Custom("SphereGeometry") => {
                                    let radius = get_3d_attr(sub_attrs, "data-radius").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                                    let width_segments = get_3d_attr(sub_attrs, "data-width-segments").and_then(|v| v.parse().ok()).unwrap_or(16);
                                    let height_segments = get_3d_attr(sub_attrs, "data-height-segments").and_then(|v| v.parse().ok()).unwrap_or(16);
                                    geometry = Geometry3D::Sphere { radius, width_segments, height_segments };
                                }
                                Tag::Custom("PlaneGeometry") => {
                                    let width = get_3d_attr(sub_attrs, "data-width").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                                    let height = get_3d_attr(sub_attrs, "data-height").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                                    geometry = Geometry3D::Plane { width, height };
                                }
                                Tag::Custom("CylinderGeometry") => {
                                    let radius_top = get_3d_attr(sub_attrs, "data-radius-top").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                                    let radius_bottom = get_3d_attr(sub_attrs, "data-radius-bottom").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                                    let height = get_3d_attr(sub_attrs, "data-height").and_then(|v| v.parse().ok()).unwrap_or(1.0);
                                    let radial_segments = get_3d_attr(sub_attrs, "data-radial-segments").and_then(|v| v.parse().ok()).unwrap_or(16);
                                    geometry = Geometry3D::Cylinder { radius_top, radius_bottom, height, radial_segments };
                                }
                                Tag::Custom("StandardMaterial") => {
                                    let color_str = get_3d_attr(sub_attrs, "data-color").unwrap_or("#ffffff");
                                    let color = strata_style::parse_color(color_str).unwrap_or(Color::WHITE);
                                    let roughness = get_3d_attr(sub_attrs, "data-roughness").and_then(|v| v.parse().ok()).unwrap_or(0.5);
                                    let metalness = get_3d_attr(sub_attrs, "data-metalness").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                                    material = Material3D::Standard { color, roughness, metalness, emissive: None };
                                }
                                Tag::Custom("ShaderMaterial") => {
                                    let glsl = get_3d_attr(sub_attrs, "data-glsl").map(|s| s.to_string());
                                    let time = get_3d_attr(sub_attrs, "data-time").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                                    let mut uniforms = BTreeMap::new();
                                    uniforms.insert("u_time".to_string(), time);
                                    material = Material3D::Shader {
                                        vertex_shader: None,
                                        fragment_shader: None,
                                        glsl_fragment: glsl,
                                        uniforms,
                                    };
                                }
                                _ => {}
                            }
                        }
                    }

                    scene.add_mesh(Mesh3D { geometry, material, transform });
                }
                _ => {}
            }
        }
    }

    scene
}
