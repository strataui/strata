//! File modification observer detecting source changes.

use std::collections::HashMap;

/// Tracks modified files and triggers hot compilation events.
#[derive(Debug, Default)]
pub struct FileChangeTracker {
    tracked_files: HashMap<String, u64>,
}

impl FileChangeTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or updates a file timestamp. Returns true if modified since last observation.
    pub fn record_file(&mut self, path: impl Into<String>, mtime: u64) -> bool {
        let p = path.into();
        let prev = self.tracked_files.insert(p, mtime);
        match prev {
            Some(old_time) => mtime > old_time,
            None => false, // Initial registration
        }
    }

    pub fn get_mtime(&self, path: &str) -> Option<u64> {
        self.tracked_files.get(path).copied()
    }

    pub fn tracked_count(&self) -> usize {
        self.tracked_files.len()
    }
}
