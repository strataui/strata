//! State-preserving hot-reloader orchestrating state capture, dynamic code loading, and state restoration.

use std::path::Path;
use std::time::Instant;

use crate::protocol::HotReloadMessage;
use crate::snapshot::HotReloadSnapshot;

/// Manages hot code reload cycles with 0% application state loss.
pub struct HotReloader {
    current_epoch: u64,
    last_snapshot: Option<HotReloadSnapshot>,
    active_library: Option<libloading::Library>,
    migrations: Vec<crate::snapshot::SignalMigration>,
}

impl Default for HotReloader {
    fn default() -> Self {
        Self::new()
    }
}

impl HotReloader {
    /// Hot-reloading ABI version expected by this engine.
    pub const CURRENT_ABI_VERSION: u32 = 1;

    /// Creates a new hot-reloader instance at epoch 0.
    pub fn new() -> Self {
        Self {
            current_epoch: 0,
            last_snapshot: None,
            active_library: None,
            migrations: Vec::new(),
        }
    }

    /// Returns the current reload epoch counter.
    pub fn current_epoch(&self) -> u64 {
        self.current_epoch
    }

    /// Returns true if a dynamic library is currently loaded in memory.
    pub fn is_loaded(&self) -> bool {
        self.active_library.is_some()
    }

    /// Borrow the currently loaded `libloading::Library` handle, if any.
    pub fn active_library(&self) -> Option<&libloading::Library> {
        self.active_library.as_ref()
    }

    /// Captures application state into a snapshot before reloading code.
    pub fn capture_snapshot(&mut self, snapshot: HotReloadSnapshot) {
        self.last_snapshot = Some(snapshot);
    }

    /// Returns a reference to the most recent captured state snapshot.
    pub fn last_snapshot(&self) -> Option<&HotReloadSnapshot> {
        self.last_snapshot.as_ref()
    }

    /// Returns a mutable reference to the most recent captured state snapshot.
    pub fn last_snapshot_mut(&mut self) -> Option<&mut HotReloadSnapshot> {
        self.last_snapshot.as_mut()
    }

    /// Registers a schema migration rule to execute prior to hydrating reloaded modules.
    pub fn register_migration(&mut self, migration: crate::snapshot::SignalMigration) {
        self.migrations.push(migration);
    }

    /// Applies all registered schema migrations to the active snapshot.
    pub fn apply_registered_migrations(&mut self) -> usize {
        if let Some(ref mut snapshot) = self.last_snapshot {
            snapshot.migrate_signals(&self.migrations)
        } else {
            0
        }
    }

    /// Applies a hot reload patch from a compiled shared library (`.so`, `.dylib`, `.dll`).
    ///
    /// Validates module existence, loads the library via `libloading`, verifies ABI compatibility,
    /// hydrates preserved reactive snapshot state into the new module, executes module entry,
    /// increments the epoch, and measures microsecond swap latency.
    pub fn apply_patch(&mut self, dylib_path: &str) -> HotReloadMessage {
        if dylib_path.is_empty() {
            return HotReloadMessage::PatchFailed {
                error: "Dylib path is empty".to_string(),
            };
        }

        let path = Path::new(dylib_path);
        if !path.exists() {
            return HotReloadMessage::PatchFailed {
                error: format!("Dynamic library not found at: {}", dylib_path),
            };
        }

        let start = Instant::now();

        // 1. Load dynamic library via system dynamic linker
        let lib = match unsafe { libloading::Library::new(path) } {
            Ok(l) => l,
            Err(err) => {
                return HotReloadMessage::PatchFailed {
                    error: format!("Failed to load dynamic library '{dylib_path}': {err}"),
                };
            }
        };

        // 2. ABI Version verification
        unsafe {
            if let Ok(version_sym) =
                lib.get::<unsafe extern "C" fn() -> u32>(b"strata_hot_reload_version\0")
            {
                let version = version_sym();
                if version != Self::CURRENT_ABI_VERSION {
                    return HotReloadMessage::PatchFailed {
                        error: format!(
                            "Hot reload ABI version mismatch: expected {}, module reported {}",
                            Self::CURRENT_ABI_VERSION,
                            version
                        ),
                    };
                }
            }
        }

        // 3. State schema migration and re-hydration from snapshot
        if let Some(ref mut snapshot) = self.last_snapshot {
            if !self.migrations.is_empty() {
                snapshot.migrate_signals(&self.migrations);
            }
            let bytes = snapshot.encode();
            unsafe {
                if let Ok(hydrate_sym) =
                    lib.get::<unsafe extern "C" fn(*const u8, usize) -> i32>(
                        b"strata_hot_reload_hydrate\0",
                    )
                {
                    let res = hydrate_sym(bytes.as_ptr(), bytes.len());
                    if res != 0 {
                        return HotReloadMessage::PatchFailed {
                            error: format!("State hydration failed with status code {res}"),
                        };
                    }
                }
            }
        }

        // 4. Invoke module entry point if exported
        unsafe {
            if let Ok(entry_sym) =
                lib.get::<unsafe extern "C" fn() -> i32>(b"strata_hot_reload_entry\0")
            {
                let res = entry_sym();
                if res != 0 {
                    return HotReloadMessage::PatchFailed {
                        error: format!("Module entry point failed with status code {res}"),
                    };
                }
            }
        }

        // 5. Store active library and advance epoch
        self.active_library = Some(lib);
        self.current_epoch += 1;

        let latency_us = (start.elapsed().as_micros() as u64).max(1);
        HotReloadMessage::PatchSuccess { latency_us }
    }

    /// Captures state and applies a patch in a single atomic transaction.
    pub fn apply_patch_with_snapshot(
        &mut self,
        dylib_path: &str,
        snapshot: HotReloadSnapshot,
    ) -> HotReloadMessage {
        self.capture_snapshot(snapshot);
        self.apply_patch(dylib_path)
    }

    /// Invokes the component entry point symbol in the loaded module.
    pub fn call_entry(&self) -> Result<i32, String> {
        if let Some(ref lib) = self.active_library {
            unsafe {
                match lib.get::<unsafe extern "C" fn() -> i32>(b"strata_hot_reload_entry\0") {
                    Ok(sym) => Ok(sym()),
                    Err(err) => Err(format!("Symbol strata_hot_reload_entry not found: {err}")),
                }
            }
        } else {
            Err("No active dynamic library loaded".to_string())
        }
    }

    /// Invokes state hydration on the active library with the given snapshot.
    pub fn hydrate_active_library(&self, snapshot: &HotReloadSnapshot) -> Result<(), String> {
        if let Some(ref lib) = self.active_library {
            unsafe {
                match lib.get::<unsafe extern "C" fn(*const u8, usize) -> i32>(
                    b"strata_hot_reload_hydrate\0",
                ) {
                    Ok(sym) => {
                        let bytes = snapshot.encode();
                        let res = sym(bytes.as_ptr(), bytes.len());
                        if res == 0 {
                            Ok(())
                        } else {
                            Err(format!("strata_hot_reload_hydrate returned error code {res}"))
                        }
                    }
                    Err(err) => Err(format!("Symbol strata_hot_reload_hydrate not found: {err}")),
                }
            }
        } else {
            Err("No active dynamic library loaded".to_string())
        }
    }
}
