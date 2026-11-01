//! Universal History and Location State Management.

use std::collections::HashMap;
use crate::matcher::{normalize_path, parse_query_string};

/// Action describing how a navigation event occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryAction {
    Push,
    Replace,
    Pop,
}

/// Represents the current URL location state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub pathname: String,
    pub search: String,
    pub hash: String,
    pub query: HashMap<String, String>,
}

impl Location {
    /// Parse a complete URL path string (e.g., `/users/12?sort=desc#profile`).
    pub fn parse(url: &str) -> Self {
        let mut pathname = url;
        let mut search = "";
        let mut hash = "";

        // Extract hash
        if let Some(pos) = pathname.find('#') {
            hash = &pathname[pos..];
            pathname = &pathname[..pos];
        }

        // Extract query search
        if let Some(pos) = pathname.find('?') {
            search = &pathname[pos..];
            pathname = &pathname[..pos];
        }

        let query = parse_query_string(search);
        let normalized_path = normalize_path(pathname);

        Self {
            pathname: normalized_path,
            search: search.to_string(),
            hash: hash.to_string(),
            query,
        }
    }

    /// Full URL path representation with query and hash.
    pub fn full_url(&self) -> String {
        format!("{}{}{}", self.pathname, self.search, self.hash)
    }
}

/// In-memory history stack implementation.
#[derive(Debug, Clone)]
pub struct MemoryHistory {
    entries: Vec<Location>,
    index: usize,
}

impl MemoryHistory {
    pub fn new(initial_path: &str) -> Self {
        Self {
            entries: vec![Location::parse(initial_path)],
            index: 0,
        }
    }

    pub fn current(&self) -> Location {
        self.entries[self.index].clone()
    }

    pub fn push(&mut self, path: &str) -> (Location, HistoryAction) {
        let loc = Location::parse(path);
        // Truncate forward history if pushing from middle
        self.entries.truncate(self.index + 1);
        self.entries.push(loc.clone());
        self.index += 1;
        (loc, HistoryAction::Push)
    }

    pub fn replace(&mut self, path: &str) -> (Location, HistoryAction) {
        let loc = Location::parse(path);
        self.entries[self.index] = loc.clone();
        (loc, HistoryAction::Replace)
    }

    pub fn back(&mut self) -> Option<(Location, HistoryAction)> {
        if self.index > 0 {
            self.index -= 1;
            Some((self.entries[self.index].clone(), HistoryAction::Pop))
        } else {
            None
        }
    }

    pub fn forward(&mut self) -> Option<(Location, HistoryAction)> {
        if self.index + 1 < self.entries.len() {
            self.index += 1;
            Some((self.entries[self.index].clone(), HistoryAction::Pop))
        } else {
            None
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.index + 1 < self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_parsing() {
        let loc = Location::parse("/settings/profile?theme=dark&notifications=true#security");
        assert_eq!(loc.pathname, "/settings/profile");
        assert_eq!(loc.search, "?theme=dark&notifications=true");
        assert_eq!(loc.hash, "#security");
        assert_eq!(loc.query.get("theme").unwrap(), "dark");
        assert_eq!(loc.query.get("notifications").unwrap(), "true");
        assert_eq!(
            loc.full_url(),
            "/settings/profile?theme=dark&notifications=true#security"
        );
    }

    #[test]
    fn test_memory_history_navigation() {
        let mut hist = MemoryHistory::new("/home");
        assert_eq!(hist.current().pathname, "/home");
        assert!(!hist.can_go_back());

        hist.push("/projects");
        assert_eq!(hist.current().pathname, "/projects");
        assert!(hist.can_go_back());

        hist.push("/projects/42");
        assert_eq!(hist.current().pathname, "/projects/42");

        let (prev, action) = hist.back().unwrap();
        assert_eq!(prev.pathname, "/projects");
        assert_eq!(action, HistoryAction::Pop);

        let (fwd, _) = hist.forward().unwrap();
        assert_eq!(fwd.pathname, "/projects/42");

        hist.replace("/projects/99");
        assert_eq!(hist.current().pathname, "/projects/99");
    }
}
