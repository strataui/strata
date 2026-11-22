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
        RUNTIME
            .try_with(|rt| {
                if let Ok(gens) = rt.subscriber_generations.try_borrow() {
                    if let Some(&gen) = gens.get(sub_id.index as usize) {
                        return gen == sub_id.generation;
                    }
                }
                false
            })
            .unwrap_or(false)
    }

    pub fn active_subscriber_count() -> usize {
        RUNTIME
            .try_with(|rt| rt.subscribers.try_borrow().map(|s| s.len()).unwrap_or(0))
            .unwrap_or(0)
    }

    pub fn signal_subscription_count(signal_id: SignalId) -> usize {
        RUNTIME
            .try_with(|rt| {
                rt.signal_subscribers
                    .try_borrow()
                    .ok()
                    .and_then(|ss| ss.get(&signal_id).map(|s| s.len()))
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    pub fn alloc_scope_id() -> ScopeId {
        RUNTIME.with(|rt| {
            let id = rt.next_scope_id.get();
            rt.next_scope_id.set(id + 1);
            id
        })
    }

    pub fn has_dirty_signals() -> bool {
        RUNTIME
            .try_with(|rt| rt.dirty_signals.get())
            .unwrap_or(false)
    }

    pub fn take_dirty_signals() -> bool {
        RUNTIME
            .try_with(|rt| rt.dirty_signals.replace(false))
            .unwrap_or(false)
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
        RUNTIME
            .try_with(|rt| {
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
                    sig_subs.entry(signal_id).or_default().insert(sub_id);
                }
                if let Ok(mut sub_sigs) = rt.subscriber_signals.try_borrow_mut() {
                    sub_sigs.entry(sub_id).or_default().insert(signal_id);
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
        let was_valid = RUNTIME
            .try_with(|rt| {
                if let Ok(gens) = rt.subscriber_generations.try_borrow() {
                    if let Some(&gen) = gens.get(sub_id.index as usize) {
                        gen == sub_id.generation
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .unwrap_or(false);

        if !was_valid {
            return;
        }

        Self::clear_subscriber_dependencies(sub_id);

        let removed = RUNTIME
            .try_with(|rt| {
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
            })
            .ok()
            .flatten();
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
                                let rank = ranks
                                    .as_ref()
                                    .and_then(|r| r.get(&sub).copied())
                                    .unwrap_or(0);
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
        f.debug_struct("WriteSignal").field("id", &self.id).finish()
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
            None => {
                panic!("Strata Reactive Guard: Memo read before initial value could be computed.")
            }
        }
    }
}

pub type CleanupFn = Option<Box<dyn FnOnce()>>;
pub type CleanupCell = Rc<RefCell<CleanupFn>>;

/// Reactive effect executing side-effects when subscribed signals mutate.
#[derive(Clone)]
pub struct Effect {
    subscriber_id: SubscriberId,
    disposed: Rc<Cell<bool>>,
    cleanup: CleanupCell,
}

impl Effect {
    pub fn new<F, R>(mut effect_fn: F) -> Self
    where
        F: FnMut() -> R + 'static,
        R: IntoEffectCleanup,
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
            let next_clean = {
                let _effect_guard = EffectBodyGuard::enter();
                effect_fn().into_effect_cleanup()
            };
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

// ============================================================================
// Component-Scoped Hook Cursor Stack & Lifecycle Management
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ComponentSegment {
    component_id: String,
    instance_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ComponentPath(Vec<ComponentSegment>);

struct PendingHookSlot;

struct ComponentInstance {
    slots: Vec<Box<dyn std::any::Any>>,
    mounted_children: std::collections::HashSet<ComponentPath>,
}

impl ComponentInstance {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            mounted_children: std::collections::HashSet::new(),
        }
    }
}

struct ActiveComponentFrame {
    path: ComponentPath,
    hook_cursor: usize,
    child_ordinals: std::collections::HashMap<String, usize>,
    visited_children: std::collections::HashSet<ComponentPath>,
    provided_context_count: usize,
}

struct HookStoreState {
    instances: std::collections::HashMap<ComponentPath, ComponentInstance>,
    active_stack: Vec<ActiveComponentFrame>,
    pending_key: Option<String>,
    next_hook_id: usize,
}

impl HookStoreState {
    fn new() -> Self {
        Self {
            instances: std::collections::HashMap::new(),
            active_stack: Vec::new(),
            pending_key: None,
            next_hook_id: 0,
        }
    }
}

struct ContextEntry {
    context_id: u64,
    value: Box<dyn std::any::Any>,
}

thread_local! {
    static HOOK_STORE: RefCell<HookStoreState> = RefCell::new(HookStoreState::new());
    static CONTEXT_STACK: RefCell<Vec<ContextEntry>> = const { RefCell::new(Vec::new()) };
    static NEXT_CONTEXT_ID: Cell<u64> = const { Cell::new(1) };
}

/// RAII guard emitted automatically at the top of every `#[component]` function body.
///
/// Tracks the component's hierarchical instance identity, resets its hook cursor to `0`
/// for the current render pass, pops component-scoped context providers on exit, and
/// disposes unmounted child component subtrees (running their `use_effect` cleanups)
/// when the render finishes without unwinding.
pub struct ComponentScopeGuard {
    _private: (),
}

impl ComponentScopeGuard {
    /// Enters a component render scope for `component_id`.
    pub fn enter(component_id: &'static str) -> Self {
        HOOK_STORE.with(|store| {
            let mut st = store.borrow_mut();
            let explicit_key = st.pending_key.take();

            let path = if let Some(parent) = st.active_stack.last_mut() {
                let instance_key = match explicit_key {
                    Some(k) => format!("k:{k}"),
                    None => {
                        let ord = parent
                            .child_ordinals
                            .entry(component_id.to_string())
                            .or_insert(0);
                        let current = *ord;
                        *ord += 1;
                        format!("#{current}")
                    }
                };
                let mut segs = parent.path.0.clone();
                segs.push(ComponentSegment {
                    component_id: component_id.to_string(),
                    instance_key,
                });
                let child_path = ComponentPath(segs);
                parent.visited_children.insert(child_path.clone());
                child_path
            } else {
                let instance_key = match explicit_key {
                    Some(k) => format!("k:{k}"),
                    None => "#0".to_string(),
                };
                ComponentPath(vec![ComponentSegment {
                    component_id: component_id.to_string(),
                    instance_key,
                }])
            };

            st.instances
                .entry(path.clone())
                .or_insert_with(ComponentInstance::new);

            st.active_stack.push(ActiveComponentFrame {
                path,
                hook_cursor: 0,
                child_ordinals: std::collections::HashMap::new(),
                visited_children: std::collections::HashSet::new(),
                provided_context_count: 0,
            });
        });

        Self { _private: () }
    }
}

fn collect_unmounted_subtree(
    instances: &mut std::collections::HashMap<ComponentPath, ComponentInstance>,
    root: &ComponentPath,
    out: &mut Vec<ComponentInstance>,
) {
    if let Some(inst) = instances.remove(root) {
        let children: Vec<ComponentPath> = inst.mounted_children.iter().cloned().collect();
        out.push(inst);
        for child in children {
            collect_unmounted_subtree(instances, &child, out);
        }
    }
}

impl Drop for ComponentScopeGuard {
    fn drop(&mut self) {
        let unwinding = std::thread::panicking();
        let (pop_contexts, evicted) = HOOK_STORE.with(|store| {
            let mut st = store.borrow_mut();
            let Some(frame) = st.active_stack.pop() else {
                return (0usize, Vec::new());
            };
            let mut evicted = Vec::new();
            if !unwinding {
                let prev_children = st
                    .instances
                    .get_mut(&frame.path)
                    .map(|inst| {
                        std::mem::replace(&mut inst.mounted_children, frame.visited_children.clone())
                    })
                    .unwrap_or_default();
                for stale in prev_children {
                    if !frame.visited_children.contains(&stale) {
                        collect_unmounted_subtree(&mut st.instances, &stale, &mut evicted);
                    }
                }
            }
            (frame.provided_context_count, evicted)
        });

        if pop_contexts > 0 {
            CONTEXT_STACK.with(|stack| {
                let mut stack = stack.borrow_mut();
                let new_len = stack.len().saturating_sub(pop_contexts);
                stack.truncate(new_len);
            });
        }

        // Drop evicted component instances OUTSIDE any HOOK_STORE borrow so `use_effect`
        // cleanup callbacks can freely inspect or update signals without borrow conflicts.
        drop(evicted);
    }
}

/// Assigns an explicit reconciliation key to the immediate child component rendered inside `f`,
/// preserving its hook state across list reorderings.
pub fn with_component_key<R>(key: impl std::fmt::Display, f: impl FnOnce() -> R) -> R {
    struct KeyRestoreGuard(Option<String>);
    impl Drop for KeyRestoreGuard {
        fn drop(&mut self) {
            let prev = self.0.take();
            let _ = HOOK_STORE.try_with(|store| {
                if let Ok(mut st) = store.try_borrow_mut() {
                    st.pending_key = prev;
                }
            });
        }
    }

    let prev = HOOK_STORE.with(|store| store.borrow_mut().pending_key.replace(key.to_string()));
    let _guard = KeyRestoreGuard(prev);
    f()
}

/// Evaluates `f` inside an implicit root component scope when no `#[component]` scope is
/// active on the current thread.
///
/// Wrapping plain root view closures (e.g. `|| rsx! { <> <Child /> <Child /> </> }`) gives
/// top-level unkeyed sibling components distinct ordinals (`#0`, `#1`, ...) instead of colliding
/// on `#0`, and runs child unmount / `use_effect` cleanup when siblings are toggled off.
pub fn with_root_component_scope<R>(f: impl FnOnce() -> R) -> R {
    let has_active = HOOK_STORE.with(|store| !store.borrow().active_stack.is_empty());
    if has_active {
        f()
    } else {
        let _root_scope = ComponentScopeGuard::enter("__strata_root__");
        f()
    }
}

/// Clears all persisted component-scoped hook state and runs pending effect cleanups on this
/// thread. Intended for test isolation or full application resets.
pub fn reset_component_hooks() {
    let evicted: Vec<ComponentInstance> = HOOK_STORE.with(|store| {
        let mut st = store.borrow_mut();
        st.active_stack.clear();
        st.pending_key = None;
        st.next_hook_id = 0;
        st.instances.drain().map(|(_, inst)| inst).collect()
    });
    CONTEXT_STACK.with(|stack| stack.borrow_mut().clear());
    drop(evicted);
}

thread_local! {
    static EFFECT_BODY_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct EffectBodyGuard;

impl EffectBodyGuard {
    #[inline]
    fn enter() -> Self {
        EFFECT_BODY_DEPTH.with(|d| d.set(d.get() + 1));
        Self
    }
}

impl Drop for EffectBodyGuard {
    #[inline]
    fn drop(&mut self) {
        let _ = EFFECT_BODY_DEPTH.try_with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Returns `true` when executing directly inside a `#[component]` render body (not inside an
/// `Effect` or `use_effect_with_deps` callback where hook slots must not be allocated).
#[inline]
pub(crate) fn in_component_hook_context() -> bool {
    let in_effect = EFFECT_BODY_DEPTH.with(|d| d.get() > 0);
    if in_effect {
        return false;
    }
    HOOK_STORE.with(|store| !store.borrow().active_stack.is_empty())
}

/// Looks up or initializes the current hook slot for the active component frame.
///
/// Returns `None` when called outside any `#[component]` scope so standalone unit tests
/// and top-level state builders continue working without a component frame.
pub(crate) fn use_hook_slot_cloned<T: Clone + 'static>(init: impl FnOnce() -> T) -> Option<T> {
    enum Lookup<T> {
        NoScope,
        Hit(T),
        Miss(ComponentPath, usize),
    }

    let lookup = HOOK_STORE.with(|store| {
        let mut st = store.borrow_mut();
        let Some(frame) = st.active_stack.last_mut() else {
            return Lookup::NoScope;
        };
        let path = frame.path.clone();
        let idx = frame.hook_cursor;
        frame.hook_cursor += 1;

        let inst = st
            .instances
            .entry(path.clone())
            .or_insert_with(ComponentInstance::new);

        if idx < inst.slots.len() && !inst.slots[idx].is::<PendingHookSlot>() {
            let existing = inst.slots[idx]
                .downcast_ref::<T>()
                .unwrap_or_else(|| {
                    panic!(
                        "Strata Rules of Hooks violation at slot #{idx} in component `{}`: hook type mismatch across renders (avoid calling hooks conditionally).",
                        path.0
                            .last()
                            .map(|s| s.component_id.as_str())
                            .unwrap_or("<root>")
                    )
                })
                .clone();
            Lookup::Hit(existing)
        } else {
            while inst.slots.len() <= idx {
                inst.slots.push(Box::new(PendingHookSlot));
            }
            Lookup::Miss(path, idx)
        }
    });

    match lookup {
        Lookup::NoScope => None,
        Lookup::Hit(val) => Some(val),
        Lookup::Miss(path, idx) => {
            // Run `init()` completely outside any HOOK_STORE borrow so closures may
            // read/write signals, access context, or construct reactive subscribers safely.
            let val = init();
            HOOK_STORE.with(|store| {
                let mut st = store.borrow_mut();
                if let Some(inst) = st.instances.get_mut(&path) {
                    if idx < inst.slots.len() {
                        inst.slots[idx] = Box::new(val.clone());
                    }
                }
            });
            Some(val)
        }
    }
}

/// Like [`use_hook_slot_cloned`], but falls back directly to `init()` when outside a
/// `#[component]` scope so non-`Clone` `FnOnce` initializers are only moved once.
fn use_hook_slot_or_init<T: Clone + 'static>(init: impl FnOnce() -> T) -> T {
    enum Lookup<T> {
        NoScope,
        Hit(T),
        Miss(ComponentPath, usize),
    }

    let lookup = HOOK_STORE.with(|store| {
        let mut st = store.borrow_mut();
        let Some(frame) = st.active_stack.last_mut() else {
            return Lookup::NoScope;
        };
        let path = frame.path.clone();
        let idx = frame.hook_cursor;
        frame.hook_cursor += 1;

        let inst = st
            .instances
            .entry(path.clone())
            .or_insert_with(ComponentInstance::new);

        if idx < inst.slots.len() && !inst.slots[idx].is::<PendingHookSlot>() {
            let existing = inst.slots[idx]
                .downcast_ref::<T>()
                .unwrap_or_else(|| {
                    panic!(
                        "Strata Rules of Hooks violation at slot #{idx} in component `{}`: hook type mismatch across renders (avoid calling hooks conditionally).",
                        path.0
                            .last()
                            .map(|s| s.component_id.as_str())
                            .unwrap_or("<root>")
                    )
                })
                .clone();
            Lookup::Hit(existing)
        } else {
            while inst.slots.len() <= idx {
                inst.slots.push(Box::new(PendingHookSlot));
            }
            Lookup::Miss(path, idx)
        }
    });

    match lookup {
        Lookup::NoScope => init(),
        Lookup::Hit(val) => val,
        Lookup::Miss(path, idx) => {
            let val = init();
            HOOK_STORE.with(|store| {
                let mut st = store.borrow_mut();
                if let Some(inst) = st.instances.get_mut(&path) {
                    if idx < inst.slots.len() {
                        inst.slots[idx] = Box::new(val.clone());
                    }
                }
            });
            val
        }
    }
}

/// Owned wrapper stored inside a component's hook slot so unmounting the component
/// automatically disposes its `Effect` and executes the cleanup closure.
#[derive(Clone)]
struct OwnedComponentEffect {
    effect: Effect,
    _owner: Rc<EffectDropGuard>,
}

struct EffectDropGuard(Effect);

impl Drop for EffectDropGuard {
    fn drop(&mut self) {
        self.0.dispose();
    }
}

/// Owned wrapper for dependency-tracked `use_effect_with_deps` cleanup closures.
struct OwnedDepsEffect<D> {
    deps: Rc<RefCell<D>>,
    cleanup: CleanupCell,
    _owner: Rc<CleanupDropGuard>,
}

impl<D> Clone for OwnedDepsEffect<D> {
    fn clone(&self) -> Self {
        Self {
            deps: Rc::clone(&self.deps),
            cleanup: Rc::clone(&self.cleanup),
            _owner: Rc::clone(&self._owner),
        }
    }
}

struct CleanupDropGuard(CleanupCell);

impl Drop for CleanupDropGuard {
    fn drop(&mut self) {
        if let Ok(mut cell) = self.0.try_borrow_mut() {
            if let Some(clean) = cell.take() {
                clean();
            }
        }
    }
}

/// Trait allowing [`use_state`] to accept either a direct initial value (`use_state(0)`)
/// or a lazy initializer closure (`use_state(|| expensive())`), matching React's `useState`.
pub trait IntoInitialState<T, Marker = ()> {
    fn into_initial_state(self) -> T;
}

impl<T> IntoInitialState<T, ()> for T {
    #[inline]
    fn into_initial_state(self) -> T {
        self
    }
}

impl<T, F: FnOnce() -> T> IntoInitialState<T, ((),)> for F {
    #[inline]
    fn into_initial_state(self) -> T {
        self()
    }
}

// ============================================================================
// Ergonomic Component Hooks
// ============================================================================

/// Creates or retrieves a component-scoped reactive [`Signal`].
///
/// Inside a `#[component]`, the signal persists across re-renders at this hook cursor slot;
/// outside any component scope, allocates a fresh standalone `Signal`.
pub fn use_signal<T: Clone + 'static>(initial: T) -> Signal<T> {
    use_hook_slot_or_init(|| Signal::new(initial))
}

thread_local! {
    static PERSISTENT_SIGNALS: RefCell<std::collections::HashMap<String, Box<dyn std::any::Any>>> =
        RefCell::new(std::collections::HashMap::new());
}

/// Returns a signal that survives root re-invocation, identified by an explicit `key`.
///
/// Useful when sharing keyed state across disconnected subtrees or outside `#[component]`.
/// Asking for the same key with a different type panics, since that is a key clash.
pub fn use_persistent_signal<T: Clone + 'static>(key: &str, init: impl FnOnce() -> T) -> Signal<T> {
    PERSISTENT_SIGNALS.with(|store| {
        let mut store = store.borrow_mut();
        if let Some(existing) = store.get(key) {
            return existing
                .downcast_ref::<Signal<T>>()
                .unwrap_or_else(|| panic!("persistent signal key `{key}` reused with another type"))
                .clone();
        }
        let signal = Signal::new(init());
        store.insert(key.to_string(), Box::new(signal.clone()));
        signal
    })
}

/// Drops all persistent signals and component-scoped hooks on this thread. Intended for tests.
pub fn clear_persistent_signals() {
    PERSISTENT_SIGNALS.with(|store| store.borrow_mut().clear());
    reset_component_hooks();
}

/// React-style component-scoped state hook returning decoupled `(ReadSignal<T>, WriteSignal<T>)`.
///
/// Accepts either a direct value (`use_state(0)`) or a lazy initializer closure
/// (`use_state(|| compute_initial())`). Inside a `#[component]`, the initializer runs only on
/// mount and the underlying signal persists across re-renders via the component hook cursor.
pub fn use_state<T: Clone + 'static, M>(
    initial: impl IntoInitialState<T, M>,
) -> (ReadSignal<T>, WriteSignal<T>) {
    let sig = use_hook_slot_or_init(|| Signal::new(initial.into_initial_state()));
    sig.split()
}

/// Component-scoped derived [`Memo`] that persists its reactive subscription across re-renders
/// and unregisters automatically when the component unmounts.
pub fn use_memo<T: Clone + PartialEq + 'static>(compute: impl FnMut() -> T + 'static) -> Memo<T> {
    use_hook_slot_or_init(|| Memo::new(compute))
}

/// React-style dependency-array memoization hook (`useMemo(() => compute(&deps), [deps])`).
///
/// Computes `compute(&deps)` on mount and recomputes only when `deps` changes (`PartialEq`)
/// across component re-renders, returning a cached clone in between.
pub fn use_memo_with_deps<D, T, F>(deps: D, compute: F) -> T
where
    D: PartialEq + 'static,
    T: Clone + 'static,
    F: FnOnce(&D) -> T,
{
    let mut deps_opt = Some(deps);
    let mut compute_opt = Some(compute);

    if let Some(cell) = use_hook_slot_cloned(|| {
        let initial_deps = deps_opt.take().unwrap();
        let initial_val = (compute_opt.take().unwrap())(&initial_deps);
        Rc::new(RefCell::new((initial_deps, initial_val)))
    }) {
        if let (Some(new_deps), Some(run_fn)) = (deps_opt.take(), compute_opt.take()) {
            let changed = cell.borrow().0 != new_deps;
            if changed {
                let next_val = run_fn(&new_deps);
                let mut guard = cell.borrow_mut();
                guard.0 = new_deps;
                guard.1 = next_val;
            }
        }
        return cell.borrow().1.clone();
    }

    let d = deps_opt.take().unwrap();
    (compute_opt.take().unwrap())(&d)
}

/// Component-scoped reactive [`Effect`] that tracks read signals, runs cleanup before each
/// re-execution, avoids duplicate registrations across re-renders, and automatically runs its
/// cleanup closure when the owning component unmounts.
pub fn use_effect<F, R>(effect_fn: F) -> Effect
where
    F: FnMut() -> R + 'static,
    R: IntoEffectCleanup,
{
    let mut effect_opt = Some(effect_fn);
    if let Some(owned) = use_hook_slot_cloned(|| {
        let eff = Effect::new(effect_opt.take().unwrap());
        OwnedComponentEffect {
            effect: eff.clone(),
            _owner: Rc::new(EffectDropGuard(eff)),
        }
    }) {
        owned.effect
    } else {
        Effect::new(effect_opt.take().unwrap())
    }
}

/// Converts the return value of a [`use_effect_with_deps`] callback into an optional cleanup closure.
///
/// Implemented for `()` (no cleanup, eliminating trailing `None`),
/// `Option<Box<dyn FnOnce()>>` (backward compatible), `Box<dyn FnOnce()>`, and `Box<F>` for any
/// `F: FnOnce() + 'static` without requiring manual trait-object casts.
pub trait IntoEffectCleanup {
    fn into_effect_cleanup(self) -> Option<Box<dyn FnOnce()>>;
}

impl IntoEffectCleanup for () {
    #[inline]
    fn into_effect_cleanup(self) -> Option<Box<dyn FnOnce()>> {
        None
    }
}

impl IntoEffectCleanup for Option<Box<dyn FnOnce()>> {
    #[inline]
    fn into_effect_cleanup(self) -> Option<Box<dyn FnOnce()>> {
        self
    }
}

impl IntoEffectCleanup for Box<dyn FnOnce()> {
    #[inline]
    fn into_effect_cleanup(self) -> Option<Box<dyn FnOnce()>> {
        Some(self)
    }
}

impl<F: FnOnce() + 'static> IntoEffectCleanup for Box<F> {
    #[inline]
    fn into_effect_cleanup(self) -> Option<Box<dyn FnOnce()>> {
        Some(self)
    }
}

/// React-style dependency-array effect hook.
///
/// Runs `effect_fn(&deps)` on mount and whenever `deps` changes (`PartialEq`) across component
/// re-renders, invoking the prior cleanup closure before re-running and again on unmount.
/// Callbacks may return `()` (no cleanup), `Box::new(move || ...)` / `Box<dyn FnOnce()>`,
/// or `Option<Box<dyn FnOnce()>>`.
pub fn use_effect_with_deps<D, R, F>(deps: D, effect_fn: F)
where
    D: PartialEq + 'static,
    R: IntoEffectCleanup,
    F: FnOnce(&D) -> R,
{
    let mut effect_opt = Some(effect_fn);
    let mut deps_opt = Some(deps);

    if let Some(slot) = use_hook_slot_cloned(|| {
        let initial_deps = deps_opt.take().unwrap();
        let initial_clean = {
            let _effect_guard = EffectBodyGuard::enter();
            (effect_opt.take().unwrap())(&initial_deps).into_effect_cleanup()
        };
        let cleanup: CleanupCell = Rc::new(RefCell::new(initial_clean));
        OwnedDepsEffect {
            deps: Rc::new(RefCell::new(initial_deps)),
            cleanup: cleanup.clone(),
            _owner: Rc::new(CleanupDropGuard(cleanup)),
        }
    }) {
        // If `effect_opt` is still `Some`, this is a re-render (slot already existed on mount).
        if let (Some(new_deps), Some(run_fn)) = (deps_opt.take(), effect_opt.take()) {
            let changed = *slot.deps.borrow() != new_deps;
            if changed {
                let prev_clean = slot.cleanup.borrow_mut().take();
                if let Some(clean) = prev_clean {
                    clean();
                }
                let next_clean = {
                    let _effect_guard = EffectBodyGuard::enter();
                    run_fn(&new_deps).into_effect_cleanup()
                };
                *slot.deps.borrow_mut() = new_deps;
                *slot.cleanup.borrow_mut() = next_clean;
            }
        }
    } else if let (Some(d), Some(run_fn)) = (deps_opt.take(), effect_opt.take()) {
        // Standalone execution outside any component scope: run immediately.
        let _effect_guard = EffectBodyGuard::enter();
        let _ = run_fn(&d).into_effect_cleanup();
    }
}

struct ReducerSlot<S: Clone + 'static, A: 'static, R: 'static> {
    state: Signal<S>,
    reducer: Rc<RefCell<R>>,
    dispatch: Rc<dyn Fn(A)>,
}

impl<S: Clone + 'static, A: 'static, R: 'static> Clone for ReducerSlot<S, A, R> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            reducer: Rc::clone(&self.reducer),
            dispatch: Rc::clone(&self.dispatch),
        }
    }
}

