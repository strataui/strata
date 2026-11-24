//! In-process overlay HUD and event interceptor with synthetic event isolation.

use strata_dom::NodeId;
use strata_layout::Rect;
use strata_platform::{KeyCode, PlatformEvent};
use strata_primitives::geom::{EdgeInsets, FixedRect};
use strata_primitives::AtomId;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_style::types::{BorderRadius, BorderStyle, Color, FontWeight, TextAlign};

use crate::memory::ArenaInspector;
use crate::profiler::FrameTimelineProfiler;
use crate::time_travel::TimeTravelDebugger;

/// Live runtime diagnostics snapshot rendered by DevTools HUD tabs (Reactivity, Memory, Time-Travel).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HudLiveTelemetry {
    pub reactive_active_signals: usize,
    pub reactive_active_subscribers: usize,
    pub reactive_total_reads: u64,
    pub reactive_total_writes: u64,
    pub reactive_total_flushes: u64,
    pub reactive_cycle_guard_trips: u64,
    pub last_rebuild_reason: String,
    pub dirty_signal_names: Vec<String>,
    /// Top active signals: `(name, write_count, read_count, subscriber_count)`.
    pub top_signals: Vec<(String, u64, u64, usize)>,
    pub dom_node_count: usize,
    pub layout_node_count: usize,
    pub display_cmd_count: usize,
    pub arena_allocated_bytes: usize,
    pub arena_capacity_bytes: usize,
    pub heap_alloc_bytes: u64,
    pub heap_alloc_count: u64,
    pub gpu_memory_bytes: u64,
    pub tiles_dirty: u32,
    pub tiles_total: u32,
    pub tile_cache_hit_pct: f32,
    pub scene3d_cache_hit_pct: f32,
    pub glyph_cache_hit_pct: f32,
    pub reconciled_updates: u64,
    pub full_rebuilds: u64,
}

/// Docking orientation and layout mode of the DevTools overlay HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HudDockMode {
    #[default]
    DockBottom,
    DockRight,
    Floating,
    CollapsedPill,
}

/// Active diagnostic panel / tab within the DevTools HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HudTab {
    #[default]
    Components,
    Reactivity,
    Performance,
    Memory,
    TimeTravel,
    Settings,
}

/// Central state model for the overlay HUD.
#[derive(Debug, Clone, PartialEq)]
pub struct HudState {
    pub is_visible: bool,
    pub dock_mode: HudDockMode,
    pub active_tab: HudTab,
    pub is_inspecting_element: bool,
    pub hovered_node: Option<NodeId>,
    pub selected_node: Option<NodeId>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            is_visible: false,
            dock_mode: HudDockMode::DockBottom,
            active_tab: HudTab::Performance,
            is_inspecting_element: false,
            hovered_node: None,
            selected_node: None,
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 320.0,
        }
    }
}

/// HUD Controller managing visibility, tab switching, and geometry bounds.
#[derive(Debug, Clone, Default)]
pub struct HudController {
    pub state: HudState,
}

impl HudController {
    pub fn new() -> Self {
        Self {
            state: HudState::default(),
        }
    }

    /// Toggles the HUD visibility.
    pub fn toggle_visibility(&mut self) -> bool {
        self.state.is_visible = !self.state.is_visible;
        self.state.is_visible
    }

    /// Sets explicit visibility.
    pub fn set_visible(&mut self, visible: bool) {
        self.state.is_visible = visible;
    }

    /// Switches the active docking mode and recalculates screen bounds.
    pub fn set_dock_mode(&mut self, mode: HudDockMode, viewport_w: f32, viewport_h: f32) {
        self.state.dock_mode = mode;
        self.update_bounds(viewport_w, viewport_h);
    }

    /// Switches the active diagnostics tab.
    pub fn set_active_tab(&mut self, tab: HudTab) {
        self.state.active_tab = tab;
    }

    /// Toggles the Inspect Element cursor mode.
    pub fn toggle_inspect_element(&mut self) -> bool {
        self.state.is_inspecting_element = !self.state.is_inspecting_element;
        self.state.is_inspecting_element
    }

    /// Selects a specific node in the component inspector.
    pub fn select_node(&mut self, node: Option<NodeId>) {
        self.state.selected_node = node;
    }

    /// Sets the hovered node during element inspection.
    pub fn hover_node(&mut self, node: Option<NodeId>) {
        self.state.hovered_node = node;
    }

