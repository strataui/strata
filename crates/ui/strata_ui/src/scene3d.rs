//! Declarative 3D Viewport and Hybrid 2D/3D Scene Components for Strata UI.

#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::Arc;
use strata_dom::{Attribute, ClassList, NodeBlueprint, Tag, View};
use strata_macro::{component, rsx};
use strata_render::orbit_controls::OrbitControls;
pub use strata_render::scene3d::{
    AmbientLight, BoxGeometry, Camera3D, Canvas3D, Canvas3DBuilder, CylinderGeometry,
    DirectionalLight, Geometry3D, IntoScene3DProp, Light3D, Material3D, Mesh, Mesh3D,
    OrthographicCamera, PerspectiveCamera, PlaneGeometry, PointLight, Scene3D, ShaderMaterial,
    ShaderSource, SphereGeometry, StandardMaterial, Transform3D,
};

pub use strata_render::scene3d::IntoCameraChangeProp;

/// High-Level Hybrid 3D Scene Viewport Component with 2D HUD Overlay.
///
/// Embeds an interactive 3D spatial scene with a zero-copy 2D HTML/RSX HUD overlay
/// (telemetry status badges, glassmorphic inspection cards, action buttons) rendered
/// directly in front of the 3D graphics in the same stacking context.
pub fn SceneViewport(
    scene: Option<Scene3D>,
    width: Option<f32>,
    height: Option<f32>,
    class: Option<&'static str>,
    on_camera_change: Option<Rc<dyn Fn(&OrbitControls)>>,
    children: Option<Vec<NodeBlueprint>>,
) -> View {
    build_scene_viewport(
        scene.map(Arc::new),
        width,
        height,
        class.map(String::from),
        None,
        on_camera_change,
        children.unwrap_or_default(),
    )
}

#[allow(non_snake_case)]
pub struct SceneViewport {}

impl SceneViewport {
    pub fn builder() -> SceneViewportBuilder {
        SceneViewportBuilder::default()
    }
}

#[derive(Default)]
pub struct SceneViewportBuilder {
    pub scene: Option<Arc<Scene3D>>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub class: Option<String>,
    pub style: Option<String>,
    pub on_camera_change: Option<Rc<dyn Fn(&OrbitControls)>>,
    pub children: Option<Vec<NodeBlueprint>>,
}

impl SceneViewportBuilder {
    pub fn scene(mut self, scene: impl IntoScene3DProp) -> Self {
        self.scene = scene.into_scene3d_prop();
        self
    }

    pub fn width(mut self, width: impl Into<Option<f32>>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Option<f32>>) -> Self {
        self.height = height.into();
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }

    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    pub fn on_camera_change(mut self, cb: impl IntoCameraChangeProp) -> Self {
        self.on_camera_change = cb.into_camera_change_prop();
        self
    }

    pub fn on_camera_change_fn(mut self, cb: impl Fn(&OrbitControls) + 'static) -> Self {
        self.on_camera_change = Some(Rc::new(cb));
        self
    }

    pub fn children(mut self, children: Vec<NodeBlueprint>) -> Self {
        self.children = Some(children);
        self
    }

    pub fn build(self) -> View {
        build_scene_viewport(
            self.scene,
            self.width,
            self.height,
            self.class,
            self.style,
            self.on_camera_change,
            self.children.unwrap_or_default(),
        )
    }
}

fn build_scene_viewport(
    scene: Option<Arc<Scene3D>>,
    width: Option<f32>,
    height: Option<f32>,
    class: Option<String>,
    extra_style: Option<String>,
    on_camera_change: Option<Rc<dyn Fn(&OrbitControls)>>,
    hud_children: Vec<NodeBlueprint>,
) -> View {
    let custom = class.as_deref().unwrap_or("");

    // Build outer container style when explicit pixel dimensions or inline styles are provided.
    let mut style_str = String::new();
    if let Some(w) = width {
        style_str.push_str(&format!("width: {}px; ", w));
    }
    if let Some(h) = height {
        style_str.push_str(&format!("height: {}px; ", h));
    }
    if let Some(s) = extra_style {
        style_str.push_str(&s);
    }

    // Build 3D Canvas backing element at z-0
    let mut canvas_attrs = Vec::with_capacity(6);
    if let Some(ref scene_arc) = scene {
        canvas_attrs.push(Attribute::property("data-strata-canvas3d", "true"));
        let cloned: Arc<Scene3D> = Arc::clone(scene_arc);
        let payload: Arc<dyn std::any::Any + Send + Sync> = cloned;
        canvas_attrs.push(Attribute::scene_payload_arc(payload));
    }

    if let Some(cb) = on_camera_change {
        strata_render::scene3d::attach_orbit_controls_listeners(
            &mut canvas_attrs,
            scene.as_deref(),
            width,
            height,
            cb,
        );
    }

    let canvas_backing = NodeBlueprint::element(
        Tag::Canvas,
        ClassList::from("absolute inset-0 w-full h-full z-0"),
        canvas_attrs,
        Vec::new(),
    );

    let mut root_attrs = Vec::new();
    if !style_str.trim().is_empty() {
        root_attrs.push(Attribute::property("style", style_str.trim()));
    }

    // Children are siblings of the canvas, not wrapped in an overlay layer, so
    // no layer intercepts pointer input meant for the canvas. Hosts position
    // them (for example `absolute` or `relative z-10`) to stack above it.
    let mut children = Vec::with_capacity(1 + hud_children.len());
    children.push(canvas_backing);
    children.extend(hud_children);

    View::new(NodeBlueprint::element(
        Tag::Div,
        ClassList::from(
            format!("relative {}", custom)
                .split_whitespace()
                .map(String::from)
                .collect::<Vec<String>>(),
        ),
        root_attrs,
        children,
    ))
}

/// Floating Telemetry HUD Card for displaying real-time metrics over a 3D scene.
#[component]
pub fn HudCard(
    title: &'static str,
    class: Option<&'static str>,
    children: Option<Vec<NodeBlueprint>>,
) -> View {
    let custom = class.unwrap_or("");
    let card_children = children.unwrap_or_default();

    rsx! {
        <div class={format!("pointer-events-auto backdrop-blur-md bg-neutral-900/75 border border-neutral-700/60 rounded-xl p-3 shadow-lg text-neutral-100 {}", custom)}>
            <div class="text-xs font-semibold text-neutral-400 uppercase tracking-wider mb-1">
                {title}
            </div>
            {card_children}
        </div>
    }
}
