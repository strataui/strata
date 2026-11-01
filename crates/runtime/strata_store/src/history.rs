//! Time-Travel History and Undo/Redo Engine for Strata Store.

use std::cell::RefCell;
use std::rc::Rc;
use crate::store::Store;

/// History store managing undo/redo snapshots with bounded buffer depth.
#[derive(Clone)]
pub struct HistoryStore<T: Clone + PartialEq + 'static> {
    pub store: Store<T>,
    past: Rc<RefCell<Vec<T>>>,
    future: Rc<RefCell<Vec<T>>>,
    max_depth: usize,
}

impl<T: Clone + PartialEq + 'static> HistoryStore<T> {
    /// Creates a history store wrapping initial state with an undo depth limit.
    pub fn new(initial: T, max_depth: usize) -> Self {
        Self {
            store: Store::new(initial),
            past: Rc::new(RefCell::new(Vec::new())),
            future: Rc::new(RefCell::new(Vec::new())),
            max_depth,
        }
    }

    /// Read the present state.
    pub fn get(&self) -> T {
        self.store.get()
    }

    /// Mutate state recording the transition in history and clearing redo buffer.
    pub fn set(&self, new_state: T) {
        let current = self.store.get();
        if current != new_state {
            let mut past = self.past.borrow_mut();
            past.push(current);
            if past.len() > self.max_depth {
                past.remove(0);
            }
            self.future.borrow_mut().clear();
            self.store.set(new_state);
        }
    }

    /// Mutate state via closure and record in undo history.
    pub fn update<F: FnOnce(&mut T)>(&self, updater: F) {
        let mut cur = self.get();
        updater(&mut cur);
        self.set(cur);
    }

    /// Checks if undo step is available.
    pub fn can_undo(&self) -> bool {
        !self.past.borrow().is_empty()
    }

    /// Checks if redo step is available.
    pub fn can_redo(&self) -> bool {
        !self.future.borrow().is_empty()
    }

    /// Step backwards to previous state. Returns true if state was restored.
    pub fn undo(&self) -> bool {
        let mut past = self.past.borrow_mut();
        if let Some(prev) = past.pop() {
            let current = self.store.get();
            self.future.borrow_mut().push(current);
            self.store.set(prev);
            true
        } else {
            false
        }
    }

    /// Step forward to next state. Returns true if state was re-applied.
    pub fn redo(&self) -> bool {
        let mut future = self.future.borrow_mut();
        if let Some(next) = future.pop() {
            let current = self.store.get();
            self.past.borrow_mut().push(current);
            self.store.set(next);
            true
        } else {
            false
        }
    }

    /// Clears both undo and redo buffers.
    pub fn clear_history(&self) {
        self.past.borrow_mut().clear();
        self.future.borrow_mut().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_travel_undo_redo() {
        let history = HistoryStore::new(10, 5);

        assert_eq!(history.get(), 10);
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        history.set(20);
        history.set(30);

        assert_eq!(history.get(), 30);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // Undo to 20
        assert!(history.undo());
        assert_eq!(history.get(), 20);
        assert!(history.can_redo());

        // Undo to 10
        assert!(history.undo());
        assert_eq!(history.get(), 10);
        assert!(!history.can_undo());

        // Redo back to 20
        assert!(history.redo());
        assert_eq!(history.get(), 20);

        // Redo back to 30
        assert!(history.redo());
        assert_eq!(history.get(), 30);
        assert!(!history.can_redo());
    }
}
