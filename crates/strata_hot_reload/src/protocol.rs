//! IPC messages for hot code swap and developer server coordination.

use crate::snapshot::HotReloadSnapshot;

/// Message exchanged over IPC socket between build toolchain and the running application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotReloadMessage {
    FileChanged {
        path: String,
        timestamp: u64,
    },
    RecompileStarted {
        module: String,
    },
    RecompileFinished {
        module: String,
        dylib_path: String,
        duration_ms: u64,
    },
    ApplyPatch {
        dylib_path: String,
        snapshot: HotReloadSnapshot,
    },
    PatchSuccess {
        latency_us: u64,
    },
    PatchFailed {
        error: String,
    },
}

impl HotReloadMessage {
    pub fn is_success(&self) -> bool {
        matches!(self, HotReloadMessage::PatchSuccess { .. })
    }
}
