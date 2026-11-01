//! Replicated reactive signal integrating CRDT state with UI listener notifications.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use crate::clock::LamportClock;
use crate::lww::LwwRegister;

type SubscriberCallback = Box<dyn Fn() + Send + Sync>;

/// A reactive signal backed by a conflict-free replicated register.
#[derive(Clone)]
pub struct ReplicatedSignal<T: Clone> {
    state: Arc<Mutex<LwwRegister<T>>>,
    subscribers: Arc<Mutex<Vec<(usize, SubscriberCallback)>>>,
    next_sub_id: Arc<AtomicUsize>,
}

impl<T: Clone> ReplicatedSignal<T> {
    pub fn new(initial: T, clock: LamportClock) -> Self {
        Self {
            state: Arc::new(Mutex::new(LwwRegister::new(initial, clock))),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            next_sub_id: Arc::new(AtomicUsize::new(1)),
        }
    }

    /// Reads the current local value.
    pub fn get(&self) -> T {
        self.state.lock().unwrap().get().clone()
    }

    /// Sets a new value with a newer timestamp and notifies subscribers if updated.
    pub fn set(&self, value: T, clock: LamportClock) -> bool {
        let updated = self.state.lock().unwrap().set(value, clock);
        if updated {
            self.notify();
        }
        updated
    }

    /// Merges remote register state and notifies subscribers if updated.
    pub fn merge(&self, other: &LwwRegister<T>) -> bool {
        let updated = self.state.lock().unwrap().merge(other);
        if updated {
            self.notify();
        }
        updated
    }

    /// Subscribes a listener callback triggered on every state update.
    pub fn subscribe<F: Fn() + Send + Sync + 'static>(&self, callback: F) -> usize {
        let id = self.next_sub_id.fetch_add(1, Ordering::SeqCst);
        self.subscribers.lock().unwrap().push((id, Box::new(callback)));
        id
    }

    pub fn unsubscribe(&self, sub_id: usize) {
        let mut subs = self.subscribers.lock().unwrap();
        subs.retain(|(id, _)| *id != sub_id);
    }

    fn notify(&self) {
        let subs = self.subscribers.lock().unwrap();
        for (_, cb) in subs.iter() {
            cb();
        }
    }
}