    /// Recalculates HUD bounds based on viewport dimensions and current dock mode.
    pub fn update_bounds(&mut self, viewport_w: f32, viewport_h: f32) {
        match self.state.dock_mode {
            HudDockMode::DockBottom => {
                let h = (viewport_h * 0.35).max(240.0).min(viewport_h);
                self.state.x = 0.0;
                self.state.y = viewport_h - h;
                self.state.width = viewport_w;
                self.state.height = h;
            }
            HudDockMode::DockRight => {
                let w = (viewport_w * 0.40).max(360.0).min(viewport_w);
                self.state.x = viewport_w - w;
                self.state.y = 0.0;
                self.state.width = w;
                self.state.height = viewport_h;
            }
            HudDockMode::Floating => {
                self.state.width = (viewport_w * 0.6).clamp(400.0, 900.0);
                self.state.height = (viewport_h * 0.5).clamp(300.0, 600.0);
                self.state.x = (viewport_w - self.state.width) * 0.5;
                self.state.y = (viewport_h - self.state.height) * 0.5;
            }
            HudDockMode::CollapsedPill => {
                self.state.width = 120.0;
                self.state.height = 36.0;
                self.state.x = viewport_w - 136.0;
                self.state.y = viewport_h - 52.0;
            }
        }
    }

    /// Returns true if coordinates `(x, y)` reside within the HUD window boundaries.
    pub fn is_point_inside_hud(&self, x: f32, y: f32) -> bool {
        if !self.state.is_visible {
            return false;
        }
        x >= self.state.x
            && x <= self.state.x + self.state.width
            && y >= self.state.y
            && y <= self.state.y + self.state.height
    }

    /// Returns HUD bounding rect as FixedRect.
    pub fn bounds_rect(&self) -> FixedRect {
        FixedRect::from_f32(
            self.state.x,
            self.state.y,
            self.state.width,
            self.state.height,
        )
    }

    /// Renders the HUD overlay into the frame display list when visible.
    pub fn render_overlay(
        &self,
        profiler: Option<&FrameTimelineProfiler>,
        display_list: &mut DisplayList,
    ) {
        self.render_overlay_with_context(profiler, None, None, None, display_list);
    }

