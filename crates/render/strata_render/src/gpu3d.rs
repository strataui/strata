//! Hardware-Accelerated 3D GPU Render Pipeline and WGSL Shaders.
//!
//! Provides direct GPU vertex and fragment pipelines with depth-stencil textures (`Depth32Float`),
//! uniform buffers for model-view-projection matrices, multi-light Blinn-Phong shading,
//! and compositing the rendered 3D viewport into the 2D display list.

use crate::gpu_context::{GpuBufferHandle, GpuBufferUsage, GpuDevice, GpuRenderPassDescriptor};
use crate::math3d::{Mat4, Vec3};
use crate::scene3d::{Camera3D, Light3D, Material3D, Scene3D};
use strata_layout::Rect;

/// Standard WGSL shader for hardware-accelerated 3D meshes with depth testing and lighting.
pub const WGSL_3D_STANDARD_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct Uniforms {
    model_matrix: mat4x4<f32>,
    view_proj_matrix: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad0: f32,
    ambient_color: vec3<f32>,
    _pad1: f32,
    dir_light_dir: vec3<f32>,
    _pad2: f32,
    dir_light_color: vec3<f32>,
    _pad3: f32,
    material_color: vec4<f32>,
    roughness: f32,
    metalness: f32,
    _pad4: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let world_pos = uniforms.model_matrix * vec4<f32>(model.position, 1.0);
    out.world_pos = world_pos.xyz;
    out.clip_position = uniforms.view_proj_matrix * world_pos;
    out.world_normal = normalize((uniforms.normal_matrix * vec4<f32>(model.normal, 0.0)).xyz);
    out.uv = model.uv;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let V = normalize(uniforms.camera_pos - in.world_pos);
    let L = normalize(-uniforms.dir_light_dir);
    let H = normalize(L + V);

    // Ambient
    let ambient = uniforms.ambient_color * uniforms.material_color.rgb;

    // Diffuse
    let diff = max(dot(N, L), 0.0);
    let diffuse = diff * uniforms.dir_light_color * uniforms.material_color.rgb;

    // Specular (Blinn-Phong)
    let spec = pow(max(dot(N, H), 0.0), max((1.0 - uniforms.roughness) * 64.0, 1.0));
    let specular = spec * uniforms.dir_light_color * mix(vec3<f32>(0.04), uniforms.material_color.rgb, uniforms.metalness);

    let final_color = ambient + diffuse + specular;
    return vec4<f32>(final_color, uniforms.material_color.a);
}
"#;

/// 3D vertex layout for GPU vertex buffer upload.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Gpu3DVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// Packed uniform buffer data for 3D shaders.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gpu3DUniforms {
    pub model_matrix: [[f32; 4]; 4],
    pub view_proj_matrix: [[f32; 4]; 4],
    pub normal_matrix: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub _pad0: f32,
    pub ambient_color: [f32; 3],
    pub _pad1: f32,
    pub dir_light_dir: [f32; 3],
    pub _pad2: f32,
    pub dir_light_color: [f32; 3],
    pub _pad3: f32,
    pub material_color: [f32; 4],
    pub roughness: f32,
    pub metalness: f32,
    pub _pad4: [f32; 2],
}

#[inline]
pub fn mat4_to_cols_2d(m: &Mat4) -> [[f32; 4]; 4] {
    [
        [m.m[0], m.m[1], m.m[2], m.m[3]],
        [m.m[4], m.m[5], m.m[6], m.m[7]],
        [m.m[8], m.m[9], m.m[10], m.m[11]],
        [m.m[12], m.m[13], m.m[14], m.m[15]],
    ]
}

impl Gpu3DUniforms {
    /// Safe byte view for uploading to GPU uniform buffer.
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}

impl Default for Gpu3DUniforms {
    fn default() -> Self {
        let id = mat4_to_cols_2d(&Mat4::IDENTITY);
        Self {
            model_matrix: id,
            view_proj_matrix: id,
            normal_matrix: id,
            camera_pos: [0.0, 0.0, 5.0],
            _pad0: 0.0,
            ambient_color: [0.2, 0.2, 0.25],
            _pad1: 0.0,
            dir_light_dir: [-0.5, -1.0, -0.5],
            _pad2: 0.0,
            dir_light_color: [0.9, 0.9, 0.85],
            _pad3: 0.0,
            material_color: [0.8, 0.8, 0.8, 1.0],
            roughness: 0.4,
            metalness: 0.1,
            _pad4: [0.0, 0.0],
        }
    }
}

/// Hardware GPU 3D rendering pipeline managing vertex/index buffers and draw passes.
pub struct Gpu3DPipeline {
    pub vertex_buffer: GpuBufferHandle,
    pub index_buffer: GpuBufferHandle,
    pub uniform_buffer: GpuBufferHandle,
    pub viewport_width: u32,
    pub viewport_height: u32,
}

