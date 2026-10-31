//! Reactive Toast Manager & Global Imperative Dispatcher.

use std::sync::atomic::{AtomicU64, Ordering};
use strata_dom::Signal;
use crate::stack::{compute_stack_layout, StackedToastVisual};
use crate::toast::{ToastItem, ToastKind, ToastPosition};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Toast state manager coordinating notifications, auto-expiry, and reactive signals.
#[derive(Clone)]
pub struct ToastManager {
    pub toasts: Signal<Vec<ToastItem>>,
    pub position: Signal<ToastPosition>,
    pub max_visible: usize,
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastManager {
    /// Creates a new toast manager.
    pub fn new() -> Self {
        Self {
            toasts: Signal::new(Vec::new()),
            position: Signal::new(ToastPosition::BottomRight),
            max_visible: 3,
        }
    }

    /// Set screen placement position.
    pub fn set_position(&self, pos: ToastPosition) {
        self.position.set(pos);
    }

    /// Add a pre-configured toast notification to the stack.
    pub fn show(&self, mut item: ToastItem) -> u64 {
        if item.id == 0 {
            item.id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        }
        let id = item.id;
        let mut list = self.toasts.get();
        // Newest toasts at front
        list.insert(0, item);
        self.toasts.set(list);
        id
    }

    /// Shows a standard informational toast.
    pub fn message(&self, title: impl Into<String>) -> u64 {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        self.show(ToastItem::new(id, title, ToastKind::Default))
    }

    /// Shows a success toast with green indicator.
    pub fn success(&self, title: impl Into<String>) -> u64 {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        self.show(ToastItem::new(id, title, ToastKind::Success))
    }

    /// Shows an error toast with red indicator.
    pub fn error(&self, title: impl Into<String>) -> u64 {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        self.show(ToastItem::new(id, title, ToastKind::Error))
    }

    /// Shows an informational toast with blue indicator.
    pub fn info(&self, title: impl Into<String>) -> u64 {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        self.show(ToastItem::new(id, title, ToastKind::Info))
    }

    /// Shows a warning toast with yellow/amber indicator.
    pub fn warning(&self, title: impl Into<String>) -> u64 {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        self.show(ToastItem::new(id, title, ToastKind::Warning))
    }

    /// Shows a persistent loading spinner toast.
    pub fn loading(&self, title: impl Into<String>) -> u64 {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let mut item = ToastItem::new(id, title, ToastKind::Loading);
        item.duration_ms = 0; // Does not auto-dismiss
        self.show(item)
    }

    /// Updates an existing toast (e.g. resolving a loading toast into success or error).
    pub fn update(&self, id: u64, kind: ToastKind, title: impl Into<String>) -> bool {
        let mut list = self.toasts.get();
        if let Some(pos) = list.iter().position(|t| t.id == id) {
            list[pos].kind = kind;
            list[pos].title = title.into();
            if kind != ToastKind::Loading && list[pos].duration_ms == 0 {
                list[pos].duration_ms = 4000;
            }
            self.toasts.set(list);
            true
        } else {
            false
        }
    }

    /// Dismiss a specific toast by ID.
    pub fn dismiss(&self, id: u64) -> bool {
        let mut list = self.toasts.get();
        let initial_len = list.len();
        list.retain(|t| t.id != id);
        let removed = list.len() < initial_len;
        if removed {
            self.toasts.set(list);
        }
        removed
    }

    /// Clear all active toasts immediately.
    pub fn dismiss_all(&self) {
        self.toasts.set(Vec::new());
    }

    /// Calculates current stacked visual coordinates for active toasts.
    pub fn stacked_toasts(&self) -> Vec<StackedToastVisual> {
        let items = self.toasts.get();
        let pos = self.position.get();
        compute_stack_layout(&items, pos, self.max_visible)
    }
}

thread_local! {
    static DEFAULT_MANAGER: ToastManager = ToastManager::new();
}

/// Dispatches a global success toast.
pub fn toast_success(title: impl Into<String>) -> u64 {
    DEFAULT_MANAGER.with(|m| m.success(title))
}

/// Dispatches a global error toast.
pub fn toast_error(title: impl Into<String>) -> u64 {
    DEFAULT_MANAGER.with(|m| m.error(title))
}

/// Dispatches a global info toast.
pub fn toast_info(title: impl Into<String>) -> u64 {
    DEFAULT_MANAGER.with(|m| m.info(title))
}

/// Dispatches a global warning toast.
pub fn toast_warning(title: impl Into<String>) -> u64 {
    DEFAULT_MANAGER.with(|m| m.warning(title))
}

/// Dismisses a global toast by ID.
pub fn toast_dismiss(id: u64) {
    DEFAULT_MANAGER.with(|m| {
        m.dismiss(id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_manager_lifecycle_and_stack() {
        let manager = ToastManager::new();

        let id1 = manager.info("Welcome to Strata");
        let id2 = manager.success("Workspace build succeeded");
        assert_eq!(manager.toasts.get().len(), 2);

        // id2 is newest (front)
        assert_eq!(manager.toasts.get()[0].id, id2);

        // Update id1 to error
        assert!(manager.update(id1, ToastKind::Error, "Connection failed"));
        assert_eq!(manager.toasts.get()[1].kind, ToastKind::Error);

        // Check stacked layout
        let stack = manager.stacked_toasts();
        assert_eq!(stack.len(), 2);
        assert_eq!(stack[0].item.id, id2);
        assert_eq!(stack[0].offset_y, 0.0);

        // Dismiss id2
        assert!(manager.dismiss(id2));
        assert_eq!(manager.toasts.get().len(), 1);
        assert_eq!(manager.toasts.get()[0].id, id1);

        // Dismiss all
        manager.dismiss_all();
        assert!(manager.toasts.get().is_empty());
    }
}
