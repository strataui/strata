//! GPU Texture Atlas with Shelf Bin-Packing, Sub-Allocation, and LRU Eviction in VRAM.
//!
//! Provides dynamic sub-allocation of 2D VRAM texture space for images, vector icons,
//! and rendered canvas elements with zero texture thrashing and automated LRU recycling.

use crate::gpu_context::{GpuDevice, GpuTextureHandle};
use crate::swapchain::SwapchainFormat;
use std::collections::HashMap;

/// Allocated sub-region within the GPU texture atlas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasRegion {
    pub id: u64,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub uv_bounds: [f32; 4], // [u_min, v_min, u_max, v_max]
    pub last_used_frame: u64,
}

/// Horizontal shelf for 2D bin-packing within the atlas.
#[derive(Debug, Clone)]
struct Shelf {
    y: u32,
    height: u32,
    current_x: u32,
}

/// High-performance GPU texture atlas with dynamic shelf bin-packing and LRU cache eviction.
pub struct GpuTextureAtlas {
    pub width: u32,
    pub height: u32,
    pub format: SwapchainFormat,
    pub texture: GpuTextureHandle,
    shelves: Vec<Shelf>,
    allocated_regions: HashMap<u64, AtlasRegion>,
    next_region_id: u64,
    next_shelf_y: u32,
    total_allocations: u64,
    total_evictions: u64,
}

impl GpuTextureAtlas {
    /// Creates a new GPU texture atlas on the device with the specified dimensions (e.g. 2048x2048 or 4096x4096).
    pub fn new(device: &GpuDevice, width: u32, height: u32, format: SwapchainFormat) -> Self {
        let texture = device.create_texture(width, height, format, false);
        Self {
            width,
            height,
            format,
            texture,
            shelves: Vec::new(),
            allocated_regions: HashMap::new(),
            next_region_id: 1,
            next_shelf_y: 0,
            total_allocations: 0,
            total_evictions: 0,
        }
    }

    /// Allocates a sub-rectangle in the texture atlas for the given dimensions,
    /// evicting least-recently-used regions if necessary.
    pub fn allocate(&mut self, req_w: u32, req_h: u32, current_frame: u64) -> Option<AtlasRegion> {
        if req_w == 0 || req_h == 0 || req_w > self.width || req_h > self.height {
            return None;
        }

        // 1. Try to fit into an existing shelf with the least height waste
        let mut best_shelf_idx: Option<usize> = None;
        let mut best_waste = u32::MAX;

        for (idx, shelf) in self.shelves.iter().enumerate() {
            if shelf.height >= req_h && (shelf.current_x + req_w) <= self.width {
                let waste = shelf.height - req_h;
                if waste < best_waste {
                    best_waste = waste;
                    best_shelf_idx = Some(idx);
                }
            }
        }

        if let Some(idx) = best_shelf_idx {
            let shelf = &mut self.shelves[idx];
            let x = shelf.current_x;
            let y = shelf.y;
            shelf.current_x += req_w;

            return Some(self.register_region(x, y, req_w, req_h, current_frame));
        }

        // 2. Try creating a new shelf below current shelves
        if self.next_shelf_y + req_h <= self.height {
            let shelf = Shelf {
                y: self.next_shelf_y,
                height: req_h,
                current_x: req_w,
            };
            let x = 0;
            let y = self.next_shelf_y;
            self.next_shelf_y += req_h;
            self.shelves.push(shelf);

            return Some(self.register_region(x, y, req_w, req_h, current_frame));
        }

        // 3. Atlas full: Evict LRU region and reclaim memory if needed
        self.evict_lru_regions(req_w, req_h, current_frame);

        // Retry allocation on reset shelf space
        if self.next_shelf_y + req_h <= self.height {
            let x = 0;
            let y = self.next_shelf_y;
            self.next_shelf_y += req_h;
            self.shelves.push(Shelf {
                y,
                height: req_h,
                current_x: req_w,
            });
            Some(self.register_region(x, y, req_w, req_h, current_frame))
        } else {
            None
        }
    }