/// React-style `useReducer` hook returning `(ReadSignal<S>, Rc<dyn Fn(A)>)`.
///
/// Persists state, reducer closure, and `dispatch` handle in the component hook slot so
/// `dispatch` preserves pointer identity (`Rc::ptr_eq`) across re-renders.
pub fn use_reducer<S, A, R>(reducer: R, initial_state: S) -> (ReadSignal<S>, Rc<dyn Fn(A)>)
where
    S: Clone + 'static,
    A: 'static,
    R: FnMut(&S, A) -> S + 'static,
{
    use_reducer_init(reducer, initial_state, |s| s)
}

/// React-style 3-argument `useReducer` hook with lazy state initialization (`init(initial_arg)`).
///
/// Preserves the `dispatch` `Rc` identity across component re-renders while refreshing the active
/// reducer closure so inline reducers always see up-to-date prop captures.
pub fn use_reducer_init<S, A, I, R, F>(
    reducer: R,
    initial_arg: I,
    init: F,
) -> (ReadSignal<S>, Rc<dyn Fn(A)>)
where
    S: Clone + 'static,
    A: 'static,
    R: FnMut(&S, A) -> S + 'static,
    F: FnOnce(I) -> S,
{
    let mut reducer_opt = Some(reducer);
    let mut init_opt = Some((initial_arg, init));

    let slot = use_hook_slot_or_init(|| {
        let (arg, init_fn) = init_opt.take().unwrap();
        let state = Signal::new(init_fn(arg));
        let reducer_cell = Rc::new(RefCell::new(reducer_opt.take().unwrap()));
        let dispatch_sig = state.clone();
        let dispatch_reducer = Rc::clone(&reducer_cell);
        let dispatch: Rc<dyn Fn(A)> = Rc::new(move |action: A| {
            let current = dispatch_sig.untracked_get();
            let next = {
                let mut r = dispatch_reducer.borrow_mut();
                r(&current, action)
            };
            dispatch_sig.set(next);
        });
        ReducerSlot {
            state,
            reducer: reducer_cell,
            dispatch,
        }
    });

    if let Some(latest_reducer) = reducer_opt.take() {
        *slot.reducer.borrow_mut() = latest_reducer;
    }

    (slot.state.split().0, slot.dispatch)
}

