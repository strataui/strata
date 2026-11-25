//! Built-in Hot Module Replacement (HMR) runtime bridge for `AppRuntime`.
//!
//! When a Strata application is compiled with `--features hot-reload` and launched by
//! `strata dev` (which exports `STRATA_HMR_SOCKET`), [`HmrRuntimeController`] opens the
//! cross-platform HMR IPC listener, accepts live compiled `cdylib` patches, migrates all
//! `use_hmr_signal(...)` values across module epochs, and hot-swaps the active [`View`]
//! inside [`AppRuntime`] while preserving scroll offsets, focus, and text input buffers.

use std::path::{Path, PathBuf};

use strata_dom::View;
use strata_hot_reload::{
    HmrIpcServer, HmrRequest, HmrResponse, HotReloadMessage, HotReloader, STRATA_HMR_SOCKET_ENV,
};

use super::{AppRuntime, RebuildReason};

/// Result of applying a hot-reload patch or rollback to an active [`AppRuntime`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HmrPatchOutcome {
    pub ok: bool,
    pub epoch: u64,
    pub swap_us: u64,
    pub signals_restored: usize,
    pub error: Option<String>,
}

/// Manages the live HMR IPC server and dynamic-library [`HotReloader`] inside a running Strata app.
pub struct HmrRuntimeController {
    reloader: HotReloader,
    server: HmrIpcServer,
    shutdown_requested: bool,
}

