//! Fine-grained reactive dependency tracking graph (SolidJS-grade, zero Virtual DOM).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

pub type SignalId = usize;
pub type ScopeId = usize;

/// Generational subscriber identifier preventing stale-reference memory leaks and ABA hazards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubscriberId {
    pub index: u32,
    pub generation: u32,
}

impl std::fmt::Display for SubscriberId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:v{}", self.index, self.generation)
    }
}

thread_local! {
    static RUNTIME: ReactiveRuntime = ReactiveRuntime::new();
}

pub type SubscriberCallback = Rc<RefCell<dyn FnMut()>>;

/// Central thread-local reactive graph coordinator.
pub struct ReactiveRuntime {
    current_subscriber: Cell<Option<SubscriberId>>,
    evaluating_subscribers: RefCell<Vec<SubscriberId>>,
    signal_subscribers: RefCell<BTreeMap<SignalId, BTreeSet<SubscriberId>>>,
    subscriber_signals: RefCell<BTreeMap<SubscriberId, BTreeSet<SignalId>>>,
    signal_producer: RefCell<BTreeMap<SignalId, SubscriberId>>,
    subscriber_ranks: RefCell<BTreeMap<SubscriberId, usize>>,
    subscribers: RefCell<BTreeMap<SubscriberId, SubscriberCallback>>,
    pending_subscribers: RefCell<BTreeSet<(usize, SubscriberId)>>,
    subscriber_generations: RefCell<Vec<u32>>,
    free_subscriber_indices: RefCell<Vec<u32>>,
    batch_depth: Cell<usize>,
    dirty_signals: Cell<bool>,
    next_signal_id: Cell<SignalId>,
    next_scope_id: Cell<ScopeId>,
}

