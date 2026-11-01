//! Fine-grained reactive dependency tracking graph (SolidJS-grade, zero Virtual DOM).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

pub type SignalId = usize;
pub type SubscriberId = usize;
pub type ScopeId = usize;

thread_local! {
    static RUNTIME: ReactiveRuntime = ReactiveRuntime::new();
}

/// Central thread-local reactive graph coordinator.
pub struct ReactiveRuntime {
    current_subscriber: Cell<Option<SubscriberId>>,
    signal_subscribers: RefCell<BTreeMap<SignalId, BTreeSet<SubscriberId>>>,
    subscriber_signals: RefCell<BTreeMap<SubscriberId, BTreeSet<SignalId>>>,
    subscribers: RefCell<BTreeMap<SubscriberId, Rc<RefCell<dyn FnMut()>>>>,
    pending_subscribers: RefCell<BTreeSet<SubscriberId>>,
    batch_depth: Cell<usize>,
    next_signal_id: Cell<SignalId>,
    next_subscriber_id: Cell<SubscriberId>,
    next_scope_id: Cell<ScopeId>,
}

impl ReactiveRuntime {
    fn new() -> Self {
        Self {
            current_subscriber: Cell::new(None),
            signal_subscribers: RefCell::new(BTreeMap::new()),
            subscriber_signals: RefCell::new(BTreeMap::new()),
            subscribers: RefCell::new(BTreeMap::new()),
            pending_subscribers: RefCell::new(BTreeSet::new()),
            batch_depth: Cell::new(0),
            next_signal_id: Cell::new(1),
            next_subscriber_id: Cell::new(1),
            next_scope_id: Cell::new(1),
        }
    }

    pub fn alloc_signal_id() -> SignalId {
        RUNTIME.with(|rt| {
            let id = rt.next_signal_id.get();
            rt.next_signal_id.set(id + 1);
            id
        })
    }

    pub fn alloc_subscriber_id() -> SubscriberId {
        RUNTIME.with(|rt| {
            let id = rt.next_subscriber_id.get();
            rt.next_subscriber_id.set(id + 1);
            id
        })
    }

    pub fn alloc_scope_id() -> ScopeId {
        RUNTIME.with(|rt| {
            let id = rt.next_scope_id.get();
            rt.next_scope_id.set(id + 1);
            id
        })
    }

    pub fn track(signal_id: SignalId) {
        RUNTIME.with(|rt| {
            if let Some(sub_id) = rt.current_subscriber.get() {
                rt.signal_subscribers
                    .borrow_mut()
                    .entry(signal_id)
                    .or_default()
                    .insert(sub_id);
                rt.subscriber_signals
                    .borrow_mut()
                    .entry(sub_id)
                    .or_default()
                    .insert(signal_id);
            }
        });
    }

    pub fn untrack<R>(f: impl FnOnce() -> R) -> R {
        RUNTIME.with(|rt| {
            let prev = rt.current_subscriber.replace(None);
            let res = f();
            rt.current_subscriber.set(prev);
            res
        })
    }

    pub fn register_subscriber(sub_id: SubscriberId, runner: Rc<RefCell<dyn FnMut()>>) {
        RUNTIME.with(|rt| {
            rt.subscribers.borrow_mut().insert(sub_id, runner);
        });
    }

    pub fn clear_subscriber_dependencies(sub_id: SubscriberId) {
        RUNTIME.with(|rt| {
            let mut sub_signals = rt.subscriber_signals.borrow_mut();
            if let Some(signals) = sub_signals.remove(&sub_id) {
                let mut sig_subs = rt.signal_subscribers.borrow_mut();
                for sig in signals {
                    if let Some(subs) = sig_subs.get_mut(&sig) {
                        subs.remove(&sub_id);
                    }
                }
            }
        });
    }

    pub fn unregister_subscriber(sub_id: SubscriberId) {
        Self::clear_subscriber_dependencies(sub_id);
        RUNTIME.with(|rt| {
            rt.subscribers.borrow_mut().remove(&sub_id);
            rt.pending_subscribers.borrow_mut().remove(&sub_id);
        });
    }

    pub fn notify(signal_id: SignalId) {
        RUNTIME.with(|rt| {
            let subs = {
                let sig_subs = rt.signal_subscribers.borrow();
                sig_subs.get(&signal_id).cloned().unwrap_or_default()
            };

            if subs.is_empty() {
                return;
            }

            rt.pending_subscribers.borrow_mut().extend(subs);

            if rt.batch_depth.get() == 0 {
                Self::flush();
            }
        });
    }

    pub fn batch<R>(f: impl FnOnce() -> R) -> R {
        RUNTIME.with(|rt| {
            let depth = rt.batch_depth.get();
            rt.batch_depth.set(depth + 1);

            let res = f();

            rt.batch_depth.set(depth);
            if depth == 0 {
                Self::flush();
            }
            res
        })
    }