    /// Renders the HUD overlay backed by live profiler, reactivity, memory, arena, and time-travel state.
    pub fn render_overlay_with_context(
        &self,
        profiler: Option<&FrameTimelineProfiler>,
        live: Option<&HudLiveTelemetry>,
        arena_inspector: Option<&ArenaInspector>,
        time_travel: Option<&TimeTravelDebugger>,
        display_list: &mut DisplayList,
    ) {
        if !self.state.is_visible {
            return;
        }

        let x = self.state.x;
        let y = self.state.y;
        let w = self.state.width;
        let h = self.state.height;

        // 1. HUD Background Panel
        display_list.push(DrawCommand::DrawRect {
            bounds: Rect {
                x,
                y,
                width: w,
                height: h,
            },
            color: Color::rgba(18, 22, 28, 242),
        });

        // Top Accent Border Line
        display_list.push(DrawCommand::DrawRect {
            bounds: Rect {
                x,
                y,
                width: w,
                height: 2.0,
            },
            color: Color::rgb(59, 130, 246),
        });

        // 2. Navigation Bar
        let nav_h = 32.0;
        display_list.push(DrawCommand::DrawRect {
            bounds: Rect {
                x,
                y: y + 2.0,
                width: w,
                height: nav_h,
            },
            color: Color::rgba(26, 31, 40, 250),
        });

        // Title text
        display_list.push(DrawCommand::DrawText {
            origin: (x + 12.0, y + 9.0),
            bounds_width: Some(150.0),
            bounds_height: Some(20.0),
            text_align: TextAlign::Left,
            text: "STRATA DEVTOOLS".to_string(),
            font_size: 11.0,
            line_height: 14.0,
            font_weight: FontWeight::Bold,
            color: Color::rgb(96, 165, 250),
            is_monospace: true,
            font_family: AtomId(0),
        });

        // Diagnostic Tabs
        let tabs = [
            (HudTab::Performance, "Performance"),
            (HudTab::Components, "Components"),
            (HudTab::Reactivity, "Reactivity"),
            (HudTab::Memory, "Memory"),
            (HudTab::TimeTravel, "TimeTravel"),
            (HudTab::Settings, "Settings"),
        ];

        let mut tab_x = x + 160.0;
        for (tab, label) in &tabs {
            let is_active = *tab == self.state.active_tab;
            let tab_w = 90.0;

            if is_active {
                display_list.push(DrawCommand::DrawRect {
                    bounds: Rect {
                        x: tab_x,
                        y: y + 4.0,
                        width: tab_w,
                        height: nav_h - 4.0,
                    },
                    color: Color::rgba(59, 130, 246, 64),
                });
                display_list.push(DrawCommand::DrawRect {
                    bounds: Rect {
                        x: tab_x,
                        y: y + nav_h,
                        width: tab_w,
                        height: 2.0,
                    },
                    color: Color::rgb(59, 130, 246),
                });
            }

            display_list.push(DrawCommand::DrawText {
                origin: (tab_x + 8.0, y + 9.0),
                bounds_width: Some(tab_w - 16.0),
                bounds_height: Some(20.0),
                text_align: TextAlign::Center,
                text: label.to_string(),
                font_size: 11.0,
                line_height: 14.0,
                font_weight: if is_active {
                    FontWeight::Bold
                } else {
                    FontWeight::Normal
                },
                color: if is_active {
                    Color::rgb(255, 255, 255)
                } else {
                    Color::rgb(156, 163, 175)
                },
                is_monospace: false,
                font_family: AtomId(0),
            });

            tab_x += tab_w + 4.0;
        }

        // Close shortcut hint on right
        display_list.push(DrawCommand::DrawText {
            origin: (x + w - 120.0, y + 9.0),
            bounds_width: Some(110.0),
            bounds_height: Some(20.0),
            text_align: TextAlign::Right,
            text: "[F12 to close]".to_string(),
            font_size: 11.0,
            line_height: 14.0,
            font_weight: FontWeight::Normal,
            color: Color::rgb(107, 114, 128),
            is_monospace: true,
            font_family: AtomId(0),
        });

        // 3. Tab Contents
        let content_y = y + nav_h + 8.0;
        match self.state.active_tab {
            HudTab::Performance => {
                self.render_performance_tab(profiler, content_y, display_list);
            }
            HudTab::Components => {
                self.render_components_tab(content_y, display_list);
            }
            HudTab::Reactivity => {
                self.render_reactivity_tab(live, content_y, display_list);
            }
            HudTab::Memory => {
                self.render_memory_tab(live, arena_inspector, content_y, display_list);
            }
            HudTab::TimeTravel => {
                self.render_time_travel_tab(time_travel, content_y, display_list);
            }
            HudTab::Settings => {
                self.render_settings_tab(content_y, display_list);
            }
        }
    }

    /// Returns generated overlay draw commands directly.
    pub fn render_overlay_commands(
        &self,
        profiler: Option<&FrameTimelineProfiler>,
    ) -> Vec<DrawCommand> {
        let mut list = DisplayList::new();
        self.render_overlay(profiler, &mut list);
        list.commands
    }

    /// Returns generated overlay draw commands with full live telemetry context.
    pub fn render_overlay_commands_with_context(
        &self,
        profiler: Option<&FrameTimelineProfiler>,
        live: Option<&HudLiveTelemetry>,
        arena_inspector: Option<&ArenaInspector>,
        time_travel: Option<&TimeTravelDebugger>,
    ) -> Vec<DrawCommand> {
        let mut list = DisplayList::new();
        self.render_overlay_with_context(profiler, live, arena_inspector, time_travel, &mut list);
        list.commands
    }

