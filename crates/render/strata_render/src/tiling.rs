//! Viewport Tiling, 256x256 Surface Partitioning, and Front-to-Back Quad Occlusion Culling.

use crate::compositor::{DisplayList, DrawCommand};
use strata_layout::Rect;

/// Standard viewport tile dimension in physical pixels (matching Chromium cc compositor standard).
pub const DEFAULT_TILE_SIZE: u32 = 256;

/// 2D Discrete Grid Coordinate for a viewport tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileCoord {
    pub col: u32,
    pub row: u32,
}

impl TileCoord {
    #[inline(always)]
    pub const fn new(col: u32, row: u32) -> Self {
        Self { col, row }
    }
}

/// A discrete 256x256 pixel viewport surface partition.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub coord: TileCoord,
    pub bounds: Rect,
    pub is_dirty: bool,
    pub is_occluded: bool,
}

impl Tile {
    pub fn new(col: u32, row: u32, bounds: Rect) -> Self {
        Self {
            coord: TileCoord::new(col, row),
            bounds,
            is_dirty: false,
            is_occluded: false,
        }
    }

    #[inline]
    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
    }

    #[inline]
    pub fn mark_occluded(&mut self) {
        self.is_occluded = true;
    }

    #[inline]
    pub fn reset(&mut self) {
        self.is_dirty = false;
        self.is_occluded = false;
    }
}

/// A regular 2D grid partitioning of a presentation surface into fixed-size tiles.
#[derive(Debug, Clone, PartialEq)]
pub struct TilingSet {
    pub tile_size: u32,
    pub cols: u32,
    pub rows: u32,
    pub surface_width: u32,
    pub surface_height: u32,
    pub tiles: Vec<Tile>,
}

impl TilingSet {
    /// Constructs a tiling grid covering the specified surface dimensions.
    pub fn new(surface_width: u32, surface_height: u32, tile_size: u32) -> Self {
        let ts = tile_size.max(16);
        let cols = if surface_width == 0 {
            0
        } else {
            surface_width.div_ceil(ts)
        };
        let rows = if surface_height == 0 {
            0
        } else {
            surface_height.div_ceil(ts)
        };

        let mut tiles = Vec::with_capacity((cols * rows) as usize);
        for r in 0..rows {
            let y = (r * ts) as f32;
            let h = ((r + 1) * ts).min(surface_height) as f32 - y;
            for c in 0..cols {
                let x = (c * ts) as f32;
                let w = ((c + 1) * ts).min(surface_width) as f32 - x;
                tiles.push(Tile::new(c, r, Rect::new(x, y, w, h)));
            }
        }

        Self {
            tile_size: ts,
            cols,
            rows,
            surface_width,
            surface_height,
            tiles,
        }
    }

    /// Default 256x256 grid constructor.
    pub fn with_default_size(surface_width: u32, surface_height: u32) -> Self {
        Self::new(surface_width, surface_height, DEFAULT_TILE_SIZE)
    }

    #[inline]
    pub fn total_tiles(&self) -> usize {
        self.tiles.len()
    }

    #[inline]
    pub fn tile_index(&self, col: u32, row: u32) -> Option<usize> {
        if col < self.cols && row < self.rows {
            Some((row * self.cols + col) as usize)
        } else {
            None
        }
    }

    #[inline]
    pub fn tile_at(&self, col: u32, row: u32) -> Option<&Tile> {
        self.tile_index(col, row).map(|idx| &self.tiles[idx])
    }

    #[inline]
    pub fn tile_at_mut(&mut self, col: u32, row: u32) -> Option<&mut Tile> {
        self.tile_index(col, row).map(|idx| &mut self.tiles[idx])
    }

    /// Marks all tiles intersecting the damage rectangle as dirty.
    pub fn mark_damage(&mut self, damage: Rect) {
        if damage.width <= 0.0 || damage.height <= 0.0 || self.cols == 0 || self.rows == 0 {
            return;
        }

        let ts = self.tile_size as f32;
        let min_col = (damage.x.max(0.0) / ts).floor() as u32;
        let max_col = ((damage.max_x() / ts).ceil() as u32).min(self.cols);
        let min_row = (damage.y.max(0.0) / ts).floor() as u32;
        let max_row = ((damage.max_y() / ts).ceil() as u32).min(self.rows);

        for r in min_row..max_row {
            for c in min_col..max_col {
                if let Some(idx) = self.tile_index(c, r) {
                    if self.tiles[idx].bounds.intersect(damage).is_some() {
                        self.tiles[idx].is_dirty = true;
                    }
                }
            }
        }
    }