/// Mutable, non-reactive reference container persisted across component re-renders (React `useRef`).
///
/// Mutating a [`Ref`] never marks the reactive graph dirty or triggers a re-render.
pub struct Ref<T> {
    inner: Rc<RefCell<T>>,
}

impl<T> Clone for Ref<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ref")
            .field("current", &*self.inner.borrow())
            .finish()
    }
}

impl<T: 'static> Ref<T> {
    pub fn new(initial: T) -> Self {
        Self {
            inner: Rc::new(RefCell::new(initial)),
        }
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.inner.borrow().clone()
    }

    pub fn set(&self, value: T) {
        *self.inner.borrow_mut() = value;
    }

    pub fn replace(&self, value: T) -> T {
        std::mem::replace(&mut *self.inner.borrow_mut(), value)
    }

    pub fn borrow(&self) -> std::cell::Ref<'_, T> {
        self.inner.borrow()
    }

    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, T> {
        self.inner.borrow_mut()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&*self.inner.borrow())
    }

    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut *self.inner.borrow_mut())
    }
}

/// React-style `useRef` hook returning a mutable [`Ref<T>`] that persists across re-renders
/// without triggering reactive updates.
pub fn use_ref<T: 'static>(initial: T) -> Ref<T> {
    use_hook_slot_or_init(|| Ref::new(initial))
}

