//! Interactive 3D Viewport Component for Strata Creative Studio.
//! Demonstrates the Unified 3D API: Three.js high-level PBR meshes and low-level custom GLSL/Vulkan shaders in the exact same scene.

use crate::state::StudioState;
use strata::prelude::*;

#[component]
pub fn Viewport3D(state: StudioState) -> View {
    let rotation = use_signal(0.45);
    let elapsed = use_signal(1.28);

    let rotation_step = rotation.clone();
    let elapsed_step = elapsed.clone();

    rsx! {
        <div class="flex-1 h-full bg-[#09090b] relative overflow-hidden flex flex-col justify-between p-6">
            <div class="flex items-center justify-between z-10">
                <div class="flex items-center gap-3">
                    <span class="px-2.5 py-1 rounded bg-indigo-500/10 border border-indigo-500/30 text-indigo-400 text-xs font-mono font-bold">
                        {"3D SPATIAL VIEWPORT"}
                    </span>
                    <span class="text-xs text-neutral-400 font-mono">
                        {"Backend: Unified Vulkan / OpenGL Pipeline"}
                    </span>
                </div>

                <div class="flex items-center gap-2">
                    <button
                        class="px-3 py-1 rounded-lg bg-neutral-900 border border-neutral-800 hover:border-neutral-700 text-xs text-white font-mono"
                        onclick={move |_| rotation_step.update(|r| *r += 0.1)}
                    >
                        {"Rotate +10°"}
                    </button>
                    <button
                        class="px-3 py-1 rounded-lg bg-neutral-900 border border-neutral-800 hover:border-neutral-700 text-xs text-white font-mono"
                        onclick={move |_| elapsed_step.update(|t| *t += 0.25)}
                    >
                        {"Step Shader Time"}
                    </button>
                </div>
            </div>

            <div class="flex-1 w-full relative flex items-center justify-center">
                <Canvas3D class="w-full h-full rounded-2xl border border-neutral-800/80 bg-neutral-950/80 shadow-2xl overflow-hidden">
                    <PerspectiveCamera fov={50.0} position={[0.0, 3.0, 7.0]} target={[0.0, 0.0, 0.0]} />
                    <AmbientLight color="#ffffff" intensity={0.4} />
                    <DirectionalLight position={[6.0, 10.0, 6.0]} intensity={1.6} cast_shadow={true} />

                    {/* 1. High-Level Three.js Style PBR Box Primitive */}
                    <Mesh position={[-2.2, 0.0, 0.0]} rotation={[0.0, rotation.get(), 0.0]}>
                        <BoxGeometry width={2.0} height={2.0} depth={2.0} />
                        <StandardMaterial color="#6366f1" roughness={0.18} metalness={0.82} />
                    </Mesh>

                    {/* 2. Unified Low-Level Custom GLSL Shader Node within the EXACT same scene */}
                    <Mesh position={[2.2, 0.0, 0.0]} rotation={[rotation.get(), 0.0, 0.0]}>
                        <SphereGeometry radius={1.2} width_segments={36} height_segments={36} />
                        <ShaderMaterial
                            glsl_fragment="
                                precision highp float;
                                varying vec3 v_normal;
                                uniform float u_time;
                                void main() {
                                    vec3 color = 0.5 + 0.5 * sin(u_time * 2.0 + v_normal * 3.14159);
                                    gl_FragColor = vec4(color, 1.0);
                                }
                            "
                            uniforms={dynamic!({ "u_time": elapsed.get() })}
                        />
                    </Mesh>
                </Canvas3D>

                <div class="absolute bottom-6 left-6 z-10 p-3 rounded-xl bg-neutral-900/90 border border-neutral-800 backdrop-blur-md flex flex-col gap-1 text-[11px] font-mono">
                    <span class="text-white font-bold">{"Unified Scene Graph Hierarchy:"}</span>
                    <span class="text-indigo-400">{"• Mesh A: High-Level <BoxGeometry> + <StandardMaterial>"}</span>
                    <span class="text-emerald-400">{"• Mesh B: Low-Level <SphereGeometry> + Custom GLSL Shader"}</span>
                </div>
            </div>
        </div>
    }
}
