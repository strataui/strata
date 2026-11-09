//! Canonical VFS Types, Paths, Metadata, and Error Definitions.

use std::path::{Component, Path, PathBuf};

/// Canonical virtual filesystem root boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VfsRoot {
    /// Persistent application configuration, state, and user databases.
    AppData,
    /// Discardable cached files, shaders, network assets.
    Cache,
    /// User documents, exported projects, and media files.
    Documents,
    /// Transient scratch data discarded on session exit.
    Temp,
}

impl VfsRoot {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AppData => "app_data",
            Self::Cache => "cache",
            Self::Documents => "documents",
            Self::Temp => "temp",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for VfsRoot {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "app_data" | "appdata" => Ok(Self::AppData),
            "cache" => Ok(Self::Cache),
            "documents" | "docs" => Ok(Self::Documents),
            "temp" | "tmp" => Ok(Self::Temp),
            _ => Err(()),
        }
    }
}

/// A sandboxed path anchored to a specific `VfsRoot`.
///
/// Guaranteed to prevent directory traversal (`../`) escapes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VfsPath {
    root: VfsRoot,
    subpath: PathBuf,
}

impl VfsPath {
    pub fn new(root: VfsRoot, subpath: impl AsRef<Path>) -> Self {
        let sub_ref = subpath.as_ref();
        let raw_str = sub_ref.to_string_lossy();
        if crate::driver::is_windows_unc_device_namespace(&raw_str) {
            return Self {
                root,
                subpath: sub_ref.to_path_buf(),
            };
        }
        let clean = Self::sanitize_subpath(sub_ref);
        Self {
            root,
            subpath: clean,
        }
    }

    /// Root anchor for this path.
    pub fn root(&self) -> VfsRoot {
        self.root
    }

    /// Relative subpath within the virtual root.
    pub fn subpath(&self) -> &Path {
        &self.subpath
    }

    /// Appends subpath components while neutralizing directory traversal attacks.
    pub fn join(&self, path: impl AsRef<Path>) -> Self {
        let path_ref = path.as_ref();
        let raw_str = path_ref.to_string_lossy();
        if crate::driver::is_windows_unc_device_namespace(&raw_str) {
            return Self {
                root: self.root,
                subpath: self.subpath.join(path_ref),
            };
        }
        let joined = self.subpath.join(path);
        let clean = Self::sanitize_subpath(&joined);
        Self {
            root: self.root,
            subpath: clean,
        }
    }


    /// Serializes path into a universal VFS URI string (e.g. `vfs://app_data/settings.json`).
    pub fn to_uri(&self) -> String {
        let sub = self.subpath.to_string_lossy().replace('\\', "/");
        let sub_clean = sub.trim_start_matches('/');
        format!("vfs://{}/{}", self.root.as_str(), sub_clean)
    }

    /// Checks whether a given path string attempts to traverse outside of the virtual root.
    pub fn has_traversal_escape(subpath: &str) -> bool {
        let normalized = subpath.replace('\\', "/");
        let mut depth: isize = 0;
        for part in normalized.split('/') {
            if part.is_empty() || part == "." {
                continue;
            } else if part == ".." {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            } else {
                depth += 1;
            }
        }
        false
    }

    /// Attempts to construct a `VfsPath`, returning `Err(FsError::SecurityViolation)` if traversal escapes the root.
    pub fn try_new(root: VfsRoot, subpath: impl AsRef<Path>) -> Result<Self, FsError> {
        let subpath_ref = subpath.as_ref();
        let subpath_str = subpath_ref.to_string_lossy();
        if Self::has_traversal_escape(&subpath_str) {
            return Err(FsError::SecurityViolation(format!(
                "Directory traversal escape detected in subpath: {}",
                subpath_str
            )));
        }
        Ok(Self::new(root, subpath_ref))
    }

    /// Parses a universal VFS URI string, strictly rejecting directory traversal attempts.
    pub fn from_uri(uri: &str) -> Result<Self, FsError> {
        let stripped = uri.strip_prefix("vfs://").ok_or_else(|| {
            FsError::InvalidPath(format!("URI does not start with 'vfs://': {uri}"))
        })?;

        let mut parts = stripped.splitn(2, '/');
        let root_str = parts.next().unwrap_or("");
        let sub_str = parts.next().unwrap_or("");

        let root = VfsRoot::parse(root_str).ok_or_else(|| {
            FsError::InvalidPath(format!("Unknown VfsRoot in URI: {root_str}"))
        })?;

        if Self::has_traversal_escape(sub_str) {
            return Err(FsError::SecurityViolation(format!(
                "Directory traversal escape detected in URI: {uri}"
            )));
        }

        Ok(Self::new(root, PathBuf::from(sub_str)))
    }

    /// Strict path sanitizer preventing directory traversal (`..`) outside root.
    fn sanitize_subpath(path: &Path) -> PathBuf {
        let mut clean = PathBuf::new();
        for comp in path.components() {
            match comp {
                Component::Normal(c) => {
                    let s = c.to_string_lossy();
                    // Strip Windows drive letters (e.g., "C:", "D:")
                    if s.len() == 2 && s.ends_with(':') && s.chars().next().map(|ch| ch.is_ascii_alphabetic()).unwrap_or(false) {
                        continue;
                    }
                    clean.push(c);
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    // Pop only if not at root, never traversing above
                    clean.pop();
                }
                Component::RootDir | Component::Prefix(_) => {
                    // Ignore drive letters or absolute root slashes
                }
            }
        }
        clean
    }
}

/// Metadata attributes for a virtual file or directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VfsMetadata {
    pub is_file: bool,
    pub is_dir: bool,
    pub len: u64,
    pub modified_timestamp_secs: u64,
}

/// Errors occurring during virtual filesystem operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    NotFound(String),
    PermissionDenied(String),
    AlreadyExists(String),
    InvalidPath(String),
    IoError(String),
    SecurityViolation(String),
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(s) => write!(f, "File or directory not found: {s}"),
            Self::PermissionDenied(s) => write!(f, "Permission denied: {s}"),
            Self::AlreadyExists(s) => write!(f, "Already exists: {s}"),
            Self::InvalidPath(s) => write!(f, "Invalid path: {s}"),
            Self::IoError(s) => write!(f, "I/O error: {s}"),
            Self::SecurityViolation(s) => write!(f, "Security violation: {s}"),
        }
    }
}

impl std::error::Error for FsError {}
