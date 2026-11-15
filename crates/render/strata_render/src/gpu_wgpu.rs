//! Hardware-accelerated `wgpu` 3D renderer with double-single RTE precision and
//! single-draw-call programmable vertex pulling for planetary/terrain tile pyramids.

use crate::scene3d::{
    Background3D, Camera3D, Geometry3D, Light3D, Material3D, Mesh3D, RetainedCameraUniforms,
    RetainedDynamicMesh, RetainedTerrainFrame, RetainedTileMesh, RetainedTileTexture,
    RteTileVertex, Scene3D, TileKeyId,
};
use bytemuck::{Pod, Zeroable};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Default maximum new tile textures uploaded to the GPU atlas per frame during continuous flight.
pub const DEFAULT_TEXTURE_UPLOAD_BUDGET: usize = 8;
/// Default maximum new tile meshes copied to the GPU storage slab per frame.
pub const DEFAULT_MESH_UPLOAD_BUDGET: usize = 24;
/// Standard terrain imagery tile dimension in the GPU `Texture2DArray` atlas.
pub const ATLAS_TILE_SIZE: u32 = 256;
/// Maximum imagery layers pre-allocated in the GPU `Texture2DArray` atlas.
pub const MAX_ATLAS_LAYERS: u32 = 256;

/// Splits a 64-bit float into `(hi, lo)` 32-bit floats for GPU double-single emulation.
#[inline]
pub fn split_f64_to_f32(v: f64) -> (f32, f32) {
    let hi = v as f32;
    let lo = (v - hi as f64) as f32;
    (hi, lo)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuCameraUniforms {
    view_proj_rte: [[f32; 4]; 4],
    eye_high: [f32; 4],
    eye_low: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    ambient_color: [f32; 4],
    sky_top: [f32; 4],
    sky_bottom: [f32; 4],
}

impl GpuCameraUniforms {
    fn from_retained(cam: &RetainedCameraUniforms) -> Self {
        let (ex_hi, ex_lo) = split_f64_to_f32(cam.eye_ecef[0]);
        let (ey_hi, ey_lo) = split_f64_to_f32(cam.eye_ecef[1]);
        let (ez_hi, ez_lo) = split_f64_to_f32(cam.eye_ecef[2]);
        let (sky_top, sky_bottom) = match cam.background {
            Background3D::Transparent => ([0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]),
            Background3D::Solid(c) => {
                let rgba = [c.r(), c.g(), c.b(), c.a()];
                (rgba, rgba)
            }
            Background3D::VerticalGradient { top, bottom } => (
                [top.r(), top.g(), top.b(), top.a()],
                [bottom.r(), bottom.g(), bottom.b(), bottom.a()],
            ),
        };
        Self {
            view_proj_rte: cam.view_proj_rte,
            eye_high: [ex_hi, ey_hi, ez_hi, 0.0],
            eye_low: [ex_lo, ey_lo, ez_lo, 0.0],
            sun_dir: [cam.sun_dir[0], cam.sun_dir[1], cam.sun_dir[2], 0.0],
            sun_color: [cam.sun_color[0], cam.sun_color[1], cam.sun_color[2], 1.0],
            ambient_color: [
                cam.ambient_color[0],
                cam.ambient_color[1],
                cam.ambient_color[2],
                1.0,
            ],
            sky_top,
            sky_bottom,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuTileInstance {
    center_high: [f32; 4],
    center_low: [f32; 4],
    uv_rect: [f32; 4],
    tint: [f32; 4],
    tile_meta: [u32; 4], // x = base_vertex, y = atlas_layer, z = unlit (0/1), w = reserved
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuDynamicUniforms {
    anchor_high: [f32; 4],
    anchor_low: [f32; 4],
    color: [f32; 4],
    dyn_meta: [u32; 4], // x = unlit, y = use_vertex_color, z = use_texture, w = atlas_layer
}

/// Builds a shared triangle index buffer for a terrain grid `(grid_res x grid_res)` with optional skirts.
pub fn build_terrain_grid_indices(grid_res: u32, has_skirts: bool) -> Vec<u32> {
    let n = grid_res.max(2);
    let cells = n - 1;
    let skirt_quads = if has_skirts { cells * 4 } else { 0 };
    let mut indices = Vec::with_capacity(((cells * cells + skirt_quads) * 6) as usize);

    for r in 0..cells {
        for c in 0..cells {
            let i00 = r * n + c;
            let i10 = i00 + 1;
            let i01 = (r + 1) * n + c;
            let i11 = i01 + 1;
            indices.extend_from_slice(&[i00, i10, i11, i00, i11, i01]);
        }
    }

    if has_skirts {
        let base = n * n;
        // South edge (row = 0, facing -North): s1 -> s0 -> k0 -> k1
        for c in 0..cells {
            let s0 = c;
            let s1 = c + 1;
            let k0 = base + c;
            let k1 = base + c + 1;
            indices.extend_from_slice(&[s1, s0, k0, s1, k0, k1]);
        }
        // East edge (col = n-1, facing +East): s1 -> s0 -> k0 -> k1
        let r_base = base + n;
        for r in 0..cells {
            let s0 = r * n + (n - 1);
            let s1 = (r + 1) * n + (n - 1);
            let k0 = r_base + r;
            let k1 = r_base + r + 1;
            indices.extend_from_slice(&[s1, s0, k0, s1, k0, k1]);
        }
        // North edge (row = n-1, facing +North): s0 -> s1 -> k1 -> k0
        let b_base = base + n * 2;
        for c in 0..cells {
            let s0 = (n - 1) * n + c;
            let s1 = s0 + 1;
            let k0 = b_base + c;
            let k1 = b_base + c + 1;
            indices.extend_from_slice(&[s0, s1, k1, s0, k1, k0]);
        }
        // West edge (col = 0, facing -East): s0 -> s1 -> k1 -> k0
        let l_base = base + n * 3;
        for r in 0..cells {
            let s0 = r * n;
            let s1 = (r + 1) * n;
            let k0 = l_base + r;
            let k1 = l_base + r + 1;
            indices.extend_from_slice(&[s0, s1, k1, s0, k1, k0]);
        }
    }

    indices
}

const SKY_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    eye_high: vec4<f32>,
    eye_low: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    ambient_color: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
}

@group(0) @binding(0) var<uniform> cam: GpuCameraUniforms;

struct SkyOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv_y: f32,
}

@vertex
fn vs_sky(@builtin(vertex_index) vid: u32) -> SkyOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let p = positions[vid];
    var out: SkyOut;
    out.pos = vec4<f32>(p, 0.9999, 1.0);
    // NDC y = +1 is top (t = 0.0), y = -1 is bottom (t = 1.0)
    out.uv_y = clamp((1.0 - p.y) * 0.5, 0.0, 1.0);
    return out;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    return mix(cam.sky_top, cam.sky_bottom, clamp(in.uv_y, 0.0, 1.0));
}
"#;

const TERRAIN_PULL_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    eye_high: vec4<f32>,
    eye_low: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    ambient_color: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
}

struct GpuTileVertex {
    pos_u: vec4<f32>,
    norm_v: vec4<f32>,
}

struct GpuTileInstance {
    center_high: vec4<f32>,
    center_low: vec4<f32>,
    uv_rect: vec4<f32>,
    tint: vec4<f32>,
    tile_meta: vec4<u32>,
}

@group(0) @binding(0) var<uniform> cam: GpuCameraUniforms;
@group(0) @binding(1) var<storage, read> vertices: array<GpuTileVertex>;
@group(0) @binding(2) var<storage, read> instances: array<GpuTileInstance>;
@group(0) @binding(3) var terrain_atlas: texture_2d_array<f32>;
@group(0) @binding(4) var terrain_sampler: sampler;

struct TerrainVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) @interpolate(flat) tex_layer: u32,
    @location(4) @interpolate(flat) unlit: u32,
}

