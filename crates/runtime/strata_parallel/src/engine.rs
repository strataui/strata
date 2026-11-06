//! Multi-Threaded Subtree Dispatcher and Deterministic O(K) Master Display List Assembly.

use crate::containment::PartitionId;
use strata_dom::NodeId;
use strata_layout::Rect;
use strata_render::{DisplayList, DrawCommand};

/// Parent geometry constraints supplied to a partition root.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConstraints {
    pub available_width: f32,
    pub available_height: f32,
    pub device_pixel_ratio: f32,
}

impl Default for LayoutConstraints {
    fn default() -> Self {
        Self {
            available_width: 1920.0,
            available_height: 1080.0,
            device_pixel_ratio: 1.0,
        }
    }
}

/// An isolated layout partition scheduled for concurrent worker execution.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutPartition {
    pub partition_id: PartitionId,
    pub origin: (f32, f32),
    pub constraints: LayoutConstraints,
    pub z_index: i32,
    pub node_id: NodeId,
}

/// An independent display list chunk emitted by a worker thread.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayListChunk {
    pub partition_id: PartitionId,
    pub z_index: i32,
    pub origin: (f32, f32),
    pub commands: Vec<DrawCommand>,
}

/// Translates a DrawCommand by an affine (dx, dy) spatial offset.
pub fn translate_draw_command(cmd: &DrawCommand, offset: (f32, f32)) -> DrawCommand {
    let (dx, dy) = offset;
    match cmd {
        DrawCommand::DrawRect { bounds, color } => DrawCommand::DrawRect {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            color: *color,
        },
        DrawCommand::DrawRoundedRect {
            bounds,
            color,
            border_radius,
            border_width,
            border_color,
        } => DrawCommand::DrawRoundedRect {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            color: *color,
            border_radius: *border_radius,
            border_width: *border_width,
            border_color: *border_color,
        },
        DrawCommand::DrawShadow { bounds, shadow } => DrawCommand::DrawShadow {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            shadow: shadow.clone(),
        },
        DrawCommand::DrawBorder {
            bounds,
            widths,
            color,
            border_radius,
            style,
        } => DrawCommand::DrawBorder {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            widths: *widths,
            color: *color,
            border_radius: *border_radius,
            style: *style,
        },
        DrawCommand::DrawText {
            origin,
            bounds_width,
            bounds_height,
            text_align,
            text,
            font_size,
            line_height,
            font_weight,
            color,
            is_monospace,
            font_family,
        } => DrawCommand::DrawText {
            origin: (origin.0 + dx, origin.1 + dy),
            bounds_width: *bounds_width,
            bounds_height: *bounds_height,
            text_align: *text_align,
            text: text.clone(),
            font_size: *font_size,
            line_height: *line_height,
            font_weight: *font_weight,
            color: *color,
            is_monospace: *is_monospace,
            font_family: *font_family,
        },
        DrawCommand::DrawTextFragments { fragments } => {
            let mut translated = fragments.clone();
            for f in &mut translated {
                f.bounds.x += dx;
                f.bounds.y += dy;
                f.baseline_y += dy;
            }
            DrawCommand::DrawTextFragments {
                fragments: translated,
            }
        }
        DrawCommand::DrawViewport3D { bounds, scene } => DrawCommand::DrawViewport3D {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            scene: scene.clone(),
        },
        DrawCommand::DrawCanvas2D { bounds, commands } => DrawCommand::DrawCanvas2D {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            commands: commands.clone(),
        },
        DrawCommand::DrawSvgPath {
            bounds,
            path_data,
            stroke_color,
            fill_color,
            stroke_width,
            stroke_linecap,
            stroke_linejoin,
            stroke_dasharray,
            fill_rule,
            view_box,
        } => DrawCommand::DrawSvgPath {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            path_data: path_data.clone(),
            stroke_color: *stroke_color,
            fill_color: *fill_color,
            stroke_width: *stroke_width,
            stroke_linecap: *stroke_linecap,
            stroke_linejoin: *stroke_linejoin,
            stroke_dasharray: stroke_dasharray.clone(),
            fill_rule: *fill_rule,
            view_box: *view_box,
        },
        DrawCommand::DrawGpuPath {
            bounds,
            path,
            stroke,
            fill_color,
            stroke_color,
            transform,
        } => DrawCommand::DrawGpuPath {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            path: path.clone(),
            stroke: *stroke,
            fill_color: *fill_color,
            stroke_color: *stroke_color,
            transform: *transform,
        },
        DrawCommand::DrawInsetShadow {
            bounds,
            shadow,
            border_radius,
        } => DrawCommand::DrawInsetShadow {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            shadow: shadow.clone(),
            border_radius: *border_radius,
        },
        DrawCommand::DrawLinearGradient {
            bounds,
            angle_deg,
            stops,
            border_radius,
        } => DrawCommand::DrawLinearGradient {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            angle_deg: *angle_deg,
            stops: stops.clone(),
            border_radius: *border_radius,
        },
        DrawCommand::DrawRadialGradient {
            bounds,
            center,
            stops,
            border_radius,
        } => DrawCommand::DrawRadialGradient {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            center: *center,
            stops: stops.clone(),
            border_radius: *border_radius,
        },
        DrawCommand::PushBackdropFilter {
            bounds,
            radius,
            filter,
        } => DrawCommand::PushBackdropFilter {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            radius: *radius,
            filter: filter.clone(),
        },
        DrawCommand::PopBackdropFilter => DrawCommand::PopBackdropFilter,
        DrawCommand::PushFilter { bounds, filter } => DrawCommand::PushFilter {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            filter: filter.clone(),
        },
        DrawCommand::PopFilter => DrawCommand::PopFilter,
        DrawCommand::PushClipPath { bounds, clip_path } => DrawCommand::PushClipPath {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            clip_path: clip_path.clone(),
        },
        DrawCommand::PopClipPath => DrawCommand::PopClipPath,
        DrawCommand::PushBlendMode { mode } => DrawCommand::PushBlendMode { mode: *mode },
        DrawCommand::PopBlendMode => DrawCommand::PopBlendMode,
        DrawCommand::PushIsolation { isolate } => DrawCommand::PushIsolation { isolate: *isolate },
        DrawCommand::PopIsolation => DrawCommand::PopIsolation,
        DrawCommand::PushOpacity { alpha } => DrawCommand::PushOpacity { alpha: *alpha },
        DrawCommand::PopOpacity => DrawCommand::PopOpacity,
        DrawCommand::PushClip { bounds, radius } => DrawCommand::PushClip {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            radius: *radius,
        },
        DrawCommand::PopClip => DrawCommand::PopClip,
        DrawCommand::PushTransform { matrix } => DrawCommand::PushTransform { matrix: *matrix },
        DrawCommand::PopTransform => DrawCommand::PopTransform,
        DrawCommand::DrawImage { bounds, image } => DrawCommand::DrawImage {
            bounds: Rect::new(bounds.x + dx, bounds.y + dy, bounds.width, bounds.height),
            image: image.clone(),
        },
    }
}

