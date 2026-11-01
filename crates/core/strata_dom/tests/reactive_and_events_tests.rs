use std::cell::{Cell, RefCell};
use std::rc::Rc;
use strata_dom::{
    batch, use_effect, use_memo, use_signal, Event, EventDispatcher, EventPhase, EventType,
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
