//! Hardware `wgpu` swapchain surface presenter and offscreen GPU bridge for `strata_platform`.
//!
//! Supports uncapped presentation (`PresentMode::Immediate`, `AutoNoVsync`, `Mailbox`),
//! persistent cached 2D HUD overlay compositing (`ui_overlay_texture`), and graceful fallback
//! when running headless without a windowing GPU surface.

use crate::surface::PresentMode;
use std::sync::Arc;
use strata_render::gpu_wgpu::Wgpu3DRenderer;
use strata_render::scene3d::Scene3D;

#[inline]
fn surface_perf_logging_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("STRATA_PERF_LOG").is_some())
}

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
        if surface_perf_logging_enabled() {
            let info = adapter.get_info();
            eprintln!(
                "[perf] wgpu adapter: {} ({:?}/{:?}), caps present_modes={:?}",
                info.name, info.backend, info.device_type, caps.present_modes
            );
        }
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
            desired_maximum_frame_latency: if selected_mode == wgpu::PresentMode::Mailbox {
                3
            } else {
                2
            },
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

    /// Uploads only damaged sub-rectangles of the 2D UI HUD overlay texture, avoiding full-frame
    /// 8 MB CPU-to-GPU transfers when only small labels or HUD counters changed.
    #[allow(clippy::too_many_arguments)]
    pub fn upload_ui_overlay_bgra_rects(
        &mut self,
        width: u32,
        height: u32,
        stride: usize,
        bgra: &[u8],
        rects: &[(u32, u32, u32, u32)],
        scratch: &mut Vec<u8>,
    ) {
        let w = width.max(1);
        let h = height.max(1);
        if stride < (w as usize) * 4 || bgra.len() < stride * (h as usize) {
            return;
        }
        let recreate = self
            .ui_overlay
            .as_ref()
            .map(|o| o.width != w || o.height != h)
            .unwrap_or(true);
        if recreate {
            if stride == (w as usize) * 4 {
                self.upload_ui_overlay_bgra(w, h, bgra);
            } else {
                scratch.resize((w * h * 4) as usize, 0);
                let row_bytes = (w as usize) * 4;
                for row in 0..(h as usize) {
                    scratch[row * row_bytes..(row + 1) * row_bytes]
                        .copy_from_slice(&bgra[row * stride..row * stride + row_bytes]);
                }
                self.upload_ui_overlay_bgra(w, h, scratch);
            }
            return;
        }
        let Some(ref ov) = self.ui_overlay else {
            return;
        };
        let queue = self.renderer.queue();
        for &(rx, ry, rw, rh) in rects {
            if rx >= w || ry >= h || rw == 0 || rh == 0 {
                continue;
            }
            let cw = rw.min(w - rx);
            let ch = rh.min(h - ry);
            let row_bytes = (cw as usize) * 4;
            let total_bytes = row_bytes * (ch as usize);
            scratch.resize(total_bytes, 0);
            for r in 0..(ch as usize) {
                let src_off = ((ry as usize) + r) * stride + (rx as usize) * 4;
                let dst_off = r * row_bytes;
                scratch[dst_off..dst_off + row_bytes]
                    .copy_from_slice(&bgra[src_off..src_off + row_bytes]);
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &ov.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: rx, y: ry, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &scratch[..total_bytes],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(cw * 4),
                    rows_per_image: Some(ch),
                },
                wgpu::Extent3d {
                    width: cw,
                    height: ch,
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
        let perf = surface_perf_logging_enabled();
        let t0 = std::time::Instant::now();
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
        let t_acq = std::time::Instant::now();
        let view = output.texture.create_view(&Default::default());
        let ov_bg = if draw_overlay {
            self.ui_overlay.as_ref().map(|ov| &ov.bind_group)
        } else {
            None
        };
        self.renderer.render_scene_and_overlay_to_view(
            scene,
            &view,
            cfg.width,
            cfg.height,
            viewport,
            false,
            ov_bg,
        );
        let t_3d = std::time::Instant::now();
        let t_ov = t_3d;
        output.present();
        let t_end = std::time::Instant::now();
        if perf {
            let acq_ms = t_acq.duration_since(t0).as_secs_f64() * 1000.0;
            let r3d_ms = t_3d.duration_since(t_acq).as_secs_f64() * 1000.0;
            let pres_ms = t_end.duration_since(t_ov).as_secs_f64() * 1000.0;
            let total_ms = t_end.duration_since(t0).as_secs_f64() * 1000.0;
            thread_local! {
                static WIN_STATS: std::cell::Cell<(u32, f64, f64, f64, f64, f64, f64)> =
                    const { std::cell::Cell::new((0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)) };
            }
            WIN_STATS.with(|c| {
                let (n, sum_a, max_a, sum_r, max_r, sum_p, max_p) = c.get();
                let next = (
                    n + 1,
                    sum_a + acq_ms,
                    max_a.max(acq_ms),
                    sum_r + r3d_ms,
                    max_r.max(r3d_ms),
                    sum_p + pres_ms,
                    max_p.max(pres_ms),
                );
                if next.0 >= 65 {
                    let nf = next.0 as f64;
                    eprintln!(
                        "[perf] present65 acq_avg={:.2} max={:.2} | r3d_avg={:.2} max={:.2} | pres_avg={:.2} max={:.2}",
                        next.1 / nf, next.2, next.3 / nf, next.4, next.5 / nf, next.6
                    );
                    c.set((0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
                } else {
                    c.set(next);
                }
            });
            if total_ms > 7.5 {
                eprintln!(
                    "[perf] present_to_surface SPIKE total={:.2}ms (acq={:.2} render3d={:.2} present={:.2})",
                    total_ms, acq_ms, r3d_ms, pres_ms
                );
            }
        }
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
