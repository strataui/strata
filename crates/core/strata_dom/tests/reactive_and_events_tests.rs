use std::cell::{Cell, RefCell};
use std::rc::Rc;
use strata_dom::{
    batch, use_effect, use_memo, use_signal, Event, EventDispatcher, EventPhase, EventType,
    ReactiveRuntime, SubscriberId,
};

#[test]
fn test_signal_basic_get_set_update() {
    let s = use_signal(42);
    assert_eq!(s.get(), 42);

    s.set(100);
    assert_eq!(s.get(), 100);

    let prev = s.update(|v| {
        let old = *v;
        *v += 5;
        old
    });
    assert_eq!(prev, 100);
    assert_eq!(s.get(), 105);
}

#[test]
fn test_memo_lazy_cached_dependency_tracking() {
    let a = use_signal(10);
    let b = use_signal(20);

    let compute_runs = Rc::new(Cell::new(0));
    let runs_clone = compute_runs.clone();

    let a_clone = a.clone();
    let b_clone = b.clone();
    let sum = use_memo(move || {
        runs_clone.set(runs_clone.get() + 1);
        a_clone.get() + b_clone.get()
    });

    assert_eq!(sum.get(), 30);
    assert_eq!(compute_runs.get(), 1);

    // Repeated gets without dependency mutation should hit cache
    assert_eq!(sum.get(), 30);
    assert_eq!(compute_runs.get(), 1);

    // Mutating an upstream signal invalidates cache
    a.set(15);
    assert_eq!(sum.get(), 35);
    assert_eq!(compute_runs.get(), 2);

    // Mutating other dependency
    b.set(25);
    assert_eq!(sum.get(), 40);
    assert_eq!(compute_runs.get(), 3);
}

#[test]
fn test_effect_reactivity_and_cleanup() {
    let count = use_signal(0);
    let history = Rc::new(RefCell::new(Vec::new()));
    let cleanups = Rc::new(RefCell::new(Vec::new()));

    let hist_clone = history.clone();
    let clean_clone = cleanups.clone();
    let count_clone = count.clone();

    let effect = use_effect(move || {
        let current = count_clone.get();
        hist_clone.borrow_mut().push(current);

        let clean = clean_clone.clone();
        Some(Box::new(move || {
            clean.borrow_mut().push(current);
        }))
    });

    assert_eq!(*history.borrow(), vec![0]);
    assert_eq!(*cleanups.borrow(), Vec::<i32>::new());

    count.set(1);
    assert_eq!(*history.borrow(), vec![0, 1]);
    assert_eq!(*cleanups.borrow(), vec![0]); // Cleanup for 0 ran before 1 was computed

    count.set(2);
    assert_eq!(*history.borrow(), vec![0, 1, 2]);
    assert_eq!(*cleanups.borrow(), vec![0, 1]);

    // Manual disposal runs final cleanup
    effect.dispose();
    assert_eq!(*cleanups.borrow(), vec![0, 1, 2]);
}

#[test]
fn test_batched_signal_mutations() {
    let a = use_signal(1);
    let b = use_signal(2);

    let run_count = Rc::new(Cell::new(0));
    let rc = run_count.clone();

    let a_clone = a.clone();
    let b_clone = b.clone();
    let _effect = use_effect(move || {
        let _sum = a_clone.get() + b_clone.get();
        rc.set(rc.get() + 1);
        None
    });

    assert_eq!(run_count.get(), 1);

    // Inside batch: 2 mutations only trigger 1 collective effect execution
    batch(|| {
        a.set(10);
        b.set(20);
    });

    assert_eq!(run_count.get(), 2);
    assert_eq!(a.get(), 10);
    assert_eq!(b.get(), 20);
}

#[test]
fn test_synthetic_event_three_phase_dispatch() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let trace_clone = trace.clone();

    // Node path: 1 (Window/Root) -> 2 (Container) -> 3 (Button)
    let path = vec![1, 2, 3];
    let event = Event::new(EventType::Click);

    EventDispatcher::dispatch(&path, &event, move |node_id, phase, _| {
        trace_clone.borrow_mut().push((node_id, phase));
    });

    assert_eq!(
        *trace.borrow(),
        vec![
            (1, EventPhase::Capturing),
            (2, EventPhase::Capturing),
            (3, EventPhase::AtTarget),
            (2, EventPhase::Bubbling),
            (1, EventPhase::Bubbling),
        ]
    );
}