    pub fn flush() {
        RUNTIME.with(|rt| {
            let mut iterations = 0;
            const MAX_ITERATIONS: usize = 1000;

            loop {
                let next_sub = {
                    let mut pending = rt.pending_subscribers.borrow_mut();
                    if let Some(&first) = pending.iter().next() {
                        pending.remove(&first);
                        Some(first)
                    } else {
                        None
                    }
                };

                let sub_id = match next_sub {
                    Some(id) => id,
                    None => break,
                };

                iterations += 1;
                if iterations > MAX_ITERATIONS {
                    eprintln!("Strata Reactive Guard: Maximum reactive update iterations exceeded. Possible circular dependency.");
                    break;
                }

                let runner = {
                    let subs = rt.subscribers.borrow();
                    subs.get(&sub_id).cloned()
                };

                if let Some(runner) = runner {
                    Self::clear_subscriber_dependencies(sub_id);
                    let prev = rt.current_subscriber.replace(Some(sub_id));
                    (runner.borrow_mut())();
                    rt.current_subscriber.set(prev);
                }
            }
        });
    }
}

/// Fine-grained reactive state container.
#[derive(Clone)]
pub struct Signal<T> {
    id: SignalId,
    value: Rc<RefCell<T>>,
}

impl<T: Clone + 'static> Signal<T> {
    pub fn new(initial: T) -> Self {
        Self {
            id: ReactiveRuntime::alloc_signal_id(),
            value: Rc::new(RefCell::new(initial)),
        }
    }

    pub fn id(&self) -> SignalId {
        self.id
    }

    pub fn get(&self) -> T {
        ReactiveRuntime::track(self.id);
        self.value.borrow().clone()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        ReactiveRuntime::track(self.id);
        f(&*self.value.borrow())
    }

    pub fn untracked_get(&self) -> T {
        self.value.borrow().clone()
    }

    pub fn set(&self, new_val: T) {
        *self.value.borrow_mut() = new_val;
        ReactiveRuntime::notify(self.id);
    }

    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let res = f(&mut *self.value.borrow_mut());
        ReactiveRuntime::notify(self.id);
        res
    }

    pub fn read(&self) -> std::cell::Ref<'_, T> {
        ReactiveRuntime::track(self.id);
        self.value.borrow()
    }

    /// Splits this combined signal into decoupled read-only and write-only capabilities.
    pub fn split(&self) -> (ReadSignal<T>, WriteSignal<T>) {
        (
            ReadSignal {
                id: self.id,
                value: self.value.clone(),
            },
            WriteSignal {
                id: self.id,
                value: self.value.clone(),
            },
        )
    }
}

/// Read-only reactive signal capability.
#[derive(Clone)]
pub struct ReadSignal<T> {
    id: SignalId,
    value: Rc<RefCell<T>>,
}

impl<T: Clone + 'static> ReadSignal<T> {
    pub fn id(&self) -> SignalId {
        self.id
    }

    pub fn get(&self) -> T {
        ReactiveRuntime::track(self.id);
        self.value.borrow().clone()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        ReactiveRuntime::track(self.id);
        f(&*self.value.borrow())
    }

    pub fn untracked_get(&self) -> T {
        self.value.borrow().clone()
    }

    pub fn read(&self) -> std::cell::Ref<'_, T> {
        ReactiveRuntime::track(self.id);
        self.value.borrow()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for ReadSignal<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadSignal")
            .field("id", &self.id)
            .field("value", &*self.value.borrow())
            .finish()
    }
}

/// Write-only reactive signal capability.
#[derive(Clone)]
pub struct WriteSignal<T> {
    id: SignalId,
    value: Rc<RefCell<T>>,
}

impl<T: Clone + 'static> WriteSignal<T> {
    pub fn id(&self) -> SignalId {
        self.id
    }

    pub fn set(&self, new_val: T) {
        *self.value.borrow_mut() = new_val;
        ReactiveRuntime::notify(self.id);
    }

    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let res = f(&mut *self.value.borrow_mut());
        ReactiveRuntime::notify(self.id);
        res
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for WriteSignal<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteSignal")
            .field("id", &self.id)
            .finish()
    }
}

/// Idiomatic SolidJS/Leptos alias for write capability.
pub type SetSignal<T> = WriteSignal<T>;

/// Creates an atomic reactive cell with decoupled read and write capabilities.
pub fn create_signal<T: Clone + 'static>(initial: T) -> (ReadSignal<T>, WriteSignal<T>) {
    let sig = Signal::new(initial);
    sig.split()
}