/// The multi-threaded parallel execution and display list assembly engine.
pub struct ParallelEngine {
    num_threads: usize,
}

impl Default for ParallelEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ParallelEngine {
    /// Creates a new parallel engine matching logical CPU core availability.
    pub fn new() -> Self {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self {
            num_threads: threads,
        }
    }

    pub fn with_threads(num_threads: usize) -> Self {
        Self {
            num_threads: num_threads.max(1),
        }
    }

    pub fn thread_count(&self) -> usize {
        self.num_threads
    }

    /// Executes layout reconciliation across independent partitions concurrently
    /// using standard library scoped threads, collecting chunks into pre-allocated slots.
    pub fn execute_parallel<F>(
        &self,
        partitions: &[LayoutPartition],
        worker_fn: F,
    ) -> Vec<Option<DisplayListChunk>>
    where
        F: Fn(&LayoutPartition) -> Vec<DrawCommand> + Send + Sync,
    {
        let k = partitions.len();
        if k == 0 {
            return Vec::new();
        }

        // Pre-allocated slot array 0..K-1
        let mut slots: Vec<Option<DisplayListChunk>> = (0..k).map(|_| None).collect();

        // If single partition or single thread configured, execute serially
        if self.num_threads <= 1 || k == 1 {
            for (idx, partition) in partitions.iter().enumerate() {
                let cmds = worker_fn(partition);
                slots[idx] = Some(DisplayListChunk {
                    partition_id: partition.partition_id,
                    z_index: partition.z_index,
                    origin: partition.origin,
                    commands: cmds,
                });
            }
            return slots;
        }

        // Concurrent execution using std::thread::scope
        let chunk_size = (k + self.num_threads - 1) / self.num_threads;
        let worker_ref = &worker_fn;

        std::thread::scope(|scope| {
            let mut handles = Vec::new();

            for chunk_slice in partitions.chunks(chunk_size) {
                let handle = scope.spawn(move || {
                    let mut local_results = Vec::with_capacity(chunk_slice.len());
                    for partition in chunk_slice {
                        let cmds = worker_ref(partition);
                        local_results.push((
                            partition.partition_id.0 as usize,
                            DisplayListChunk {
                                partition_id: partition.partition_id,
                                z_index: partition.z_index,
                                origin: partition.origin,
                                commands: cmds,
                            },
                        ));
                    }
                    local_results
                });
                handles.push(handle);
            }

            for handle in handles {
                if let Ok(results) = handle.join() {
                    for (slot_idx, chunk) in results {
                        if slot_idx < slots.len() {
                            slots[slot_idx] = Some(chunk);
                        }
                    }
                }
            }
        });

        slots
    }

    /// Assembles completed partition chunks into the final Master Display List in deterministic O(K) time
    /// without sorting, applying spatial origin offsets directly.
    pub fn assemble_master_display_list(&self, slots: &[Option<DisplayListChunk>]) -> DisplayList {
        let mut master = DisplayList::new();

        for slot in slots {
            if let Some(chunk) = slot {
                for cmd in &chunk.commands {
                    master.push(translate_draw_command(cmd, chunk.origin));
                }
            }
        }

        master
    }
}
