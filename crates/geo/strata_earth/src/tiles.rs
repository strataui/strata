//! Tile imagery and elevation sourcing.
//!
//! The reference app couples tile loading to two app-specific things: an
//! `egui` texture upload and a `curl` subprocess. Neither belongs in the
//! engine. Here the two halves are separated:
//!
//! - [`TileSource`] is the decision layer: given a [`TileKey`], return the
//!   decoded pixels and where they came from. Implementations decide cache
//!   layout, transport, and format.
//! - [`TileRegistry`] is the upload layer: it owns the [`TextureStore`] and maps
//!   source identities to stable texture handles.
//!
//! Because the transport sits behind a trait, the plugin has no shell
//! dependency and tests can use an in-memory source.

use crate::geodesy::TileKey;
use strata_render::texture3d::{
    AddressMode, FilterMode, SamplerDesc, TextureData, TextureDesc, TextureFormat, TextureId,
    TextureStore,
};

/// Decoded image data for one tile.
#[derive(Debug, Clone, PartialEq)]
pub struct TileImage {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA8, top row first.
    pub pixels: Vec<u8>,
}

impl TileImage {
    /// Builds a texture payload, validating the pixel count.
    pub fn to_texture_data(&self) -> Option<TextureData> {
        let expected = self.width as usize * self.height as usize * 4;
        if self.pixels.len() != expected {
            return None;
        }
        Some(TextureData {
            desc: TextureDesc {
                width: self.width,
                height: self.height,
                format: TextureFormat::Rgba8,
            },
            pixels: self.pixels.clone().into(),
        })
    }
}

/// Elevation sample for one tile.
#[derive(Debug, Clone, PartialEq)]
pub struct TileElevation {
    pub width: u32,
    pub height: u32,
    /// Height in metres, row-major, `width * height` entries.
    pub heights_m: Vec<f32>,
}

/// Errors a tile source can report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileError {
    /// The tile is not in the local cache and no transport is configured.
    NotCached,
    /// Transport failed; the message is for logs, not for display.
    Transport(String),
    /// The payload could not be decoded.
    Decode(String),
}

/// Where tile data comes from.
///
/// Implementations must be cheap to call per frame (hits should not block).
/// A miss may block, but callers should treat it as optional and keep drawing
/// the ancestor imagery instead.
pub trait TileSource {
    /// Returns the imagery for a tile, if available right now.
    fn imagery(&mut self, key: TileKey) -> Result<Option<TileImage>, TileError>;

    /// Returns the elevation for a tile, if available right now.
    fn elevation(&mut self, key: TileKey) -> Result<Option<TileElevation>, TileError>;
}

/// Identifies an uploaded tile image so repeated frames reuse one handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureSlot {
    pub key: TileKey,
    /// Bumped when the source replaces an image for the same key.
    pub revision: u32,
}

/// Default sampler for satellite imagery: linear filtering, clamped edges.
pub fn tile_sampler() -> SamplerDesc {
    SamplerDesc {
        filter: FilterMode::Bilinear,
        address_u: AddressMode::ClampToEdge,
        address_v: AddressMode::ClampToEdge,
    }
}

/// Owns uploaded tile textures and hands out stable handles.
#[derive(Debug, Default)]
pub struct TileRegistry {
    store: TextureStore,
    slots: std::collections::HashMap<TextureSlot, TextureId>,
}

impl TileRegistry {
    pub fn new() -> Self {
        Self {
            store: TextureStore::new(),
            slots: std::collections::HashMap::new(),
        }
    }

    /// Returns the store the renderer samples from.
    pub fn store(&self) -> &TextureStore {
        &self.store
    }

    /// Uploads an image for a slot if it is not present yet.
    pub fn ensure(&mut self, slot: TextureSlot, image: &TileImage) -> Option<TextureId> {
        if let Some(existing) = self.slots.get(&slot) {
            return Some(*existing);
        }
        let data = image.to_texture_data()?;
        let id = self.store.insert(data, tile_sampler())?;
        self.slots.insert(slot, id);
        Some(id)
    }

    /// Looks up a previously uploaded slot.
    pub fn get(&self, slot: TextureSlot) -> Option<TextureId> {
        self.slots.get(&slot).copied()
    }

    /// Drops slots that are not in `keep`, releasing their texture handles.
    ///
    /// Returns the number of evicted slots.
    pub fn retain(&mut self, keep: &std::collections::HashSet<TextureSlot>) -> usize {
        let before = self.slots.len();
        let evicted: Vec<TextureSlot> = self
            .slots
            .keys()
            .copied()
            .filter(|slot| !keep.contains(slot))
            .collect();
        for slot in &evicted {
            if let Some(id) = self.slots.remove(slot) {
                self.store.remove(id);
            }
        }
        before - self.slots.len()
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// A tile source backed by an in-memory map, useful for tests and for callers
/// that decode tiles themselves.
#[derive(Debug, Default)]
pub struct MemoryTileSource {
    imagery: std::collections::HashMap<TileKey, TileImage>,
    elevation: std::collections::HashMap<TileKey, TileElevation>,
    /// Optional revision counter per key, so a test can force a re-upload.
    revisions: std::collections::HashMap<TileKey, u32>,
}

impl MemoryTileSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_imagery(&mut self, key: TileKey, image: TileImage) -> &mut Self {
        self.imagery.insert(key, image);
        self
    }

    pub fn insert_elevation(&mut self, key: TileKey, elevation: TileElevation) -> &mut Self {
        self.elevation.insert(key, elevation);
        self
    }

    pub fn set_revision(&mut self, key: TileKey, revision: u32) -> &mut Self {
        self.revisions.insert(key, revision);
        self
    }

    pub fn revision(&self, key: TileKey) -> u32 {
        self.revisions.get(&key).copied().unwrap_or(0)
    }
}

impl TileSource for MemoryTileSource {
    fn imagery(&mut self, key: TileKey) -> Result<Option<TileImage>, TileError> {
        Ok(self.imagery.get(&key).cloned())
    }

    fn elevation(&mut self, key: TileKey) -> Result<Option<TileElevation>, TileError> {
        Ok(self.elevation.get(&key).cloned())
    }
}
