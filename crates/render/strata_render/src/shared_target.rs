//! Shared Render Target: Hybrid 2D/3D Viewport Compositing Bridge.
//!
//! Provides offscreen 3D rendering buffers with dirty-rect caching and zero-desync
//! 2D HUD telemetry overlay compositing.

use crate::compositor::DisplayList;
use crate::rasterizer::FrameBuffer;
use crate::rasterizer3d::render_scene_3d;
use crate::scene3d::Scene3D;
use strata_layout::Rect;
use strata_style::types::Color;

/// Offscreen Shared Render Target for 3D Viewports.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedRenderTarget {
    pub bounds: Rect,
    pub width: usize,
    pub height: usize,
    pub color_buffer: FrameBuffer,
    pub depth_buffer: Vec<f32>,
    pub dirty: bool,
    pub background_color: Color,
}

impl SharedRenderTarget {
    /// Creates a new offscreen render target with specified bounds.
    pub fn new(bounds: Rect) -> Self {
        let width = bounds.width.ceil().max(1.0) as usize;
        let height = bounds.height.ceil().max(1.0) as usize;

        Self {
            bounds,
            width,
            height,
            color_buffer: FrameBuffer::new(width, height),
            depth_buffer: vec![1.0; width * height],
            dirty: true,
            background_color: Color::rgb(10, 15, 25), // Dark space / infrastructure backdrop
        }
    }

    /// Resizes the shared render target if the viewport dimensions change.
    pub fn resize(&mut self, bounds: Rect) {
        let width = bounds.width.ceil().max(1.0) as usize;
        let height = bounds.height.ceil().max(1.0) as usize;

        if width != self.width || height != self.height {
            self.bounds = bounds;
            self.width = width;
            self.height = height;
            self.color_buffer = FrameBuffer::new(width, height);
            self.depth_buffer = vec![1.0; width * height];
            self.dirty = true;
        } else {
            self.bounds = bounds;
        }
    }

    /// Marks the 3D target as dirty, forcing re-rendering on the next frame.
    #[inline]
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Clears the color buffer to background color and resets the depth buffer.
    pub fn clear(&mut self) {
        self.color_buffer.clear(self.background_color);
        self.depth_buffer.fill(1.0);
    }

    /// Renders the 3D scene into this offscreen buffer if marked dirty.
    pub fn render_if_dirty(&mut self, scene: &Scene3D) {
        if !self.dirty {
            return;
        }

        self.clear();
        let local_viewport = Rect::new(0.0, 0.0, self.width as f32, self.height as f32);
        render_scene_3d(&mut self.color_buffer, local_viewport, scene, None);
        self.dirty = false;
    }

    /// Forces rendering of the 3D scene into this offscreen buffer.
    pub fn render_scene(&mut self, scene: &Scene3D) {
        self.clear();
        let local_viewport = Rect::new(0.0, 0.0, self.width as f32, self.height as f32);
        render_scene_3d(&mut self.color_buffer, local_viewport, scene, None);
        self.dirty = false;
    }

    /// Composites (blits) the rendered 3D viewport into a target 2D FrameBuffer.
    pub fn composite_to_surface(&self, dst: &mut FrameBuffer, clip: Option<Rect>) {
        let start_x = self.bounds.x.max(0.0) as usize;
        let start_y = self.bounds.y.max(0.0) as usize;

        let end_x = ((self.bounds.x + self.width as f32).min(dst.width as f32)) as usize;
        let end_y = ((self.bounds.y + self.height as f32).min(dst.height as f32)) as usize;

        for y in start_y..end_y {
            let src_y = y - start_y;
            if src_y >= self.height {
                continue;
            }

            for x in start_x..end_x {
                let src_x = x - start_x;
                if src_x >= self.width {
                    continue;
                }

                // Check optional scissor clip
                if let Some(c) = clip {
                    let fx = x as f32;
                    let fy = y as f32;
                    if fx < c.x || fx >= c.max_x() || fy < c.y || fy >= c.max_y() {
                        continue;
                    }
                }

                let pixel = self.color_buffer.get_pixel(src_x, src_y);
                dst.set_pixel(x, y, pixel);
            }
        }
    }

