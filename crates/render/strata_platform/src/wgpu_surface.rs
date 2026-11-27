//! Hardware `wgpu` swapchain surface presenter and offscreen GPU bridge for `strata_platform`.
//!
//! Supports uncapped presentation (`PresentMode::Immediate`, `AutoNoVsync`, `Mailbox`),
//! persistent cached 2D HUD overlay compositing (`ui_overlay_texture`), and graceful fallback
//! when running headless without a windowing GPU surface.

use crate::surface::{PresentError, PresentMode, SurfaceConfig, SurfacePresenter};
use std::sync::Arc;
use strata_layout::Rect;
use strata_render::compositor::DisplayList;
use strata_render::gpu_wgpu::Wgpu3DRenderer;
use strata_render::scene3d::Scene3D;
use strata_render::{
    GpuBatchSummary, GpuStencilClipPipeline, Hardware2DFrame, HardwareRenderItem,
    UnifiedGpuCollector, Wgpu2DRenderer,
};

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

struct CachedScene3DTarget {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    cache_key: u64,
}

fn compute_viewports_3d_cache_key(
    viewports: &[([f32; 4], &Scene3D)],
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mix = |acc: &mut u64, v: u64| {
        *acc ^= v;
        *acc = acc.wrapping_mul(0x100_0000_01b3);
    };
    mix(&mut h, width as u64);
    mix(&mut h, height as u64);
    {
        use std::hash::{Hash, Hasher};
        let mut fmt_hasher = std::collections::hash_map::DefaultHasher::new();
        format.hash(&mut fmt_hasher);
        mix(&mut h, fmt_hasher.finish());
    }
    mix(&mut h, viewports.len() as u64);
    for (vp, scene) in viewports {
        for c in vp {
            mix(&mut h, c.to_bits() as u64);
        }
        mix(&mut h, scene.cache_fingerprint());
    }
    h
}

struct DisplayOffscreenTarget {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    color: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

/// Hardware `wgpu` window surface presenter managing 3D rendering, hardware 2D/stencil-clip
/// [`Wgpu2DRenderer`] batches, offscreen 3D scene caching, and cached 2D HUD compositing.
pub struct WgpuWindowPresenter {
    // Drop order matters for Vulkan/wgpu teardown: cached textures and device/queue renderers
    // must drop before `surface`, and `_instance` must drop last.
    ui_overlay: Option<CachedOverlayTexture>,
    scene3d_cache: Option<CachedScene3DTarget>,
    display_offscreen: Option<DisplayOffscreenTarget>,
    renderer_2d: Wgpu2DRenderer,
    renderer: Wgpu3DRenderer,
    collector: UnifiedGpuCollector,
    last_batch_summary: GpuBatchSummary,
    present_mode: PresentMode,
    scene3d_cache_hits: u64,
    scene3d_cache_misses: u64,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    surface: Option<wgpu::Surface<'static>>,
    _instance: wgpu::Instance,
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