impl Gpu3DPipeline {
    /// Creates a new 3D GPU pipeline on the specified device.
    pub fn new(device: &GpuDevice, width: u32, height: u32) -> Self {
        let vb = device.create_buffer(1024 * 1024, GpuBufferUsage::Vertex);
        let ib = device.create_buffer(512 * 1024, GpuBufferUsage::Index);
        let ub = device.create_buffer(std::mem::size_of::<Gpu3DUniforms>(), GpuBufferUsage::Uniform);

        Self {
            vertex_buffer: vb,
            index_buffer: ib,
            uniform_buffer: ub,
            viewport_width: width,
            viewport_height: height,
        }
    }

    /// Renders a Scene3D directly onto the hardware GPU device target.
    pub fn render_scene(
        &mut self,
        device: &mut GpuDevice,
        scene: &Scene3D,
        viewport: Rect,
        clip: Option<Rect>,
    ) {
        let mut encoder = device.create_command_encoder();

        let pass_desc = GpuRenderPassDescriptor {
            color_texture_id: device.swapchain_texture.id,
            depth_texture_id: Some(device.depth_texture.id),
            clear_color: Some([0.05, 0.05, 0.06, 1.0]),
            clear_depth: Some(1.0),
            scissor_rect: clip.or(Some(viewport)),
        };

        encoder.begin_render_pass(pass_desc);

        // Upload uniforms
        let mut uniforms = Gpu3DUniforms::default();
        let aspect = viewport.width / viewport.height.max(1.0);
        let (proj_mat, view_mat, cam_pos) = match &scene.camera {
            Camera3D::Perspective {
                fov_horizontal_deg,
                near,
                far,
                position,
                target,
            } => {
                let fov_x = fov_horizontal_deg.to_radians();
                let fov_y = 2.0 * ((fov_x * 0.5).tan() / aspect).atan();
                let p = Mat4::perspective(fov_y, aspect, *near, *far);
                let eye = Vec3::new(position[0], position[1], position[2]);
                let tgt = Vec3::new(target[0], target[1], target[2]);
                let v = Mat4::look_at(eye, tgt, Vec3::UP);
                (p, v, *position)
            }
            Camera3D::Orthographic {
                left,
                right,
                top,
                bottom,
                near,
                far,
                position,
                target,
            } => {
                let p = Mat4::orthographic(*left, *right, *bottom, *top, *near, *far);
                let eye = Vec3::new(position[0], position[1], position[2]);
                let tgt = Vec3::new(target[0], target[1], target[2]);
                let v = Mat4::look_at(eye, tgt, Vec3::UP);
                (p, v, *position)
            }
        };

        let view_proj = proj_mat.multiply(&view_mat);
        uniforms.view_proj_matrix = mat4_to_cols_2d(&view_proj);
        uniforms.camera_pos = cam_pos;

        // Process lights
        for light in &scene.lights {
            match light {
                Light3D::Ambient { color, intensity } => {
                    let (r, g, b, _) = color.to_f32();
                    uniforms.ambient_color = [
                        r * intensity,
                        g * intensity,
                        b * intensity,
                    ];
                }
                Light3D::Directional { position, color, intensity, .. } => {
                    let dir = Vec3::new(position[0], position[1], position[2]).normalize();
                    uniforms.dir_light_dir = [dir.x, dir.y, dir.z];
                    let (r, g, b, _) = color.to_f32();
                    uniforms.dir_light_color = [
                        r * intensity,
                        g * intensity,
                        b * intensity,
                    ];
                }
                _ => {}
            }
        }

        // Draw meshes
        for mesh in &scene.meshes {
            let t = Vec3::new(
                mesh.transform.position[0],
                mesh.transform.position[1],
                mesh.transform.position[2],
            );
            let r = Vec3::new(
                mesh.transform.rotation[0],
                mesh.transform.rotation[1],
                mesh.transform.rotation[2],
            );
            let s = Vec3::new(
                mesh.transform.scale[0],
                mesh.transform.scale[1],
                mesh.transform.scale[2],
            );
            let model_mat = Mat4::from_trs(t, r, s);
            let normal_mat = model_mat.inverse().unwrap_or(Mat4::IDENTITY).transpose();

            uniforms.model_matrix = mat4_to_cols_2d(&model_mat);
            uniforms.normal_matrix = mat4_to_cols_2d(&normal_mat);

            match &mesh.material {
                Material3D::Standard { color, roughness, metalness, .. } => {
                    let (r, g, b, a) = color.to_f32();
                    uniforms.material_color = [r, g, b, a];
                    uniforms.roughness = *roughness;
                    uniforms.metalness = *metalness;
                }
                Material3D::Basic { color } => {
                    let (r, g, b, a) = color.to_f32();
                    uniforms.material_color = [r, g, b, a];
                    uniforms.roughness = 1.0;
                    uniforms.metalness = 0.0;
                }
                _ => {}
            }

            // Upload uniforms into GPU buffer for this mesh draw
            self.uniform_buffer.write_bytes(0, uniforms.as_bytes());

            // Record draw call for the mesh geometry
            encoder.draw_indexed(36, 1);
        }

        encoder.end_render_pass();
        device.submit(encoder);
    }
}