impl ReactiveRuntime {
    fn new() -> Self {
        Self {
            current_subscriber: Cell::new(None),
            evaluating_subscribers: RefCell::new(Vec::new()),
            signal_subscribers: RefCell::new(BTreeMap::new()),
            subscriber_signals: RefCell::new(BTreeMap::new()),
            signal_producer: RefCell::new(BTreeMap::new()),
            subscriber_ranks: RefCell::new(BTreeMap::new()),
            subscribers: RefCell::new(BTreeMap::new()),
            pending_subscribers: RefCell::new(BTreeSet::new()),
            subscriber_generations: RefCell::new(Vec::new()),
            free_subscriber_indices: RefCell::new(Vec::new()),
            batch_depth: Cell::new(0),
            dirty_signals: Cell::new(false),
            next_signal_id: Cell::new(1),
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
            let mut free = rt.free_subscriber_indices.borrow_mut();
            let mut gens = rt.subscriber_generations.borrow_mut();
            if let Some(idx) = free.pop() {
                let gen = gens[idx as usize];
                SubscriberId {
                    index: idx,
                    generation: gen,
                }
            } else {
                let idx = gens.len() as u32;
                gens.push(1);
                SubscriberId {
                    index: idx,
                    generation: 1,
                }
            }
        })
    }

    pub fn is_subscriber_alive(sub_id: SubscriberId) -> bool {
        RUNTIME.try_with(|rt| {
            if let Ok(gens) = rt.subscriber_generations.try_borrow() {
                if let Some(&gen) = gens.get(sub_id.index as usize) {
                    return gen == sub_id.generation;
                }
            }
            false
        }).unwrap_or(false)
    }

    pub fn active_subscriber_count() -> usize {
        RUNTIME.try_with(|rt| {
            rt.subscribers.try_borrow().map(|s| s.len()).unwrap_or(0)
        }).unwrap_or(0)
    }

    pub fn signal_subscription_count(signal_id: SignalId) -> usize {
        RUNTIME.try_with(|rt| {
            rt.signal_subscribers.try_borrow().ok()
                .and_then(|ss| ss.get(&signal_id).map(|s| s.len()))
                .unwrap_or(0)
        }).unwrap_or(0)
    }

    pub fn alloc_scope_id() -> ScopeId {
        RUNTIME.with(|rt| {
            let id = rt.next_scope_id.get();
            rt.next_scope_id.set(id + 1);
            id
        })
    }

    pub fn has_dirty_signals() -> bool {
        RUNTIME.try_with(|rt| rt.dirty_signals.get()).unwrap_or(false)
    }

    pub fn take_dirty_signals() -> bool {
        RUNTIME.try_with(|rt| rt.dirty_signals.replace(false)).unwrap_or(false)
    }

    pub fn mark_dirty() {
        let _ = RUNTIME.try_with(|rt| rt.dirty_signals.set(true));
    }

    pub fn push_evaluating(sub_id: SubscriberId) -> bool {
        RUNTIME.with(|rt| {
            if let Ok(mut stack) = rt.evaluating_subscribers.try_borrow_mut() {
                if stack.contains(&sub_id) {
                    false
                } else {
                    stack.push(sub_id);
                    true
                }
            } else {
                false
            }
        })
    }

    pub fn pop_evaluating(sub_id: SubscriberId) {
        let _ = RUNTIME.try_with(|rt| {
            if let Ok(mut stack) = rt.evaluating_subscribers.try_borrow_mut() {
                if let Some(pos) = stack.iter().rposition(|&id| id == sub_id) {
                    stack.remove(pos);
                }
            }
        });
    }

    pub fn is_evaluating(sub_id: SubscriberId) -> bool {
        RUNTIME.try_with(|rt| {
            rt.evaluating_subscribers
                .try_borrow()
                .map(|stack| stack.contains(&sub_id))
                .unwrap_or(false)
        })
        .unwrap_or(false)
    }

    pub fn register_signal_producer(signal_id: SignalId, sub_id: SubscriberId) {
        RUNTIME.with(|rt| {
            if let Ok(mut map) = rt.signal_producer.try_borrow_mut() {
                map.insert(signal_id, sub_id);
            }
        });
    }

    pub fn track(signal_id: SignalId) {
        RUNTIME.with(|rt| {
            if let Some(sub_id) = rt.current_subscriber.get() {
                if let Ok(mut sig_subs) = rt.signal_subscribers.try_borrow_mut() {
                    sig_subs
                        .entry(signal_id)
                        .or_default()
                        .insert(sub_id);
                }
                if let Ok(mut sub_sigs) = rt.subscriber_signals.try_borrow_mut() {
                    sub_sigs
                        .entry(sub_id)
                        .or_default()
                        .insert(signal_id);
                }

                // If this signal is produced by an upstream subscriber, update current subscriber's topological rank
                let producer_id = if let Ok(producers) = rt.signal_producer.try_borrow() {
                    producers.get(&signal_id).copied()
                } else {
                    None
                };

                if let Some(producer) = producer_id {
                    let producer_rank = if let Ok(ranks) = rt.subscriber_ranks.try_borrow() {
                        ranks.get(&producer).copied().unwrap_or(0)
                    } else {
                        0
                    };
                    if let Ok(mut ranks) = rt.subscriber_ranks.try_borrow_mut() {
                        let current_rank = ranks.entry(sub_id).or_insert(0);
                        if *current_rank <= producer_rank {
                            *current_rank = producer_rank + 1;
                        }
                    }
                }
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
            if let Ok(mut subs) = rt.subscribers.try_borrow_mut() {
                subs.insert(sub_id, runner);
            }
        });
    }

    pub fn clear_subscriber_dependencies(sub_id: SubscriberId) {
        let _ = RUNTIME.try_with(|rt| {
            let signals = if let Ok(mut sub_signals) = rt.subscriber_signals.try_borrow_mut() {
                sub_signals.remove(&sub_id)
            } else {
                None
            };
            if let Some(signals) = signals {
                if let Ok(mut sig_subs) = rt.signal_subscribers.try_borrow_mut() {
                    let mut empty_signals = Vec::new();
                    for sig in signals {
                        if let Some(subs) = sig_subs.get_mut(&sig) {
                            subs.remove(&sub_id);
                            if subs.is_empty() {
                                empty_signals.push(sig);
                            }
                        }
                    }
                    for sig in empty_signals {
                        sig_subs.remove(&sig);
                    }
                }
            }
        });
    }

    pub fn unregister_subscriber(sub_id: SubscriberId) {
        let was_valid = RUNTIME.try_with(|rt| {
            if let Ok(gens) = rt.subscriber_generations.try_borrow() {
                if let Some(&gen) = gens.get(sub_id.index as usize) {
                    gen == sub_id.generation
                } else {
                    false
                }
            } else {
                false
            }
        }).unwrap_or(false);

        if !was_valid {
            return;
        }

        Self::clear_subscriber_dependencies(sub_id);

        let removed = RUNTIME.try_with(|rt| {
            if let Ok(mut ranks) = rt.subscriber_ranks.try_borrow_mut() {
                if let Some(rank) = ranks.remove(&sub_id) {
                    if let Ok(mut pending) = rt.pending_subscribers.try_borrow_mut() {
                        pending.remove(&(rank, sub_id));
                    }
                }
            }
            if let Ok(mut producers) = rt.signal_producer.try_borrow_mut() {
                producers.retain(|_, &mut prod_sub| prod_sub != sub_id);
            }
            if let Ok(mut gens) = rt.subscriber_generations.try_borrow_mut() {
                if let Some(gen) = gens.get_mut(sub_id.index as usize) {
                    *gen = gen.wrapping_add(1);
                }
            }
            if let Ok(mut free) = rt.free_subscriber_indices.try_borrow_mut() {
                free.push(sub_id.index);
            }
            if let Ok(mut evaluating) = rt.evaluating_subscribers.try_borrow_mut() {
                evaluating.retain(|&id| id != sub_id);
            }
            if let Ok(mut subs) = rt.subscribers.try_borrow_mut() {
                subs.remove(&sub_id)
            } else {
                None
            }
        }).ok().flatten();
        drop(removed);
    }

    pub fn notify(signal_id: SignalId) {
        let _ = RUNTIME.try_with(|rt| {
            rt.dirty_signals.set(true);
            let mut stale_subs: Option<Vec<SubscriberId>> = None;

            if let Ok(sig_subs) = rt.signal_subscribers.try_borrow() {
                if let Some(subs_set) = sig_subs.get(&signal_id) {
                    if subs_set.is_empty() {
                        return;
                    }
                    let gens = rt.subscriber_generations.try_borrow().ok();
                    let ranks = rt.subscriber_ranks.try_borrow().ok();
                    if let Ok(mut pending) = rt.pending_subscribers.try_borrow_mut() {
                        for &sub in subs_set {
                            let is_alive = if let Some(ref g) = gens {
                                g.get(sub.index as usize).copied() == Some(sub.generation)
                            } else {
                                true
                            };
                            if is_alive {
                                let rank = ranks.as_ref().and_then(|r| r.get(&sub).copied()).unwrap_or(0);
                                pending.insert((rank, sub));
                            } else {
                                stale_subs.get_or_insert_with(Vec::new).push(sub);
                            }
                        }
                    }
                } else {
                    return;
                }
            } else {
                return;
            }

            // Evict stale subscriptions automatically only when stale subscribers are encountered
            if let Some(stale_list) = stale_subs {
                if let Ok(mut sig_subs) = rt.signal_subscribers.try_borrow_mut() {
                    if let Some(subs_set) = sig_subs.get_mut(&signal_id) {
                        for stale in stale_list {
                            subs_set.remove(&stale);
                        }
                        if subs_set.is_empty() {
                            sig_subs.remove(&signal_id);
                        }
                    }
                }
            }

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
            let mut loop_count: usize = 0;
            let mut run_counts: Option<std::collections::HashMap<SubscriberId, usize>> = None;
            const MAX_RUNS_PER_SUBSCRIBER: usize = 50;

            loop {
                let next_sub = {
                    let mut pending = if let Ok(p) = rt.pending_subscribers.try_borrow_mut() {
                        p
                    } else {
                        break;
                    };
                    if let Some(&(rank, sub_id)) = pending.iter().next() {
                        pending.remove(&(rank, sub_id));
                        Some(sub_id)
                    } else {
                        None
                    }
                };

                let sub_id = match next_sub {
                    Some(id) => id,
                    None => break,
                };

                if !Self::is_subscriber_alive(sub_id) {
                    continue;
                }

                loop_count += 1;
                if loop_count > 64 {
                    let map = run_counts.get_or_insert_with(std::collections::HashMap::new);
                    let count = map.entry(sub_id).or_insert(0);
                    *count += 1;
                    if *count > MAX_RUNS_PER_SUBSCRIBER {
                        eprintln!(
                            "Strata Reactive Guard: Subscriber {} exceeded maximum recursive executions (possible circular dependency).",
                            sub_id
                        );
                        continue;
                    }
                }

                let runner = {
                    if let Ok(subs) = rt.subscribers.try_borrow() {
                        subs.get(&sub_id).cloned()
                    } else {
                        None
                    }
                };

                if let Some(runner) = runner {
                    Self::clear_subscriber_dependencies(sub_id);
                    let prev = rt.current_subscriber.replace(Some(sub_id));
                    if let Ok(mut r) = runner.try_borrow_mut() {
                        (*r)();
                    }
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

/// RAII guard tracking an actively evaluating subscriber on the thread-local call stack.
pub struct EvaluatingGuard(SubscriberId);

impl EvaluatingGuard {
    pub fn new(sub_id: SubscriberId) -> Option<Self> {
        if ReactiveRuntime::push_evaluating(sub_id) {
            Some(Self(sub_id))
        } else {
            None
        }
    }
}

impl Drop for EvaluatingGuard {
    fn drop(&mut self) {
        ReactiveRuntime::pop_evaluating(self.0);
    }
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

impl<T: Clone + PartialEq + 'static> Memo<T> {
    pub fn new(compute: impl FnMut() -> T + 'static) -> Self {
        Self::new_with_compare(compute, |a, b| a == b)
    }

    pub fn new_with_compare(
        compute: impl FnMut() -> T + 'static,
        is_equal: impl Fn(&T, &T) -> bool + 'static,
    ) -> Self {
        let signal_id = ReactiveRuntime::alloc_signal_id();
        let subscriber_id = ReactiveRuntime::alloc_subscriber_id();

        ReactiveRuntime::register_signal_producer(signal_id, subscriber_id);

        let value = Rc::new(RefCell::new(None));
        let compute_cell = Rc::new(RefCell::new(compute));
        let eq_cell = Rc::new(is_equal);

        let value_clone = value.clone();
        let compute_clone = compute_cell.clone();
        let eq_clone = eq_cell.clone();

        let runner = Rc::new(RefCell::new(move || {
            let _guard = match EvaluatingGuard::new(subscriber_id) {
                Some(guard) => guard,
                None => {
                    eprintln!(
                        "Strata Reactive Guard: Circular memo dependency detected for subscriber {}. Returning cached value.",
                        subscriber_id
                    );
                    return;
                }
            };

            ReactiveRuntime::clear_subscriber_dependencies(subscriber_id);
            let prev = RUNTIME.with(|rt| rt.current_subscriber.replace(Some(subscriber_id)));
            let maybe_new_val = match compute_clone.try_borrow_mut() {
                Ok(mut compute) => Some(compute()),
                Err(_) => {
                    eprintln!(
                        "Strata Reactive Guard: Memo compute closure already borrowed for subscriber {}. Returning cached value.",
                        subscriber_id
                    );
                    None
                }
            };
            RUNTIME.with(|rt| rt.current_subscriber.set(prev));

            if let Some(new_val) = maybe_new_val {
                let changed = match value_clone.try_borrow().as_deref() {
                    Ok(Some(old_val)) => !eq_clone(old_val, &new_val),
                    Ok(None) => true,
                    Err(_) => true,
                };

                if let Ok(mut val) = value_clone.try_borrow_mut() {
                    *val = Some(new_val);
                }

                // Halt downstream propagation if output has not changed (O(1) memoization)
                if changed {
                    ReactiveRuntime::notify(signal_id);
                }
            }
        }));

        ReactiveRuntime::register_subscriber(subscriber_id, runner.clone());

        // Initial evaluation
        if let Ok(r) = runner.try_borrow_mut() {
            (*r)();
        }

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
        match self.value.borrow().as_ref() {
            Some(v) => v.clone(),
            None => panic!(
                "Strata Reactive Guard: Memo read before initial value could be computed (unresolvable circular dependency during initialization)."
            ),
        }
    }

    pub fn try_get(&self) -> Option<T> {
        ReactiveRuntime::track(self.id);
        self.value.borrow().as_ref().cloned()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        ReactiveRuntime::track(self.id);
        match self.value.borrow().as_ref() {
            Some(v) => f(v),
            None => panic!(
                "Strata Reactive Guard: Memo read before initial value could be computed (unresolvable circular dependency during initialization)."
            ),
        }
    }

    pub fn untracked_get(&self) -> T {
        match self.value.borrow().as_ref() {
            Some(v) => v.clone(),
            None => panic!(
                "Strata Reactive Guard: Memo read before initial value could be computed."
            ),
        }
    }
}

pub type CleanupFn = Option<Box<dyn FnOnce()>>;
pub type CleanupCell = Rc<RefCell<CleanupFn>>;

/// Reactive effect executing side-effects when subscribed signals mutate.
pub struct Effect {
    subscriber_id: SubscriberId,
    disposed: Rc<Cell<bool>>,
    cleanup: CleanupCell,
}

impl Effect {
    pub fn new<F>(mut effect_fn: F) -> Self
    where
        F: FnMut() -> Option<Box<dyn FnOnce()>> + 'static,
    {
        let subscriber_id = ReactiveRuntime::alloc_subscriber_id();
        let cleanup: CleanupCell = Rc::new(RefCell::new(None));
        let disposed = Rc::new(Cell::new(false));

        let cleanup_runner = cleanup.clone();
        let disposed_runner = disposed.clone();

        let runner = Rc::new(RefCell::new(move || {
            if disposed_runner.get() {
                return;
            }

            let _guard = match EvaluatingGuard::new(subscriber_id) {
                Some(guard) => guard,
                None => {
                    eprintln!(
                        "Strata Reactive Guard: Circular effect dependency detected for subscriber {}.",
                        subscriber_id
                    );
                    return;
                }
            };

            // Run previous cleanup closure if any
            if let Ok(mut cleanup) = cleanup_runner.try_borrow_mut() {
                if let Some(prev_clean) = cleanup.take() {
                    prev_clean();
                }
            }

            // Run effect
            let next_clean = effect_fn();
            if let Ok(mut cleanup) = cleanup_runner.try_borrow_mut() {
                *cleanup = next_clean;
            }
        }));

        ReactiveRuntime::register_subscriber(subscriber_id, runner.clone());

        // Initial run
        {
            let prev = RUNTIME.with(|rt| rt.current_subscriber.replace(Some(subscriber_id)));
            if let Ok(mut r) = runner.try_borrow_mut() {
                (*r)();
            }
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
            if let Ok(mut cleanup) = self.cleanup.try_borrow_mut() {
                if let Some(clean) = cleanup.take() {
                    clean();
                }
            }
        }
    }
    /// Converts the effect into an RAII guard that disposes the effect when dropped.
    pub fn into_guard(self) -> EffectGuard {
        EffectGuard { effect: self }
    }
}

/// RAII guard that automatically disposes an effect when dropped.
pub struct EffectGuard {
    effect: Effect,
}

impl Drop for EffectGuard {
    fn drop(&mut self) {
        self.effect.dispose();
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

pub fn use_memo<T: Clone + PartialEq + 'static>(compute: impl FnMut() -> T + 'static) -> Memo<T> {
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

pub fn has_dirty_signals() -> bool {
    ReactiveRuntime::has_dirty_signals()
}

pub fn take_dirty_signals() -> bool {
    ReactiveRuntime::take_dirty_signals()
}

pub fn mark_dirty() {
    ReactiveRuntime::mark_dirty()
}
