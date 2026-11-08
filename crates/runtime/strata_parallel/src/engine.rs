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
    pub computational_weight: usize,
}

impl Default for LayoutPartition {
    fn default() -> Self {
        Self {
            partition_id: PartitionId(0),
            origin: (0.0, 0.0),
            constraints: LayoutConstraints::default(),
            z_index: 0,
            node_id: NodeId(0),
            computational_weight: 1,
        }
    }
}

impl LayoutPartition {
    pub fn new(partition_id: PartitionId, node_id: NodeId) -> Self {
        Self {
            partition_id,
            origin: (0.0, 0.0),
            constraints: LayoutConstraints::default(),
            z_index: 0,
            node_id,
            computational_weight: 1,
        }
    }

    pub fn with_weight(mut self, weight: usize) -> Self {
        self.computational_weight = weight.max(1);
        self
    }

    pub fn with_origin(mut self, x: f32, y: f32) -> Self {
        self.origin = (x, y);
        self
    }

    pub fn with_z_index(mut self, z_index: i32) -> Self {
        self.z_index = z_index;
        self
    }
}

/// Adaptive partitioner that decomposes heavy layout subtrees into independent partitions
/// to prevent worker thread starvation.
#[derive(Debug, Clone, Copy)]
pub struct AdaptivePartitioner {
    pub max_partition_weight: usize,
}

impl AdaptivePartitioner {
    pub fn new(max_partition_weight: usize) -> Self {
        Self {
            max_partition_weight: max_partition_weight.max(1),
        }
    }

    /// Splits oversized partitions into balanced sub-tasks when weight exceeds max_partition_weight.
    pub fn balance_partitions(&self, partitions: Vec<LayoutPartition>) -> Vec<LayoutPartition> {
        let mut balanced = Vec::with_capacity(partitions.len());
        for p in partitions {
            if p.computational_weight > self.max_partition_weight {
                let sub_count = p.computational_weight.div_ceil(self.max_partition_weight);
                let sub_weight = (p.computational_weight / sub_count).max(1);
                for i in 0..sub_count {
                    balanced.push(LayoutPartition {
                        partition_id: PartitionId(p.partition_id.0 * 1000 + i as u32),
                        origin: p.origin,
                        constraints: p.constraints,
                        z_index: p.z_index,
                        node_id: p.node_id,
                        computational_weight: sub_weight,
                    });
                }
            } else {
                balanced.push(p);
            }
        }
        balanced
    }
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

use std::sync::Arc;

/// The multi-threaded parallel execution and display list assembly engine.
/// Powered by an authentic Rayon work-stealing threadpool with zero central mutex lock contention.
#[derive(Clone)]
pub struct ParallelEngine {
    num_threads: usize,
    pool: Arc<rayon::ThreadPool>,
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
        Self::with_threads(threads)
    }

    /// Creates a new parallel engine with a specified persistent Rayon work-stealing thread count.
    pub fn with_threads(num_threads: usize) -> Self {
        let count = num_threads.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(count)
            .thread_name(|i| format!("strata-parallel-{i}"))
            .build()
            .expect("Failed to initialize Rayon work-stealing threadpool");
        Self {
            num_threads: count,
            pool: Arc::new(pool),
        }
    }

    pub fn thread_count(&self) -> usize {
        self.num_threads
    }

    /// Returns a reference to the underlying Rayon work-stealing threadpool.
    pub fn rayon_pool(&self) -> &rayon::ThreadPool {
        &self.pool
    }

    /// Executes layout reconciliation across independent partitions concurrently
    /// using persistent Rayon work-stealing threads, collecting chunks into pre-allocated slots.
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

        if k == 1 || self.num_threads == 1 {
            return partitions
                .iter()
                .map(|partition| {
                    crate::arena::reset_thread_local_arena();
                    let cmds = worker_fn(partition);
                    Some(DisplayListChunk {
                        partition_id: partition.partition_id,
                        z_index: partition.z_index,
                        origin: partition.origin,
                        commands: cmds,
                    })
                })
                .collect();
        }

