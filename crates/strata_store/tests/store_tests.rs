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