        let dev_arc = Arc::new(device);
        let queue_arc = Arc::new(queue);
        let format = wgpu::TextureFormat::Bgra8Unorm;
        let renderer = Wgpu3DRenderer::from_device(Arc::clone(&dev_arc), Arc::clone(&queue_arc), format);
        let renderer_2d = Wgpu2DRenderer::from_device(dev_arc, queue_arc, format);
        Ok(Self {
            _instance: instance,
            surface: None,
            surface_config: None,
            renderer,
            renderer_2d,
            collector: UnifiedGpuCollector::new(1280.0, 800.0),
            last_batch_summary: GpuBatchSummary::default(),
            present_mode,
            ui_overlay: None,
            scene3d_cache: None,
            scene3d_cache_hits: 0,
            scene3d_cache_misses: 0,
            display_offscreen: None,
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

        let dev_arc = Arc::new(device);
        let queue_arc = Arc::new(queue);
        let renderer = Wgpu3DRenderer::from_device(Arc::clone(&dev_arc), Arc::clone(&queue_arc), format);
        let renderer_2d = Wgpu2DRenderer::from_device(dev_arc, queue_arc, format);
        Ok(Self {
            _instance: instance,
            surface: Some(surface),
            surface_config: Some(cfg),
            renderer,
            renderer_2d,
            collector: UnifiedGpuCollector::new(width.max(1) as f32, height.max(1) as f32),
            last_batch_summary: GpuBatchSummary::default(),
            present_mode,
            ui_overlay: None,
            scene3d_cache: None,
            scene3d_cache_hits: 0,
            scene3d_cache_misses: 0,
            display_offscreen: None,
        })
    }

    pub fn renderer_mut(&mut self) -> &mut Wgpu3DRenderer {
        &mut self.renderer
    }

    pub fn renderer_2d(&self) -> &Wgpu2DRenderer {
        &self.renderer_2d
    }

    pub fn renderer_2d_mut(&mut self) -> &mut Wgpu2DRenderer {
        &mut self.renderer_2d
    }

    pub fn batch_collector(&self) -> &UnifiedGpuCollector {
        &self.collector
    }

    pub fn last_batch_summary(&self) -> GpuBatchSummary {
        self.last_batch_summary
    }

    pub fn stencil_clip_pipeline(&self) -> &GpuStencilClipPipeline {
        self.renderer_2d.stencil_clip_pipeline()
    }

    pub fn active_wgpu_present_mode(&self) -> Option<wgpu::PresentMode> {
        self.surface_config.as_ref().map(|c| c.present_mode)
    }

    /// Returns `(hits, misses)` for the offscreen `Scene3D` color attachment cache.
    pub fn scene3d_cache_stats(&self) -> (u64, u64) {
        (self.scene3d_cache_hits, self.scene3d_cache_misses)
    }

    /// Total allocated GPU VRAM bytes across 3D targets, tile atlases, 2D buffers, and cached overlays.
    pub fn gpu_memory_bytes(&self) -> u64 {
        let mut bytes = self.renderer.gpu_memory_bytes() + self.renderer_2d.gpu_memory_bytes();
        if let Some(ref ov) = self.ui_overlay {
            bytes += (ov.width as u64) * (ov.height as u64) * 4;
        }
        if let Some(ref s3d) = self.scene3d_cache {
            bytes += (s3d.width as u64) * (s3d.height as u64) * 4;
        }
        if let Some(ref off) = self.display_offscreen {
            bytes += (off.width as u64) * (off.height as u64) * 4
                + (off.padded_bytes_per_row as u64) * (off.height as u64);
        }
        bytes
    }

    /// Snapshots hardware GPU pass timings, `Scene3D` cache hit/miss counts, and VRAM bytes.
    pub fn gpu_telemetry(&mut self) -> crate::PlatformGpuTelemetry {
        self.renderer.poll_gpu_timestamps();
        let ts = self.renderer.last_gpu_pass_timings();
        crate::PlatformGpuTelemetry {
            gpu_timestamps_supported: ts.supported,
            gpu_shadow_pass_us: ts.shadow_pass_us,
            gpu_3d_pass_us: ts.total_3d_gpu_us,
            gpu_2d_overlay_pass_us: ts.overlay_pass_us,
            scene3d_cache_hits: self.scene3d_cache_hits,
            scene3d_cache_misses: self.scene3d_cache_misses,
            gpu_memory_bytes: self.gpu_memory_bytes(),
        }
    }

    fn ensure_scene3d_cache(&mut self, width: u32, height: u32, format: wgpu::TextureFormat) {
        let w = width.max(1);
        let h = height.max(1);
        let needs_alloc = self
            .scene3d_cache
            .as_ref()
            .map(|c| c.width != w || c.height != h || c.format != format)
            .unwrap_or(true);
        if needs_alloc {
            let texture = self.renderer.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("strata_cached_scene3d_color_tex"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind_group = self.renderer.create_overlay_bind_group(&view);
            self.scene3d_cache = Some(CachedScene3DTarget {
                width: w,
                height: h,
                format,
                _texture: texture,
                view,
                bind_group,
                cache_key: 0,
            });
        }
    }

    /// Renders `viewports` using the offscreen `Scene3D` color attachment cache into `target_view`
    /// and composites the optional 2D UI overlay on top. Returns `true` on a 3D cache hit.
    pub fn render_viewports_cached_to_view(
        &mut self,
        viewports: &[([f32; 4], &Scene3D)],
        target_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        draw_overlay: bool,
    ) -> bool {
        let w = width.max(1);
        let h = height.max(1);
        let key = compute_viewports_3d_cache_key(viewports, w, h, format);
        let cache_hit = self.scene3d_cache.as_ref().is_some_and(|c| {
            c.width == w && c.height == h && c.format == format && c.cache_key == key
        });
        if cache_hit {
            self.scene3d_cache_hits += 1;
        } else {
            self.scene3d_cache_misses += 1;
            self.ensure_scene3d_cache(w, h, format);
            if let Some(ref mut cache) = self.scene3d_cache {
                if viewports.len() == 1 {
                    let (vp, scene) = viewports[0];
                    self.renderer.render_scene_and_overlay_to_view(
                        scene,
                        &cache.view,
                        w,
                        h,
                        Some(vp),
                        false,
                        None,
                    );
                } else {
                    self.renderer.render_viewports_and_overlay_to_view(
                        viewports,
                        &cache.view,
                        w,
                        h,
                        false,
                        None,
                    );
                }
                cache.cache_key = key;
            }
        }
        let ov_bg = if draw_overlay {
            self.ui_overlay.as_ref().map(|ov| &ov.bind_group)
        } else {
            None
        };
        if let Some(ref cache) = self.scene3d_cache {
            self.renderer.composite_cached_scene_and_overlay_to_view(
                target_view,
                &cache.bind_group,
                ov_bg,
            );
        }
        cache_hit
    }

    pub fn surface_size(&self) -> (u32, u32) {
        self.surface_config
            .as_ref()
            .map(|c| (c.width, c.height))
            .unwrap_or((0, 0))
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
        let vp = viewport.unwrap_or_else(|| {
            let (w, h) = self
                .surface_config
                .as_ref()
                .map(|c| (c.width as f32, c.height as f32))
                .unwrap_or((1.0, 1.0));
            [0.0, 0.0, w, h]
        });
        self.present_viewports_to_surface(&[(vp, scene)], draw_overlay)
    }

    /// Presents one or more 3D viewports directly into the single swapchain frame and composites
    /// the cached 2D UI overlay once on top before presenting.
    pub fn present_viewports_to_surface(
        &mut self,
        viewports: &[([f32; 4], &Scene3D)],
        draw_overlay: bool,
    ) -> Result<(), String> {
        self.present_viewports_to_surface_timed(viewports, draw_overlay)
            .map(|_| ())
    }

    /// Presents one or more 3D viewports directly into the swapchain frame and returns
    /// `(wgpu_submit_duration, swapchain_present_duration)`.
    pub fn present_viewports_to_surface_timed(
        &mut self,
        viewports: &[([f32; 4], &Scene3D)],
        draw_overlay: bool,
    ) -> Result<(std::time::Duration, std::time::Duration), String> {
        if viewports.is_empty() {
            return Err("no 3D viewports to present".to_string());
        }
        let (cfg_w, cfg_h, cfg_fmt) = {
            let Some(ref cfg) = self.surface_config else {
                return Err("surface not configured".to_string());
            };
            (cfg.width, cfg.height, cfg.format)
        };
        let perf = surface_perf_logging_enabled();
        let t0 = std::time::Instant::now();
        let output = {
            let Some(ref surface) = self.surface else {
                return Err("no active wgpu swapchain surface".to_string());
            };
            let Some(ref cfg) = self.surface_config else {
                return Err("surface not configured".to_string());
            };
            match surface.get_current_texture() {
                Ok(frame) => frame,
                Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                    surface.configure(self.renderer.device(), cfg);
                    surface
                        .get_current_texture()
                        .map_err(|e| format!("acquire surface texture failed: {e}"))?
                }
                Err(e) => return Err(format!("acquire surface texture failed: {e}")),
            }
        };
        let t_acq = std::time::Instant::now();
        let view = output.texture.create_view(&Default::default());
        let _cache_hit = self.render_viewports_cached_to_view(
            viewports,
            &view,
            cfg_w,
            cfg_h,
            cfg_fmt,
            draw_overlay,
        );
        let t_3d = std::time::Instant::now();
        let t_ov = t_3d;
        output.present();
        let t_end = std::time::Instant::now();
        let wgpu_submit_dur = t_3d.duration_since(t0);
        let present_dur = t_end.duration_since(t_ov);
        if perf {
            let acq_ms = t_acq.duration_since(t0).as_secs_f64() * 1000.0;
            let r3d_ms = t_3d.duration_since(t_acq).as_secs_f64() * 1000.0;
            let pres_ms = present_dur.as_secs_f64() * 1000.0;
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
        Ok((wgpu_submit_dur, present_dur))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_hybrid_frame_to_view(
        &mut self,
        frame: &Hardware2DFrame,
        target_view: &wgpu::TextureView,
        target_w: u32,
        target_h: u32,
        logical_w: f32,
        logical_h: f32,
        dpr: f32,
        clear_color: [f32; 4],
        damage: Option<Rect>,
    ) {
        self.renderer_2d
            .prepare_frame(frame, logical_w, logical_h, dpr);

        let sx = target_w as f32 / logical_w.max(1.0);
        let sy = target_h as f32 / logical_h.max(1.0);
        let mut has_drawn = false;
        let mut idx = 0usize;
        let n = frame.items.len();

        while idx < n {
            match &frame.items[idx] {
                HardwareRenderItem::Batch(_) => {
                    let start = idx;
                    while idx < n && matches!(frame.items[idx], HardwareRenderItem::Batch(_)) {
                        idx += 1;
                    }
                    let clear = if !has_drawn { Some(clear_color) } else { None };
                    self.renderer_2d.render_batch_slice_to_view(
                        frame,
                        start..idx,
                        target_view,
                        target_w,
                        target_h,
                        logical_w,
                        logical_h,
                        clear,
                        damage,
                    );
                    has_drawn = true;
                }
                HardwareRenderItem::Viewport3D { bounds, scene } => {
                    let vp = [
                        bounds.x * sx,
                        bounds.y * sy,
                        bounds.width * sx,
                        bounds.height * sy,
                    ];
                    if !has_drawn {
                        self.renderer.render_scene_and_overlay_to_view(
                            scene,
                            target_view,
                            target_w,
                            target_h,
                            Some(vp),
                            false,
                            None,
                        );
                    } else {
                        self.renderer.render_scene_preserving_color_to_view(
                            scene,
                            target_view,
                            target_w,
                            target_h,
                            Some(vp),
                            false,
                        );
                    }
                    has_drawn = true;
                    idx += 1;
                }
            }
        }

        if !has_drawn {
            self.renderer_2d.render_batch_slice_to_view(
                frame,
                0..0,
                target_view,
                target_w,
                target_h,
                logical_w,
                logical_h,
                Some(clear_color),
                damage,
            );
        }
    }

    /// Presents an entire `DisplayList` (2D quads, vector paths with caps/joins, dashed strokes,
    /// Slug text, polygonal [`GpuStencilClipPipeline`] masks, and any interleaved `DrawViewport3D`)
    /// directly onto the hardware `wgpu::Surface` swapchain via [`UnifiedGpuCollector`] + [`Wgpu2DRenderer`].
    pub fn present_display_list_to_surface(
        &mut self,
        list: &DisplayList,
        logical_width: f32,
        logical_height: f32,
        dpr: f32,
        damage: Option<Rect>,
        clear_color: [f32; 4],
    ) -> Result<GpuBatchSummary, String> {
        self.present_display_list_to_surface_timed(
            list,
            logical_width,
            logical_height,
            dpr,
            damage,
            clear_color,
        )
        .map(|(summary, _, _, _)| summary)
    }

    /// Presents an entire `DisplayList` onto the swapchain and returns
    /// `(summary, raster_2d_collect_dur, wgpu_submit_dur, swapchain_present_dur)`.
    pub fn present_display_list_to_surface_timed(
        &mut self,
        list: &DisplayList,
        logical_width: f32,
        logical_height: f32,
        dpr: f32,
        damage: Option<Rect>,
        clear_color: [f32; 4],
    ) -> Result<
        (
            GpuBatchSummary,
            std::time::Duration,
            std::time::Duration,
            std::time::Duration,
        ),
        String,
    > {
        let Some(ref surface) = self.surface else {
            return Err("no active wgpu swapchain surface".to_string());
        };
        let Some(ref cfg) = self.surface_config else {
            return Err("surface not configured".to_string());
        };
        let target_w = cfg.width;
        let target_h = cfg.height;

        let t_collect0 = std::time::Instant::now();
        let lw = logical_width.max(1.0);
        let lh = logical_height.max(1.0);
        self.collector.resize(lw, lh);
        let frame = self.collector.collect_hardware_frame(list, damage);
        self.last_batch_summary = frame.summary;
        let raster_2d_dur = t_collect0.elapsed();

        let t_sub0 = std::time::Instant::now();
        let output = match surface.get_current_texture() {
            Ok(frame_tex) => frame_tex,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                surface.configure(self.renderer.device(), cfg);
                surface
                    .get_current_texture()
                    .map_err(|e| format!("acquire surface texture failed: {e}"))?
            }
            Err(e) => return Err(format!("acquire surface texture failed: {e}")),
        };
        let view = output.texture.create_view(&Default::default());
        self.render_hybrid_frame_to_view(
            &frame,
            &view,
            target_w,
            target_h,
            lw,
            lh,
            dpr,
            clear_color,
            damage,
        );
        let wgpu_submit_dur = t_sub0.elapsed();
        let t_pres0 = std::time::Instant::now();
        output.present();
        let present_dur = t_pres0.elapsed();
        Ok((frame.summary, raster_2d_dur, wgpu_submit_dur, present_dur))
    }

    /// Presents the cached `ui_overlay` texture directly onto the swapchain surface.
    pub fn present_overlay_to_surface(&mut self, clear_color: [f32; 4]) -> Result<(), String> {
        let Some(ref surface) = self.surface else {
            return Err("no active wgpu swapchain surface".to_string());
        };
        let Some(ref cfg) = self.surface_config else {
            return Err("surface not configured".to_string());
        };
        let output = match surface.get_current_texture() {
            Ok(frame_tex) => frame_tex,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                surface.configure(self.renderer.device(), cfg);
                surface
                    .get_current_texture()
                    .map_err(|e| format!("acquire surface texture failed: {e}"))?
            }
            Err(e) => return Err(format!("acquire surface texture failed: {e}")),
        };
        let view = output.texture.create_view(&Default::default());
        let empty = Hardware2DFrame::default();
        self.renderer_2d.render_batch_slice_to_view(
            &empty,
            0..0,
            &view,
            cfg.width,
            cfg.height,
            cfg.width.max(1) as f32,
            cfg.height.max(1) as f32,
            Some(clear_color),
            None,
        );
        if let Some(ref ov) = self.ui_overlay {
            self.renderer
                .composite_overlay_to_view(&view, &ov.bind_group);
        }
        output.present();
        Ok(())
    }

    /// Renders a `DisplayList` (pure 2D or hybrid 3D+2D) via [`UnifiedGpuCollector`],
    /// [`GpuStencilClipPipeline`], and [`Wgpu2DRenderer`] into an offscreen BGRA8 target,
    /// returning raw BGRA8 pixels when `readback` is `true`.
    #[allow(clippy::too_many_arguments)]
    pub fn render_display_list_offscreen_bgra(
        &mut self,
        list: &DisplayList,
        width: u32,
        height: u32,
        dpr: f32,
        damage: Option<Rect>,
        clear_color: [f32; 4],
        readback: bool,
    ) -> Result<(GpuBatchSummary, Vec<u8>), String> {
        let w = width.max(1);
        let h = height.max(1);
        let lw = w as f32 / dpr.max(0.1);
        let lh = h as f32 / dpr.max(0.1);
        self.collector.resize(lw, lh);
        let frame = self.collector.collect_hardware_frame(list, damage);
        self.last_batch_summary = frame.summary;

        let has_3d = frame
            .items
            .iter()
            .any(|it| matches!(it, HardwareRenderItem::Viewport3D { .. }));
        if !has_3d {
            return self.renderer_2d.render_display_list_offscreen(
                &mut self.collector,
                list,
                w,
                h,
                dpr,
                clear_color,
                damage,
                readback,
            );
        }

        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let unpadded = w * 4;
        let padded = unpadded.div_ceil(align) * align;
        let format = self.renderer_2d.color_format();
        let recreate = self
            .display_offscreen
            .as_ref()
            .is_none_or(|o| o.width != w || o.height != h || o.format != format);
        if recreate {
            let dev = self.renderer_2d.device();
            let color = dev.create_texture(&wgpu::TextureDescriptor {
                label: Some("strata_hybrid_offscreen_color"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = color.create_view(&Default::default());
            let rb = dev.create_buffer(&wgpu::BufferDescriptor {
                label: Some("strata_hybrid_offscreen_readback"),
                size: (padded * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.display_offscreen = Some(DisplayOffscreenTarget {
                width: w,
                height: h,
                format,
                color,
                view,
                readback: rb,
                padded_bytes_per_row: padded,
            });
        }

        let target_view = self.display_offscreen.as_ref().unwrap().view.clone();
        self.render_hybrid_frame_to_view(
            &frame,
            &target_view,
            w,
            h,
            lw,
            lh,
            dpr,
            clear_color,
            damage,
        );

        let dev = self.renderer_2d.device();
        if !readback {
            dev.poll(wgpu::Maintain::Wait);
            return Ok((frame.summary, Vec::new()));
        }

        let off = self.display_offscreen.as_ref().unwrap();
        let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("strata_hybrid_readback_encoder"),
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
        self.renderer_2d.queue().submit(Some(enc.finish()));

        let slice = off.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        dev.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("hybrid readback recv error: {e}"))?
            .map_err(|e| format!("hybrid readback map error: {e:?}"))?;

        let data = slice.get_mapped_range();
        let mut out = vec![0u8; (w * h * 4) as usize];
        let row_len = w as usize * 4;
        for row in 0..h as usize {
            let src_start = row * off.padded_bytes_per_row as usize;
            let dst_start = row * row_len;
            out[dst_start..dst_start + row_len]
                .copy_from_slice(&data[src_start..src_start + row_len]);
        }
        drop(data);
        off.readback.unmap();
        Ok((frame.summary, out))
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

    /// Renders 3D viewports through the offscreen Scene3D cache and composites the 2D UI overlay,
    /// reading back BGRA8 pixels for headless verification and benchmarking.
    pub fn render_viewports_cached_offscreen_bgra(
        &mut self,
        viewports: &[([f32; 4], &Scene3D)],
        width: u32,
        height: u32,
        composite_overlay: bool,
    ) -> Result<Vec<u8>, String> {
        let w = width.max(1);
        let h = height.max(1);
        let fmt = self.renderer.color_format();
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = (w * 4).div_ceil(align) * align;
        let recreate = self
            .display_offscreen
            .as_ref()
            .is_none_or(|o| o.width != w || o.height != h || o.format != fmt);
        if recreate {
            let dev = self.renderer.device();
            let color = dev.create_texture(&wgpu::TextureDescriptor {
                label: Some("strata_cached_3d_offscreen_color"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: fmt,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = color.create_view(&Default::default());
            let rb = dev.create_buffer(&wgpu::BufferDescriptor {
                label: Some("strata_cached_3d_offscreen_readback"),
                size: (padded * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.display_offscreen = Some(DisplayOffscreenTarget {
                width: w,
                height: h,
                format: fmt,
                color,
                view,
                readback: rb,
                padded_bytes_per_row: padded,
            });
        }
        let target_view = self.display_offscreen.as_ref().unwrap().view.clone();
        self.render_viewports_cached_to_view(viewports, &target_view, w, h, fmt, composite_overlay);

        let dev = self.renderer.device();
        let off = self.display_offscreen.as_ref().unwrap();
        let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("strata_cached_3d_readback_encoder"),
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
        self.renderer.queue().submit(Some(enc.finish()));

        let slice = off.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        dev.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("cached 3d readback recv error: {e}"))?
            .map_err(|e| format!("cached 3d readback map error: {e:?}"))?;

        let data = slice.get_mapped_range();
        let mut out = vec![0u8; (w * h * 4) as usize];
        let row_len = w as usize * 4;
        for row in 0..h as usize {
            let src_start = row * off.padded_bytes_per_row as usize;
            let dst_start = row * row_len;
            out[dst_start..dst_start + row_len]
                .copy_from_slice(&data[src_start..src_start + row_len]);
        }
        drop(data);
        off.readback.unmap();
        Ok(out)
    }

    pub fn present_mode(&self) -> PresentMode {
        self.present_mode
    }
}

/// Hardware `wgpu` Swapchain Presenter wiring [`UnifiedGpuCollector`],
/// [`GpuStencilClipPipeline`], and [`Wgpu2DRenderer`] into `strata_platform`'s swapchain layer.
pub struct WgpuSwapchainPresenter {
    config: SurfaceConfig,
    presenter: WgpuWindowPresenter,
    pub presented_frames: u64,
    pub damage_clipped_frames: u64,
    upload_scratch: Vec<u8>,
}

impl WgpuSwapchainPresenter {
    /// Creates a headless/offscreen hardware GPU swapchain presenter.
    pub fn new_offscreen(config: SurfaceConfig) -> Result<Self, String> {
        let presenter = WgpuWindowPresenter::new_offscreen(config.present_mode)?;
        Ok(Self {
            config,
            presenter,
            presented_frames: 0,
            damage_clipped_frames: 0,
            upload_scratch: Vec::new(),
        })
    }

    /// Creates a hardware GPU swapchain presenter bound directly to a native window handle pair.
    ///
    /// # Safety
    /// `window_handle` and `display_handle` must remain valid for the lifetime of this presenter.
    pub unsafe fn from_raw_handles(
        window_handle: raw_window_handle::RawWindowHandle,
        display_handle: raw_window_handle::RawDisplayHandle,
        config: SurfaceConfig,
    ) -> Result<Self, String> {
        let (pw, ph) = config.physical_size();
        let presenter = unsafe {
            WgpuWindowPresenter::from_raw_handles(
                window_handle,
                display_handle,
                pw.max(1),
                ph.max(1),
                config.present_mode,
            )?
        };
        Ok(Self {
            config,
            presenter,
            presented_frames: 0,
            damage_clipped_frames: 0,
            upload_scratch: Vec::new(),
        })
    }

    #[inline]
    pub fn config(&self) -> &SurfaceConfig {
        &self.config
    }

    #[inline]
    pub fn presenter(&self) -> &WgpuWindowPresenter {
        &self.presenter
    }

    #[inline]
    pub fn presenter_mut(&mut self) -> &mut WgpuWindowPresenter {
        &mut self.presenter
    }

    #[inline]
    pub fn batch_collector(&self) -> &UnifiedGpuCollector {
        self.presenter.batch_collector()
    }

    #[inline]
    pub fn last_batch_summary(&self) -> GpuBatchSummary {
        self.presenter.last_batch_summary()
    }

    #[inline]
    pub fn stencil_clip_pipeline(&self) -> &GpuStencilClipPipeline {
        self.presenter.stencil_clip_pipeline()
    }

    /// Compiles and presents a `DisplayList` directly via `Wgpu2DRenderer` + `GpuStencilClipPipeline`
    /// onto the configured swapchain (or offscreen target when headless).
    pub fn present_display_list(
        &mut self,
        list: &DisplayList,
        damage: Option<Rect>,
        clear_color: [f32; 4],
    ) -> Result<GpuBatchSummary, PresentError> {
        let (pw, ph) = self.config.physical_size();
        let summary = if self.presenter.has_surface() {
            self.presenter
                .present_display_list_to_surface(
                    list,
                    self.config.width.max(1) as f32,
                    self.config.height.max(1) as f32,
                    self.config.scale_factor,
                    damage,
                    clear_color,
                )
                .map_err(PresentError::Custom)?
        } else {
            let (s, _) = self
                .presenter
                .render_display_list_offscreen_bgra(
                    list,
                    pw.max(1),
                    ph.max(1),
                    self.config.scale_factor,
                    damage,
                    clear_color,
                    false,
                )
                .map_err(PresentError::Custom)?;
            s
        };
        self.presented_frames += 1;
        if damage.is_some() {
            self.damage_clipped_frames += 1;
        }
        Ok(summary)
    }

    /// Renders a `DisplayList` into BGRA8 pixels via the GPU 2D + stencil clip + 3D pipelines.
    pub fn render_display_list_to_bgra(
        &mut self,
        list: &DisplayList,
        damage: Option<Rect>,
        clear_color: [f32; 4],
    ) -> Result<(GpuBatchSummary, Vec<u8>), PresentError> {
        let (pw, ph) = self.config.physical_size();
        let res = self
            .presenter
            .render_display_list_offscreen_bgra(
                list,
                pw.max(1),
                ph.max(1),
                self.config.scale_factor,
                damage,
                clear_color,
                true,
            )
            .map_err(PresentError::Custom)?;
        self.presented_frames += 1;
        if damage.is_some() {
            self.damage_clipped_frames += 1;
        }
        Ok(res)
    }
}

impl SurfacePresenter for WgpuSwapchainPresenter {
    fn present(
        &mut self,
        buffer: &strata_render::FrameBuffer,
        damage: Option<Rect>,
    ) -> Result<(), PresentError> {
        let w = buffer.width as u32;
        let h = buffer.height as u32;
        let expected = buffer.width * buffer.height * 4;
        if buffer.pixels.len() < expected || w == 0 || h == 0 {
            return Err(PresentError::BufferTooSmall);
        }

        // Convert RGBA8 FrameBuffer pixels into BGRA8 matching WgpuWindowPresenter overlay format.
        self.upload_scratch.resize(expected, 0);
        for (dst, src) in self
            .upload_scratch
            .chunks_exact_mut(4)
            .zip(buffer.pixels[..expected].chunks_exact(4))
        {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = src[3];
        }
        self.presenter
            .upload_ui_overlay_bgra(w, h, &self.upload_scratch);
        if self.presenter.has_surface() {
            self.presenter
                .present_overlay_to_surface([0.0, 0.0, 0.0, 1.0])
                .map_err(PresentError::Custom)?;
        }
        self.presented_frames += 1;
        if damage.is_some() {
            self.damage_clipped_frames += 1;
        }
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        let (pw, ph) = self.config.physical_size();
        self.presenter.resize(pw.max(1), ph.max(1));
        Ok(())
    }

    fn surface_size(&self) -> (u32, u32) {
        let (pw, ph) = self.config.physical_size();
        (pw.max(1), ph.max(1))
    }
}

thread_local! {
    static THREAD_WGPU_RENDERER: std::cell::RefCell<Option<Result<WgpuWindowPresenter, String>>> =
        const { std::cell::RefCell::new(None) };
}

/// Explicitly drops any cached thread-local offscreen [`WgpuWindowPresenter`] so wgpu
/// resources are released cleanly before process TLS destruction (`__call_tls_dtors`).
pub fn clear_thread_wgpu_renderer() {
    let _ = THREAD_WGPU_RENDERER.try_with(|slot| {
        if let Ok(mut cell) = slot.try_borrow_mut() {
            let _ = cell.take();
        }
    });
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

#[cfg(test)]
mod present_mode_tests {
    use super::*;

    fn caps_with_modes(modes: Vec<wgpu::PresentMode>) -> wgpu::SurfaceCapabilities {
        wgpu::SurfaceCapabilities {
            formats: vec![wgpu::TextureFormat::Bgra8Unorm],
            present_modes: modes,
            alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque],
            usages: wgpu::TextureUsages::RENDER_ATTACHMENT,
        }
    }

    #[test]
    fn test_fifo_and_mailbox_preserve_vsync_on_x11_without_mailbox() {
        // Typical X11 Vulkan driver exposing only [Fifo, Immediate] (no Mailbox).
        let caps = caps_with_modes(vec![wgpu::PresentMode::Fifo, wgpu::PresentMode::Immediate]);
        assert_eq!(
            select_wgpu_present_mode(PresentMode::Fifo, &caps),
            wgpu::PresentMode::Fifo,
            "explicit PresentMode::Fifo must never degrade to Immediate"
        );
        assert_eq!(
            select_wgpu_present_mode(PresentMode::Mailbox, &caps),
            wgpu::PresentMode::Fifo,
            "PresentMode::Mailbox must fall back to tear-free Fifo when Mailbox is unsupported"
        );
        assert_eq!(
            select_wgpu_present_mode(PresentMode::FifoRelaxed, &caps),
            wgpu::PresentMode::Fifo
        );
        assert_eq!(
            select_wgpu_present_mode(PresentMode::Immediate, &caps),
            wgpu::PresentMode::Immediate
        );
    }

    #[test]
    fn test_mailbox_honored_when_supported_and_fifo_never_upgraded() {
        let caps = caps_with_modes(vec![
            wgpu::PresentMode::Fifo,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::Immediate,
        ]);
        assert_eq!(
            select_wgpu_present_mode(PresentMode::Fifo, &caps),
            wgpu::PresentMode::Fifo
        );
        assert_eq!(
            select_wgpu_present_mode(PresentMode::Mailbox, &caps),
            wgpu::PresentMode::Mailbox
        );
    }

    #[test]
    fn test_clear_thread_wgpu_renderer_is_idempotent() {
        clear_thread_wgpu_renderer();
        clear_thread_wgpu_renderer();
    }
}
