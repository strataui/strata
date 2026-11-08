//! Multi-Window Swapchain Compositor and Per-Window Damage Tracking.
//!
//! Provides simultaneous multi-window composition, isolated per-surface swapchains,
//! dirty damage rect accumulation, and presentation synchronization across multiple OS windows.

use crate::compositor::{DamageTracker, DisplayList};
use crate::rasterizer::FrameBuffer;
use crate::swapchain::{PresentError, SoftwareSurfacePresenter, SurfacePresenter, SwapchainFormat};
use strata_layout::Rect;
use std::collections::HashMap;

/// State tracked per active window swapchain.
pub struct WindowSwapchainState {
    pub window_id: u32,
    pub width: u32,
    pub height: u32,
    pub dpr: f32,
    pub frame_buffer: FrameBuffer,
    pub presenter: SoftwareSurfacePresenter,
    pub damage_tracker: DamageTracker,
    pub total_presented_frames: u64,
}

impl WindowSwapchainState {
    pub fn new(window_id: u32, width: u32, height: u32, dpr: f32) -> Self {
        let frame_buffer = FrameBuffer::new(width as usize, height as usize);
        let presenter = SoftwareSurfacePresenter::new(width, height, SwapchainFormat::Bgra8Unorm);
        let mut damage_tracker = DamageTracker::new();
        damage_tracker.add_damage(Rect::new(0.0, 0.0, width as f32, height as f32));

        Self {
            window_id,
            width,
            height,
            dpr,
            frame_buffer,
            presenter,
            damage_tracker,
            total_presented_frames: 0,
        }
    }

    /// Resizes this window swapchain.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        self.width = width;
        self.height = height;
        self.frame_buffer = FrameBuffer::new(width as usize, height as usize);
        self.presenter.resize(width, height)?;
        self.damage_tracker.reset();
        self.damage_tracker.add_damage(Rect::new(0.0, 0.0, width as f32, height as f32));
        Ok(())
    }
}

/// Multi-window compositor managing swapchain state and damage scissoring across multiple surfaces.
#[derive(Default)]
pub struct MultiWindowCompositor {
    windows: HashMap<u32, WindowSwapchainState>,
}

impl MultiWindowCompositor {
    /// Creates a new empty multi-window compositor.
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
        }
    }

    /// Registers a new active window surface.
    pub fn register_window(&mut self, window_id: u32, width: u32, height: u32, dpr: f32) {
        let state = WindowSwapchainState::new(window_id, width, height, dpr);
        self.windows.insert(window_id, state);
    }

    /// Removes an unmapped or closed window.
    pub fn unregister_window(&mut self, window_id: u32) {
        self.windows.remove(&window_id);
    }

    /// Checks if a window is actively managed.
    pub fn contains_window(&self, window_id: u32) -> bool {
        self.windows.contains_key(&window_id)
    }

    /// Marks a damage rect for a specific window.
    pub fn mark_damage(&mut self, window_id: u32, damage: Rect) {
        if let Some(win) = self.windows.get_mut(&window_id) {
            win.damage_tracker.add_damage(damage);
        }
    }

    /// Resizes a specific window's swapchain and damage tracking bounds.
    pub fn resize_window(&mut self, window_id: u32, width: u32, height: u32) -> Result<(), PresentError> {
        if let Some(win) = self.windows.get_mut(&window_id) {
            win.resize(width, height)?;
        }
        Ok(())
    }

    /// Renders and presents a frame for the specified window, returning a reference to the presented BGRA buffer.
    pub fn render_and_present(
        &mut self,
        window_id: u32,
        display_list: &DisplayList,
        force_full_redraw: bool,
    ) -> Result<&[u8], PresentError> {
        let win = self
            .windows
            .get_mut(&window_id)
            .ok_or(PresentError::SurfaceLost)?;

        let damage = if force_full_redraw {
            win.damage_tracker.reset();
            Some(Rect {
                x: 0.0,
                y: 0.0,
                width: win.width as f32,
                height: win.height as f32,
            })
        } else {
            win.damage_tracker.bounding_damage()
        };

        // Render display list commands into the window's framebuffer
        win.frame_buffer.execute_display_list(display_list);

        // Present to software surface presenter with damage scissoring
        win.presenter.present(&win.frame_buffer, damage)?;
        win.damage_tracker.reset();
        win.total_presented_frames += 1;

        Ok(&win.presenter.backing_buffer)
    }

    /// Returns the number of currently active windows.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }
}