    /// Safe zero-copy byte slice view of color buffer pixels.
    #[inline]
    pub fn as_raw_bytes(&self) -> &[u8] {
        self.color_buffer.as_raw_bytes()
    }
}

/// Executes a complete hybrid 2D/3D presentation frame:
/// 1. Renders 3D scene into the offscreen SharedRenderTarget.
/// 2. Blits the 3D buffer to the destination FrameBuffer at `target.bounds`.
/// 3. Executes the 2D DisplayList (telemetry badges, HUD cards, buttons) directly over the 3D surface.
pub fn render_hybrid_frame(
    target: &mut SharedRenderTarget,
    scene: &Scene3D,
    overlay_display_list: &DisplayList,
    dst_frame_buffer: &mut FrameBuffer,
    clip: Option<Rect>,
) {
    // 1. Render 3D Scene to offscreen buffer
    target.render_if_dirty(scene);

    // 2. Blit 3D Viewport to target frame buffer
    target.composite_to_surface(dst_frame_buffer, clip);

    // 3. Render 2D UI Overlay on top
    if let Some(damage) = clip {
        dst_frame_buffer.execute_display_list_damaged(overlay_display_list, damage);
    } else {
        dst_frame_buffer.execute_display_list(overlay_display_list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::DrawCommand;
    use crate::scene3d::{Camera3D, Geometry3D, Material3D, Mesh3D};

    #[test]
    fn test_shared_render_target_lifecycle() {
        let bounds = Rect::new(50.0, 50.0, 200.0, 150.0);
        let mut target = SharedRenderTarget::new(bounds);
        assert_eq!(target.width, 200);
        assert_eq!(target.height, 150);
        assert!(target.dirty);

        let mut scene = Scene3D::new();
        scene.camera = Camera3D::Perspective {
            fov: 60.0,
            aspect: 200.0 / 150.0,
            near: 0.1,
            far: 100.0,
            position: [0.0, 0.0, 5.0],
            target: [0.0, 0.0, 0.0],
        };
        scene.add_mesh(Mesh3D::new(
            Geometry3D::Box { width: 1.0, height: 1.0, depth: 1.0 },
            Material3D::Basic { color: Color::rgb(34, 197, 94) },
        ));

        target.render_if_dirty(&scene);
        assert!(!target.dirty);

        // Blit to main surface (400x300)
        let mut main_surface = FrameBuffer::new(400, 300);
        target.composite_to_surface(&mut main_surface, None);

        // Outside 3D viewport (10, 10) must be transparent
        assert_eq!(main_surface.get_pixel(10, 10), Color::TRANSPARENT);

        // Inside 3D viewport (e.g. 50+100, 50+75 = 150, 125) center should have rendered color
        let center_pixel = main_surface.get_pixel(150, 125);
        assert_ne!(center_pixel, Color::TRANSPARENT);
    }

    #[test]
    fn test_hybrid_frame_2d_overlay_over_3d() {
        let bounds = Rect::new(0.0, 0.0, 300.0, 300.0);
        let mut target = SharedRenderTarget::new(bounds);
        let scene = Scene3D::new();

        // 2D Overlay DisplayList with a solid red telemetry badge at (20, 20, 80, 40)
        let mut overlay_dl = DisplayList::new();
        overlay_dl.push(DrawCommand::DrawRect {
            bounds: Rect::new(20.0, 20.0, 80.0, 40.0),
            color: Color::rgb(239, 68, 68), // Red-500
        });

        let mut main_surface = FrameBuffer::new(300, 300);
        render_hybrid_frame(&mut target, &scene, &overlay_dl, &mut main_surface, None);

        // Pixel inside 2D badge (30, 30) MUST be exact overlay color Red-500
        let badge_pixel = main_surface.get_pixel(30, 30);
        assert_eq!(badge_pixel, Color::rgb(239, 68, 68));

        // Pixel outside badge (200, 200) should be 3D background color
        let bg_pixel = main_surface.get_pixel(200, 200);
        assert_eq!(bg_pixel, target.background_color);
    }
}