    /// Registers and stores an allocated sub-region with normalized UV coordinates.
    fn register_region(&mut self, x: u32, y: u32, w: u32, h: u32, current_frame: u64) -> AtlasRegion {
        let id = self.next_region_id;
        self.next_region_id += 1;
        self.total_allocations += 1;

        let uv_min_x = x as f32 / self.width as f32;
        let uv_min_y = y as f32 / self.height as f32;
        let uv_max_x = (x + w) as f32 / self.width as f32;
        let uv_max_y = (y + h) as f32 / self.height as f32;

        let region = AtlasRegion {
            id,
            x,
            y,
            width: w,
            height: h,
            uv_bounds: [uv_min_x, uv_min_y, uv_max_x, uv_max_y],
            last_used_frame: current_frame,
        };

        self.allocated_regions.insert(id, region);
        region
    }

    /// Evicts the least recently used regions from the atlas to reclaim space.
    fn evict_lru_regions(&mut self, _needed_w: u32, _needed_h: u32, _current_frame: u64) {
        if self.allocated_regions.is_empty() {
            return;
        }

        // Find oldest regions not accessed in recent frames
        let mut sorted: Vec<(u64, u64)> = self
            .allocated_regions
            .iter()
            .map(|(&id, r)| (id, r.last_used_frame))
            .collect();
        sorted.sort_by_key(|&(_, frame)| frame);

        // Evict oldest 25% of regions
        let evict_count = (sorted.len() / 4).max(1);
        for &(id, _) in sorted.iter().take(evict_count) {
            self.allocated_regions.remove(&id);
            self.total_evictions += 1;
        }

        // If compaction needed, repack remaining regions preserving their IDs
        if self.allocated_regions.len() < 10 && !self.shelves.is_empty() {
            let mut remaining: Vec<AtlasRegion> = self.allocated_regions.values().copied().collect();
            self.shelves.clear();
            self.next_shelf_y = 0;
            for r in &mut remaining {
                let mut placed = false;
                for shelf in &mut self.shelves {
                    if shelf.height >= r.height && (shelf.current_x + r.width) <= self.width {
                        r.x = shelf.current_x;
                        r.y = shelf.y;
                        shelf.current_x += r.width;
                        placed = true;
                        break;
                    }
                }
                if !placed && self.next_shelf_y + r.height <= self.height {
                    let y = self.next_shelf_y;
                    self.shelves.push(Shelf {
                        y,
                        height: r.height,
                        current_x: r.width,
                    });
                    self.next_shelf_y += r.height;
                    r.x = 0;
                    r.y = y;
                }
                r.uv_bounds = [
                    r.x as f32 / self.width as f32,
                    r.y as f32 / self.height as f32,
                    (r.x + r.width) as f32 / self.width as f32,
                    (r.y + r.height) as f32 / self.height as f32,
                ];
                self.allocated_regions.insert(r.id, *r);
            }
        }
    }

    /// Touches a region to update its LRU timestamp.
    pub fn touch(&mut self, region_id: u64, current_frame: u64) {
        if let Some(region) = self.allocated_regions.get_mut(&region_id) {
            region.last_used_frame = current_frame;
        }
    }

    /// Uploads CPU pixels into an allocated atlas sub-region.
    pub fn upload_sub_image(&mut self, region: &AtlasRegion, pixels: &[u8]) {
        let bpp = self.format.bytes_per_pixel();
        let region_stride = region.width as usize * bpp;
        let atlas_stride = self.width as usize * bpp;

        for row in 0..region.height as usize {
            let src_start = row * region_stride;
            let src_end = src_start + region_stride;
            let dst_start = (region.y as usize + row) * atlas_stride + (region.x as usize * bpp);

            if src_end <= pixels.len() && (dst_start + region_stride) <= self.texture.data.len() {
                self.texture.data[dst_start..dst_start + region_stride]
                    .copy_from_slice(&pixels[src_start..src_end]);
            }
        }
    }

    /// Returns the number of currently active allocated regions.
    pub fn allocated_region_count(&self) -> usize {
        self.allocated_regions.len()
    }

    /// Deallocates an existing region by ID.
    pub fn deallocate(&mut self, region_id: u64) -> bool {
        self.allocated_regions.remove(&region_id).is_some()
    }

    /// Explicitly triggers LRU cache eviction of aged regions.
    pub fn evict_lru(&mut self, current_frame: u64) {
        self.evict_lru_regions(0, 0, current_frame);
    }

    /// Returns allocation statistics: (allocated_count, total_allocations, total_evictions).
    pub fn stats(&self) -> (usize, u64, u64) {
        (
            self.allocated_regions.len(),
            self.total_allocations,
            self.total_evictions,
        )
    }
}
