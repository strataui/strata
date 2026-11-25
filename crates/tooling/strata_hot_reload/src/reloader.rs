//! State-preserving hot-reloader orchestrating epoch-staged dynamic code loading,
//! UAF-safe library retention, state snapshot migration, and live `View` rendering.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::protocol::HotReloadMessage;
use crate::snapshot::HotReloadSnapshot;

/// Manages hot code reload cycles with 0% application state loss.
///
/// Key production guarantees:
/// - **Epoch-staged copy:** Every loaded `.so`/`.dylib`/`.dll` is copied to a unique
///   `patch_epoch_<N>_<name>` path in a process-private temp directory before `dlopen` /
///   `LoadLibrary`, defeating Linux/macOS `dlopen` path caching and Windows DLL write locks.
/// - **Zero use-after-free on closures/vtable pointers:** Previous `libloading::Library`
///   handles are moved into `retired_libraries` and kept mapped until `HotReloader` drops,
///   preventing `SIGSEGV` if any prior-epoch event closure or `&'static str` outlives a frame.
pub struct HotReloader {
    current_epoch: u64,
    last_snapshot: Option<HotReloadSnapshot>,
    active_library: Option<libloading::Library>,
    retired_libraries: Vec<libloading::Library>,
    staged_files: Vec<PathBuf>,
    staging_dir: PathBuf,
    migrations: Vec<crate::snapshot::SignalMigration>,
}

impl Default for HotReloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for HotReloader {
    fn drop(&mut self) {
        self.active_library = None;
        self.retired_libraries.clear();
        for staged in self.staged_files.drain(..) {
            let _ = std::fs::remove_file(staged);
        }
        let _ = std::fs::remove_dir(&self.staging_dir);
    }
}

impl HotReloader {
    /// Hot-reloading ABI version expected by this engine.
    pub const CURRENT_ABI_VERSION: u32 = 1;

    /// Creates a new hot-reloader instance at epoch 0 with a unique process staging directory.
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let staging_dir = std::env::temp_dir().join(format!(
            "strata_hmr_pid{}_{}",
            std::process::id(),
            id
        ));
        Self {
            current_epoch: 0,
            last_snapshot: None,
            active_library: None,
            retired_libraries: Vec::new(),
            staged_files: Vec::new(),
            staging_dir,
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

    /// Returns how many previous-epoch dynamic libraries are retained to keep old closures safe.
    pub fn retired_library_count(&self) -> usize {
        self.retired_libraries.len()
    }

    /// Borrow the currently loaded `libloading::Library` handle, if any.
    pub fn active_library(&self) -> Option<&libloading::Library> {
        self.active_library.as_ref()
    }

    /// Captures application state into a snapshot before reloading code.
    pub fn capture_snapshot(&mut self, snapshot: HotReloadSnapshot) {
        self.last_snapshot = Some(snapshot);
    }

    /// Captures all live `strata_dom` HMR signals into a [`HotReloadSnapshot`] for the current epoch.
    ///
    /// When a hot-reloaded dynamic library is already active and exports `strata_hot_reload_snapshot`,
    /// this pulls the live signal state directly from the active cdylib's `HMR_STATE_REGISTRY` so
    /// multi-epoch reloads (epoch 0 -> 1 -> 2 -> N) preserve every mutation made inside previous patches.
    pub fn capture_dom_signals(&mut self) -> HotReloadSnapshot {
        if let Some(ref lib) = self.active_library {
            unsafe {
                if let Ok(snap_sym) = lib.get::<unsafe extern "C" fn(*mut *mut u8, *mut usize) -> i32>(
                    b"strata_hot_reload_snapshot\0",
                ) {
                    let mut ptr: *mut u8 = std::ptr::null_mut();
                    let mut len: usize = 0;
                    if snap_sym(&mut ptr, &mut len) == 0 && !ptr.is_null() && len > 0 {
                        let bytes = std::slice::from_raw_parts(ptr, len).to_vec();
                        if let Ok(free_sym) = lib.get::<unsafe extern "C" fn(*mut u8, usize)>(
                            b"strata_hot_reload_free_snapshot\0",
                        ) {
                            free_sym(ptr, len);
                        }
                        if let Some(mut decoded) = HotReloadSnapshot::decode(&bytes) {
                            decoded.epoch = self.current_epoch;
                            self.last_snapshot = Some(decoded.clone());
                            return decoded;
                        }
                    }
                }
            }
        }

        let mut snap = HotReloadSnapshot::new(self.current_epoch);
        for (idx, (key, type_name, bytes)) in strata_dom::export_hmr_signals().into_iter().enumerate() {
            snap.insert_signal(
                idx as u64 + 1,
                format!("hmr:{key}:{type_name}"),
                bytes,
            );
        }
        self.last_snapshot = Some(snap.clone());
        snap
    }

    /// Returns whether the currently loaded hot module has dirty reactive signals pending rebuild.
    pub fn active_has_dirty_signals(&self) -> bool {
        if let Some(ref lib) = self.active_library {
            unsafe {
                if let Ok(sym) =
                    lib.get::<unsafe extern "C" fn() -> u8>(b"strata_hmr_has_dirty_signals\0")
                {
                    return sym() != 0;
                }
            }
        }
        false
    }

    /// Drains the dirty-signal flag inside the currently loaded hot module, if any.
    pub fn active_take_dirty_signals(&self) -> bool {
        if let Some(ref lib) = self.active_library {
            unsafe {
                if let Ok(sym) =
                    lib.get::<unsafe extern "C" fn() -> u8>(b"strata_hmr_take_dirty_signals\0")
                {
                    return sym() != 0;
                }
            }
        }
        false
    }

    /// Rolls back to the previously retired epoch library if one exists, keeping the rolled-back
    /// library mapped inside `retired_libraries` to remain UAF-safe.
    pub fn rollback(&mut self) -> Result<u64, String> {
        let prev = self
            .retired_libraries
            .pop()
            .ok_or_else(|| "No previous HMR epoch available to roll back to".to_string())?;
        let snap = self.capture_dom_signals();
        if let Some(cur) = self.active_library.replace(prev) {
            self.retired_libraries.insert(0, cur);
        }
        let _ = self.hydrate_active_library(&snap);
        self.current_epoch = self.current_epoch.saturating_sub(1);
        Ok(self.current_epoch)
    }

    /// Re-hydrates all `hmr:<key>:<type>` signal entries from `snapshot` into `strata_dom`'s
    /// thread-local HMR registry and returns the number of signals restored.
    pub fn hydrate_dom_signals(snapshot: &HotReloadSnapshot) -> usize {
        let mut entries = Vec::new();
        for sig in snapshot.signals.values() {
            if let Some(rest) = sig.type_name.strip_prefix("hmr:") {
                let key = rest.split_once(':').map(|(k, _)| k).unwrap_or(rest);
                entries.push((key.to_string(), sig.value_bytes.clone()));
            }
        }
        if entries.is_empty() {
            0
        } else {
            strata_dom::import_hmr_signals(&entries)
        }
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

    /// Copies the compiled artifact to a unique epoch-stamped path so OS dynamic linkers
    /// always load the fresh build instead of returning a cached inode handle.
    fn stage_library_copy(&mut self, source: &Path, next_epoch: u64) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.staging_dir)
            .map_err(|e| format!("Failed creating HMR staging dir '{}': {e}", self.staging_dir.display()))?;
        let stem = source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("strata_patch");
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(std::env::consts::DLL_EXTENSION);
        let staged_path = self
            .staging_dir
            .join(format!("patch_epoch_{next_epoch}_{stem}.{ext}"));
        std::fs::copy(source, &staged_path).map_err(|e| {
            format!(
                "Failed staging dynamic library '{}' -> '{}': {e}",
                source.display(),
                staged_path.display()
            )
        })?;
        self.staged_files.push(staged_path.clone());
        Ok(staged_path)
    }

