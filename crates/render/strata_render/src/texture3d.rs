//! Texture resources, sampler state, and per-triangle sampling for the 3D path.
//!
//! The 3D rasterizer previously had no way to represent an image on a mesh, so
//! tile imagery could not be drawn at all. This module provides the minimum a
//! renderer needs: an RGBA texture store with stable handles, sampler state
//! (filter and address mode), and a UV transform that maps a tile's sub-rect of
//! a loaded ancestor texture onto a unit quad.
//!
//! Sampling is perspective correct: callers interpolate `u/w` and `v/w`
//! alongside `1/w`, then divide. See `crate::rasterizer3d`.

use std::collections::HashMap;
use std::sync::Arc;

/// Opaque handle to a texture in a [`TextureStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureId(pub u32);

/// Pixel layout of a texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextureFormat {
    /// 8-bit per channel RGBA, rows top to bottom, no padding.
    #[default]
    Rgba8,
}

impl TextureFormat {
    /// Bytes occupied by one pixel.
    #[inline(always)]
    pub const fn bytes_per_pixel(self) -> usize {
        4
    }
}

/// Minification and magnification filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FilterMode {
    /// Nearest neighbour: exact texel, no blending.
    Nearest,
    /// Bilinear: four-tap blend of neighbouring texels.
    #[default]
    Bilinear,
}

/// Out-of-range behaviour for texture coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AddressMode {
    /// Coordinates are clamped to the border texels.
    #[default]
    ClampToEdge,
    /// Coordinates wrap to `frac(u)`.
    Repeat,
    /// Coordinates mirror on each integer boundary.
    MirrorRepeat,
}

impl AddressMode {
    /// Resolves a normalized coordinate into a wrapped or clamped value.
    #[inline(always)]
    pub fn wrap(self, t: f32) -> f32 {
        match self {
            Self::ClampToEdge => t.clamp(0.0, 1.0),
            Self::Repeat => t - t.floor(),
            Self::MirrorRepeat => {
                // Period is 2: `0..=1` mirrors, `1..=2` reflects back.
                let period = 2.0;
                let mut m = t - (t / period).floor() * period;
                if m > 1.0 {
                    m = period - m;
                }
                m
            }
        }
    }
}

/// An affine transform applied to texture coordinates before sampling.
///
/// The reference resolves a tile to the closest loaded ancestor image plus the
/// exact sub-rectangle of that image the tile occupies. That rectangle is
/// `uv * scale + offset`, where `scale` is the sub-rect extent and `offset` is
/// its origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvTransform {
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

impl Default for UvTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl UvTransform {
    /// Samples the whole texture.
    pub const IDENTITY: Self = Self {
        scale: [1.0, 1.0],
        offset: [0.0, 0.0],
    };

    /// Builds a transform from the sub-rect `[u_min, v_min] ..= [u_max, v_max]`.
    pub fn from_sub_rect(u_min: f32, v_min: f32, u_max: f32, v_max: f32) -> Self {
        Self {
            scale: [u_max - u_min, v_max - v_min],
            offset: [u_min, v_min],
        }
    }

    /// Applies the transform to a normalized coordinate.
    #[inline(always)]
    pub fn apply(self, uv: (f32, f32)) -> (f32, f32) {
        (
            uv.0 * self.scale[0] + self.offset[0],
            uv.1 * self.scale[1] + self.offset[1],
        )
    }
}

/// Sampler state for one texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SamplerDesc {
    pub filter: FilterMode,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
}

/// Immutable texture description.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureDesc {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
}

impl TextureDesc {
    /// Number of bytes a correctly sized pixel buffer occupies.
    #[inline(always)]
    pub const fn byte_len(&self) -> usize {
        self.width as usize * self.height as usize * self.format.bytes_per_pixel()
    }
}

/// A described texture payload.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureData {
    pub desc: TextureDesc,
    /// Tightly packed pixels; length must equal `desc.byte_len()`.
    pub pixels: Arc<[u8]>,
}

/// A stored texture: payload plus sampler state.
#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    pub data: TextureData,
    pub sampler: SamplerDesc,
}

/// Registry of textures addressable by [`TextureId`].
///
/// Ids are stable for the lifetime of the store and are never reused, so a
/// handle held by a mesh cannot accidentally resolve to a different image
/// after an eviction.
#[derive(Debug, Clone, Default)]
pub struct TextureStore {
    textures: HashMap<TextureId, Texture>,
    next_id: u32,
    generation: u64,
}

