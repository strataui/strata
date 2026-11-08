//! Strata VFS: Universal, Sandboxed Virtual File System with Zero `#[cfg]` in User Code.

pub mod types;
pub mod resolver;
pub mod driver;

pub use types::{FsError, VfsMetadata, VfsPath, VfsRoot};
pub use resolver::{clear_test_override, clear_test_overrides, get_app_id, set_app_id, set_test_override};
pub use driver::{
    create_dir_all, exists, metadata, read_dir, read_to_string, read_to_vec, remove_dir_all,
    remove_file, resolve_secure_physical_path, to_physical_path, write_all,
};

/// Root anchor for persistent application state and configurations.
pub fn app_data() -> VfsPath {
    VfsPath::new(VfsRoot::AppData, "")
}

/// Root anchor for discardable caches and assets.
pub fn cache() -> VfsPath {
    VfsPath::new(VfsRoot::Cache, "")
}

/// Root anchor for user documents and media files.
pub fn documents() -> VfsPath {
    VfsPath::new(VfsRoot::Documents, "")
}

/// Root anchor for transient session scratch files.
pub fn temp() -> VfsPath {
    VfsPath::new(VfsRoot::Temp, "")
}

pub mod prelude {
    pub use super::types::{FsError, VfsMetadata, VfsPath, VfsRoot};
    pub use super::{
        app_data, cache, create_dir_all, documents, exists, metadata, read_dir, read_to_string,
        read_to_vec, remove_dir_all, remove_file, temp, write_all,
    };
}