    fn render_performance_tab(
        &self,
        profiler: Option<&FrameTimelineProfiler>,
        content_y: f32,
        display_list: &mut DisplayList,
    ) {
        let x = self.state.x + 16.0;
        let w = self.state.width - 32.0;

        let (avg_cpu, p50_cpu, p95_cpu, p99_cpu, jank_count, total_samples, budget_us, latest_sample) =
            if let Some(p) = profiler {
                (
                    p.average_cpu_time_us(),
                    p.p50_cpu_time_us(),
                    p.p95_cpu_time_us(),
                    p.p99_cpu_time_us(),
                    p.jank_frame_count(),
                    p.sample_count(),
                    p.frame_budget_us(),
                    p.samples().last().copied(),
                )
            } else {
                (0.0, 0, 0, 0, 0, 0, 16666, None)
            };

        let budget_ms = budget_us as f32 / 1000.0;
        let gpu_tag = latest_sample
            .map(|s| {
                if s.gpu_timestamps_valid {
                    format!(
                        "GPU HW(3D:{}us 2D:{}us)",
                        s.gpu_3d_pass_us, s.gpu_2d_overlay_pass_us
                    )
                } else {
                    format!("GPU:{}us", s.gpu_raster_duration_us)
                }
            })
            .unwrap_or_else(|| "GPU:0us".to_string());
        let summary = format!(
            "Avg CPU: {:.1}us  |  P50/P95/P99: {}/{}/{}us  |  Jank: {}/{}  |  {}  |  Budget: {:.1}ms",
            avg_cpu, p50_cpu, p95_cpu, p99_cpu, jank_count, total_samples, gpu_tag, budget_ms
        );

        display_list.push(DrawCommand::DrawText {
            origin: (x, content_y),
            bounds_width: Some(w),
            bounds_height: Some(18.0),
            text_align: TextAlign::Left,
            text: summary,
            font_size: 12.0,
            line_height: 16.0,
            font_weight: FontWeight::Bold,
            color: Color::rgb(229, 231, 235),
            is_monospace: true,
            font_family: AtomId(0),
        });

        // Multi-stage Frame Timeline Flamegraph Bar
        let flamegraph_y = content_y + 24.0;
        let flamegraph_h = 24.0;

        display_list.push(DrawCommand::DrawRect {
            bounds: Rect {
                x,
                y: flamegraph_y,
                width: w,
                height: flamegraph_h,
            },
            color: Color::rgb(15, 18, 24),
        });

        let stages = if let Some(ref s) = latest_sample {
            [
                ("Input", s.input_dispatch_us, Color::rgb(234, 179, 8)),
                ("Signals", s.signal_drain_us, Color::rgb(168, 85, 247)),
                ("View", s.view_render_us, Color::rgb(139, 92, 246)),
                (
                    "Reconcile",
                    s.parallel_reconcile_us,
                    Color::rgb(59, 130, 246),
                ),
                ("Style", s.style_cascade_us, Color::rgb(6, 182, 212)),
                ("Layout", s.layout_solve_us, Color::rgb(16, 185, 129)),
                (
                    "DisplayList",
                    s.display_list_build_us,
                    Color::rgb(249, 115, 22),
                ),
                ("2D Raster", s.raster_2d_us, Color::rgb(236, 72, 153)),
                ("GPU Submit", s.gpu_submit_us, Color::rgb(239, 68, 68)),
                (
                    "GPU Raster",
                    if s.gpu_timestamps_valid {
                        s.gpu_shadow_pass_us + s.gpu_3d_pass_us + s.gpu_2d_overlay_pass_us
                    } else {
                        s.gpu_raster_duration_us
                    },
                    Color::rgb(245, 158, 11),
                ),
                ("Present", s.present_us, Color::rgb(100, 116, 139)),
                ("VSync Wait", s.vsync_wait_us, Color::rgb(71, 85, 105)),
            ]
        } else {
            [
                ("Input", 0, Color::rgb(234, 179, 8)),
                ("Signals", 0, Color::rgb(168, 85, 247)),
                ("View", 0, Color::rgb(139, 92, 246)),
                ("Reconcile", 0, Color::rgb(59, 130, 246)),
                ("Style", 0, Color::rgb(6, 182, 212)),
                ("Layout", 0, Color::rgb(16, 185, 129)),
                ("DisplayList", 0, Color::rgb(249, 115, 22)),
                ("2D Raster", 0, Color::rgb(236, 72, 153)),
                ("GPU Submit", 0, Color::rgb(239, 68, 68)),
                ("GPU Raster", 0, Color::rgb(245, 158, 11)),
                ("Present", 0, Color::rgb(100, 116, 139)),
                ("VSync Wait", 0, Color::rgb(71, 85, 105)),
            ]
        };

        let total_time: u32 = stages.iter().map(|(_, us, _)| *us).sum();
        let total_time = total_time.max(1);

        let mut seg_x = x;
        for (_, duration_us, color) in &stages {
            if *duration_us > 0 {
                let seg_w = (w * (*duration_us as f32 / total_time as f32)).max(2.0);
                let actual_w = seg_w.min(x + w - seg_x);
                if actual_w > 0.0 {
                    display_list.push(DrawCommand::DrawRect {
                        bounds: Rect {
                            x: seg_x,
                            y: flamegraph_y,
                            width: actual_w,
                            height: flamegraph_h,
                        },
                        color: *color,
                    });
                    seg_x += actual_w;
                }
            }
        }

        // Stage Breakdown Legend
        let legend_y = flamegraph_y + flamegraph_h + 10.0;
        let col_w = w / 3.0;

        for (i, (name, duration_us, color)) in stages.iter().enumerate() {
            let col = i % 3;
            let row = i / 3;
            let item_x = x + col as f32 * col_w;
            let item_y = legend_y + row as f32 * 18.0;

            // Color swatch dot
            display_list.push(DrawCommand::DrawRect {
                bounds: Rect {
                    x: item_x,
                    y: item_y + 3.0,
                    width: 8.0,
                    height: 8.0,
                },
                color: *color,
            });

            let pct = (*duration_us as f32 / total_time as f32) * 100.0;
            let text = format!("{}: {}us ({:.1}%)", name, duration_us, pct);
            display_list.push(DrawCommand::DrawText {
                origin: (item_x + 14.0, item_y),
                bounds_width: Some(col_w - 20.0),
                bounds_height: Some(16.0),
                text_align: TextAlign::Left,
                text,
                font_size: 11.0,
                line_height: 14.0,
                font_weight: FontWeight::Normal,
                color: Color::rgb(209, 213, 219),
                is_monospace: true,
                font_family: AtomId(0),
            });
        }
    }

