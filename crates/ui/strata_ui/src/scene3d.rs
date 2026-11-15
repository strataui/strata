//! Declarative 3D Viewport and Hybrid 2D/3D Scene Components for Strata UI.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use strata_dom::{Attribute, ClassList, Event, EventType, NodeBlueprint, Tag, View};
use strata_macro::{component, rsx};
use strata_render::orbit_controls::OrbitControls;
pub use strata_render::scene3d::{
    AmbientLight, BoxGeometry, Camera3D, Canvas3D, Canvas3DBuilder, CylinderGeometry,
    DirectionalLight, Geometry3D, IntoScene3DProp, Light3D, Material3D, Mesh, Mesh3D,
    OrthographicCamera, PerspectiveCamera, PlaneGeometry, PointLight, Scene3D, ShaderMaterial,
    ShaderSource, SphereGeometry, StandardMaterial, Transform3D,
};

/// Conversion trait allowing `on_camera_change` on [`SceneViewport`] to accept closures,
/// `Rc<dyn Fn(&OrbitControls)>`, or `Option<Rc<dyn Fn(&OrbitControls)>>`.
pub trait IntoCameraChangeProp {
    fn into_camera_change_prop(self) -> Option<Rc<dyn Fn(&OrbitControls)>>;
}

impl IntoCameraChangeProp for Option<Rc<dyn Fn(&OrbitControls)>> {
    fn into_camera_change_prop(self) -> Option<Rc<dyn Fn(&OrbitControls)>> {
        self
    }
}

impl IntoCameraChangeProp for Rc<dyn Fn(&OrbitControls)> {
    fn into_camera_change_prop(self) -> Option<Rc<dyn Fn(&OrbitControls)>> {
        Some(self)
    }
}

impl<F: Fn(&OrbitControls) + 'static> IntoCameraChangeProp for F {
    fn into_camera_change_prop(self) -> Option<Rc<dyn Fn(&OrbitControls)>> {
        Some(Rc::new(self))
    }
}

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
        let mut initial_controls = match scene.as_deref() {
            Some(s) => OrbitControls::from_camera(&s.camera),
            None => OrbitControls::default(),
        };
        initial_controls.enable_damping = false;
        let fov_deg = match scene.as_deref().map(|s| &s.camera) {
            Some(Camera3D::Perspective {
                fov_horizontal_deg, ..
            }) => *fov_horizontal_deg,
            _ => 60.0,
        };
        let vw = width.unwrap_or(800.0).max(1.0);
        let vh = height.unwrap_or(600.0).max(1.0);
        let controls = Rc::new(RefCell::new(initial_controls));

        let ctrl_down = Rc::clone(&controls);
        canvas_attrs.push(Attribute::event_listener(
            EventType::PointerDown,
            move |ev: Event| {
                let btn = ev.button.max(0) as u8;
                ctrl_down
                    .borrow_mut()
                    .on_pointer_down(ev.client_x as f32, ev.client_y as f32, btn);
            },
        ));

        let ctrl_move = Rc::clone(&controls);
        let cb_move = Rc::clone(&cb);
        canvas_attrs.push(Attribute::event_listener(
            EventType::PointerMove,
            move |ev: Event| {
                let changed = {
                    let mut c = ctrl_move.borrow_mut();
                    let active = c.active_mode != strata_render::OrbitInteractionMode::None;
                    if active {
                        c.on_pointer_move(
                            ev.client_x as f32,
                            ev.client_y as f32,
                            vw,
                            vh,
                            fov_deg,
                        );
                    }
                    active
                };
                if changed {
                    let c = ctrl_move.borrow();
                    cb_move(&c);
                }
            },
        ));

        let ctrl_up = Rc::clone(&controls);
        canvas_attrs.push(Attribute::event_listener(
            EventType::PointerUp,
            move |_ev: Event| {
                ctrl_up.borrow_mut().on_pointer_up();
            },
        ));

        let ctrl_wheel = Rc::clone(&controls);
        let cb_wheel = Rc::clone(&cb);
        canvas_attrs.push(Attribute::event_listener(
            EventType::Wheel,
            move |ev: Event| {
                ev.default_prevented.set(true);
                {
                    let mut c = ctrl_wheel.borrow_mut();
                    c.on_wheel(ev.delta_y as f32);
                }
                let c = ctrl_wheel.borrow();
                cb_wheel(&c);
            },
        ));
    }

    let canvas_backing = NodeBlueprint::element(
        Tag::Canvas,
        ClassList::from("absolute inset-0 w-full h-full z-0 pointer-events-auto"),
        canvas_attrs,
        Vec::new(),
    );

    let hud_overlay = NodeBlueprint::element(
        Tag::Div,
        ClassList::from(
            "relative z-10 w-full h-full pointer-events-none p-4 flex flex-col justify-between",
        ),
        Vec::new(),
        hud_children,
    );

    let mut root_attrs = Vec::new();
    if !style_str.trim().is_empty() {
        root_attrs.push(Attribute::property("style", style_str.trim()));
    }

    View::new(NodeBlueprint::element(
        Tag::Div,
        ClassList::from(
            format!(
                "relative w-full h-full overflow-hidden bg-neutral-950 rounded-2xl border border-neutral-800/80 shadow-2xl {}",
                custom
            )
            .split_whitespace()
            .map(String::from)
            .collect::<Vec<String>>(),
        ),
        root_attrs,
        vec![canvas_backing, hud_overlay],
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