@vertex
fn vs_terrain(
    @builtin(vertex_index) local_v: u32,
    @builtin(instance_index) inst_id: u32,
) -> TerrainVsOut {
    let inst = instances[inst_id];
    let v = vertices[inst.tile_meta.x + local_v];
    let center_rel = (inst.center_high.xyz - cam.eye_high.xyz)
                   + (inst.center_low.xyz - cam.eye_low.xyz);
    let world_rel = center_rel + v.pos_u.xyz;
    let raw_uv = vec2<f32>(v.pos_u.w, v.norm_v.w);

    var out: TerrainVsOut;
    out.clip_pos = cam.view_proj_rte * vec4<f32>(world_rel, 1.0);
    out.normal = normalize(v.norm_v.xyz);
    out.uv = mix(inst.uv_rect.xy, inst.uv_rect.zw, raw_uv);
    out.tint = inst.tint;
    out.tex_layer = inst.tile_meta.y;
    out.unlit = inst.tile_meta.z;
    return out;
}

@fragment
fn fs_terrain(in: TerrainVsOut) -> @location(0) vec4<f32> {
    let sampled = textureSample(terrain_atlas, terrain_sampler, in.uv, i32(in.tex_layer));
    let base = sampled * in.tint;
    if (base.a <= 0.001) {
        discard;
    }
    if (in.unlit != 0u) {
        return base;
    }
    let n = normalize(in.normal);
    let ndotl = max(dot(n, normalize(cam.sun_dir.xyz)), 0.0);
    let lit = cam.ambient_color.rgb + cam.sun_color.rgb * ndotl;
    return vec4<f32>(base.rgb * lit, base.a);
}
"#;

const DYNAMIC_MESH_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    eye_high: vec4<f32>,
    eye_low: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    ambient_color: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
}

struct GpuDynamicUniforms {
    anchor_high: vec4<f32>,
    anchor_low: vec4<f32>,
    color: vec4<f32>,
    dyn_meta: vec4<u32>,
}

@group(0) @binding(0) var<uniform> cam: GpuCameraUniforms;
@group(0) @binding(1) var<uniform> dyn_u: GpuDynamicUniforms;

struct DynVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) unlit: u32,
}

@vertex
fn vs_dynamic(
    @location(0) pos_u: vec4<f32>,
    @location(1) norm_v: vec4<f32>,
) -> DynVsOut {
    let center_rel = (dyn_u.anchor_high.xyz - cam.eye_high.xyz)
                   + (dyn_u.anchor_low.xyz - cam.eye_low.xyz);
    let world_rel = center_rel + pos_u.xyz;
    var out: DynVsOut;
    out.clip_pos = cam.view_proj_rte * vec4<f32>(world_rel, 1.0);
    out.normal = norm_v.xyz;
    out.color = dyn_u.color;
    out.unlit = dyn_u.dyn_meta.x;
    return out;
}

@fragment
fn fs_dynamic(in: DynVsOut) -> @location(0) vec4<f32> {
    if (in.unlit != 0u) {
        return in.color;
    }
    let len_sq = dot(in.normal, in.normal);
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(in.normal), len_sq > 0.0001);
    let ndotl = max(dot(n, normalize(cam.sun_dir.xyz)), 0.0);
    let lit = cam.ambient_color.rgb + cam.sun_color.rgb * ndotl;
    return vec4<f32>(in.color.rgb * lit, in.color.a);
}
"#;

const OVERLAY_COMPOSITE_WGSL: &str = r#"
@group(0) @binding(0) var overlay_tex: texture_2d<f32>;
@group(0) @binding(1) var overlay_sampler: sampler;

struct OverlayOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_overlay(@builtin(vertex_index) vid: u32) -> OverlayOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let p = positions[vid];
    var out: OverlayOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5);
    return out;
}

@fragment
fn fs_overlay(in: OverlayOut) -> @location(0) vec4<f32> {
    return textureSample(overlay_tex, overlay_sampler, in.uv);
}
"#;

#[derive(Clone, Copy)]
struct SlabEntry {
    revision: u64,
    offset: u32,
    len: u32,
    last_used_frame: u64,
}

/// Free-list GPU vertex slab allocator holding tile-local RTC vertices across frames.
struct GpuTileSlabAllocator {
    buffer: wgpu::Buffer,
    capacity_vertices: u32,
    resident: HashMap<TileKeyId, SlabEntry>,
    free_blocks: Vec<(u32, u32)>, // (offset, len)
    bump_cursor: u32,
}