/// React-style `useCallback` hook that memoizes a closure `Rc<F>` across re-renders while
/// `deps` remains equal (`PartialEq`).
///
/// When `deps` is unchanged, the returned `Rc<F>` preserves exact pointer identity (`Rc::ptr_eq`).
pub fn use_callback<D, F>(deps: D, callback: F) -> Rc<F>
where
    D: PartialEq + 'static,
    F: 'static,
{
    let mut deps_opt = Some(deps);
    let mut cb_opt = Some(callback);

    if let Some(cell) = use_hook_slot_cloned(|| {
        Rc::new(RefCell::new((
            deps_opt.take().unwrap(),
            Rc::new(cb_opt.take().unwrap()),
        )))
    }) {
        if let (Some(new_deps), Some(new_cb)) = (deps_opt.take(), cb_opt.take()) {
            let mut guard = cell.borrow_mut();
            if guard.0 != new_deps {
                guard.0 = new_deps;
                guard.1 = Rc::new(new_cb);
            }
            return Rc::clone(&guard.1);
        }
        return Rc::clone(&cell.borrow().1);
    }

    Rc::new(cb_opt.take().unwrap())
}

/// React-style `useId` hook generating a stable unique identifier (e.g. `":r0:"`) that persists
/// across re-renders of the same component instance and is unique across sibling instances.
pub fn use_id() -> String {
    let alloc = || {
        HOOK_STORE.with(|store| {
            let mut st = store.borrow_mut();
            let id = st.next_hook_id;
            st.next_hook_id += 1;
            format!(":r{id}:")
        })
    };
    use_hook_slot_cloned(alloc).unwrap_or_else(alloc)
}