    fn render_components_tab(&self, content_y: f32, display_list: &mut DisplayList) {
        let x = self.state.x + 16.0;
        let w = self.state.width - 32.0;

        let status = format!(
            "Inspect Mode: {}  |  Selected Node: {:?}  |  Hovered Node: {:?}",
            if self.state.is_inspecting_element {
                "ACTIVE (Click to select)"
            } else {
                "Inactive (Ctrl+Shift+C)"
            },
            self.state.selected_node,
            self.state.hovered_node,
        );

        display_list.push(DrawCommand::DrawText {
            origin: (x, content_y),
            bounds_width: Some(w),
            bounds_height: Some(18.0),
            text_align: TextAlign::Left,
            text: status,
            font_size: 12.0,
            line_height: 16.0,
            font_weight: FontWeight::Bold,
            color: Color::rgb(229, 231, 235),
            is_monospace: true,
            font_family: AtomId(0),
        });

        display_list.push(DrawCommand::DrawText {
            origin: (x, content_y + 24.0),
            bounds_width: Some(w),
            bounds_height: Some(18.0),
            text_align: TextAlign::Left,
            text: "Use mouse to inspect live DOM hierarchy and box models.".to_string(),
            font_size: 11.0,
            line_height: 14.0,
            font_weight: FontWeight::Normal,
            color: Color::rgb(156, 163, 175),
            is_monospace: false,
            font_family: AtomId(0),
        });
    }

    fn render_reactivity_tab(
        &self,
        live: Option<&HudLiveTelemetry>,
        content_y: f32,
        display_list: &mut DisplayList,
    ) {
        let x = self.state.x + 16.0;
        let w = self.state.width - 32.0;

        let default_live = HudLiveTelemetry::default();
        let data = live.unwrap_or(&default_live);

        let header = format!(
            "Signals: {}  |  Subscribers: {}  |  Reads: {}  |  Writes: {}  |  Flushes: {}  |  Cycle Trips: {}",
            data.reactive_active_signals,
            data.reactive_active_subscribers,
            data.reactive_total_reads,
            data.reactive_total_writes,
            data.reactive_total_flushes,
            data.reactive_cycle_guard_trips,
        );
        display_list.push(DrawCommand::DrawText {
            origin: (x, content_y),
            bounds_width: Some(w),
            bounds_height: Some(18.0),
            text_align: TextAlign::Left,
            text: header,
            font_size: 12.0,
            line_height: 16.0,
            font_weight: FontWeight::Bold,
            color: Color::rgb(229, 231, 235),
            is_monospace: true,
            font_family: AtomId(0),
        });

        let dirty_desc = if data.dirty_signal_names.is_empty() {
            "none".to_string()
        } else {
            data.dirty_signal_names
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        };
        let reason_line = format!(
            "Last Rebuild: {}  |  Reconciled/Full: {}/{}  |  Dirty Signals: {}",
            if data.last_rebuild_reason.is_empty() {
                "Clean"
            } else {
                &data.last_rebuild_reason
            },
            data.reconciled_updates,
            data.full_rebuilds,
            dirty_desc
        );
        display_list.push(DrawCommand::DrawText {
            origin: (x, content_y + 22.0),
            bounds_width: Some(w),
            bounds_height: Some(18.0),
            text_align: TextAlign::Left,
            text: reason_line,
            font_size: 11.0,
            line_height: 14.0,
            font_weight: FontWeight::Normal,
            color: Color::rgb(96, 165, 250),
            is_monospace: true,
            font_family: AtomId(0),
        });

        let mut row_y = content_y + 44.0;
        if data.top_signals.is_empty() {
            display_list.push(DrawCommand::DrawText {
                origin: (x, row_y),
                bounds_width: Some(w),
                bounds_height: Some(16.0),
                text_align: TextAlign::Left,
                text: "No signal reads or writes recorded yet.".to_string(),
                font_size: 11.0,
                line_height: 14.0,
                font_weight: FontWeight::Normal,
                color: Color::rgb(156, 163, 175),
                is_monospace: true,
                font_family: AtomId(0),
            });
        } else {
            for (name, writes, reads, subs) in data.top_signals.iter().take(6) {
                let color = if *writes >= 60 {
                    Color::rgb(239, 68, 68)
                } else if *writes >= 10 {
                    Color::rgb(245, 158, 11)
                } else {
                    Color::rgb(16, 185, 129)
                };
                display_list.push(DrawCommand::DrawText {
                    origin: (x, row_y),
                    bounds_width: Some(w),
                    bounds_height: Some(16.0),
                    text_align: TextAlign::Left,
                    text: format!(
                        "{:<24}  writes={:<6}  reads={:<7}  subs={}",
                        name, writes, reads, subs
                    ),
                    font_size: 11.0,
                    line_height: 14.0,
                    font_weight: FontWeight::Normal,
                    color,
                    is_monospace: true,
                    font_family: AtomId(0),
                });
                row_y += 17.0;
            }
        }
    }