impl GpuTileSlabAllocator {
    fn new(device: &wgpu::Device, initial_capacity_vertices: u32) -> Self {
        let cap = initial_capacity_vertices.max(65_536);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_earth_tile_vertex_slab"),
            size: (cap as u64) * (std::mem::size_of::<RteTileVertex>() as u64),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            capacity_vertices: cap,
            resident: HashMap::new(),
            free_blocks: Vec::new(),
            bump_cursor: 0,
        }
    }

    fn alloc_range(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        needed: u32,
        current_frame: u64,
    ) -> (u32, bool) {
        // 1. Reuse best-fit free block.
        if let Some((idx, (off, len))) = self
            .free_blocks
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, (_, l))| *l >= needed)
            .min_by_key(|(_, (_, l))| *l)
        {
            self.free_blocks.swap_remove(idx);
            if len > needed {
                self.free_blocks.push((off + needed, len - needed));
            }
            return (off, false);
        }

        // 2. Bump allocate if space remains.
        if self.bump_cursor + needed <= self.capacity_vertices {
            let off = self.bump_cursor;
            self.bump_cursor += needed;
            return (off, false);
        }

        // 3. Evict LRU tiles not used this frame.
        let mut stale: Vec<(TileKeyId, SlabEntry)> = self
            .resident
            .iter()
            .filter(|(_, e)| e.last_used_frame < current_frame)
            .map(|(k, e)| (*k, *e))
            .collect();
        stale.sort_by_key(|(_, e)| e.last_used_frame);

        let mut reclaimed = 0u32;
        for (k, e) in stale {
            self.resident.remove(&k);
            self.free_blocks.push((e.offset, e.len));
            reclaimed += e.len;
            if reclaimed >= needed {
                break;
            }
        }
        self.coalesce_free_blocks();

        if let Some((idx, (off, len))) = self
            .free_blocks
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, (_, l))| *l >= needed)
            .min_by_key(|(_, (_, l))| *l)
        {
            self.free_blocks.swap_remove(idx);
            if len > needed {
                self.free_blocks.push((off + needed, len - needed));
            }
            return (off, false);
        }

        // 4. Grow buffer if still tight.
        let new_cap = (self.capacity_vertices * 2).max(self.bump_cursor + needed + 65_536);
        let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_earth_tile_vertex_slab_grown"),
            size: (new_cap as u64) * (std::mem::size_of::<RteTileVertex>() as u64),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let copy_bytes = (self.bump_cursor as u64) * (std::mem::size_of::<RteTileVertex>() as u64);
        if copy_bytes > 0 {
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_earth_slab_grow_copy"),
            });
            enc.copy_buffer_to_buffer(&self.buffer, 0, &new_buf, 0, copy_bytes);
            queue.submit(Some(enc.finish()));
        }
        self.buffer = new_buf;
        self.capacity_vertices = new_cap;
        let off = self.bump_cursor;
        self.bump_cursor += needed;
        (off, true)
    }

    fn coalesce_free_blocks(&mut self) {
        if self.free_blocks.len() < 2 {
            return;
        }
        self.free_blocks.sort_unstable_by_key(|(o, _)| *o);
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.free_blocks.len());
        for (o, l) in self.free_blocks.drain(..) {
            if let Some(last) = merged.last_mut() {
                if last.0 + last.1 == o {
                    last.1 += l;
                    continue;
                }
            }
            merged.push((o, l));
        }
        // Reclaim tail back into bump_cursor.
        while let Some(&(o, l)) = merged.last() {
            if o + l == self.bump_cursor {
                self.bump_cursor = o;
                merged.pop();
            } else {
                break;
            }
        }
        self.free_blocks = merged;
    }

    fn upload_meshes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &[Arc<RetainedTileMesh>],
        frame_idx: u64,
        budget: usize,
    ) -> bool {
        let mut bind_group_dirty = false;
        let mut uploaded = 0usize;

        // First mark already-resident matching revisions as fresh this frame.
        for mesh in meshes {
            if let Some(entry) = self.resident.get_mut(&mesh.key) {
                if entry.revision == mesh.revision && entry.len == mesh.vertices.len() as u32 {
                    entry.last_used_frame = frame_idx;
                }
            }
        }

        for mesh in meshes {
            let needed = mesh.vertices.len() as u32;
            if needed == 0 {
                continue;
            }
            if let Some(entry) = self.resident.get(&mesh.key) {
                if entry.revision == mesh.revision && entry.len == needed {
                    continue;
                }
            }
            if uploaded >= budget {
                continue;
            }
            if let Some(old) = self.resident.remove(&mesh.key) {
                self.free_blocks.push((old.offset, old.len));
                self.coalesce_free_blocks();
            }
            let (off, grew) = self.alloc_range(device, queue, needed, frame_idx);
            if grew {
                bind_group_dirty = true;
            }
            let byte_offset = (off as u64) * (std::mem::size_of::<RteTileVertex>() as u64);
            queue.write_buffer(
                &self.buffer,
                byte_offset,
                bytemuck::cast_slice(&mesh.vertices),
            );
            self.resident.insert(
                mesh.key,
                SlabEntry {
                    revision: mesh.revision,
                    offset: off,
                    len: needed,
                    last_used_frame: frame_idx,
                },
            );
            uploaded += 1;
        }
        bind_group_dirty
    }

    fn base_vertex(&self, key: TileKeyId, revision: u64) -> Option<u32> {
        self.resident
            .get(&key)
            .filter(|e| e.revision == revision)
            .map(|e| e.offset)
    }
}

#[derive(Clone, Copy)]
struct AtlasSlot {
    tex_id: u64,
    revision: u64,
    layer: u32,
    last_used_frame: u64,
}

/// LRU `Texture2DArray` tile imagery atlas (layer 0 = solid white fallback).
struct GpuImageryAtlas {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    slots_by_id: HashMap<u64, AtlasSlot>,
    layer_owner: Vec<Option<u64>>,
}

impl GpuImageryAtlas {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, layers: u32) -> Self {
        let layers = layers.max(16);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("strata_earth_imagery_atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_TILE_SIZE,
                height: ATLAS_TILE_SIZE,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Layer 0: solid white fallback texture.
        let white = vec![255u8; (ATLAS_TILE_SIZE * ATLAS_TILE_SIZE * 4) as usize];
        Self::write_layer(queue, &texture, 0, ATLAS_TILE_SIZE, ATLAS_TILE_SIZE, &white);

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("strata_earth_imagery_atlas_view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        Self {
            _texture: texture,
            view,
            slots_by_id: HashMap::new(),
            layer_owner: vec![None; layers as usize],
        }
    }

    fn write_layer(
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        layer: u32,
        w: u32,
        h: u32,
        rgba: &[u8],
    ) {
        if w == ATLAS_TILE_SIZE
            && h == ATLAS_TILE_SIZE
            && rgba.len() >= (ATLAS_TILE_SIZE * ATLAS_TILE_SIZE * 4) as usize
        {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &rgba[..(ATLAS_TILE_SIZE * ATLAS_TILE_SIZE * 4) as usize],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ATLAS_TILE_SIZE * 4),
                    rows_per_image: Some(ATLAS_TILE_SIZE),
                },
                wgpu::Extent3d {
                    width: ATLAS_TILE_SIZE,
                    height: ATLAS_TILE_SIZE,
                    depth_or_array_layers: 1,
                },
            );
            return;
        }
        // Nearest/bilinear resample into ATLAS_TILE_SIZE x ATLAS_TILE_SIZE on CPU edge case.
        let mut resampled = vec![255u8; (ATLAS_TILE_SIZE * ATLAS_TILE_SIZE * 4) as usize];
        if w > 0 && h > 0 && rgba.len() >= (w * h * 4) as usize {
            for dy in 0..ATLAS_TILE_SIZE {
                let sy = ((dy as u64 * h as u64) / ATLAS_TILE_SIZE as u64).min(h as u64 - 1) as u32;
                for dx in 0..ATLAS_TILE_SIZE {
                    let sx =
                        ((dx as u64 * w as u64) / ATLAS_TILE_SIZE as u64).min(w as u64 - 1) as u32;
                    let si = ((sy * w + sx) * 4) as usize;
                    let di = ((dy * ATLAS_TILE_SIZE + dx) * 4) as usize;
                    resampled[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
                }
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &resampled,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_TILE_SIZE * 4),
                rows_per_image: Some(ATLAS_TILE_SIZE),
            },
            wgpu::Extent3d {
                width: ATLAS_TILE_SIZE,
                height: ATLAS_TILE_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    fn upload_textures(
        &mut self,
        queue: &wgpu::Queue,
        textures: &[Arc<RetainedTileTexture>],
        frame_idx: u64,
        budget: usize,
    ) {
        for tex in textures {
            if let Some(slot) = self.slots_by_id.get_mut(&tex.id) {
                if slot.revision == tex.revision {
                    slot.last_used_frame = frame_idx;
                }
            }
        }
        let mut uploaded = 0usize;
        for tex in textures {
            if let Some(slot) = self.slots_by_id.get(&tex.id) {
                if slot.revision == tex.revision {
                    continue;
                }
            }
            if uploaded >= budget {
                continue;
            }
            let target_layer = if let Some(existing) = self.slots_by_id.get(&tex.id) {
                existing.layer
            } else if let Some((idx, _)) = self
                .layer_owner
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, owner)| owner.is_none())
            {
                idx as u32
            } else if let Some((victim_id, victim_layer)) = self
                .slots_by_id
                .values()
                .filter(|s| s.last_used_frame < frame_idx)
                .min_by_key(|s| s.last_used_frame)
                .map(|s| (s.tex_id, s.layer))
            {
                self.slots_by_id.remove(&victim_id);
                victim_layer
            } else {
                continue;
            };

            Self::write_layer(
                queue,
                &self._texture,
                target_layer,
                tex.width,
                tex.height,
                &tex.rgba8,
            );
            self.layer_owner[target_layer as usize] = Some(tex.id);
            self.slots_by_id.insert(
                tex.id,
                AtlasSlot {
                    tex_id: tex.id,
                    revision: tex.revision,
                    layer: target_layer,
                    last_used_frame: frame_idx,
                },
            );
            uploaded += 1;
        }
    }

    #[inline]
    fn layer_for(&self, tex_id: Option<u64>, revision: u64) -> u32 {
        let Some(id) = tex_id else {
            return 0;
        };
        self.slots_by_id
            .get(&id)
            .filter(|s| s.revision == revision)
            .map(|s| s.layer)
            .unwrap_or(0)
    }
}

