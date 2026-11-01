//! Cross-Platform Physical Directory Resolution for VfsRoot.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::RwLock;
use crate::types::VfsRoot;

static APP_ID: RwLock<Option<String>> = RwLock::new(None);
static TEST_OVERRIDES: RwLock<Option<HashMap<VfsRoot, PathBuf>>> = RwLock::new(None);

/// Sets the application identifier used in directory naming (e.g. `com.example.app`).
pub fn set_app_id(id: impl Into<String>) {
    if let Ok(mut lock) = APP_ID.write() {
        *lock = Some(id.into());
    }
}

/// Retrieves the current application identifier.
pub fn get_app_id() -> String {
    if let Ok(lock) = APP_ID.read() {
        if let Some(ref id) = *lock {
            return id.clone();
        }
    }
    String::from("strata_app")
}

/// Sets a test directory override for a specific virtual root (for hermetic unit testing).
pub fn set_test_override(root: VfsRoot, dir: PathBuf) {
    if let Ok(mut lock) = TEST_OVERRIDES.write() {
        let map = lock.get_or_insert_with(HashMap::new);
        map.insert(root, dir);
    }
}

/// Clears a specific active test override.
pub fn clear_test_override(root: VfsRoot) {
    if let Ok(mut lock) = TEST_OVERRIDES.write() {
        if let Some(ref mut map) = *lock {
            map.remove(&root);
        }
    }
}

/// Clears any active test overrides.
pub fn clear_test_overrides() {
    if let Ok(mut lock) = TEST_OVERRIDES.write() {
        *lock = None;
    }
}

/// Resolves the physical base directory for a given `VfsRoot`.
pub fn resolve_root_dir(root: VfsRoot) -> PathBuf {
    // Check test override first
    if let Ok(lock) = TEST_OVERRIDES.read() {
        if let Some(ref map) = *lock {
            if let Some(dir) = map.get(&root) {
                return dir.clone();
            }
        }
    }

    let app_id = get_app_id();

    #[cfg(target_os = "windows")]
    {
        resolve_windows(root, &app_id)
    }

    #[cfg(target_os = "macos")]
    {
        resolve_macos(root, &app_id)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        resolve_linux(root, &app_id)
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn resolve_linux(root: VfsRoot, app_id: &str) -> PathBuf {
    let home = env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/tmp"));

    match root {
        VfsRoot::AppData => {
            if let Ok(xdg_data) = env::var("XDG_DATA_HOME") {
                PathBuf::from(xdg_data).join(app_id)
            } else {
                home.join(".local/share").join(app_id)
            }
        }
        VfsRoot::Cache => {
            if let Ok(xdg_cache) = env::var("XDG_CACHE_HOME") {
                PathBuf::from(xdg_cache).join(app_id)
            } else {
                home.join(".cache").join(app_id)
            }
        }
        VfsRoot::Documents => {
            home.join("Documents")
        }
        VfsRoot::Temp => {
            if let Ok(tmp) = env::var("TMPDIR") {
                PathBuf::from(tmp).join(app_id)
            } else {
                PathBuf::from("/tmp").join(app_id)
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn resolve_macos(root: VfsRoot, app_id: &str) -> PathBuf {
    let home = env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/tmp"));

    match root {
        VfsRoot::AppData => {
            home.join("Library/Application Support").join(app_id)
        }
        VfsRoot::Cache => {
            home.join("Library/Caches").join(app_id)
        }
        VfsRoot::Documents => {
            home.join("Documents")
        }
        VfsRoot::Temp => {
            if let Ok(tmp) = env::var("TMPDIR") {
                PathBuf::from(tmp).join(app_id)
            } else {
                PathBuf::from("/tmp").join(app_id)
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn resolve_windows(root: VfsRoot, app_id: &str) -> PathBuf {
    let local_app_data = env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| {
        env::var("USERPROFILE").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("C:\\"))
    });
    let user_profile = env::var("USERPROFILE").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("C:\\"));

    match root {
        VfsRoot::AppData => local_app_data.join(app_id),
        VfsRoot::Cache => local_app_data.join(app_id).join("Cache"),
        VfsRoot::Documents => user_profile.join("Documents"),
        VfsRoot::Temp => {
            if let Ok(tmp) = env::var("TEMP") {
                PathBuf::from(tmp).join(app_id)
            } else {
                local_app_data.join("Temp").join(app_id)
            }
        }
    }
}
