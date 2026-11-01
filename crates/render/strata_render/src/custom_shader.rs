//! Low-Level GPU Escape Hatch, Custom WGSL Shader Pipelines, and Compute/Fragment Execution.
//!
//! Provides direct access to raw WGSL shader compilation, uniform buffer bindings,
//! and hardware render pass lifecycle hooks (`GpuRenderContext`).

use crate::math3d::Mat4;
use strata_layout::Rect;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Embedded Standard WGSL 3D Physically-Based Rendering Shader.
pub const WGSL_PBR_3D_SHADER: &str = r#"
struct CameraUniforms {
    view_proj: mat4x4<f32>,
    eye_pos: vec4<f32>,
};

struct LightUniforms {
    dir_light_dir: vec4<f32>,
    dir_light_color: vec4<f32>,
    ambient_color: vec4<f32>,
};

struct MaterialUniforms {
    base_color: vec4<f32>,
    roughness: f32,
    metalness: f32,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniforms;
@group(0) @binding(1) var<uniform> light: LightUniforms;
@group(0) @binding(2) var<uniform> material: MaterialUniforms;

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

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.world_pos = model.position;
    out.world_normal = model.normal;
    out.uv = model.uv;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let L = normalize(light.dir_light_dir.xyz);
    let V = normalize(camera.eye_pos.xyz - in.world_pos);
    let H = normalize(L + V);

    let n_dot_l = max(dot(N, L), 0.0);
    let n_dot_h = max(dot(N, H), 0.0);

    // Blinn-Phong specular term modulated by roughness
    let spec_power = (1.0 - material.roughness) * 64.0 + 1.0;
    let specular = pow(n_dot_h, spec_power) * material.metalness;

    let diffuse = light.dir_light_color.rgb * light.dir_light_color.w * n_dot_l;
    let ambient = light.ambient_color.rgb * light.ambient_color.w;

    let final_rgb = material.base_color.rgb * (ambient + diffuse) + specular;
    return vec4<f32>(final_rgb, material.base_color.a);
}
"#;

/// Embedded WGSL Compute Shader for Parallel Telemetry Signal Processing.
pub const WGSL_COMPUTE_TELEMETRY_SHADER: &str = r#"
@group(0) @binding(0) var<storage, read_write> telemetry_samples: array<f32>;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    if (index >= arrayLength(&telemetry_samples)) {
        return;
    }
    // High-frequency telemetry noise filtering
    let val = telemetry_samples[index];
    telemetry_samples[index] = val * 0.95 + sin(f32(index) * 0.1) * 0.05;
}
"#;

/// WGSL Shader Definition.
#[derive(Debug, Clone, PartialEq)]
pub struct WgslShaderSource {
    pub name: String,
    pub source: String,
    pub vertex_entry: String,
    pub fragment_entry: String,
    pub compute_entry: Option<String>,
}

impl WgslShaderSource {
    pub fn pbr_3d() -> Self {
        Self {
            name: "strata_pbr_3d".to_string(),
            source: WGSL_PBR_3D_SHADER.to_string(),
            vertex_entry: "vs_main".to_string(),
            fragment_entry: "fs_main".to_string(),
            compute_entry: None,
        }
    }

    pub fn compute_telemetry() -> Self {
        Self {
            name: "strata_compute_telemetry".to_string(),
            source: WGSL_COMPUTE_TELEMETRY_SHADER.to_string(),
            vertex_entry: String::new(),
            fragment_entry: String::new(),
            compute_entry: Some("cs_main".to_string()),
        }
    }

    pub fn custom(name: &str, source: &str, vs: &str, fs: &str) -> Self {
        Self {
            name: name.to_string(),
            source: source.to_string(),
            vertex_entry: vs.to_string(),
            fragment_entry: fs.to_string(),
            compute_entry: None,
        }
    }
}

/// Primitive topology for vertex assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrimitiveTopology {
    #[default]
    TriangleList,
    TriangleStrip,
    LineList,
    LineStrip,
    PointList,
}

/// Triangle face culling mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FaceCullMode {
    #[default]
    Back,
    Front,
    None,
}