// ============================================================================
// React-Style Context API (`create_context` / `provide_context` / `use_context`)
// ============================================================================

/// Typed context handle created by [`create_context`].
///
/// Provide values to descendant components either via [`provide_context`] (scoped to the
/// enclosing `#[component]`) or [`Context::provide`] (scoped to an explicit closure).
pub struct Context<T: Clone + 'static> {
    id: u64,
    default_value: Rc<T>,
}

impl<T: Clone + 'static> Clone for Context<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            default_value: Rc::clone(&self.default_value),
        }
    }
}

impl<T: Clone + std::fmt::Debug + 'static> std::fmt::Debug for Context<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("id", &self.id)
            .field("default_value", &*self.default_value)
            .finish()
    }
}

impl<T: Clone + 'static> Context<T> {
    /// Returns this context's unique identifier.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Returns a clone of the default fallback value supplied at creation time.
    pub fn default_value(&self) -> T {
        (*self.default_value).clone()
    }

    /// Provides `value` to all components evaluated within `f`, restoring the previous
    /// context value automatically even if `f` unwinds.
    pub fn provide<R>(&self, value: T, f: impl FnOnce() -> R) -> R {
        struct ContextPopGuard;
        impl Drop for ContextPopGuard {
            fn drop(&mut self) {
                let _ = CONTEXT_STACK.try_with(|stack| {
                    if let Ok(mut st) = stack.try_borrow_mut() {
                        st.pop();
                    }
                });
            }
        }

        CONTEXT_STACK.with(|stack| {
            stack.borrow_mut().push(ContextEntry {
                context_id: self.id,
                value: Box::new(value),
            });
        });
        let _guard = ContextPopGuard;
        f()
    }
}

