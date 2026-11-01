//! Filesystem Driver for VfsPath Operations.

use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use crate::resolver::resolve_root_dir;
use crate::types::{FsError, VfsMetadata, VfsPath};

/// Resolves a `VfsPath` into an absolute physical path on the host filesystem.
pub fn to_physical_path(path: &VfsPath) -> PathBuf {
    let base = resolve_root_dir(path.root());
    base.join(path.subpath())
}

/// Checks whether a file or directory exists at `path`.
pub fn exists(path: &VfsPath) -> bool {
    let physical = to_physical_path(path);
    physical.exists()
}

/// Reads the entire contents of a file into a byte vector.
pub fn read_to_vec(path: &VfsPath) -> Result<Vec<u8>, FsError> {
    let physical = to_physical_path(path);
    fs::read(&physical).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound(path.to_uri()),
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(path.to_uri()),
        _ => FsError::IoError(format!("{}: {e}", path.to_uri())),
    })
}

/// Reads the entire contents of a file into a UTF-8 string.
pub fn read_to_string(path: &VfsPath) -> Result<String, FsError> {
    let bytes = read_to_vec(path)?;
    String::from_utf8(bytes).map_err(|e| {
        FsError::IoError(format!("File at {} is not valid UTF-8: {e}", path.to_uri()))
    })
}

/// Writes byte slice to file at `path`, automatically creating parent directories.
pub fn write_all(path: &VfsPath, data: &[u8]) -> Result<(), FsError> {
    let physical = to_physical_path(path);
    if let Some(parent) = physical.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            FsError::IoError(format!("Failed to create parent directory for {}: {e}", path.to_uri()))
        })?;
    }
    fs::write(&physical, data).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(path.to_uri()),
        _ => FsError::IoError(format!("{}: {e}", path.to_uri())),
    })
}

/// Queries metadata for the file or directory at `path`.
pub fn metadata(path: &VfsPath) -> Result<VfsMetadata, FsError> {
    let physical = to_physical_path(path);
    let meta = fs::metadata(&physical).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound(path.to_uri()),
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(path.to_uri()),
        _ => FsError::IoError(format!("{}: {e}", path.to_uri())),
    })?;

    let modified_timestamp_secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    Ok(VfsMetadata {
        is_file: meta.is_file(),
        is_dir: meta.is_dir(),
        len: meta.len(),
        modified_timestamp_secs,
    })
}

/// Removes a file at `path`.
pub fn remove_file(path: &VfsPath) -> Result<(), FsError> {
    let physical = to_physical_path(path);
    fs::remove_file(&physical).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound(path.to_uri()),
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(path.to_uri()),
        _ => FsError::IoError(format!("{}: {e}", path.to_uri())),
    })
}

/// Recursively removes a directory at `path`.
pub fn remove_dir_all(path: &VfsPath) -> Result<(), FsError> {
    let physical = to_physical_path(path);
    if !physical.exists() {
        return Ok(());
    }
    fs::remove_dir_all(&physical).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound(path.to_uri()),
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(path.to_uri()),
        _ => FsError::IoError(format!("{}: {e}", path.to_uri())),
    })
}

/// Creates a directory and all parent directories if missing.
pub fn create_dir_all(path: &VfsPath) -> Result<(), FsError> {
    let physical = to_physical_path(path);
    fs::create_dir_all(&physical).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(path.to_uri()),
        _ => FsError::IoError(format!("{}: {e}", path.to_uri())),
    })
}

/// Reads the directory at `path`, returning all entries as `VfsPath`s.
pub fn read_dir(path: &VfsPath) -> Result<Vec<VfsPath>, FsError> {
    let physical = to_physical_path(path);
    let entries = fs::read_dir(&physical).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound(path.to_uri()),
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(path.to_uri()),
        _ => FsError::IoError(format!("{}: {e}", path.to_uri())),
    })?;

    let mut result = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        result.push(path.join(name));
    }

    Ok(result)
}