struct MemoInner {
    subscriber_id: SubscriberId,
}

impl Drop for MemoInner {
    fn drop(&mut self) {
        ReactiveRuntime::unregister_subscriber(self.subscriber_id);
    }
}

/// Derived computed value that caches its output and only recomputes when dependencies change.
#[derive(Clone)]
pub struct Memo<T> {
    id: SignalId,
    value: Rc<RefCell<Option<T>>>,
    _inner: Rc<MemoInner>,
}

impl<T: Clone + 'static> Memo<T> {
    pub fn new(compute: impl FnMut() -> T + 'static) -> Self {
        let signal_id = ReactiveRuntime::alloc_signal_id();
        let subscriber_id = ReactiveRuntime::alloc_subscriber_id();

        let value = Rc::new(RefCell::new(None));
        let compute_cell = Rc::new(RefCell::new(compute));

        let value_clone = value.clone();
        let compute_clone = compute_cell.clone();

        let runner = Rc::new(RefCell::new(move || {
            ReactiveRuntime::clear_subscriber_dependencies(subscriber_id);
            let prev = RUNTIME.with(|rt| rt.current_subscriber.replace(Some(subscriber_id)));
            let new_val = (compute_clone.borrow_mut())();
            RUNTIME.with(|rt| rt.current_subscriber.set(prev));

            *value_clone.borrow_mut() = Some(new_val);
            ReactiveRuntime::notify(signal_id);
        }));

        ReactiveRuntime::register_subscriber(subscriber_id, runner.clone());

        // Initial evaluation
        (runner.borrow_mut())();

        Self {
            id: signal_id,
            value,
            _inner: Rc::new(MemoInner { subscriber_id }),
        }
    }

    pub fn id(&self) -> SignalId {
        self.id
    }

    pub fn get(&self) -> T {
        ReactiveRuntime::track(self.id);
        self.value.borrow().as_ref().unwrap().clone()
    }
}

/// Reactive effect executing side-effects when subscribed signals mutate.
pub struct Effect {
    subscriber_id: SubscriberId,
    disposed: Rc<Cell<bool>>,
    cleanup: Rc<RefCell<Option<Box<dyn FnOnce()>>>>,
}

impl Effect {
    pub fn new<F>(mut effect_fn: F) -> Self
    where
        F: FnMut() -> Option<Box<dyn FnOnce()>> + 'static,
    {
        let subscriber_id = ReactiveRuntime::alloc_subscriber_id();
        let cleanup: Rc<RefCell<Option<Box<dyn FnOnce()>>>> = Rc::new(RefCell::new(None));
        let disposed = Rc::new(Cell::new(false));

        let cleanup_runner = cleanup.clone();
        let disposed_runner = disposed.clone();

        let runner = Rc::new(RefCell::new(move || {
            if disposed_runner.get() {
                return;
            }

            // Run previous cleanup closure if any
            if let Some(prev_clean) = cleanup_runner.borrow_mut().take() {
                prev_clean();
            }

            // Run effect
            let next_clean = effect_fn();
            *cleanup_runner.borrow_mut() = next_clean;
        }));

        ReactiveRuntime::register_subscriber(subscriber_id, runner.clone());

        // Initial run
        {
            let prev = RUNTIME.with(|rt| rt.current_subscriber.replace(Some(subscriber_id)));
            (runner.borrow_mut())();
            RUNTIME.with(|rt| rt.current_subscriber.set(prev));
        }

        Self {
            subscriber_id,
            disposed,
            cleanup,
        }
    }

    pub fn dispose(&self) {
        if !self.disposed.replace(true) {
            ReactiveRuntime::unregister_subscriber(self.subscriber_id);
            if let Some(clean) = self.cleanup.borrow_mut().take() {
                clean();
            }
        }
    }
}

impl Drop for Effect {
    fn drop(&mut self) {
        self.dispose();
    }
}

// Ergonomic Hooks

pub fn use_signal<T: Clone + 'static>(initial: T) -> Signal<T> {
    Signal::new(initial)
}

pub fn use_state<T: Clone + 'static>(init: impl FnOnce() -> T) -> (Signal<T>, Signal<T>) {
    let s = Signal::new(init());
    (s.clone(), s)
}

pub fn use_memo<T: Clone + 'static>(compute: impl FnMut() -> T + 'static) -> Memo<T> {
    Memo::new(compute)
}

pub fn use_effect<F>(effect_fn: F) -> Effect
where
    F: FnMut() -> Option<Box<dyn FnOnce()>> + 'static,
{
    Effect::new(effect_fn)
}

pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    ReactiveRuntime::batch(f)
}

pub fn untrack<R>(f: impl FnOnce() -> R) -> R {
    ReactiveRuntime::untrack(f)
}