struct CachedIndexTopology {
    buffer: wgpu::Buffer,
    index_count: u32,
}

struct CachedDynamicGpuMesh {
    revision: u64,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
}

struct OffscreenTarget {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

/// Hardware-accelerated `wgpu` 3D renderer capable of rendering directly into any `TextureView`
/// (swapchain surface) or offscreen RGBA target.
pub struct Wgpu3DRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    color_format: wgpu::TextureFormat,
    camera_buffer: wgpu::Buffer,
    sky_pipeline: wgpu::RenderPipeline,
    sky_bind_group: wgpu::BindGroup,
    terrain_pipeline: wgpu::RenderPipeline,
    terrain_bind_group_layout: wgpu::BindGroupLayout,
    terrain_bind_group: wgpu::BindGroup,
    terrain_sampler: wgpu::Sampler,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    slab: GpuTileSlabAllocator,
    atlas: GpuImageryAtlas,
    topologies: HashMap<(u32, bool), CachedIndexTopology>,
    dynamic_pipeline: wgpu::RenderPipeline,
    dynamic_bind_group_layout: wgpu::BindGroupLayout,
    dynamic_uniform_buffer: wgpu::Buffer,
    dynamic_uniform_capacity: usize,
    dynamic_uniform_stride: u32,
    dynamic_bind_group: wgpu::BindGroup,
    dynamic_meshes: HashMap<u64, CachedDynamicGpuMesh>,
    overlay_pipeline: wgpu::RenderPipeline,
    overlay_bind_group_layout: wgpu::BindGroupLayout,
    overlay_sampler: wgpu::Sampler,
    depth_target: Option<(u32, u32, wgpu::TextureView)>,
    offscreen: Option<OffscreenTarget>,
    frame_index: u64,
    last_geometry_rev: u64,
    last_terrain_frame: Option<Arc<RetainedTerrainFrame>>,
    last_scene_ptr: usize,
    cached_fallback_frame: Option<Arc<RetainedTerrainFrame>>,
}