    fn render_memory_tab(
        &self,
        live: Option<&HudLiveTelemetry>,
        arena_inspector: Option<&ArenaInspector>,
        content_y: f32,
        display_list: &mut DisplayList,
    ) {
        let x = self.state.x + 16.0;
        let w = self.state.width - 32.0;

        let default_live = HudLiveTelemetry::default();
        let data = live.unwrap_or(&default_live);

        let (arena_alloc, arena_cap) = arena_inspector
            .and_then(|ai| ai.samples().last())
            .map(|s| (s.allocated_bytes, s.capacity_bytes))
            .unwrap_or((data.arena_allocated_bytes, data.arena_capacity_bytes));
        let arena_pct = if arena_cap > 0 {
            (arena_alloc as f32 / arena_cap as f32) * 100.0
        } else {
            0.0
        };

        let counts_line = format!(
            "DOM Nodes: {}  |  Layout Nodes: {}  |  Display Cmds: {}  |  Tiles Dirty/Total: {}/{}",
            data.dom_node_count,
            data.layout_node_count,
            data.display_cmd_count,
            data.tiles_dirty,
            data.tiles_total
        );
        display_list.push(DrawCommand::DrawText {
            origin: (x, content_y),
            bounds_width: Some(w),
            bounds_height: Some(18.0),
            text_align: TextAlign::Left,
            text: counts_line,
            font_size: 12.0,
            line_height: 16.0,
            font_weight: FontWeight::Bold,
            color: Color::rgb(229, 231, 235),
            is_monospace: true,
            font_family: AtomId(0),
        });

        let gpu_mb = data.gpu_memory_bytes as f64 / (1024.0 * 1024.0);
        let heap_kb = data.heap_alloc_bytes as f64 / 1024.0;
        let arena_kb = arena_alloc as f64 / 1024.0;
        let arena_cap_kb = arena_cap as f64 / 1024.0;
        let mem_line = format!(
            "Arena: {:.1}/{:.1} KiB ({:.1}%)  |  GPU VRAM: {:.2} MiB  |  Frame Heap: {:.1} KiB ({} allocs)",
            arena_kb, arena_cap_kb, arena_pct, gpu_mb, heap_kb, data.heap_alloc_count
        );
        display_list.push(DrawCommand::DrawText {
            origin: (x, content_y + 22.0),
            bounds_width: Some(w),
            bounds_height: Some(18.0),
            text_align: TextAlign::Left,
            text: mem_line,
            font_size: 11.0,
            line_height: 14.0,
            font_weight: FontWeight::Normal,
            color: Color::rgb(16, 185, 129),
            is_monospace: true,
            font_family: AtomId(0),
        });

        // Visual Arena / Cache Utilization Bar
        let bar_y = content_y + 44.0;
        let bar_h = 12.0;
        display_list.push(DrawCommand::DrawRect {
            bounds: Rect {
                x,
                y: bar_y,
                width: w,
                height: bar_h,
            },
            color: Color::rgb(15, 18, 24),
        });
        let fill_w = (w * (arena_pct / 100.0).clamp(0.0, 1.0)).max(0.0);
        if fill_w > 0.0 {
            let bar_color = if arena_pct >= 85.0 {
                Color::rgb(239, 68, 68)
            } else if arena_pct >= 60.0 {
                Color::rgb(245, 158, 11)
            } else {
                Color::rgb(16, 185, 129)
            };
            display_list.push(DrawCommand::DrawRect {
                bounds: Rect {
                    x,
                    y: bar_y,
                    width: fill_w,
                    height: bar_h,
                },
                color: bar_color,
            });
        }

        let cache_line = format!(
            "Cache Hit Rates — Tile Cache: {:.1}%  |  Scene3D Cache: {:.1}%  |  Glyph/Text Cache: {:.1}%",
            data.tile_cache_hit_pct, data.scene3d_cache_hit_pct, data.glyph_cache_hit_pct
        );
        display_list.push(DrawCommand::DrawText {
            origin: (x, bar_y + 20.0),
            bounds_width: Some(w),
            bounds_height: Some(18.0),
            text_align: TextAlign::Left,
            text: cache_line,
            font_size: 11.0,
            line_height: 14.0,
            font_weight: FontWeight::Normal,
            color: Color::rgb(209, 213, 219),
            is_monospace: true,
            font_family: AtomId(0),
        });
    }

