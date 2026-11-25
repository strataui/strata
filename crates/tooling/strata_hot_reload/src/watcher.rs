//! File system watcher for source code changes driving incremental HMR recompilation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Lightweight hash-based tracker for unit tests and virtual change simulation.
#[derive(Debug, Default)]
pub struct FileChangeTracker {
    file_hashes: HashMap<PathBuf, u64>,
}

impl FileChangeTracker {
    /// Creates an empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a file's modification timestamp. Returns `true` only if an already-tracked file
    /// has a strictly newer timestamp (`mtime > prev`).
    pub fn record_file(&mut self, path: impl Into<PathBuf>, mtime: u64) -> bool {
        let p = path.into();
        match self.file_hashes.get(&p).copied() {
            None => {
                self.file_hashes.insert(p, mtime);
                false
            }
            Some(prev) if mtime > prev => {
                self.file_hashes.insert(p, mtime);
                true
            }
            Some(_) => false,
        }
    }

    /// Returns the last recorded modification timestamp/hash for `path`, if tracked.
    pub fn get_mtime(&self, path: impl AsRef<Path>) -> Option<u64> {
        self.file_hashes.get(path.as_ref()).copied()
    }

    /// Registers or updates a file's content hash. Returns `true` if modified or newly observed.
    pub fn check_modified(&mut self, path: impl Into<PathBuf>, content_hash: u64) -> bool {
        let p = path.into();
        match self.file_hashes.get(&p) {
            Some(&prev) if prev == content_hash => false,
            _ => {
                self.file_hashes.insert(p, content_hash);
                true
            }
        }
    }

    /// Returns number of tracked source files.
    pub fn tracked_count(&self) -> usize {
        self.file_hashes.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified: SystemTime,
    len: u64,
}

/// Real recursive, debounced filesystem watcher monitoring project sources (`.rs`, `.css`, `.toml`)
/// while ignoring build artifacts (`target/`, `.git/`, `node_modules/`).
pub struct DirectoryWatcher {
    roots: Vec<PathBuf>,
    extensions: Vec<String>,
    debounce: Duration,
    stamps: HashMap<PathBuf, FileStamp>,
    pending_changed: Vec<PathBuf>,
    last_event_at: Option<Instant>,
}

impl DirectoryWatcher {
    /// Creates a new directory watcher rooted at `root` with a default `80ms` debounce window.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let mut watcher = Self {
            roots: vec![root.into()],
            extensions: vec!["rs".to_string(), "css".to_string(), "toml".to_string()],
            debounce: Duration::from_millis(80),
            stamps: HashMap::new(),
            pending_changed: Vec::new(),
            last_event_at: None,
        };
        watcher.seed_initial_snapshot();
        watcher
    }

    /// Overrides the debounce quiet window before emitting a consolidated change batch.
    pub fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce = debounce;
        self
    }

    /// Adds an extra root directory or file to monitor.
    pub fn add_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.roots.push(root.into());
        self.seed_initial_snapshot();
        self
    }

    /// Overrides the watched file extensions (lowercase, without leading dot).
    pub fn with_extensions(mut self, exts: &[&str]) -> Self {
        self.extensions = exts.iter().map(|s| s.to_ascii_lowercase()).collect();
        self.stamps.clear();
        self.seed_initial_snapshot();
        self
    }

    /// Returns the number of source files currently indexed.
    pub fn tracked_count(&self) -> usize {
        self.stamps.len()
    }

    fn seed_initial_snapshot(&mut self) {
        let mut current = HashMap::new();
        for root in &self.roots {
            Self::scan_dir(root, &self.extensions, &mut current);
        }
        self.stamps = current;
    }

    /// Polls the filesystem for modifications, creations, or deletions and returns a debounced
    /// batch of changed paths once edits settle for `self.debounce`.
    pub fn poll_changes(&mut self) -> Vec<PathBuf> {
        let mut current = HashMap::new();
        for root in &self.roots {
            Self::scan_dir(root, &self.extensions, &mut current);
        }

        let mut newly_changed = Vec::new();
        for (path, stamp) in &current {
            match self.stamps.get(path) {
                Some(prev) if prev == stamp => {}
                _ => newly_changed.push(path.clone()),
            }
        }
        for old_path in self.stamps.keys() {
            if !current.contains_key(old_path) {
                newly_changed.push(old_path.clone());
            }
        }
        self.stamps = current;

        if !newly_changed.is_empty() {
            for p in newly_changed {
                if !self.pending_changed.contains(&p) {
                    self.pending_changed.push(p);
                }
            }
            self.last_event_at = Some(Instant::now());
        }

        if let Some(last) = self.last_event_at {
            if !self.pending_changed.is_empty() && last.elapsed() >= self.debounce {
                self.last_event_at = None;
                let mut batch = std::mem::take(&mut self.pending_changed);
                batch.sort();
                return batch;
            }
        }

        Vec::new()
    }

    fn scan_dir(path: &Path, extensions: &[String], out: &mut HashMap<PathBuf, FileStamp>) {
        if !path.exists() {
            return;
        }
        if path.is_file() {
            Self::record_file(path, extensions, out);
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) {
                if matches!(name, "target" | ".git" | "node_modules" | "dist" | ".cargo") {
                    continue;
                }
            }
            if entry_path.is_dir() {
                Self::scan_dir(&entry_path, extensions, out);
            } else if entry_path.is_file() {
                Self::record_file(&entry_path, extensions, out);
            }
        }
    }

    fn record_file(path: &Path, extensions: &[String], out: &mut HashMap<PathBuf, FileStamp>) {
        let ext_ok = extensions.is_empty()
            || path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| extensions.iter().any(|allowed| allowed.eq_ignore_ascii_case(e)))
                .unwrap_or(false);
        if !ext_ok {
            return;
        }
        if let Ok(meta) = std::fs::metadata(path) {
            if let Ok(modified) = meta.modified() {
                out.insert(
                    path.to_path_buf(),
                    FileStamp {
                        modified,
                        len: meta.len(),
                    },
                );
            }
        }
    }
}