#[test]
fn test_synthetic_event_stop_propagation() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let trace_clone = trace.clone();

    let path = vec![1, 2, 3];
    let event = Event::new(EventType::Click);

    EventDispatcher::dispatch(&path, &event, move |node_id, phase, evt| {
        trace_clone.borrow_mut().push((node_id, phase));
        // Stop propagation at node 2 during bubbling
        if node_id == 2 && phase == EventPhase::Bubbling {
            evt.stop_propagation();
        }
    });

    assert_eq!(
        *trace.borrow(),
        vec![
            (1, EventPhase::Capturing),
            (2, EventPhase::Capturing),
            (3, EventPhase::AtTarget),
            (2, EventPhase::Bubbling),
            // (1, Bubbling) was prevented!
        ]
    );
}

#[test]
fn test_nested_memo_destructor_drop_safety() {
    // Tests that dropping closures that capture other Memos does NOT panic in destructor
    let base = use_signal(1);
    let mut prev = {
        let b = base.clone();
        use_memo(move || b.get())
    };
    let mut keep = Vec::new();
    for _ in 0..10 {
        let p = prev.clone();
        let next = use_memo(move || p.get() + 1);
        keep.push(prev);
        prev = next;
    }
    assert_eq!(prev.get(), 11);

    // Dropping this nested closure chain previously triggered RefCell already borrowed in Drop -> abort
    drop(keep);
    drop(prev);
    // Success: did not abort or panic!
}

#[test]
fn test_more_than_1000_subscribers_scaling() {
    let theme_sig = use_signal(0);
    let updated_count = Rc::new(Cell::new(0usize));

    let mut _effects = Vec::with_capacity(2500);
    for _ in 0..2500 {
        let ts = theme_sig.clone();
        let uc = updated_count.clone();
        _effects.push(use_effect(move || {
            let _ = ts.get();
            uc.set(uc.get() + 1);
            None
        }));
    }

    // Initial mount: all 2500 ran
    assert_eq!(updated_count.get(), 2500);

    // Mutation: all 2500 must run again (previously capped at 1000)
    updated_count.set(0);
    theme_sig.set(1);
    assert_eq!(updated_count.get(), 2500);
}

#[test]
fn test_use_effect_as_statement_persists() {
    let count = use_signal(0);
    let run_count = Rc::new(Cell::new(0usize));

    let rc = run_count.clone();
    let c = count.clone();
    // Called as an unassigned statement (temporary dropped immediately)
    use_effect(move || {
        let _ = c.get();
        rc.set(rc.get() + 1);
        None
    });

    assert_eq!(run_count.get(), 1);

    count.set(10);
    assert_eq!(run_count.get(), 2);

    count.set(20);
    assert_eq!(run_count.get(), 3);
}

#[test]
fn test_diamond_dag_glitch_freedom_atomic_consistency() {
    // Diamond DAG:
    //         A
    //       /   \
    //      B     C
    //       \   /
    //         D
    let a = use_signal(1);

    let a_clone_b = a.clone();
    let b = use_memo(move || a_clone_b.get() * 2);

    let a_clone_c = a.clone();
    let c = use_memo(move || a_clone_c.get() * 3);

    let b_clone = b.clone();
    let c_clone = c.clone();
    let d = use_memo(move || b_clone.get() + c_clone.get());

    let history = Rc::new(RefCell::new(Vec::new()));
    let hist_clone = history.clone();
    let d_clone = d.clone();

    let _effect = use_effect(move || {
        hist_clone.borrow_mut().push(d_clone.get());
        None
    });

    // Initial state: A=1, B=2, C=3, D=5
    assert_eq!(*history.borrow(), vec![5]);

    // Mutate root: A=10
    // Expected: B=20, C=30, D=50
    // In a glitched system without topological ordering, D would observe transient state 23 (B=20, C=3)
    a.set(10);

    // Glitch-free invariant: D transitions directly from 5 to 50 with ZERO intermediate states
    assert_eq!(*history.borrow(), vec![5, 50]);
}