    fn render_time_travel_tab(
        &self,
        time_travel: Option<&TimeTravelDebugger>,
        content_y: f32,
        display_list: &mut DisplayList,
    ) {
        let x = self.state.x + 16.0;
        let w = self.state.width - 32.0;

        let Some(tt) = time_travel else {
            display_list.push(DrawCommand::DrawText {
                origin: (x, content_y),
                bounds_width: Some(w),
                bounds_height: Some(18.0),
                text_align: TextAlign::Left,
                text: "Time-Travel Ring Buffer: 0 snapshots captured".to_string(),
                font_size: 12.0,
                line_height: 16.0,
                font_weight: FontWeight::Bold,
                color: Color::rgb(229, 231, 235),
                is_monospace: true,
                font_family: AtomId(0),
            });
            return;
        };

        let total = tt.len();
        let cur_idx = if total == 0 { 0 } else { tt.current_index() + 1 };
        let header = format!(
            "Time-Travel Snapshots: {}/{}  |  Back: {}  |  Forward: {}",
            cur_idx,
            total,
            if tt.can_step_back() { "available" } else { "start" },
            if tt.can_step_forward() { "available" } else { "head" },
        );
        display_list.push(DrawCommand::DrawText {
            origin: (x, content_y),
            bounds_width: Some(w),
            bounds_height: Some(18.0),
            text_align: TextAlign::Left,
            text: header,
            font_size: 12.0,
            line_height: 16.0,
            font_weight: FontWeight::Bold,
            color: Color::rgb(229, 231, 235),
            is_monospace: true,
            font_family: AtomId(0),
        });

        let mut row_y = content_y + 22.0;
        if let Some(cur) = tt.current_snapshot() {
            let delta_count = if tt.current_index() > 0 {
                let prev = &tt.snapshots()[tt.current_index() - 1];
                tt.compute_delta(prev.id, cur.id)
                    .map(|d| d.len())
                    .unwrap_or(0)
            } else {
                0
            };
            let active_line = format!(
                "Active #{} @ {}us  |  {}  |  signals={}  deltas={}  ir={:#x}",
                cur.id,
                cur.timestamp_us,
                cur.label,
                cur.signal_values.len(),
                delta_count,
                cur.ir_hash
            );
            display_list.push(DrawCommand::DrawText {
                origin: (x, row_y),
                bounds_width: Some(w),
                bounds_height: Some(16.0),
                text_align: TextAlign::Left,
                text: active_line,
                font_size: 11.0,
                line_height: 14.0,
                font_weight: FontWeight::Bold,
                color: Color::rgb(96, 165, 250),
                is_monospace: true,
                font_family: AtomId(0),
            });
            row_y += 20.0;

            let snaps = tt.snapshots();
            let start = snaps.len().saturating_sub(5);
            for (offset, snap) in snaps[start..].iter().enumerate() {
                let abs_idx = start + offset;
                let marker = if abs_idx == tt.current_index() {
                    ">"
                } else {
                    " "
                };
                display_list.push(DrawCommand::DrawText {
                    origin: (x, row_y),
                    bounds_width: Some(w),
                    bounds_height: Some(15.0),
                    text_align: TextAlign::Left,
                    text: format!(
                        "{} #{:<4} {:>9}us  {:<32}  ir={:#010x}",
                        marker, snap.id, snap.timestamp_us, snap.label, snap.ir_hash as u32
                    ),
                    font_size: 10.5,
                    line_height: 14.0,
                    font_weight: FontWeight::Normal,
                    color: if abs_idx == tt.current_index() {
                        Color::rgb(255, 255, 255)
                    } else {
                        Color::rgb(156, 163, 175)
                    },
                    is_monospace: true,
                    font_family: AtomId(0),
                });
                row_y += 16.0;
            }
        } else {
            display_list.push(DrawCommand::DrawText {
                origin: (x, row_y),
                bounds_width: Some(w),
                bounds_height: Some(16.0),
                text_align: TextAlign::Left,
                text: "No frame snapshots recorded yet.".to_string(),
                font_size: 11.0,
                line_height: 14.0,
                font_weight: FontWeight::Normal,
                color: Color::rgb(156, 163, 175),
                is_monospace: true,
                font_family: AtomId(0),
            });
        }
    }

