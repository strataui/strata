//! Per-phase frame profiling: rebuild, layout, compile, raster and present.

use std::time::Duration;

/// Wall-clock cost of each frame phase since the last [`FrameProfile::take`].
///
/// Phases accumulate, so a frame that rebuilds twice reports both costs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameProfile {
    /// Blueprint to layout-tree rebuild, including style resolution.
    pub rebuild: Duration,
    /// Layout computation.
    pub layout: Duration,
    /// Display-list compilation (compositor, scrollbars, path batch).
    pub compile: Duration,
    /// Rasterization, recorded by the presenting backend.
    pub raster: Duration,
    /// Presentation to the surface, recorded by the presenting backend.
    pub present: Duration,
}

impl FrameProfile {
    /// Sum of all phases.
    pub fn total(&self) -> Duration {
        self.rebuild + self.layout + self.compile + self.raster + self.present
    }
}

/// Receives a completed [`FrameProfile`] each time a frame is presented.
pub struct FrameProfileHook(pub Box<dyn FnMut(&FrameProfile)>);

impl std::fmt::Debug for FrameProfileHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FrameProfileHook(..)")
    }
}