impl TextureStore {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            next_id: 1,
            generation: 0,
        }
    }

    /// Inserts a texture and returns its handle.
    ///
    /// Returns `None` when the pixel buffer does not match the description.
    pub fn insert(&mut self, data: TextureData, sampler: SamplerDesc) -> Option<TextureId> {
        if data.pixels.len() != data.desc.byte_len()
            || data.desc.width == 0
            || data.desc.height == 0
        {
            return None;
        }
        let id = TextureId(self.next_id);
        self.next_id += 1;
        self.textures.insert(id, Texture { data, sampler });
        self.generation += 1;
        Some(id)
    }

    /// Inserts a texture with the default sampler.
    pub fn insert_rgba8(
        &mut self,
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Option<TextureId> {
        self.insert(
            TextureData {
                desc: TextureDesc {
                    width,
                    height,
                    format: TextureFormat::Rgba8,
                },
                pixels: pixels.into(),
            },
            SamplerDesc::default(),
        )
    }

    pub fn get(&self, id: TextureId) -> Option<&Texture> {
        self.textures.get(&id)
    }

    pub fn remove(&mut self, id: TextureId) -> Option<Texture> {
        let removed = self.textures.remove(&id);
        if removed.is_some() {
            self.generation += 1;
        }
        removed
    }

    pub fn len(&self) -> usize {
        self.textures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Number of mutations since creation. GPU upload caches compare this to
    /// decide whether a bind group must be rebuilt.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Samples a texture with perspective-correct inputs already divided out.
    ///
    /// `uv` is the interpolated `(u/w, v/w)` divided by the interpolated
    /// `1/w` by the caller. Returns straight RGBA8.
    pub fn sample(&self, id: TextureId, uv: (f32, f32)) -> Option<[u8; 4]> {
        let texture = self.get(id)?;
        Some(sample_texture(texture, uv))
    }
}

/// Samples one texture. Pure function so the rasterizer can call it per pixel
/// without touching the store's hash map.
#[inline]
pub fn sample_texture(texture: &Texture, uv: (f32, f32)) -> [u8; 4] {
    let width = texture.data.desc.width as usize;
    let height = texture.data.desc.height as usize;
    let pixels = &texture.data.pixels;

    let u = texture.sampler.address_u.wrap(uv.0);
    let v = texture.sampler.address_v.wrap(uv.1);

    match texture.sampler.filter {
        FilterMode::Nearest => {
            let x = ((u * width as f32) as isize).clamp(0, width as isize - 1) as usize;
            let y = ((v * height as f32) as isize).clamp(0, height as isize - 1) as usize;
            texel(pixels, width, x, y)
        }
        FilterMode::Bilinear => {
            // Texel centres sit at (i + 0.5) / size.
            let fx = u * width as f32 - 0.5;
            let fy = v * height as f32 - 0.5;
            let x0 = fx.floor();
            let y0 = fy.floor();
            let tx = fx - x0;
            let ty = fy - y0;

            let xi = |raw: f32| -> usize { raw.clamp(0.0, width as f32 - 1.0) as usize };
            let yi = |raw: f32| -> usize { raw.clamp(0.0, height as f32 - 1.0) as usize };

            let x0u = xi(x0);
            let x1u = xi(x0 + 1.0);
            let y0u = yi(y0);
            let y1u = yi(y0 + 1.0);

            let c00 = texel(pixels, width, x0u, y0u);
            let c10 = texel(pixels, width, x1u, y0u);
            let c01 = texel(pixels, width, x0u, y1u);
            let c11 = texel(pixels, width, x1u, y1u);

            let mut out = [0u8; 4];
            for ch in 0..4 {
                let top = c00[ch] as f32 * (1.0 - tx) + c10[ch] as f32 * tx;
                let bottom = c01[ch] as f32 * (1.0 - tx) + c11[ch] as f32 * tx;
                let value = top * (1.0 - ty) + bottom * ty;
                out[ch] = value.round().clamp(0.0, 255.0) as u8;
            }
            out
        }
    }
}

#[inline(always)]
fn texel(pixels: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let idx = (y * width + x) * 4;
    [
        pixels[idx],
        pixels[idx + 1],
        pixels[idx + 2],
        pixels[idx + 3],
    ]
}
