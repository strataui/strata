use serde::{Deserialize, Serialize};
use strata_store::prelude::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CartState {
    items: Vec<String>,
    total_cents: u64,
}

#[test]
fn test_cart_store_selectors_and_history() {
    let history_store = HistoryStore::new(
        CartState {
            items: Vec::new(),
            total_cents: 0,
        },
        10,
    );

    // Initial state
    assert_eq!(history_store.get().total_cents, 0);

    // Add item
    history_store.update(|c| {
        c.items.push("Strata Pro License".to_string());
        c.total_cents += 9900;
    });

    assert_eq!(history_store.get().total_cents, 9900);
    assert_eq!(history_store.get().items.len(), 1);

    // Add another item
    history_store.update(|c| {
        c.items.push("Support Plan".to_string());
        c.total_cents += 4900;
    });

    assert_eq!(history_store.get().total_cents, 14800);
    assert_eq!(history_store.get().items.len(), 2);

    // Undo last addition
    assert!(history_store.undo());
    assert_eq!(history_store.get().total_cents, 9900);
    assert_eq!(history_store.get().items.len(), 1);

    // Redo
    assert!(history_store.redo());
    assert_eq!(history_store.get().total_cents, 14800);
}

#[derive(ReactiveStore, Clone, Debug, Default, PartialEq)]
pub struct ClusterNodeStoreState {
    pub hostname: String,
    pub load_avg: f32,
    pub active_tasks: u32,
}

#[test]
fn test_reactive_store_derive_integration() {
    let initial = ClusterNodeStoreState {
        hostname: "worker-01".to_string(),
        load_avg: 1.25,
        active_tasks: 8,
    };

    let store = initial.into_store();
    assert_eq!(store.hostname.get(), "worker-01");
    assert_eq!(store.load_avg.get(), 1.25);
    assert_eq!(store.active_tasks.get(), 8);

    store.batch(|s| {
        s.load_avg.set(2.85);
        s.active_tasks.set(16);
    });

    assert_eq!(store.load_avg.get(), 2.85);
    assert_eq!(store.active_tasks.get(), 16);

    let snap = store.snapshot();
    assert_eq!(snap.load_avg, 2.85);
    assert_eq!(snap.active_tasks, 16);
}