    /// Returns an iterator over all dirty tiles.
    pub fn dirty_tiles(&self) -> impl Iterator<Item = &Tile> {
        self.tiles.iter().filter(|t| t.is_dirty)
    }

    /// Returns an iterator over dirty tiles that are not occluded.
    pub fn active_render_tiles(&self) -> impl Iterator<Item = &Tile> {
        self.tiles.iter().filter(|t| t.is_dirty && !t.is_occluded)
    }

    pub fn dirty_count(&self) -> usize {
        self.tiles.iter().filter(|t| t.is_dirty).count()
    }

    pub fn occluded_count(&self) -> usize {
        self.tiles.iter().filter(|t| t.is_occluded).count()
    }

    pub fn reset_dirty(&mut self) {
        for t in &mut self.tiles {
            t.is_dirty = false;
        }
    }

    pub fn reset_occlusion(&mut self) {
        for t in &mut self.tiles {
            t.is_occluded = false;
        }
    }

    pub fn reset_all(&mut self) {
        for t in &mut self.tiles {
            t.reset();
        }
    }
}

/// Tracks fully opaque rectangular occluders to eliminate redundant rasterization of underlying elements.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct OcclusionTracker {
    pub opaque_occluders: Vec<Rect>,
}

impl OcclusionTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a fully opaque axis-aligned rectangle to the occlusion database.
    pub fn add_occluder(&mut self, rect: Rect) {
        if rect.width > 0.0 && rect.height > 0.0 {
            // Check if already subsumed by an existing occluder
            for o in &self.opaque_occluders {
                if o.contains_rect(rect) {
                    return;
                }
            }

            // Remove any existing occluders fully subsumed by the new rect
            self.opaque_occluders.retain(|o| !rect.contains_rect(*o));
            self.opaque_occluders.push(rect);
        }
    }

    /// Returns true if the target rectangle is completely covered by an opaque occluder.
    pub fn is_occluded(&self, rect: Rect) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return true;
        }

        // Direct single-occluder containment check (covers 95%+ of UI cases)
        for o in &self.opaque_occluders {
            if o.contains_rect(rect) {
                return true;
            }
        }

        false
    }

    /// Evaluates occlusion against a tiling set, marking fully covered tiles as occluded.
    pub fn evaluate_tiling_set(&self, tiling_set: &mut TilingSet) {
        for tile in &mut tiling_set.tiles {
            if self.is_occluded(tile.bounds) {
                tile.is_occluded = true;
            }
        }
    }

    /// Analyzes a slice of DisplayList commands in reverse (front-to-back),
    /// identifying opaque occluders and marking occluded background commands.
    ///
    /// Returns the accumulated OcclusionTracker and a boolean mask indicating whether
    /// each command is occluded (true = occluded, skip rendering).
    pub fn analyze_front_to_back(commands: &[DrawCommand]) -> (Self, Vec<bool>) {
        let mut tracker = Self::new();
        let mut occluded_mask = vec![false; commands.len()];

        // Scan in reverse order: frontmost commands first
        for (i, cmd) in commands.iter().enumerate().rev() {
            if cmd.is_state_command() {
                continue;
            }

            if let Some(bounds) = cmd.bounds() {
                if tracker.is_occluded(bounds) {
                    occluded_mask[i] = true;
                    continue;
                }

                // Check if this command can act as an opaque occluder
                match cmd {
                    DrawCommand::DrawRect { bounds, color } => {
                        if color.a() >= 0.999 {
                            tracker.add_occluder(*bounds);
                        }
                    }
                    DrawCommand::DrawRoundedRect {
                        bounds,
                        color,
                        border_radius,
                        ..
                    } => {
                        // A rounded rect is opaque in its inner core box
                        if color.a() >= 0.999
                            && border_radius.top_left == 0.0
                            && border_radius.top_right == 0.0
                            && border_radius.bottom_right == 0.0
                            && border_radius.bottom_left == 0.0
                        {
                            tracker.add_occluder(*bounds);
                        }
                    }
                    _ => {}
                }
            }
        }

        (tracker, occluded_mask)
    }

    /// Filters a display list, dropping non-clipping commands that are completely occluded.
    pub fn cull_display_list(list: &DisplayList) -> DisplayList {
        let (_, mask) = Self::analyze_front_to_back(list.commands());
        let mut culled = DisplayList::new();

        for (cmd, is_occluded) in list.commands().iter().zip(mask.iter()) {
            if !*is_occluded || cmd.is_state_command() {
                culled.push(cmd.clone());
            }
        }

        culled
    }

    pub fn clear(&mut self) {
        self.opaque_occluders.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_style::types::Color;

    #[test]
    fn test_tiling_set_dimensions() {
        let set = TilingSet::new(1920, 1080, 256);
        assert_eq!(set.cols, 8); // 1920 / 256 = 7.5 -> 8
        assert_eq!(set.rows, 5); // 1080 / 256 = 4.21875 -> 5
        assert_eq!(set.total_tiles(), 40);

        let t0 = set.tile_at(0, 0).unwrap();
        assert_eq!(t0.bounds, Rect::new(0.0, 0.0, 256.0, 256.0));

        let t_last = set.tile_at(7, 4).unwrap();
        assert_eq!(t_last.bounds.x, 7.0 * 256.0);
        assert_eq!(t_last.bounds.y, 4.0 * 256.0);
        assert_eq!(t_last.bounds.width, 1920.0 - 7.0 * 256.0); // 128.0
        assert_eq!(t_last.bounds.height, 1080.0 - 4.0 * 256.0); // 56.0
    }

    #[test]
    fn test_tiling_mark_damage() {
        let mut set = TilingSet::new(1920, 1080, 256);
        assert_eq!(set.dirty_count(), 0);

        // Damage badge in top-left (10, 10, 120, 36)
        set.mark_damage(Rect::new(10.0, 10.0, 120.0, 36.0));
        assert_eq!(set.dirty_count(), 1);
        assert!(set.tile_at(0, 0).unwrap().is_dirty);

        // Reset
        set.reset_dirty();
        assert_eq!(set.dirty_count(), 0);

        // Damage overlapping column border: (250, 100, 20, 50) -> spans col 0 and col 1
        set.mark_damage(Rect::new(250.0, 100.0, 20.0, 50.0));
        assert_eq!(set.dirty_count(), 2);
        assert!(set.tile_at(0, 0).unwrap().is_dirty);
        assert!(set.tile_at(1, 0).unwrap().is_dirty);
    }

    #[test]
    fn test_occlusion_tracker_culling() {
        let mut tracker = OcclusionTracker::new();
        let background = Rect::new(0.0, 0.0, 500.0, 500.0);
        let modal = Rect::new(50.0, 50.0, 400.0, 400.0);
        let button = Rect::new(100.0, 100.0, 80.0, 32.0);

        tracker.add_occluder(modal);

        // Button is inside modal -> occluded from behind
        assert!(tracker.is_occluded(button));

        // Background is larger than modal -> not completely occluded
        assert!(!tracker.is_occluded(background));

        // Evaluate tiles
        let mut set = TilingSet::new(500, 500, 100);
        tracker.evaluate_tiling_set(&mut set);

        // Tile at col 2, row 2 bounds is (200, 200, 100, 100), fully inside modal (50, 50, 400, 400)
        let t22 = set.tile_at(2, 2).unwrap();
        assert!(t22.is_occluded);

        // Tile at col 0, row 0 bounds is (0, 0, 100, 100), partially outside modal -> not fully occluded
        let t00 = set.tile_at(0, 0).unwrap();
        assert!(!t00.is_occluded);
    }

    #[test]
    fn test_cull_display_list_front_to_back() {
        let mut list = DisplayList::new();

        // 1. Bottom wallpaper (1920x1080)
        list.push(DrawCommand::DrawRect {
            bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
            color: Color::rgb(15, 23, 42),
        });

        // 2. Underlying button that will be occluded
        list.push(DrawCommand::DrawRect {
            bounds: Rect::new(500.0, 300.0, 100.0, 40.0),
            color: Color::rgb(255, 0, 0),
        });

        // 3. Large opaque dialog window covering (400, 200, 800, 600)
        list.push(DrawCommand::DrawRect {
            bounds: Rect::new(400.0, 200.0, 800.0, 600.0),
            color: Color::rgb(30, 41, 59),
        });

        // 4. Dialog close icon
        list.push(DrawCommand::DrawRect {
            bounds: Rect::new(1150.0, 220.0, 24.0, 24.0),
            color: Color::rgb(255, 255, 255),
        });

        let culled = OcclusionTracker::cull_display_list(&list);

        // The underlying button (command 2) was completely occluded by the dialog (command 3),
        // so it must have been culled out!
        assert_eq!(culled.len(), 3);
        assert_eq!(culled.commands()[0], list.commands()[0]); // wallpaper
        assert_eq!(culled.commands()[1], list.commands()[2]); // dialog
        assert_eq!(culled.commands()[2], list.commands()[3]); // close icon
    }
}
