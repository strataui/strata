//! Hardware `wgpu` swapchain surface presenter and offscreen GPU bridge for `strata_platform`.
//!
//! Supports uncapped presentation (`PresentMode::Immediate`, `AutoNoVsync`, `Mailbox`),
//! persistent cached 2D HUD overlay compositing (`ui_overlay_texture`), and graceful fallback
//! when running headless without a windowing GPU surface.

use crate::surface::PresentMode;
use std::sync::Arc;
use strata_render::gpu_wgpu::Wgpu3DRenderer;
use strata_render::scene3d::Scene3D;

/// Maps platform [`PresentMode`] to the best supported [`wgpu::PresentMode`].
pub fn select_wgpu_present_mode(
    desired: PresentMode,
    caps: &wgpu::SurfaceCapabilities,
) -> wgpu::PresentMode {
    let has = |m: wgpu::PresentMode| caps.present_modes.contains(&m);
    match desired {
        PresentMode::Immediate => {
            if has(wgpu::PresentMode::Immediate) {
                wgpu::PresentMode::Immediate
            } else if has(wgpu::PresentMode::AutoNoVsync) {
                wgpu::PresentMode::AutoNoVsync
            } else if has(wgpu::PresentMode::Mailbox) {
                wgpu::PresentMode::Mailbox
            } else if has(wgpu::PresentMode::FifoRelaxed) {
                wgpu::PresentMode::FifoRelaxed
            } else {
                wgpu::PresentMode::Fifo
            }
        }
        PresentMode::Mailbox => {
            if has(wgpu::PresentMode::Mailbox) {
                wgpu::PresentMode::Mailbox
            } else if has(wgpu::PresentMode::Immediate) {
                wgpu::PresentMode::Immediate
            } else if has(wgpu::PresentMode::AutoNoVsync) {
                wgpu::PresentMode::AutoNoVsync
            } else {
                wgpu::PresentMode::Fifo
            }
        }
        PresentMode::FifoRelaxed => {
            if has(wgpu::PresentMode::FifoRelaxed) {
                wgpu::PresentMode::FifoRelaxed
            } else {
                wgpu::PresentMode::Fifo
            }
        }
        PresentMode::Fifo => wgpu::PresentMode::Fifo,
    }
}

struct CachedOverlayTexture {
    width: u32,
    height: u32,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

/// Hardware `wgpu` window surface presenter managing 3D rendering + cached 2D HUD compositing.
pub struct WgpuWindowPresenter {
    _instance: wgpu::Instance,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    renderer: Wgpu3DRenderer,
    present_mode: PresentMode,
    ui_overlay: Option<CachedOverlayTexture>,
}

impl WgpuWindowPresenter {
    /// Creates an offscreen-capable `WgpuWindowPresenter` with BGRA8 pixel format matching Cairo.
    pub fn new_offscreen(present_mode: PresentMode) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let (_adapter, device, queue) = strata_render::gpu_wgpu::request_working_adapter_and_device(
            &instance,
            None,
            "strata_platform_wgpu_device",
        )?;

        let renderer = Wgpu3DRenderer::from_device(
            Arc::new(device),
            Arc::new(queue),
            wgpu::TextureFormat::Bgra8Unorm,
        );
        Ok(Self {
            _instance: instance,
            surface: None,
            surface_config: None,
            renderer,
            present_mode,
            ui_overlay: None,
        })
    }

    /// Creates a `WgpuWindowPresenter` bound directly to a native window/display handle pair.
    ///
    /// # Safety
    /// `window_handle` and `display_handle` must remain valid for the lifetime of this presenter.
    pub unsafe fn from_raw_handles(
        window_handle: raw_window_handle::RawWindowHandle,
        display_handle: raw_window_handle::RawDisplayHandle,
        width: u32,
        height: u32,
        present_mode: PresentMode,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: display_handle,
            raw_window_handle: window_handle,
        };
        let surface = unsafe { instance.create_surface_unsafe(target) }
            .map_err(|e| format!("create_surface_unsafe failed: {e}"))?;

        let (adapter, device, queue) = strata_render::gpu_wgpu::request_working_adapter_and_device(
            &instance,
            Some(&surface),
            "strata_platform_surface_device",
        )?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .unwrap_or_else(|| caps.formats[0]);
        let selected_mode = select_wgpu_present_mode(present_mode, &caps);
        let alpha_mode = caps.alpha_modes[0];