/// Creates a new typed [`Context<T>`] with a fallback `default_value`.
pub fn create_context<T: Clone + 'static>(default_value: T) -> Context<T> {
    let id = NEXT_CONTEXT_ID.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    });
    Context {
        id,
        default_value: Rc::new(default_value),
    }
}

/// Provides `value` for `context` to all child components rendered in the remainder of the
/// current `#[component]` scope.
///
/// The provided entry is automatically popped when the enclosing `#[component]` finishes
/// rendering. Outside a `#[component]` scope, prefer [`Context::provide`].
pub fn provide_context<T: Clone + 'static>(context: &Context<T>, value: T) {
    CONTEXT_STACK.with(|stack| {
        stack.borrow_mut().push(ContextEntry {
            context_id: context.id,
            value: Box::new(value),
        });
    });
    HOOK_STORE.with(|store| {
        let mut st = store.borrow_mut();
        if let Some(frame) = st.active_stack.last_mut() {
            frame.provided_context_count += 1;
        }
    });
}

/// Reads the nearest ancestor value provided for `context`, or its default value when no
/// ancestor provider is active.
pub fn use_context<T: Clone + 'static>(context: &Context<T>) -> T {
    CONTEXT_STACK.with(|stack| {
        let stack = stack.borrow();
        for entry in stack.iter().rev() {
            if entry.context_id == context.id {
                if let Some(val) = entry.value.downcast_ref::<T>() {
                    return val.clone();
                }
            }
        }
        context.default_value()
    })
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
