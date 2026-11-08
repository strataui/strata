//! Replicated reactive signal integrating CRDT state with Strata UI listener notifications and fine-grained reactive Signal graphs.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use crate::clock::{LamportClock, PeerId};
use crate::counter::PnCounter;
use crate::lww::LwwRegister;
use crate::sync::SyncMessage;

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

    /// Returns the current Lamport timestamp.
    pub fn timestamp(&self) -> LamportClock {
        self.state.lock().unwrap().timestamp()
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

impl<T: Clone + Send + 'static> ReplicatedSignal<T> {
    /// Bridges this replicated CRDT signal to a fine-grained reactive `Signal<T>`
    /// via a new `CrdtSignalStore`.
    pub fn to_store(&self, peer_id: PeerId) -> CrdtSignalStore<T> {
        CrdtSignalStore::from_replicated(self.clone(), peer_id)
    }

    /// Returns a fine-grained reactive `Signal<T>` bound to this replicated signal.
    pub fn to_reactive_signal(&self, peer_id: PeerId) -> strata_dom::reactive::Signal<T> {
        let store = self.to_store(peer_id);
        store.signal().clone()
    }
}

/// A collaborative, local-first reactive state store bridging CRDT data types with
/// Strata's fine-grained reactive UI signal engine.
pub struct CrdtSignalStore<T: Clone + Send + 'static> {
    replicated: ReplicatedSignal<T>,
    signal: strata_dom::reactive::Signal<T>,
    clock: Arc<Mutex<LamportClock>>,
    _sender: Sender<T>,
    receiver: Receiver<T>,
    sub_id: usize,
}

impl<T: Clone + Send + 'static> CrdtSignalStore<T> {
    pub fn new(initial: T, peer_id: PeerId) -> Self {
        let clock = LamportClock::new(1, peer_id);
        let replicated = ReplicatedSignal::new(initial, clock);
        Self::from_replicated(replicated, peer_id)
    }

    pub fn from_replicated(replicated: ReplicatedSignal<T>, peer_id: PeerId) -> Self {
        let initial_val = replicated.get();
        let signal = strata_dom::reactive::Signal::new(initial_val);
        let (sender, receiver) = channel();

        let tx = sender.clone();
        let rep_clone = replicated.clone();
        let sub_id = replicated.subscribe(move || {
            let val = rep_clone.get();
            let _ = tx.send(val);
        });

        let current_clock = replicated.timestamp();
        let mut clock = LamportClock::new(current_clock.counter, peer_id);
        if clock.counter == 0 {
            clock.counter = 1;
        }

        Self {
            replicated,
            signal,
            clock: Arc::new(Mutex::new(clock)),
            _sender: sender,
            receiver,
            sub_id,
        }
    }

    /// Poll any pending replicated updates from background threads or merges
    /// and dispatch them directly into the reactive Signal graph.
    pub fn poll_updates(&self) -> bool {
        let mut updated = false;
        while let Ok(val) = self.receiver.try_recv() {
            self.signal.set(val);
            updated = true;
        }
        updated
    }

    /// Access the native Strata reactive Signal for fine-grained UI binding.
    pub fn signal(&self) -> &strata_dom::reactive::Signal<T> {
        self.poll_updates();
        &self.signal
    }

    /// Read current value with fine-grained reactive dependency tracking.
    pub fn get(&self) -> T {
        self.poll_updates();
        self.signal.get()
    }

    /// Mutate the store locally, ticking the LamportClock and updating both
    /// the CRDT register and the reactive Signal.
    pub fn set(&self, value: T) -> bool {
        let next_clock = self.clock.lock().unwrap().tick();
        if self.replicated.set(value.clone(), next_clock) {
            self.poll_updates();
            true
        } else {
            false
        }
    }

    /// Merge remote CRDT register state, updating the reactive Signal if the remote state dominates.
    pub fn merge_remote(&self, other: &LwwRegister<T>) -> bool {
        self.clock.lock().unwrap().witness(other.timestamp());
        if self.replicated.merge(other) {
            self.poll_updates();
            true
        } else {
            false
        }
    }

    /// Merge from a sync message payload.
    pub fn merge_sync_message(&self, msg: &SyncMessage, deserialize: impl Fn(&[u8]) -> Option<T>) -> bool {
        match msg {
            SyncMessage::RegisterDelta { clock, payload, .. } => {
                if let Some(val) = deserialize(payload) {
                    let remote_reg = LwwRegister::new(val, *clock);
                    self.merge_remote(&remote_reg)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Export the local state as a sync message.
    pub fn export_sync_message(&self, key: impl Into<String>, serialize: impl Fn(&T) -> Vec<u8>) -> SyncMessage {
        let clock = *self.clock.lock().unwrap();
        let val = self.get();
        SyncMessage::RegisterDelta {
            clock,
            key: key.into(),
            payload: serialize(&val),
        }
    }

    pub fn replicated(&self) -> &ReplicatedSignal<T> {
        &self.replicated
    }

    pub fn clock(&self) -> LamportClock {
        *self.clock.lock().unwrap()
    }
}

impl<T: Clone + Send + 'static> Drop for CrdtSignalStore<T> {
    fn drop(&mut self) {
        self.replicated.unsubscribe(self.sub_id);
    }
}

/// Collaborative reactive counter store bridging PnCounter with a fine-grained reactive Signal.
pub struct CrdtCounterStore {
    counter: Arc<Mutex<PnCounter>>,
    signal: strata_dom::reactive::Signal<i64>,
    peer_id: PeerId,
}

impl CrdtCounterStore {
    pub fn new(peer_id: PeerId) -> Self {
        Self {
            counter: Arc::new(Mutex::new(PnCounter::new())),
            signal: strata_dom::reactive::Signal::new(0),
            peer_id,
        }
    }

    pub fn signal(&self) -> &strata_dom::reactive::Signal<i64> {
        &self.signal
    }

    pub fn value(&self) -> i64 {
        self.signal.get()
    }

    pub fn inc(&self, amount: u64) {
        let mut c = self.counter.lock().unwrap();
        c.inc(self.peer_id, amount);
        let val = c.value();
        drop(c);
        self.signal.set(val);
    }

    pub fn dec(&self, amount: u64) {
        let mut c = self.counter.lock().unwrap();
        c.dec(self.peer_id, amount);
        let val = c.value();
        drop(c);
        self.signal.set(val);
    }

    pub fn merge(&self, other: &PnCounter) {
        let mut c = self.counter.lock().unwrap();
        c.merge(other);
        let val = c.value();
        drop(c);
        self.signal.set(val);
    }

    pub fn snapshot(&self) -> PnCounter {
        self.counter.lock().unwrap().clone()
    }

    pub fn export_sync_message(&self) -> SyncMessage {
        let c = self.counter.lock().unwrap();
        let (pos, neg) = c.peer_values(self.peer_id);
        SyncMessage::CounterDelta {
            peer: self.peer_id,
            positive: pos,
            negative: neg,
        }
    }
}