        // Compute LPT (Longest Processing Time first) priority order:
        // Heaviest partitions are scheduled first, maximizing work-stealing opportunities for remaining cores.
        let mut order: Vec<usize> = (0..k).collect();
        order.sort_by_key(|&idx| std::cmp::Reverse(partitions[idx].computational_weight));

        let results: Vec<(usize, DisplayListChunk)> = self.pool.install(|| {
            use rayon::prelude::*;
            order
                .into_par_iter()
                .map(|idx| {
                    let partition = &partitions[idx];
                    crate::arena::reset_thread_local_arena();
                    let cmds = worker_fn(partition);
                    (
                        idx,
                        DisplayListChunk {
                            partition_id: partition.partition_id,
                            z_index: partition.z_index,
                            origin: partition.origin,
                            commands: cmds,
                        },
                    )
                })
                .collect()
        });

        let mut slots: Vec<Option<DisplayListChunk>> = (0..k).map(|_| None).collect();
        for (idx, chunk) in results {
            slots[idx] = Some(chunk);
        }

        debug_assert!(
            slots.iter().all(|s| s.is_some()),
            "Every scheduled partition must produce a corresponding display list chunk"
        );

        slots
    }

    /// Computes layout for a slice of independent layout subtrees in parallel
    /// across Rayon work-stealing threads with zero mutex contention.
    pub fn compute_subtrees_parallel(
        &self,
        subtrees: &mut [strata_layout::LayoutNode],
        available_width: f32,
        available_height: f32,
    ) {
        use rayon::prelude::*;
        if subtrees.is_empty() {
            return;
        }
        if subtrees.len() == 1 || self.num_threads == 1 {
            for tree in subtrees {
                tree.compute_layout(available_width, available_height);
            }
            return;
        }

        self.pool.install(|| {
            subtrees.par_iter_mut().for_each(|node| {
                crate::arena::reset_thread_local_arena();
                node.compute_layout(available_width, available_height);
            });
        });
    }

    /// Recursively divides and computes independent layout subtrees in parallel
    /// using `rayon::join()` for binary divide-and-conquer work-stealing.
    pub fn compute_subtrees_recursive_join(
        &self,
        subtrees: &mut [strata_layout::LayoutNode],
        available_width: f32,
        available_height: f32,
    ) {
        if subtrees.is_empty() {
            return;
        }
        if subtrees.len() == 1 || self.num_threads == 1 {
            for tree in subtrees {
                tree.compute_layout(available_width, available_height);
            }
            return;
        }

        self.pool.install(|| {
            fn split_and_compute(slice: &mut [strata_layout::LayoutNode], w: f32, h: f32) {
                match slice.len() {
                    0 => {}
                    1 => {
                        crate::arena::reset_thread_local_arena();
                        slice[0].compute_layout(w, h);
                    }
                    _ => {
                        let mid = slice.len() / 2;
                        let (left, right) = slice.split_at_mut(mid);
                        rayon::join(
                            || split_and_compute(left, w, h),
                            || split_and_compute(right, w, h),
                        );
                    }
                }
            }
            split_and_compute(subtrees, available_width, available_height);
        });
    }

    /// Assembles completed partition chunks into the final Master Display List,
    /// sorting chunks by resolved CSS stacking context order (z_index, DOM tree order)
    /// and applying spatial origin offsets directly.
    pub fn assemble_master_display_list(&self, slots: &[Option<DisplayListChunk>]) -> DisplayList {
        let mut master = DisplayList::new();

        let mut present_chunks: Vec<(usize, &DisplayListChunk)> = slots
            .iter()
            .enumerate()
            .filter_map(|(idx, s)| s.as_ref().map(|c| (idx, c)))
            .collect();

        // Stable sort by z_index ascending, preserving DOM tree order for identical z_index
        present_chunks.sort_by(|(idx_a, chunk_a), (idx_b, chunk_b)| {
            chunk_a
                .z_index
                .cmp(&chunk_b.z_index)
                .then_with(|| idx_a.cmp(idx_b))
        });

        for (_idx, chunk) in present_chunks {
            for cmd in &chunk.commands {
                master.push(translate_draw_command(cmd, chunk.origin));
            }
        }

        master
    }
}
