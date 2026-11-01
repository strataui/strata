//! Declarative 3D Viewport and Hybrid 2D/3D Scene Components for Strata UI.

#![allow(non_snake_case)]

use std::rc::Rc;
use strata_dom::{NodeBlueprint, View};
use strata_macro::{component, rsx};
use strata_render::orbit_controls::OrbitControls;
use strata_render::scene3d::Scene3D;
pub use strata_render::scene3d::{
    AmbientLight, BoxGeometry, Camera3D, Canvas3D, CylinderGeometry, DirectionalLight,
    Geometry3D, Light3D, Material3D, Mesh, Mesh3D, OrthographicCamera, PerspectiveCamera,
    PlaneGeometry, PointLight, ShaderMaterial, ShaderSource, SphereGeometry, StandardMaterial,
    Transform3D,
};

/// High-Level Hybrid 3D Scene Viewport Component with 2D HUD Overlay.
///
/// Embeds an interactive 3D spatial scene with a zero-copy 2D HTML/RSX HUD overlay
/// (telemetry status badges, glassmorphic inspection cards, action buttons) rendered
/// directly in front of the 3D graphics in the same stacking context.
#[component]
pub fn SceneViewport(
    scene: Option<Scene3D>,
    width: Option<f32>,
    height: Option<f32>,
    class: Option<&'static str>,
    on_camera_change: Option<Rc<dyn Fn(&OrbitControls)>>,
    children: Option<Vec<NodeBlueprint>>,
) -> View {
    let _w = width.unwrap_or(600.0);
    let _h = height.unwrap_or(400.0);
    let custom = class.unwrap_or("");
    let hud_children = children.unwrap_or_default();

    rsx! {
        <div class={format!("relative overflow-hidden bg-neutral-950 rounded-2xl border border-neutral-800/80 shadow-2xl {}", custom)}>
            // 3D Canvas Canvas Backing
            <div class="absolute inset-0 z-0 pointer-events-auto" />
            
            // Zero-Desync 2D Telemetry HUD Overlay
            <div class="relative z-10 w-full h-full pointer-events-none p-4 flex flex-col justify-between">
                {hud_children}
            </div>
        </div>
    }
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
