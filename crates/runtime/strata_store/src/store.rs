//! Core Signal-Driven Reactive Global State Store.

use std::cell::RefCell;
use std::rc::Rc;
use strata_dom::Signal;

type Listener<T> = Box<dyn Fn(&T)>;

/// Global reactive state container with atomic signal updates and slice selectors.
#[derive(Clone)]
pub struct Store<T: Clone + PartialEq + 'static> {
    pub state: Signal<T>,
    listeners: Rc<RefCell<Vec<Rc<Listener<T>>>>>,
}

impl<T: Clone + PartialEq + 'static> Store<T> {
    /// Creates a new reactive store initialized with initial state.
    pub fn new(initial: T) -> Self {
        Self {
            state: Signal::new(initial),
            listeners: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Read the current snapshot of store state.
    pub fn get(&self) -> T {
        self.state.get()
    }

    /// Replace store state and notify subscribers.
    pub fn set(&self, next: T) {
        if next != self.state.get() {
            self.state.set(next.clone());
            let callbacks = self.listeners.borrow().clone();
            for cb in callbacks {
                cb(&next);
            }
        }
    }

    /// Mutate state in-place using a closure and trigger reactive updates.
    pub fn update<F: FnOnce(&mut T)>(&self, updater: F) {
        let mut cur = self.state.get();
        updater(&mut cur);
        self.set(cur);
    }

    /// Slice selector: derives a fine-grained sub-signal that only emits updates
    /// when the selected property value changes.
    pub fn select<R: Clone + PartialEq + 'static, F: Fn(&T) -> R + 'static>(&self, selector: F) -> Signal<R> {
        let initial_slice = selector(&self.state.get());
        let slice_signal = Signal::new(initial_slice);

        let s_signal = slice_signal.clone();
        self.subscribe(move |new_state| {
            let next_slice = selector(new_state);
            if next_slice != s_signal.get() {
                s_signal.set(next_slice);
            }
        });

        slice_signal
    }

    /// Subscribes an imperative callback invoked whenever store state changes.
    pub fn subscribe<F: Fn(&T) + 'static>(&self, listener: F) {
        self.listeners.borrow_mut().push(Rc::new(Box::new(listener)));
    }
}

/// Convenience hook to create a reactive state store.
pub fn create_store<T: Clone + PartialEq + 'static>(initial: T) -> Store<T> {
    Store::new(initial)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct AppState {
        user: String,
        count: i32,
    }

    #[test]
    fn test_store_updates_and_selectors() {
        let store = create_store(AppState {
            user: "Alice".to_string(),
            count: 0,
        });

        let count_signal = store.select(|s| s.count);
        let user_signal = store.select(|s| s.user.clone());

        assert_eq!(count_signal.get(), 0);
        assert_eq!(user_signal.get(), "Alice");

        // Update count only
        store.update(|s| s.count += 1);
        assert_eq!(store.get().count, 1);
        assert_eq!(count_signal.get(), 1);
        assert_eq!(user_signal.get(), "Alice"); // Unchanged

        // Update user
        store.update(|s| s.user = "Bob".to_string());
        assert_eq!(user_signal.get(), "Bob");
    }
}