impl HmrRuntimeController {
    /// Binds an HMR IPC endpoint at `socket_path`, invoking `on_notify` on incoming commands
    /// so the platform event loop wakes immediately.
    pub fn bind<F>(
        socket_path: impl Into<PathBuf>,
        on_notify: F,
    ) -> Result<Self, std::io::Error>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let server = HmrIpcServer::bind(socket_path, on_notify)?;
        Ok(Self {
            reloader: HotReloader::new(),
            server,
            shutdown_requested: false,
        })
    }

    /// Starts an HMR controller if the `STRATA_HMR_SOCKET` environment variable is set.
    pub fn from_env<F>(on_notify: F) -> Option<Self>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let path = std::env::var(STRATA_HMR_SOCKET_ENV).ok()?;
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return None;
        }
        match Self::bind(trimmed, on_notify) {
            Ok(ctrl) => Some(ctrl),
            Err(err) => {
                eprintln!("[Strata HMR] Failed binding IPC socket '{trimmed}': {err}");
                None
            }
        }
    }

    /// Returns the filesystem path of the bound HMR IPC endpoint.
    pub fn socket_path(&self) -> &Path {
        self.server.socket_path()
    }

    /// Returns the current dynamic-library reload epoch (`0` before the first hot patch).
    pub fn current_epoch(&self) -> u64 {
        self.reloader.current_epoch()
    }

    /// Returns `true` once at least one hot-reloaded dynamic library is active.
    pub fn is_hot_active(&self) -> bool {
        self.reloader.is_loaded()
    }

    /// Returns `true` if `strata dev` requested a graceful shutdown over the HMR channel.
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    /// Returns `true` if the currently active hot-reloaded library has dirty reactive signals.
    pub fn has_dirty_signals(&self) -> bool {
        self.reloader.active_has_dirty_signals()
    }

    /// Drains the dirty-signal flag from the currently active hot-reloaded library.
    pub fn take_dirty_signals(&self) -> bool {
        self.reloader.active_take_dirty_signals()
    }

    /// Renders a fresh [`View`] from the currently active hot-reloaded module, if loaded.
    pub fn render_active_view(&self) -> Option<View> {
        if !self.reloader.is_loaded() {
            return None;
        }
        match self.reloader.render_active_view() {
            Ok(view) => Some(view),
            Err(err) => {
                eprintln!("[Strata HMR] Active epoch render failed: {err}");
                None
            }
        }
    }

    /// Captures live HMR signals, loads `dylib_path` into a new epoch, hydrates signals,
    /// renders the updated root [`View`], and updates `runtime` in-place preserving scroll,
    /// focus, and text input states.
    pub fn apply_dylib_patch(
        &mut self,
        runtime: &mut AppRuntime,
        dylib_path: &str,
    ) -> HmrPatchOutcome {
        let snapshot = self.reloader.capture_dom_signals();
        let signals_restored = snapshot.signals.len();

        match self.reloader.apply_patch(dylib_path) {
            HotReloadMessage::PatchSuccess { latency_us } => {
                let epoch = self.reloader.current_epoch();
                match self.reloader.render_active_view() {
                    Ok(new_view) => {
                        runtime.set_rebuild_reason(RebuildReason::HmrPatch(epoch));
                        runtime.update_view(new_view);
                        runtime.mark_paint_dirty();
                        HmrPatchOutcome {
                            ok: true,
                            epoch,
                            swap_us: latency_us,
                            signals_restored,
                            error: None,
                        }
                    }
                    Err(err) => HmrPatchOutcome {
                        ok: false,
                        epoch,
                        swap_us: latency_us,
                        signals_restored,
                        error: Some(err),
                    },
                }
            }
            HotReloadMessage::PatchFailed { error } => HmrPatchOutcome {
                ok: false,
                epoch: self.reloader.current_epoch(),
                swap_us: 0,
                signals_restored: 0,
                error: Some(error),
            },
            other => HmrPatchOutcome {
                ok: false,
                epoch: self.reloader.current_epoch(),
                swap_us: 0,
                signals_restored: 0,
                error: Some(format!("Unexpected HMR reloader response: {other:?}")),
            },
        }
    }

    /// Drains all queued HMR IPC commands from `strata dev`, executes them against `runtime`,
    /// sends JSON-Lines acknowledgements, and returns the most recent patch outcome if a patch
    /// or rollback ran.
    pub fn poll_and_apply(&mut self, runtime: &mut AppRuntime) -> Option<HmrPatchOutcome> {
        let mut last_outcome = None;
        for cmd in self.server.drain_pending() {
            match cmd.request {
                HmrRequest::Ping => {
                    let _ = cmd.responder.send(HmrResponse::Pong {
                        epoch: self.reloader.current_epoch(),
                    });
                }
                HmrRequest::ApplyPatch { dylib_path, .. } => {
                    let outcome = self.apply_dylib_patch(runtime, &dylib_path);
                    let _ = cmd.responder.send(HmrResponse::PatchAck {
                        ok: outcome.ok,
                        epoch: outcome.epoch,
                        swap_us: outcome.swap_us,
                        signals_restored: outcome.signals_restored,
                        error: outcome.error.clone(),
                    });
                    last_outcome = Some(outcome);
                }
                HmrRequest::Rollback => {
                    let start = std::time::Instant::now();
                    let signals_restored = self
                        .reloader
                        .last_snapshot()
                        .map(|s| s.signals.len())
                        .unwrap_or(0);
                    let outcome = match self.reloader.rollback() {
                        Ok(epoch) => match self.reloader.render_active_view() {
                            Ok(new_view) => {
                                runtime.set_rebuild_reason(RebuildReason::HmrPatch(epoch));
                                runtime.update_view(new_view);
                                runtime.mark_paint_dirty();
                                HmrPatchOutcome {
                                    ok: true,
                                    epoch,
                                    swap_us: (start.elapsed().as_micros() as u64).max(1),
                                    signals_restored,
                                    error: None,
                                }
                            }
                            Err(err) => HmrPatchOutcome {
                                ok: false,
                                epoch,
                                swap_us: (start.elapsed().as_micros() as u64).max(1),
                                signals_restored: 0,
                                error: Some(err),
                            },
                        },
                        Err(err) => HmrPatchOutcome {
                            ok: false,
                            epoch: self.reloader.current_epoch(),
                            swap_us: 0,
                            signals_restored: 0,
                            error: Some(err),
                        },
                    };
                    let _ = cmd.responder.send(HmrResponse::PatchAck {
                        ok: outcome.ok,
                        epoch: outcome.epoch,
                        swap_us: outcome.swap_us,
                        signals_restored: outcome.signals_restored,
                        error: outcome.error.clone(),
                    });
                    last_outcome = Some(outcome);
                }
                HmrRequest::Shutdown => {
                    self.shutdown_requested = true;
                    let _ = cmd.responder.send(HmrResponse::ShutdownAck);
                }
            }
        }
        last_outcome
    }
}
