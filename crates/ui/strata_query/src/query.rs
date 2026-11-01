//! Declarative Asynchronous Query Hook (use_query).

use std::rc::Rc;
use std::time::Duration;
use strata_dom::reactive::Signal;

use crate::cache::QueryClient;

/// Lifecycle status of an asynchronous query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryStatus {
    Pending,
    Success,
    Error,
}

/// Options to configure query caching and refetch behavior.
#[derive(Debug, Clone)]
pub struct QueryOptions {
    pub stale_time: Duration,
    pub retry_count: usize,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            stale_time: Duration::from_secs(30),
            retry_count: 2,
        }
    }
}

/// Reactive result handle returned by `use_query`.
#[derive(Clone)]
pub struct QueryResult<T: Clone + 'static, E: Clone + 'static> {
    pub data: Signal<Option<T>>,
    pub error: Signal<Option<E>>,
    pub status: Signal<QueryStatus>,
    pub is_loading: Signal<bool>,
    pub is_fetching: Signal<bool>,
    refetch_fn: Rc<dyn Fn()>,
}

impl<T: Clone + 'static, E: Clone + 'static> QueryResult<T, E> {
    /// Manually trigger a fresh refetch.
    pub fn refetch(&self) {
        (self.refetch_fn)();
    }
}

/// Hook to execute and cache an asynchronous query.
pub fn use_query<T, E, F>(
    client: &QueryClient,
    key: &str,
    fetcher: F,
    options: QueryOptions,
) -> QueryResult<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
    F: Fn() -> Result<T, E> + 'static,
{
    let data_signal = Signal::new(None);
    let error_signal = Signal::new(None);
    let status_signal = Signal::new(QueryStatus::Pending);
    let is_loading_signal = Signal::new(true);
    let is_fetching_signal = Signal::new(true);

    // 1. Check cache first (Stale-While-Revalidate)
    let mut initial_stale = true;
    if let Some((cached_data, is_stale)) = client.get::<T>(key) {
        data_signal.set(Some(cached_data));
        status_signal.set(QueryStatus::Success);
        is_loading_signal.set(false);
        initial_stale = is_stale;
    }

    let client_clone = client.clone();
    let key_str = key.to_string();
    let data_sig = data_signal.clone();
    let err_sig = error_signal.clone();
    let stat_sig = status_signal.clone();
    let loading_sig = is_loading_signal.clone();
    let fetching_sig = is_fetching_signal.clone();
    let fetcher_rc = Rc::new(fetcher);

    let execute_fetch = {
        let fetcher = fetcher_rc.clone();
        let key_str = key_str.clone();
        let client = client_clone.clone();
        let data_sig = data_sig.clone();
        let err_sig = err_sig.clone();
        let stat_sig = stat_sig.clone();
        let loading_sig = loading_sig.clone();
        let fetching_sig = fetching_sig.clone();
        let stale_time = options.stale_time;

        Rc::new(move || {
            fetching_sig.set(true);
            match fetcher() {
                Ok(fresh_data) => {
                    client.set(&key_str, fresh_data.clone(), stale_time);
                    data_sig.set(Some(fresh_data));
                    err_sig.set(None);
                    stat_sig.set(QueryStatus::Success);
                }
                Err(e) => {
                    err_sig.set(Some(e));
                    stat_sig.set(QueryStatus::Error);
                }
            }
            loading_sig.set(false);
            fetching_sig.set(false);
        })
    };

    // If cache was empty or stale, execute fetch immediately
    if initial_stale {
        execute_fetch();
    } else {
        is_fetching_signal.set(false);
    }

    QueryResult {
        data: data_signal,
        error: error_signal,
        status: status_signal,
        is_loading: is_loading_signal,
        is_fetching: is_fetching_signal,
        refetch_fn: execute_fetch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_use_query_fetch_and_cache() {
        let client = QueryClient::new();
        let call_count = Rc::new(std::cell::Cell::new(0));

        let query = {
            let count = call_count.clone();
            use_query::<String, String, _>(
                &client,
                "profile",
                move || {
                    count.set(count.get() + 1);
                    Ok(format!("User #{}", count.get()))
                },
                QueryOptions::default(),
            )
        };

        assert_eq!(call_count.get(), 1);
        assert_eq!(query.status.get(), QueryStatus::Success);
        assert_eq!(query.data.get().unwrap(), "User #1");

        // Subsequent query with same client uses cached data without refetch
        let count2 = call_count.clone();
        let query2 = use_query::<String, String, _>(
            &client,
            "profile",
            move || {
                count2.set(count2.get() + 1);
                Ok("Fresh".into())
            },
            QueryOptions::default(),
        );

        assert_eq!(call_count.get(), 1); // No new fetch occurred
        assert_eq!(query2.data.get().unwrap(), "User #1");

        // Manual refetch
        query.refetch();
        assert_eq!(call_count.get(), 2);
    }
}