/// Low-Level Custom Hardware Shader Pipeline Descriptor and Execution State.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomShaderPipeline {
    pub shader: WgslShaderSource,
    pub topology: PrimitiveTopology,
    pub cull_mode: FaceCullMode,
    pub depth_write_enabled: bool,
    pub depth_compare: bool,
    pub uniforms_f32: BTreeMap<String, f32>,
    pub uniforms_vec4: BTreeMap<String, [f32; 4]>,
    pub uniforms_mat4: BTreeMap<String, [f32; 16]>,
}

impl CustomShaderPipeline {
    /// Creates a new custom shader pipeline wrapping the given WGSL source.
    pub fn new(shader: WgslShaderSource) -> Self {
        Self {
            shader,
            topology: PrimitiveTopology::TriangleList,
            cull_mode: FaceCullMode::Back,
            depth_write_enabled: true,
            depth_compare: true,
            uniforms_f32: BTreeMap::new(),
            uniforms_vec4: BTreeMap::new(),
            uniforms_mat4: BTreeMap::new(),
        }
    }

    /// Sets a single scalar f32 uniform parameter.
    pub fn set_uniform_f32(&mut self, name: &str, value: f32) {
        self.uniforms_f32.insert(name.to_string(), value);
    }

    /// Sets a 4-component vector uniform parameter.
    pub fn set_uniform_vec4(&mut self, name: &str, value: [f32; 4]) {
        self.uniforms_vec4.insert(name.to_string(), value);
    }

    /// Sets a 4x4 matrix uniform parameter.
    pub fn set_uniform_mat4(&mut self, name: &str, matrix: &Mat4) {
        self.uniforms_mat4.insert(name.to_string(), matrix.m);
    }

    /// Validates that the shader source is non-empty and entry points are declared.
    pub fn validate(&self) -> Result<(), String> {
        if self.shader.source.trim().is_empty() {
            return Err("WGSL shader source is empty".to_string());
        }
        if let Some(cs) = &self.shader.compute_entry {
            if !self.shader.source.contains(cs) {
                return Err(format!("Compute entry point '{}' not found in shader", cs));
            }
        } else {
            if !self.shader.source.contains(&self.shader.vertex_entry) {
                return Err(format!("Vertex entry point '{}' not found in shader", self.shader.vertex_entry));
            }
            if !self.shader.source.contains(&self.shader.fragment_entry) {
                return Err(format!("Fragment entry point '{}' not found in shader", self.shader.fragment_entry));
            }
        }
        Ok(())
    }
}

/// Execution Context passed to raw GPU rendering hooks.
pub struct GpuRenderContext<'a> {
    pub viewport_bounds: Rect,
    pub delta_time: f32,
    pub frame_index: u64,
    pub user_data: Option<&'a dyn std::any::Any>,
}

impl<'a> GpuRenderContext<'a> {
    pub fn new(bounds: Rect, dt: f32, frame_index: u64) -> Self {
        Self {
            viewport_bounds: bounds,
            delta_time: dt,
            frame_index,
            user_data: None,
        }
    }
}

/// Callback hook for direct raw GPU rendering.
pub type GpuRenderCallback = Arc<dyn Fn(&mut GpuRenderContext) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wgsl_pbr_shader_validation() {
        let pbr_shader = WgslShaderSource::pbr_3d();
        let pipeline = CustomShaderPipeline::new(pbr_shader);
        assert!(pipeline.validate().is_ok());
    }

    #[test]
    fn test_wgsl_compute_shader_validation() {
        let compute_shader = WgslShaderSource::compute_telemetry();
        let pipeline = CustomShaderPipeline::new(compute_shader);
        assert!(pipeline.validate().is_ok());
    }

    #[test]
    fn test_custom_shader_uniform_bindings() {
        let mut pipeline = CustomShaderPipeline::new(WgslShaderSource::pbr_3d());
        pipeline.set_uniform_f32("roughness", 0.35);
        pipeline.set_uniform_vec4("color", [0.2, 0.4, 0.8, 1.0]);
        pipeline.set_uniform_mat4("model", &Mat4::IDENTITY);

        assert_eq!(*pipeline.uniforms_f32.get("roughness").unwrap(), 0.35);
        assert_eq!(*pipeline.uniforms_vec4.get("color").unwrap(), [0.2, 0.4, 0.8, 1.0]);
        assert_eq!(*pipeline.uniforms_mat4.get("model").unwrap(), Mat4::IDENTITY.m);
    }
}