        let cfg = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: selected_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &cfg);

        let renderer = Wgpu3DRenderer::from_device(Arc::new(device), Arc::new(queue), format);
        Ok(Self {
            _instance: instance,
            surface: Some(surface),
            surface_config: Some(cfg),
            renderer,
            present_mode,
            ui_overlay: None,
        })
    }

    pub fn renderer_mut(&mut self) -> &mut Wgpu3DRenderer {
        &mut self.renderer
    }

    pub fn active_wgpu_present_mode(&self) -> Option<wgpu::PresentMode> {
        self.surface_config.as_ref().map(|c| c.present_mode)
    }

    pub fn has_surface(&self) -> bool {
        self.surface.is_some()
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        if let (Some(surface), Some(cfg)) = (&self.surface, &mut self.surface_config) {
            if cfg.width == w && cfg.height == h {
                return;
            }
            cfg.width = w;
            cfg.height = h;
            surface.configure(self.renderer.device(), cfg);
        }
    }

    /// Uploads a new cached 2D UI HUD overlay (BGRA8 pixels matching Cairo `ARgb32`).
    ///
    /// Called only when 2D layout/DOM `paint_dirty` repaints; unchanged camera-only 3D frames
    /// reuse this GPU texture with zero CPU work.
    pub fn upload_ui_overlay_bgra(&mut self, width: u32, height: u32, bgra: &[u8]) {
        let w = width.max(1);
        let h = height.max(1);
        if bgra.len() < (w * h * 4) as usize {
            return;
        }
        let recreate = self
            .ui_overlay
            .as_ref()
            .map(|o| o.width != w || o.height != h)
            .unwrap_or(true);
        if recreate {
            let tex = self
                .renderer
                .device()
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("strata_2d_ui_overlay_bgra"),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
            let view = tex.create_view(&Default::default());
            let bind_group = self.renderer.create_overlay_bind_group(&view);
            self.ui_overlay = Some(CachedOverlayTexture {
                width: w,
                height: h,
                texture: tex,
                bind_group,
            });
        }
        if let Some(ref ov) = self.ui_overlay {
            self.renderer.queue().write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &ov.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &bgra[..(w * h * 4) as usize],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Presents `scene` directly to the swapchain surface and composites cached 2D UI overlay if enabled.
    pub fn present_to_surface(
        &mut self,
        scene: &Scene3D,
        viewport: Option<[f32; 4]>,
        draw_overlay: bool,
    ) -> Result<(), String> {
        let Some(ref surface) = self.surface else {
            return Err("no active wgpu swapchain surface".to_string());
        };
        let Some(ref cfg) = self.surface_config else {
            return Err("surface not configured".to_string());
        };
        let output = match surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                surface.configure(self.renderer.device(), cfg);
                surface
                    .get_current_texture()
                    .map_err(|e| format!("acquire surface texture failed: {e}"))?
            }
            Err(e) => return Err(format!("acquire surface texture failed: {e}")),
        };
        let view = output.texture.create_view(&Default::default());
        self.renderer
            .render_scene_to_view(scene, &view, cfg.width, cfg.height, viewport, false);
        if draw_overlay {
            if let Some(ref ov) = self.ui_overlay {
                self.renderer
                    .composite_overlay_to_view(&view, &ov.bind_group);
            }
        }
        output.present();
        Ok(())
    }

    /// Offscreen render returning BGRA8 pixels (or empty vec if `readback == false`).
    pub fn render_offscreen_bgra(
        &mut self,
        scene: &Scene3D,
        width: u32,
        height: u32,
        readback: bool,
    ) -> Result<Vec<u8>, String> {
        self.renderer
            .render_offscreen(scene, width, height, readback, true)
    }

    pub fn present_mode(&self) -> PresentMode {
        self.present_mode
    }
}

thread_local! {
    static THREAD_WGPU_RENDERER: std::cell::RefCell<Option<Result<WgpuWindowPresenter, String>>> =
        const { std::cell::RefCell::new(None) };
}

/// Renders `scene` into BGRA8 pixels on a cached thread-local `Wgpu3DRenderer`.
///
/// Returns `None` if no wgpu adapter is available in the current environment so callers can
/// transparently fall back to the software `rasterizer3d` path.
pub fn render_scene_3d_wgpu_bgra(scene: &Scene3D, width: u32, height: u32) -> Option<Vec<u8>> {
    THREAD_WGPU_RENDERER.with(|slot| {
        let mut cell = slot.borrow_mut();
        if cell.is_none() {
            *cell = Some(WgpuWindowPresenter::new_offscreen(PresentMode::Fifo));
        }
        let Ok(ref mut presenter) = cell.as_mut().unwrap() else {
            return None;
        };
        presenter
            .render_offscreen_bgra(scene, width.max(1), height.max(1), true)
            .ok()
    })
}