    fn render_settings_tab(&self, content_y: f32, display_list: &mut DisplayList) {
        let x = self.state.x + 16.0;
        let w = self.state.width - 32.0;

        let dock_str = match self.state.dock_mode {
            HudDockMode::DockBottom => "Bottom",
            HudDockMode::DockRight => "Right",
            HudDockMode::Floating => "Floating",
            HudDockMode::CollapsedPill => "Collapsed Pill",
        };

        display_list.push(DrawCommand::DrawText {
            origin: (x, content_y),
            bounds_width: Some(w),
            bounds_height: Some(18.0),
            text_align: TextAlign::Left,
            text: format!("Dock Orientation: {dock_str}"),
            font_size: 12.0,
            line_height: 16.0,
            font_weight: FontWeight::Bold,
            color: Color::rgb(229, 231, 235),
            is_monospace: true,
            font_family: AtomId(0),
        });
    }

    /// Renders highlight overlay for an inspected element.
    pub fn render_inspect_highlight(&self, bounds: Rect, display_list: &mut DisplayList) {
        display_list.push(DrawCommand::DrawRect {
            bounds,
            color: Color::rgba(59, 130, 246, 90),
        });
        display_list.push(DrawCommand::DrawBorder {
            bounds,
            widths: EdgeInsets::all(1.0),
            color: Color::rgb(59, 130, 246),
            border_radius: BorderRadius::all(0.0),
            style: BorderStyle::Solid,
        });
    }
}

/// Event interceptor routing keyboard shortcuts and isolating pointer input.
pub struct DevToolsEventInterceptor;

impl DevToolsEventInterceptor {
    /// Evaluates a platform event. Returns `true` if the event was consumed by DevTools.
    pub fn intercept(hud: &mut HudController, event: &PlatformEvent) -> bool {
        match event {
            // Hotkey detection: F12, Ctrl+Shift+I, Cmd+Option+I
            PlatformEvent::KeyDown { key, modifiers } => {
                if *key == KeyCode::F12 {
                    hud.toggle_visibility();
                    return true;
                }
                if *key == KeyCode::KeyI && (modifiers.ctrl || modifiers.meta) && modifiers.shift {
                    hud.toggle_visibility();
                    return true;
                }
                if *key == KeyCode::KeyC && (modifiers.ctrl || modifiers.meta) && modifiers.shift {
                    hud.toggle_inspect_element();
                    return true;
                }

                // If HUD is visible and focused, consume keyboard events
                if hud.state.is_visible {
                    return false;
                }
                false
            }

            // Pointer event boundary isolation
            PlatformEvent::MouseMove { x, y } => {
                if hud.state.is_visible && hud.is_point_inside_hud(*x, *y) {
                    return true;
                }
                if hud.state.is_inspecting_element {
                    return true;
                }
                false
            }

            PlatformEvent::MouseDown { x, y, .. } | PlatformEvent::MouseUp { x, y, .. } => {
                if hud.state.is_visible && hud.is_point_inside_hud(*x, *y) {
                    return true;
                }
                if hud.state.is_inspecting_element {
                    hud.state.is_inspecting_element = false;
                    return true;
                }
                false
            }

            PlatformEvent::Scroll { .. } => false,

            _ => false,
        }
    }
}
