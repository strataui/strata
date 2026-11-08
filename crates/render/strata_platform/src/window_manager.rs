//! Multi-window desktop manager with parent-child modality, focus arbitration, and event multiplexing.

use crate::events::PlatformEvent;
use crate::window::{MockPlatformWindow, PlatformWindow, WindowConfig};
use std::collections::HashMap;

/// Unique 64-bit identifier for an active platform window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub u64);

impl WindowId {
    pub const MAIN: Self = Self(1);
}

/// Internal tracking entry for a managed window.
struct ManagedWindowEntry {
    #[allow(dead_code)]
    id: WindowId,
    window: Box<dyn PlatformWindow>,
    is_modal: bool,
    parent_id: Option<WindowId>,
    children: Vec<WindowId>,
}

/// Multi-window desktop manager coordinating multiple concurrent windows.
pub struct WindowManager {
    windows: HashMap<u64, ManagedWindowEntry>,
    active_window: Option<WindowId>,
    next_id: u64,
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowManager {
    /// Creates a new empty window manager.
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            active_window: None,
            next_id: 1,
        }
    }

    /// Spawns a new independent top-level window with the specified configuration.
    pub fn create_window(&mut self, config: WindowConfig) -> WindowId {
        let mock = Box::new(MockPlatformWindow::new(config));
        self.register_window(mock)
    }

    /// Registers an existing platform window instance into the manager.
    pub fn register_window(&mut self, window: Box<dyn PlatformWindow>) -> WindowId {
        let id = WindowId(self.next_id);
        self.next_id += 1;

        let entry = ManagedWindowEntry {
            id,
            window,
            is_modal: false,
            parent_id: None,
            children: Vec::new(),
        };

        self.windows.insert(id.0, entry);

        if self.active_window.is_none() {
            self.active_window = Some(id);
        }

        id
    }

    /// Spawns a modal child dialog window attached to a specified parent window.
    pub fn create_modal(
        &mut self,
        parent: WindowId,
        config: WindowConfig,
    ) -> Result<WindowId, String> {
        let mock = Box::new(MockPlatformWindow::new(config));
        self.register_modal(parent, mock)
    }

    /// Registers a modal platform window attached to a parent window.
    pub fn register_modal(
        &mut self,
        parent: WindowId,
        window: Box<dyn PlatformWindow>,
    ) -> Result<WindowId, String> {
        if !self.windows.contains_key(&parent.0) {
            return Err(format!("Parent window {:?} does not exist", parent));
        }

        let id = WindowId(self.next_id);
        self.next_id += 1;

        let entry = ManagedWindowEntry {
            id,
            window,
            is_modal: true,
            parent_id: Some(parent),
            children: Vec::new(),
        };

        self.windows.insert(id.0, entry);

        if let Some(parent_entry) = self.windows.get_mut(&parent.0) {
            parent_entry.children.push(id);
        }

        // Focus the modal window
        self.active_window = Some(id);

        Ok(id)
    }

    /// Closes a managed window, unblocking any parents and dismissing descendants.
    pub fn close_window(&mut self, id: WindowId) -> Result<(), String> {
        let entry = self
            .windows
            .remove(&id.0)
            .ok_or_else(|| format!("Window {:?} not found", id))?;

        // Remove from parent's children list
        if let Some(parent_id) = entry.parent_id {
            if let Some(parent) = self.windows.get_mut(&parent_id.0) {
                parent.children.retain(|&c| c != id);
            }
        }

        // Recursively close all children
        for child_id in entry.children {
            let _ = self.close_window(child_id);
        }

        // If this was active window, transfer focus to parent or any remaining window
        if self.active_window == Some(id) {
            self.active_window = entry
                .parent_id
                .filter(|p| self.windows.contains_key(&p.0))
                .or_else(|| self.windows.keys().next().map(|&k| WindowId(k)));
        }

        Ok(())
    }

    /// Returns a reference to the window with the given ID.
    pub fn get_window(&self, id: WindowId) -> Option<&dyn PlatformWindow> {
        self.windows.get(&id.0).map(|e| e.window.as_ref())
    }

    /// Returns a mutable reference to the window with the given ID.
    pub fn get_window_mut(&mut self, id: WindowId) -> Option<&mut (dyn PlatformWindow + 'static)> {
        self.windows.get_mut(&id.0).map(|e| e.window.as_mut())
    }

    /// Returns the active window ID currently possessing focus.
    pub fn active_window(&self) -> Option<WindowId> {
        self.active_window
    }

    /// Sets the active focused window ID.
    pub fn set_active_window(&mut self, id: WindowId) -> Result<(), String> {
        if !self.windows.contains_key(&id.0) {
            return Err(format!("Window {:?} not found", id));
        }

        // Check if modal blocked
        if self.is_modal_blocked(id) {
            return Err(format!(
                "Window {:?} cannot receive focus while blocked by a modal dialog",
                id
            ));
        }

        // Update focus flags
        for (wid, entry) in self.windows.iter_mut() {
            entry.window.set_focused(*wid == id.0);
        }

        self.active_window = Some(id);
        Ok(())
    }

    /// Checks if a window is blocked from interaction by an active modal child.
    pub fn is_modal_blocked(&self, id: WindowId) -> bool {
        if let Some(entry) = self.windows.get(&id.0) {
            entry.children.iter().any(|&child_id| {
                self.windows
                    .get(&child_id.0)
                    .is_some_and(|child| child.is_modal)
            })
        } else {
            false
        }
    }

    /// Total number of open windows.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Dispatches an event to all open non-blocked windows (e.g. system theme changes).
    pub fn broadcast_event(&mut self, event: PlatformEvent) {
        for entry in self.windows.values_mut() {
            entry.window.push_event(event.clone());
        }
    }

    /// Polls all window event queues and collects pending events labeled by WindowId.
    pub fn poll_all_events(&mut self) -> Vec<(WindowId, PlatformEvent)> {
        let mut all_events = Vec::new();
        for (&id, entry) in self.windows.iter_mut() {
            let events = entry.window.poll_events();
            for ev in events {
                all_events.push((WindowId(id), ev));
            }
        }
        all_events
    }
}