impl Wgpu3DRenderer {
    /// Creates a headless `Wgpu3DRenderer` using any available hardware or software Vulkan/GL adapter.
    pub fn new_headless(color_format: wgpu::TextureFormat) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| "no wgpu adapter available".to_string())?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("strata_render_wgpu_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|e| format!("request_device failed: {e}"))?;
        Ok(Self::from_device(
            Arc::new(device),
            Arc::new(queue),
            color_format,
        ))
    }

    /// Creates a `Wgpu3DRenderer` bound to an existing `wgpu::Device` & `wgpu::Queue` (e.g., from a window surface).
    pub fn from_device(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        color_format: wgpu::TextureFormat,
    ) -> Self {
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_camera_uniforms"),
            size: std::mem::size_of::<GpuCameraUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let initial_instances = 1024usize;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_tile_instances"),
            size: (initial_instances * std::mem::size_of::<GpuTileInstance>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let slab = GpuTileSlabAllocator::new(&device, 262_144);
        let atlas = GpuImageryAtlas::new(&device, &queue, MAX_ATLAS_LAYERS);

        let terrain_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("strata_3d_terrain_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        // Sky pass
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_sky_wgsl"),
            source: wgpu::ShaderSource::Wgsl(SKY_WGSL.into()),
        });
        let sky_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("strata_3d_sky_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let sky_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_sky_bg"),
            layout: &sky_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let sky_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_sky_pl"),
            bind_group_layouts: &[&sky_bgl],
            push_constant_ranges: &[],
        });
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_3d_sky_pipeline"),
            layout: Some(&sky_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs_sky"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_sky"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Terrain vertex-pulling pipeline
        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_terrain_pull_wgsl"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_PULL_WGSL.into()),
        });
        let terrain_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_3d_terrain_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let terrain_bind_group = Self::create_terrain_bind_group(
            &device,
            &terrain_bind_group_layout,
            &camera_buffer,
            &slab.buffer,
            &instance_buffer,
            &atlas.view,
            &terrain_sampler,
        );
        let terrain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_terrain_pl"),
            bind_group_layouts: &[&terrain_bind_group_layout],
            push_constant_ranges: &[],
        });
        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_3d_terrain_pipeline"),
            layout: Some(&terrain_layout),
            vertex: wgpu::VertexState {
                module: &terrain_shader,
                entry_point: Some("vs_terrain"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &terrain_shader,
                entry_point: Some("fs_terrain"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Dynamic RTC mesh pipeline (buildings, entities, fallback meshes)
        let dynamic_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_dynamic_wgsl"),
            source: wgpu::ShaderSource::Wgsl(DYNAMIC_MESH_WGSL.into()),
        });
        let align = device.limits().min_uniform_buffer_offset_alignment.max(256);
        let dyn_size = std::mem::size_of::<GpuDynamicUniforms>() as u32;
        let dynamic_uniform_stride = dyn_size.div_ceil(align) * align;
        let dynamic_uniform_capacity = 64usize;
        let dynamic_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_dynamic_uniforms"),
            size: (dynamic_uniform_capacity * dynamic_uniform_stride as usize) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dynamic_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_3d_dynamic_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<
                                GpuDynamicUniforms,
                            >(
                            )
                                as u64),
                        },
                        count: None,
                    },
                ],
            });
        let dynamic_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_dynamic_bg"),
            layout: &dynamic_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &dynamic_uniform_buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(
                            std::mem::size_of::<GpuDynamicUniforms>() as u64
                        ),
                    }),
                },
            ],
        });
        let dynamic_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_dynamic_pl"),
            bind_group_layouts: &[&dynamic_bind_group_layout],
            push_constant_ranges: &[],
        });
        let dynamic_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_3d_dynamic_pipeline"),
            layout: Some(&dynamic_layout),
            vertex: wgpu::VertexState {
                module: &dynamic_shader,
                entry_point: Some("vs_dynamic"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RteTileVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 1,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &dynamic_shader,
                entry_point: Some("fs_dynamic"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 2D overlay composite pipeline
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_2d_overlay_wgsl"),
            source: wgpu::ShaderSource::Wgsl(OVERLAY_COMPOSITE_WGSL.into()),
        });
        let overlay_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("strata_2d_overlay_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let overlay_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_2d_overlay_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let overlay_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_2d_overlay_pl"),
            bind_group_layouts: &[&overlay_bind_group_layout],
            push_constant_ranges: &[],
        });
        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_2d_overlay_pipeline"),
            layout: Some(&overlay_layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_overlay"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_overlay"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            device,
            queue,
            color_format,
            camera_buffer,
            sky_pipeline,
            sky_bind_group,
            terrain_pipeline,
            terrain_bind_group_layout,
            terrain_bind_group,
            terrain_sampler,
            instance_buffer,
            instance_capacity: initial_instances,
            slab,
            atlas,
            topologies: HashMap::new(),
            dynamic_pipeline,
            dynamic_bind_group_layout,
            dynamic_uniform_buffer,
            dynamic_uniform_capacity,
            dynamic_uniform_stride,
            dynamic_bind_group,
            dynamic_meshes: HashMap::new(),
            overlay_pipeline,
            overlay_bind_group_layout,
            overlay_sampler,
            depth_target: None,
            offscreen: None,
            frame_index: 0,
            last_geometry_rev: 0,
            last_terrain_frame: None,
            last_scene_ptr: 0,
            cached_fallback_frame: None,
        }
    }

    pub fn device(&self) -> &Arc<wgpu::Device> {
        &self.device
    }

    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        &self.queue
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }

    fn create_terrain_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        camera_buffer: &wgpu::Buffer,
        slab_buffer: &wgpu::Buffer,
        instance_buffer: &wgpu::Buffer,
        atlas_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_terrain_bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: slab_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: instance_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    fn ensure_topology(&mut self, grid_res: u32, has_skirts: bool) -> (wgpu::Buffer, u32) {
        if let Some(cached) = self.topologies.get(&(grid_res, has_skirts)) {
            return (cached.buffer.clone(), cached.index_count);
        }
        let indices = build_terrain_grid_indices(grid_res, has_skirts);
        let count = indices.len() as u32;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_grid_topology"),
            size: (indices.len() * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !indices.is_empty() {
            self.queue
                .write_buffer(&buffer, 0, bytemuck::cast_slice(&indices));
        }
        self.topologies.insert(
            (grid_res, has_skirts),
            CachedIndexTopology {
                buffer: buffer.clone(),
                index_count: count,
            },
        );
        (buffer, count)
    }

    fn ensure_depth_view(&mut self, width: u32, height: u32) -> wgpu::TextureView {
        if let Some((w, h, view)) = &self.depth_target {
            if *w == width && *h == height {
                return view.clone();
            }
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("strata_3d_depth"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.depth_target = Some((width, height, view.clone()));
        view
    }

    /// Creates an RGBA `wgpu::BindGroup` for compositing a 2D UI HUD texture over a 3D frame.
    pub fn create_overlay_bind_group(&self, overlay_view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_2d_overlay_bg"),
            layout: &self.overlay_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(overlay_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.overlay_sampler),
                },
            ],
        })
    }

    /// Draws `scene` (or its [`RetainedScene3D`] / [`RetainedTerrainFrame`]) directly into `target_view`.
    ///
    /// When `viewport` is `Some([x, y, w, h])`, restricts rendering to that sub-rectangle of the
    /// render target and uses `w/h` as the projection aspect ratio.
    pub fn render_scene_to_view(
        &mut self,
        scene: &Scene3D,
        target_view: &wgpu::TextureView,
        target_width: u32,
        target_height: u32,
        viewport: Option<[f32; 4]>,
        unlimited_upload_budget: bool,
    ) {
        self.frame_index += 1;
        let frame_idx = self.frame_index;
        let mesh_budget = if unlimited_upload_budget || frame_idx <= 2 {
            usize::MAX
        } else {
            DEFAULT_MESH_UPLOAD_BUDGET
        };
        let tex_budget = if unlimited_upload_budget || frame_idx <= 2 {
            usize::MAX
        } else {
            DEFAULT_TEXTURE_UPLOAD_BUDGET
        };

        let (cam, terrain_frame_opt) = if let Some(retained) = scene.retained_handle() {
            let terrain = retained.snapshot_terrain();
            let cam = retained.camera();
            (cam, terrain)
        } else if let Some(terrain) = scene.retained_terrain() {
            (terrain.camera, Some(Arc::clone(terrain)))
        } else {
            let scene_ptr = scene as *const Scene3D as usize;
            if scene_ptr != self.last_scene_ptr || self.cached_fallback_frame.is_none() {
                self.last_scene_ptr = scene_ptr;
                self.cached_fallback_frame = Some(Arc::new(fallback_frame_from_scene3d(
                    scene,
                    target_width.max(1) as f32,
                    target_height.max(1) as f32,
                )));
            }
            let frame = self.cached_fallback_frame.clone().unwrap();
            (frame.camera, Some(frame))
        };

        // 1. Update camera uniform buffer unconditionally (O(1) tiny 256-byte write).
        let gpu_cam = GpuCameraUniforms::from_retained(&cam);
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&gpu_cam));

        // 2. Upload incremental tile meshes/textures and prepare draw instances.
        let mut draw_count = 0usize;
        let mut grid_topology: Option<(u32, bool)> = None;
        let mut dynamic_draws: Vec<(u64, [f64; 3], [f32; 4], bool)> = Vec::new();

        if let Some(frame) = terrain_frame_opt.as_ref() {
            let frame_changed = frame.geometry_revision != self.last_geometry_rev
                || self
                    .last_terrain_frame
                    .as_ref()
                    .map(|p| !Arc::ptr_eq(p, frame))
                    .unwrap_or(true);
            let _ = frame_changed;
            self.last_geometry_rev = frame.geometry_revision;
            self.last_terrain_frame = Some(Arc::clone(frame));

            let bg_dirty = self.slab.upload_meshes(
                &self.device,
                &self.queue,
                &frame.meshes,
                frame_idx,
                mesh_budget,
            );
            self.atlas
                .upload_textures(&self.queue, &frame.textures, frame_idx, tex_budget);

            let mut instances_gpu: Vec<GpuTileInstance> = Vec::with_capacity(frame.instances.len());
            for inst in &frame.instances {
                let Some(base_v) = self.slab.base_vertex(inst.key, inst.mesh_revision) else {
                    continue;
                };
                if grid_topology.is_none() {
                    if let Some(m) = frame.meshes.iter().find(|m| m.key == inst.key) {
                        grid_topology = Some((m.grid_res, m.has_skirts));
                    }
                }
                let (cx_hi, cx_lo) = split_f64_to_f32(inst.center_ecef[0]);
                let (cy_hi, cy_lo) = split_f64_to_f32(inst.center_ecef[1]);
                let (cz_hi, cz_lo) = split_f64_to_f32(inst.center_ecef[2]);
                let tex_layer = self.atlas.layer_for(inst.texture_id, inst.texture_revision);
                instances_gpu.push(GpuTileInstance {
                    center_high: [cx_hi, cy_hi, cz_hi, 0.0],
                    center_low: [cx_lo, cy_lo, cz_lo, 0.0],
                    uv_rect: inst.uv_rect,
                    tint: inst.tint,
                    tile_meta: [base_v, tex_layer, u32::from(inst.unlit), 0],
                });
            }

            let mut instance_resized = false;
            if instances_gpu.len() > self.instance_capacity {
                let new_cap = instances_gpu.len().next_power_of_two().max(1024);
                self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("strata_3d_tile_instances_grown"),
                    size: (new_cap * std::mem::size_of::<GpuTileInstance>()) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.instance_capacity = new_cap;
                instance_resized = true;
            }
            if bg_dirty || instance_resized {
                self.terrain_bind_group = Self::create_terrain_bind_group(
                    &self.device,
                    &self.terrain_bind_group_layout,
                    &self.camera_buffer,
                    &self.slab.buffer,
                    &self.instance_buffer,
                    &self.atlas.view,
                    &self.terrain_sampler,
                );
            }
            if !instances_gpu.is_empty() {
                self.queue.write_buffer(
                    &self.instance_buffer,
                    0,
                    bytemuck::cast_slice(&instances_gpu),
                );
                draw_count = instances_gpu.len();
            }

            for dm in &frame.dynamic_meshes {
                if dm.vertices.is_empty() || dm.indices.is_empty() {
                    continue;
                }
                let needs_upload = self
                    .dynamic_meshes
                    .get(&dm.id)
                    .map(|c| c.revision != dm.revision)
                    .unwrap_or(true);
                if needs_upload {
                    let vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("strata_3d_dyn_vb"),
                        size: (dm.vertices.len() * std::mem::size_of::<RteTileVertex>()) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.queue
                        .write_buffer(&vbuf, 0, bytemuck::cast_slice(&dm.vertices));
                    let ibuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("strata_3d_dyn_ib"),
                        size: (dm.indices.len() * 4) as u64,
                        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.queue
                        .write_buffer(&ibuf, 0, bytemuck::cast_slice(&dm.indices));
                    self.dynamic_meshes.insert(
                        dm.id,
                        CachedDynamicGpuMesh {
                            revision: dm.revision,
                            vbuf,
                            ibuf,
                            index_count: dm.indices.len() as u32,
                        },
                    );
                }
                dynamic_draws.push((dm.id, dm.anchor_ecef, dm.color, dm.unlit));
            }
        }

        if !dynamic_draws.is_empty() {
            let stride = self.dynamic_uniform_stride as usize;
            if dynamic_draws.len() > self.dynamic_uniform_capacity {
                let new_cap = dynamic_draws.len().next_power_of_two().max(64);
                self.dynamic_uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("strata_3d_dynamic_uniforms_grown"),
                    size: (new_cap * stride) as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.dynamic_uniform_capacity = new_cap;
                self.dynamic_bind_group =
                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("strata_3d_dynamic_bg_grown"),
                        layout: &self.dynamic_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: self.camera_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: &self.dynamic_uniform_buffer,
                                    offset: 0,
                                    size: std::num::NonZeroU64::new(std::mem::size_of::<
                                        GpuDynamicUniforms,
                                    >(
                                    )
                                        as u64),
                                }),
                            },
                        ],
                    });
            }
            let mut staged = vec![0u8; dynamic_draws.len() * stride];
            for (i, (_, anchor, color, unlit)) in dynamic_draws.iter().enumerate() {
                let (ax_hi, ax_lo) = split_f64_to_f32(anchor[0]);
                let (ay_hi, ay_lo) = split_f64_to_f32(anchor[1]);
                let (az_hi, az_lo) = split_f64_to_f32(anchor[2]);
                let dyn_u = GpuDynamicUniforms {
                    anchor_high: [ax_hi, ay_hi, az_hi, 0.0],
                    anchor_low: [ax_lo, ay_lo, az_lo, 0.0],
                    color: *color,
                    dyn_meta: [u32::from(*unlit), 0, 0, 0],
                };
                let bytes = bytemuck::bytes_of(&dyn_u);
                staged[i * stride..i * stride + bytes.len()].copy_from_slice(bytes);
            }
            self.queue
                .write_buffer(&self.dynamic_uniform_buffer, 0, &staged);
        }

        let topology_res = grid_topology.map(|(g, s)| self.ensure_topology(g, s));
        let depth_view = self.ensure_depth_view(target_width.max(1), target_height.max(1));

        let clear_op = match cam.background {
            Background3D::Transparent => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            Background3D::Solid(c) => wgpu::LoadOp::Clear(wgpu::Color {
                r: c.r() as f64,
                g: c.g() as f64,
                b: c.b() as f64,
                a: c.a() as f64,
            }),
            Background3D::VerticalGradient { .. } => wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_3d_render_encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("strata_3d_main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: clear_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if let Some([vx, vy, vw, vh]) = viewport {
                let x = vx.max(0.0).min(target_width as f32);
                let y = vy.max(0.0).min(target_height as f32);
                let w = vw.max(1.0).min((target_width as f32 - x).max(1.0));
                let h = vh.max(1.0).min((target_height as f32 - y).max(1.0));
                pass.set_viewport(x, y, w, h, 0.0, 1.0);
                pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
            }

            if matches!(cam.background, Background3D::VerticalGradient { .. }) {
                pass.set_pipeline(&self.sky_pipeline);
                pass.set_bind_group(0, &self.sky_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            if draw_count > 0 {
                if let Some((ref idx_buf, idx_count)) = topology_res {
                    if idx_count > 0 {
                        pass.set_pipeline(&self.terrain_pipeline);
                        pass.set_bind_group(0, &self.terrain_bind_group, &[]);
                        pass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..idx_count, 0, 0..draw_count as u32);
                    }
                }
            }

            if !dynamic_draws.is_empty() {
                pass.set_pipeline(&self.dynamic_pipeline);
                for (i, (id, _, _, _)) in dynamic_draws.iter().enumerate() {
                    let Some(gpu_mesh) = self.dynamic_meshes.get(id) else {
                        continue;
                    };
                    let offset = (i as u32) * self.dynamic_uniform_stride;
                    pass.set_bind_group(0, &self.dynamic_bind_group, &[offset]);
                    pass.set_vertex_buffer(0, gpu_mesh.vbuf.slice(..));
                    pass.set_index_buffer(gpu_mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..gpu_mesh.index_count, 0, 0..1);
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Composites a GPU 2D UI HUD overlay bind group on top of `target_view` (preserving alpha).
    pub fn composite_overlay_to_view(
        &self,
        target_view: &wgpu::TextureView,
        overlay_bind_group: &wgpu::BindGroup,
    ) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_2d_overlay_encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("strata_2d_overlay_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.overlay_pipeline);
            pass.set_bind_group(0, overlay_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Renders `scene` into a persistent offscreen texture and performs GPU->CPU readback only
    /// when `readback` is `true` (pure GPU timing benchmarks pass `readback = false`).
    pub fn render_offscreen(
        &mut self,
        scene: &Scene3D,
        width: u32,
        height: u32,
        readback: bool,
        unlimited_upload_budget: bool,
    ) -> Result<Vec<u8>, String> {
        let w = width.max(1);
        let h = height.max(1);
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let unpadded = w * 4;
        let padded = unpadded.div_ceil(align) * align;

        let recreate = self
            .offscreen
            .as_ref()
            .map(|o| o.width != w || o.height != h || o.format != self.color_format)
            .unwrap_or(true);
        if recreate {
            let color = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("strata_3d_offscreen_color"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.color_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let color_view = color.create_view(&Default::default());
            let depth = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("strata_3d_offscreen_depth"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let depth_view = depth.create_view(&Default::default());
            let rb = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("strata_3d_offscreen_readback"),
                size: (padded * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.offscreen = Some(OffscreenTarget {
                width: w,
                height: h,
                format: self.color_format,
                color,
                color_view,
                _depth: depth,
                depth_view,
                readback: rb,
                padded_bytes_per_row: padded,
            });
        }

        let target_view = self.offscreen.as_ref().unwrap().color_view.clone();
        self.depth_target = Some((w, h, self.offscreen.as_ref().unwrap().depth_view.clone()));
        self.render_scene_to_view(scene, &target_view, w, h, None, unlimited_upload_budget);

        if !readback {
            self.device.poll(wgpu::Maintain::Wait);
            return Ok(Vec::new());
        }

        let off = self.offscreen.as_ref().unwrap();
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_3d_readback_encoder"),
            });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &off.color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &off.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(off.padded_bytes_per_row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(enc.finish()));

        let slice = off.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("readback recv error: {e}"))?
            .map_err(|e| format!("readback map error: {e:?}"))?;

        let data = slice.get_mapped_range();
        let mut out = vec![0u8; (w * h * 4) as usize];
        for row in 0..h as usize {
            let src_start = row * off.padded_bytes_per_row as usize;
            let dst_start = row * (w as usize * 4);
            out[dst_start..dst_start + w as usize * 4]
                .copy_from_slice(&data[src_start..src_start + w as usize * 4]);
        }
        drop(data);
        off.readback.unmap();
        Ok(out)
    }
}

/// Converts a regular [`Scene3D`] (primitives/custom meshes) into a [`RetainedTerrainFrame`]
/// so standard 3D scenes also accelerate seamlessly through [`Wgpu3DRenderer`].
pub fn fallback_frame_from_scene3d(
    scene: &Scene3D,
    viewport_w: f32,
    viewport_h: f32,
) -> RetainedTerrainFrame {
    let camera = retained_camera_from_scene3d(scene, viewport_w, viewport_h);
    let mut dynamic_meshes = Vec::new();
    for (idx, mesh) in scene.meshes.iter().enumerate() {
        if let Some((verts, inds, color, unlit)) = mesh3d_to_rte(mesh) {
            dynamic_meshes.push(RetainedDynamicMesh {
                id: idx as u64 + 1,
                revision: 1,
                anchor_ecef: [
                    mesh.transform.position[0] as f64,
                    mesh.transform.position[1] as f64,
                    mesh.transform.position[2] as f64,
                ],
                vertices: Arc::new(verts),
                indices: Arc::new(inds),
                color,
                unlit,
            });
        }
    }
    RetainedTerrainFrame {
        camera,
        geometry_revision: 1,
        meshes: Vec::new(),
        instances: Vec::new(),
        textures: Vec::new(),
        dynamic_meshes,
    }
}

fn retained_camera_from_scene3d(
    scene: &Scene3D,
    width: f32,
    height: f32,
) -> RetainedCameraUniforms {
    let aspect = (width.max(1.0) / height.max(1.0)).max(0.01);
    let (eye, target, proj) = match scene.camera {
        Camera3D::Perspective {
            fov_horizontal_deg,
            near,
            far,
            position,
            target,
        } => {
            let hfov_rad = fov_horizontal_deg.to_radians().max(0.05);
            let half_w = (hfov_rad * 0.5).tan();
            let vfov_rad = 2.0 * (half_w / aspect).atan();
            (
                position,
                target,
                perspective_rh_zo(vfov_rad, aspect, near.max(0.01), far.max(near + 1.0)),
            )
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
        } => (
            position,
            target,
            ortho_rh_zo(left, right, bottom, top, near, far),
        ),
    };

    let up_hint = scene.camera_up().unwrap_or([0.0, 1.0, 0.0]);
    let view_rot = look_rot_rh(eye, target, up_hint);
    let view_proj_rte = mul_mat4(proj, view_rot);

    let mut sun_dir = [0.577_350_26f32, 0.577_350_26, 0.577_350_26];
    let mut sun_color = [1.0f32, 0.98, 0.95];
    let mut ambient_color = [0.22f32, 0.24, 0.28];
    for light in &scene.lights {
        match light {
            Light3D::Ambient { color, intensity } => {
                ambient_color = [
                    color.r() * intensity,
                    color.g() * intensity,
                    color.b() * intensity,
                ];
            }
            Light3D::Directional {
                position,
                color,
                intensity,
                ..
            } => {
                let len = (position[0] * position[0]
                    + position[1] * position[1]
                    + position[2] * position[2])
                    .sqrt()
                    .max(1e-6);
                sun_dir = [position[0] / len, position[1] / len, position[2] / len];
                sun_color = [
                    color.r() * intensity,
                    color.g() * intensity,
                    color.b() * intensity,
                ];
            }
            Light3D::Point { .. } => {}
        }
    }

    RetainedCameraUniforms {
        proj,
        view_rot,
        view_proj_rte,
        eye_ecef: [eye[0] as f64, eye[1] as f64, eye[2] as f64],
        sun_dir,
        sun_color,
        ambient_color,
        background: scene.background(),
        near: 0.1,
        far: 100_000_000.0,
    }
}

fn perspective_rh_zo(vfov_rad: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let f = 1.0 / (vfov_rad * 0.5).tan();
    let nf = 1.0 / (near - far);
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far * nf, -1.0],
        [0.0, 0.0, (far * near) * nf, 0.0],
    ]
}

fn ortho_rh_zo(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> [[f32; 4]; 4] {
    let rml = (r - l).max(1e-5);
    let tmb = (t - b).max(1e-5);
    let fmn = (f - n).max(1e-5);
    [
        [2.0 / rml, 0.0, 0.0, 0.0],
        [0.0, 2.0 / tmb, 0.0, 0.0],
        [0.0, 0.0, -1.0 / fmn, 0.0],
        [-(r + l) / rml, -(t + b) / tmb, -n / fmn, 1.0],
    ]
}

fn look_rot_rh(eye: [f32; 3], target: [f32; 3], up_hint: [f64; 3]) -> [[f32; 4]; 4] {
    let fx = (target[0] - eye[0]) as f64;
    let fy = (target[1] - eye[1]) as f64;
    let fz = (target[2] - eye[2]) as f64;
    let flen = (fx * fx + fy * fy + fz * fz).sqrt().max(1e-9);
    let f = [fx / flen, fy / flen, fz / flen];
    let mut s = [
        f[1] * up_hint[2] - f[2] * up_hint[1],
        f[2] * up_hint[0] - f[0] * up_hint[2],
        f[0] * up_hint[1] - f[1] * up_hint[0],
    ];
    let mut slen = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
    if slen < 1e-6 {
        let alt = if f[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        s = [
            f[1] * alt[2] - f[2] * alt[1],
            f[2] * alt[0] - f[0] * alt[2],
            f[0] * alt[1] - f[1] * alt[0],
        ];
        slen = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt().max(1e-9);
    }
    let s = [s[0] / slen, s[1] / slen, s[2] / slen];
    let u = [
        s[1] * f[2] - s[2] * f[1],
        s[2] * f[0] - s[0] * f[2],
        s[0] * f[1] - s[1] * f[0],
    ];
    [
        [s[0] as f32, u[0] as f32, -f[0] as f32, 0.0],
        [s[1] as f32, u[1] as f32, -f[1] as f32, 0.0],
        [s[2] as f32, u[2] as f32, -f[2] as f32, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn mul_mat4(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] =
                a[0][r] * b[c][0] + a[1][r] * b[c][1] + a[2][r] * b[c][2] + a[3][r] * b[c][3];
        }
    }
    out
}

fn mesh3d_to_rte(mesh: &Mesh3D) -> Option<(Vec<RteTileVertex>, Vec<u32>, [f32; 4], bool)> {
    let (color, unlit) = match &mesh.material {
        Material3D::Standard { color, .. } | Material3D::Physical { color, .. } => {
            ([color.r(), color.g(), color.b(), color.a()], false)
        }
        Material3D::Basic { color } => ([color.r(), color.g(), color.b(), color.a()], true),
        Material3D::Textured(tm) => (
            [
                tm.tint.r(),
                tm.tint.g(),
                tm.tint.b(),
                tm.tint.a() * tm.opacity,
            ],
            !tm.lit,
        ),
        Material3D::TexturedBatch(tb) => {
            let tint = tb
                .draws
                .first()
                .map(|d| d.material.tint)
                .unwrap_or(strata_style::Color::WHITE);
            ([tint.r(), tint.g(), tint.b(), tint.a()], false)
        }
        Material3D::Shader { .. } => ([0.8, 0.8, 0.8, 1.0], false),
    };

    let scale = mesh.transform.scale;
    let map_pos = |x: f32, y: f32, z: f32| [x * scale[0], y * scale[1], z * scale[2]];

    match &mesh.geometry {
        Geometry3D::Custom { vertices, indices } => {
            let mut out_v = Vec::with_capacity(vertices.len() / 6);
            for chunk in vertices.chunks_exact(6) {
                out_v.push(RteTileVertex::new(
                    map_pos(chunk[0], chunk[1], chunk[2]),
                    [chunk[3], chunk[4], chunk[5]],
                    [0.0, 0.0],
                ));
            }
            Some((out_v, indices.clone(), color, unlit))
        }
        Geometry3D::CustomTextured { vertices, indices } => {
            let mut out_v = Vec::with_capacity(vertices.len() / 8);
            for chunk in vertices.chunks_exact(8) {
                out_v.push(RteTileVertex::new(
                    map_pos(chunk[0], chunk[1], chunk[2]),
                    [chunk[3], chunk[4], chunk[5]],
                    [chunk[6], chunk[7]],
                ));
            }
            Some((out_v, indices.clone(), color, unlit))
        }
        Geometry3D::Box {
            width,
            height,
            depth,
        } => {
            let hw = width * 0.5;
            let hh = height * 0.5;
            let hd = depth * 0.5;
            let corners = [
                [-hw, -hh, -hd],
                [hw, -hh, -hd],
                [hw, hh, -hd],
                [-hw, hh, -hd],
                [-hw, -hh, hd],
                [hw, -hh, hd],
                [hw, hh, hd],
                [-hw, hh, hd],
            ];
            let faces: [([usize; 4], [f32; 3]); 6] = [
                ([4, 5, 6, 7], [0.0, 0.0, 1.0]),
                ([1, 0, 3, 2], [0.0, 0.0, -1.0]),
                ([5, 1, 2, 6], [1.0, 0.0, 0.0]),
                ([0, 4, 7, 3], [-1.0, 0.0, 0.0]),
                ([7, 6, 2, 3], [0.0, 1.0, 0.0]),
                ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
            ];
            let mut verts = Vec::with_capacity(24);
            let mut inds = Vec::with_capacity(36);
            for (f_idx, (c, n)) in faces.into_iter().enumerate() {
                let b = (f_idx * 4) as u32;
                verts.push(RteTileVertex::new(
                    map_pos(corners[c[0]][0], corners[c[0]][1], corners[c[0]][2]),
                    n,
                    [0.0, 0.0],
                ));
                verts.push(RteTileVertex::new(
                    map_pos(corners[c[1]][0], corners[c[1]][1], corners[c[1]][2]),
                    n,
                    [1.0, 0.0],
                ));
                verts.push(RteTileVertex::new(
                    map_pos(corners[c[2]][0], corners[c[2]][1], corners[c[2]][2]),
                    n,
                    [1.0, 1.0],
                ));
                verts.push(RteTileVertex::new(
                    map_pos(corners[c[3]][0], corners[c[3]][1], corners[c[3]][2]),
                    n,
                    [0.0, 1.0],
                ));
                inds.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
            }
            Some((verts, inds, color, unlit))
        }
        Geometry3D::Plane { width, height } => {
            let hw = width * 0.5;
            let hh = height * 0.5;
            let verts = vec![
                RteTileVertex::new(map_pos(-hw, 0.0, -hh), [0.0, 1.0, 0.0], [0.0, 0.0]),
                RteTileVertex::new(map_pos(hw, 0.0, -hh), [0.0, 1.0, 0.0], [1.0, 0.0]),
                RteTileVertex::new(map_pos(hw, 0.0, hh), [0.0, 1.0, 0.0], [1.0, 1.0]),
                RteTileVertex::new(map_pos(-hw, 0.0, hh), [0.0, 1.0, 0.0], [0.0, 1.0]),
            ];
            Some((verts, vec![0, 1, 2, 0, 2, 3], color, unlit))
        }
        Geometry3D::Sphere {
            radius,
            width_segments,
            height_segments,
        } => {
            let ws = (*width_segments).max(6);
            let hs = (*height_segments).max(4);
            let mut verts = Vec::new();
            let mut inds = Vec::new();
            for iy in 0..=hs {
                let v = iy as f32 / hs as f32;
                let phi = v * std::f32::consts::PI;
                for ix in 0..=ws {
                    let u = ix as f32 / ws as f32;
                    let theta = u * std::f32::consts::TAU;
                    let nx = -phi.sin() * theta.cos();
                    let ny = phi.cos();
                    let nz = phi.sin() * theta.sin();
                    verts.push(RteTileVertex::new(
                        map_pos(nx * radius, ny * radius, nz * radius),
                        [nx, ny, nz],
                        [u, v],
                    ));
                }
            }
            for iy in 0..hs {
                for ix in 0..ws {
                    let a = iy * (ws + 1) + ix;
                    let b = a + 1;
                    let c = (iy + 1) * (ws + 1) + ix;
                    let d = c + 1;
                    inds.extend_from_slice(&[a, c, d, a, d, b]);
                }
            }
            Some((verts, inds, color, unlit))
        }
        Geometry3D::Cylinder {
            radius_top,
            radius_bottom,
            height,
            radial_segments,
        } => {
            let segs = (*radial_segments).max(6);
            let hh = height * 0.5;
            let mut verts = Vec::new();
            let mut inds = Vec::new();
            for i in 0..=segs {
                let u = i as f32 / segs as f32;
                let a = u * std::f32::consts::TAU;
                let (sa, ca) = a.sin_cos();
                verts.push(RteTileVertex::new(
                    map_pos(ca * radius_top, hh, sa * radius_top),
                    [ca, 0.0, sa],
                    [u, 0.0],
                ));
                verts.push(RteTileVertex::new(
                    map_pos(ca * radius_bottom, -hh, sa * radius_bottom),
                    [ca, 0.0, sa],
                    [u, 1.0],
                ));
            }
            for i in 0..segs {
                let t0 = i * 2;
                let b0 = t0 + 1;
                let t1 = t0 + 2;
                let b1 = t0 + 3;
                inds.extend_from_slice(&[t0, b0, b1, t0, b1, t1]);
            }
            Some((verts, inds, color, unlit))
        }
    }
}

// Keep unused collection imports suppressed cleanly if any.
#[allow(dead_code)]
fn _assert_collections_used(_: BTreeMap<u64, u64>, _: HashSet<u64>) {}