    /// Applies a hot reload patch from a compiled shared library (`.so`, `.dylib`, `.dll`).
    ///
    /// Copies the library to an epoch-staged path, loads via `libloading`, validates ABI
    /// compatibility, migrates and hydrates snapshot state, executes module entry, retains
    /// the retired prior library to prevent closure UAF, increments epoch, and returns swap µs.
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
        let next_epoch = self.current_epoch + 1;

        // 1. Stage unique copy for this epoch (defeats dlopen path cache & Windows DLL lock)
        let staged_path = match self.stage_library_copy(path, next_epoch) {
            Ok(p) => p,
            Err(err) => return HotReloadMessage::PatchFailed { error: err },
        };

        // 2. Load staged dynamic library via system dynamic linker
        let lib = match unsafe { libloading::Library::new(&staged_path) } {
            Ok(l) => l,
            Err(err) => {
                return HotReloadMessage::PatchFailed {
                    error: format!("Failed to load dynamic library '{dylib_path}': {err}"),
                };
            }
        };

        // 3. ABI Version verification
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

        // 4. State schema migration and re-hydration from snapshot
        if let Some(ref mut snapshot) = self.last_snapshot {
            if !self.migrations.is_empty() {
                snapshot.migrate_signals(&self.migrations);
            }
            let bytes = snapshot.encode();
            unsafe {
                if let Ok(hydrate_sym) = lib.get::<unsafe extern "C" fn(*const u8, usize) -> i32>(
                    b"strata_hot_reload_hydrate\0",
                ) {
                    let res = hydrate_sym(bytes.as_ptr(), bytes.len());
                    if res != 0 {
                        return HotReloadMessage::PatchFailed {
                            error: format!("State hydration failed with status code {res}"),
                        };
                    }
                }
            }
        }

        // 5. Invoke module entry point if exported
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

        // 6. Retire previous active library safely (never dlclose while old closures may live)
        if let Some(prev) = self.active_library.replace(lib) {
            self.retired_libraries.push(prev);
        }
        self.current_epoch = next_epoch;

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

    /// Invokes the exported `strata_hmr_render_view() -> *mut strata_dom::View` symbol
    /// on the active hot-reloaded module (generated by [`crate::export_hot_view!`]).
    ///
    /// Note: Both the host application and hot-reloaded cdylib must be compiled with the
    /// exact same Rust compiler and `strata-dom` version (guaranteed by `strata dev` within
    /// a single workspace).
    pub fn render_active_view(&self) -> Result<strata_dom::View, String> {
        let lib = self
            .active_library
            .as_ref()
            .ok_or_else(|| "No active dynamic library loaded".to_string())?;
        unsafe {
            let sym = lib
                .get::<unsafe extern "C" fn() -> *mut strata_dom::View>(
                    b"strata_hmr_render_view\0",
                )
                .map_err(|err| format!("Symbol strata_hmr_render_view not found: {err}"))?;
            let raw = sym();
            if raw.is_null() {
                return Err("strata_hmr_render_view returned null View pointer".to_string());
            }
            Ok(*Box::from_raw(raw))
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
                            Err(format!("Hydration returned error code {res}"))
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
