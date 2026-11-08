//! Filesystem Driver for VfsPath Operations.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;
use crate::resolver::resolve_root_dir;
use crate::types::{FsError, VfsMetadata, VfsPath};

/// Identifies legacy DOS and Windows device names that bypass standard filesystem sandboxes.
pub fn is_windows_reserved_device(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
            | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    )
}

/// Canonicalizes existing ancestors of a path and appends remaining non-existent components.
fn canonicalize_ancestor(path: &Path) -> Result<PathBuf, FsError> {
    let mut ancestor = path;
    let mut remaining = Vec::new();
    while !ancestor.exists() && !ancestor.is_symlink() {
        if let Some(name) = ancestor.file_name() {
            remaining.push(name);
        }
        if let Some(parent) = ancestor.parent() {
            ancestor = parent;
        } else {
            break;
        }
    }
    let mut canonical = if ancestor.exists() {
        fs::canonicalize(ancestor).map_err(|e| {
            FsError::IoError(format!("Failed to canonicalize ancestor {}: {e}", ancestor.display()))
        })?
    } else {
        ancestor.to_path_buf()
    };
    for comp in remaining.into_iter().rev() {
        canonical.push(comp);
    }
    Ok(canonical)
}

/// Resolves a `VfsPath` into a verified absolute physical path on the host filesystem,
/// strictly enforcing that symlinks or path components never escape the virtual root sandbox.
pub fn resolve_secure_physical_path(path: &VfsPath, create_root_if_missing: bool) -> Result<PathBuf, FsError> {
    let base = resolve_root_dir(path.root());

    if !base.exists() {
        if create_root_if_missing {
            fs::create_dir_all(&base).map_err(|e| {
                FsError::IoError(format!("Failed to create sandbox root {}: {e}", base.display()))
            })?;
        } else {
            return Err(FsError::NotFound(path.to_uri()));
        }
    }

    let canonical_base = fs::canonicalize(&base).map_err(|e| {
        FsError::IoError(format!("Failed to canonicalize sandbox root {}: {e}", base.display()))
    })?;

    let mut current = canonical_base.clone();
    for comp in path.subpath().components() {
        match comp {
            Component::Normal(c) => {
                let comp_str = c.to_string_lossy();
                if comp_str.contains(':') || is_windows_reserved_device(&comp_str) || comp_str.contains('\0') {
                    return Err(FsError::SecurityViolation(format!(
                        "Illegal path component '{comp_str}' (Windows drive letter, ADS stream, or reserved DOS device) in sandboxed path: {}",
                        path.to_uri()
                    )));
                }
                current.push(c);
                if current.is_symlink() || current.exists() {
                    let real = if current.is_symlink() {
                        let target = fs::read_link(&current).map_err(|e| {
                            FsError::IoError(format!("Failed to read symlink at {}: {e}", current.display()))
                        })?;
                        let resolved = if target.is_relative() {
                            let parent = current.parent().unwrap_or(&canonical_base);
                            parent.join(target)
                        } else {
                            target
                        };
                        if resolved.exists() {
                            fs::canonicalize(&resolved).map_err(|e| {
                                FsError::IoError(format!("Failed to canonicalize target {}: {e}", resolved.display()))
                            })?
                        } else {
                            canonicalize_ancestor(&resolved)?
                        }
                    } else {
                        fs::canonicalize(&current).map_err(|e| {
                            FsError::IoError(format!("Failed to canonicalize {}: {e}", current.display()))
                        })?
                    };

                    if !real.starts_with(&canonical_base) {
                        return Err(FsError::SecurityViolation(format!(
                            "Symlink or directory traversal outside sandbox detected for '{}': resolved to '{}' which escapes sandbox root '{}'",
                            path.to_uri(),
                            real.display(),
                            canonical_base.display()
                        )));
                    }
                    current = real;
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(FsError::SecurityViolation(format!(
                    "Illegal path component in sandboxed subpath: {}",
                    path.to_uri()
                )));
            }
        }
    }

    if !current.starts_with(&canonical_base) {
        return Err(FsError::SecurityViolation(format!(
            "Path traversal escape detected: resolved to '{}' which is outside sandbox root '{}'",
            current.display(),
            canonical_base.display()
        )));
    }

    Ok(current)
}

/// Resolves a `VfsPath` into an absolute physical path on the host filesystem.
/// Returns `Err(FsError::SecurityViolation)` if the path escapes the virtual sandbox.
pub fn to_physical_path(path: &VfsPath) -> Result<PathBuf, FsError> {
    match resolve_secure_physical_path(path, false) {
        Ok(p) => Ok(p),
        Err(FsError::NotFound(_)) => {
            // Root base directory does not exist on disk yet. Resolve path relative to base
            // while verifying there is no sandbox escape or traversal attempt.
            let base = resolve_root_dir(path.root());
            for comp in path.subpath().components() {
                match comp {
                    Component::Normal(_) | Component::CurDir => {}
                    Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                        return Err(FsError::SecurityViolation(format!(
                            "Illegal path component in sandboxed subpath: {}",
                            path.to_uri()
                        )));
                    }
                }
            }
            Ok(base.join(path.subpath()))
        }
        Err(e) => Err(e),
    }
}

/// Checks whether a file or directory exists at `path`.
pub fn exists(path: &VfsPath) -> bool {
    match resolve_secure_physical_path(path, false) {
        Ok(physical) => physical.exists(),
        Err(_) => false,
    }
}

/// Reads the entire contents of a file into a byte vector.
pub fn read_to_vec(path: &VfsPath) -> Result<Vec<u8>, FsError> {
    let physical = resolve_secure_physical_path(path, false)?;
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
    let physical = resolve_secure_physical_path(path, true)?;
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
    let physical = resolve_secure_physical_path(path, false)?;
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
    let physical = resolve_secure_physical_path(path, false)?;
    fs::remove_file(&physical).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound(path.to_uri()),
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(path.to_uri()),
        _ => FsError::IoError(format!("{}: {e}", path.to_uri())),
    })
}

/// Recursively removes a directory at `path`.
pub fn remove_dir_all(path: &VfsPath) -> Result<(), FsError> {
    let physical = match resolve_secure_physical_path(path, false) {
        Ok(p) => p,
        Err(FsError::NotFound(_)) => return Ok(()),
        Err(e) => return Err(e),
    };
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
    let physical = resolve_secure_physical_path(path, true)?;
    fs::create_dir_all(&physical).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(path.to_uri()),
        _ => FsError::IoError(format!("{}: {e}", path.to_uri())),
    })
}

/// Reads the directory at `path`, returning all entries as `VfsPath`s.
pub fn read_dir(path: &VfsPath) -> Result<Vec<VfsPath>, FsError> {
    let physical = resolve_secure_physical_path(path, false)?;
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
