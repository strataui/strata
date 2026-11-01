use strata_query::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

#[test]
fn test_query_lifecycle_and_stale_revalidation() {
    let client = QueryClient::new();
    let fetch_count = Rc::new(Cell::new(0));

    let fetch_count_clone = fetch_count.clone();
    let query = use_query::<i32, String, _>(
        &client,
        "metrics:qps",
        move || {
            fetch_count_clone.set(fetch_count_clone.get() + 1);
            Ok(15000)
        },
        QueryOptions {
            stale_time: Duration::from_secs(60),
            retry_count: 1,
        },
    );

    assert_eq!(fetch_count.get(), 1);
    assert_eq!(query.status.get(), QueryStatus::Success);
    assert_eq!(query.data.get(), Some(15000));
    assert!(!query.is_loading.get());

    // Invalidate and refetch
    client.invalidate_queries("metrics:qps");
    query.refetch();
    assert_eq!(fetch_count.get(), 2);
}

#[test]
fn test_mutation_pipeline() {
    let mutation = use_mutation(|id: u64| {
        if id == 0 {
            Err("ID cannot be zero")
        } else {
            Ok(format!("Deleted item #{id}"))
        }
    });

    assert!(!mutation.is_pending.get());

    mutation.mutate(0);
    assert!(!mutation.is_success.get());
    assert_eq!(mutation.error.get(), Some("ID cannot be zero"));

    mutation.mutate(42);
    assert!(mutation.is_success.get());
    assert_eq!(mutation.data.get(), Some("Deleted item #42".to_string()));
    assert_eq!(mutation.error.get(), None);
}