#[test]
fn test_memoization_halts_propagation_on_equal_value() {
    let a = use_signal(10); // Even number

    // Memo b evaluates to 1 if odd, 0 if even
    let a_clone = a.clone();
    let b = use_memo(move || a_clone.get() % 2);

    let c_runs = Rc::new(Cell::new(0usize));
    let c_runs_clone = c_runs.clone();
    let b_clone = b.clone();

    // Memo c depends on b
    let c = use_memo(move || {
        c_runs_clone.set(c_runs_clone.get() + 1);
        b_clone.get() + 100
    });

    assert_eq!(c.get(), 100);
    assert_eq!(c_runs.get(), 1);

    // Mutate A from 10 to 12 (still even)
    // B evaluates to 12 % 2 = 0 (same as previous 0)
    // Propagation must HALT at B! C must NOT re-evaluate!
    a.set(12);
    assert_eq!(b.get(), 0);
    assert_eq!(c.get(), 100);
    assert_eq!(c_runs.get(), 1, "C must not re-evaluate when upstream memo output is unchanged");

    // Mutate A from 12 to 13 (now odd)
    // B evaluates to 13 % 2 = 1 (changed)
    // Propagation continues to C
    a.set(13);
    assert_eq!(b.get(), 1);
    assert_eq!(c.get(), 101);
    assert_eq!(c_runs.get(), 2, "C must re-evaluate when upstream memo output changes");
}

#[test]
fn test_generational_subscriber_disposal_and_leak_elimination() {
    let sig = use_signal(100);
    let sig_id = sig.id();

    let initial_active = ReactiveRuntime::active_subscriber_count();
    let initial_sig_subs = ReactiveRuntime::signal_subscription_count(sig_id);

    // Create 10 temporary effects subscribing to sig
    let mut effects = Vec::new();
    let counters = Rc::new(RefCell::new(Vec::new()));

    for i in 0..10 {
        let sig_clone = sig.clone();
        let counters_clone = counters.clone();
        let eff = use_effect(move || {
            let val = sig_clone.get();
            counters_clone.borrow_mut().push((i, val));
            None
        });
        effects.push(eff);
    }

    assert_eq!(ReactiveRuntime::active_subscriber_count(), initial_active + 10);
    assert_eq!(ReactiveRuntime::signal_subscription_count(sig_id), initial_sig_subs + 10);

    // Mutate signal, verify all 10 effects fire
    sig.set(200);
    assert_eq!(counters.borrow().len(), 20); // 10 on mount + 10 on update

    // Dispose first 5 effects explicitly
    for eff in effects.drain(0..5) {
        eff.dispose();
    }

    assert_eq!(ReactiveRuntime::active_subscriber_count(), initial_active + 5);
    assert_eq!(ReactiveRuntime::signal_subscription_count(sig_id), initial_sig_subs + 5);

    // Dispose remaining 5 effects
    for eff in effects.drain(..) {
        eff.dispose();
    }

    assert_eq!(ReactiveRuntime::active_subscriber_count(), initial_active);
    assert_eq!(ReactiveRuntime::signal_subscription_count(sig_id), 0);

    // Allocate a new subscriber - must recycle index from freed pool with bumped generation
    let sub1 = ReactiveRuntime::alloc_subscriber_id();
    assert!(ReactiveRuntime::is_subscriber_alive(sub1));
    assert_eq!(sub1.generation, 2, "Recycled subscriber slot must increment generation to 2");

    // Old generation (index, 1) is dead
    let dead_sub = SubscriberId { index: sub1.index, generation: 1 };
    assert!(!ReactiveRuntime::is_subscriber_alive(dead_sub));

    // Unregistering sub1 frees it and increments generation again
    ReactiveRuntime::unregister_subscriber(sub1);
    assert!(!ReactiveRuntime::is_subscriber_alive(sub1));

    let sub2 = ReactiveRuntime::alloc_subscriber_id();
    assert_eq!(sub2.index, sub1.index);
    assert_eq!(sub2.generation, 3, "Second recycled slot must increment generation to 3");
    ReactiveRuntime::unregister_subscriber(sub2);
}

