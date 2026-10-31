//! In-Memory Query Cache with Stale-While-Revalidate and Invalidation.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Cached entry metadata and type-erased payload.
#[derive(Clone)]
struct CacheEntry {
    data: Arc<dyn Any + Send + Sync>,
    updated_at: Instant,
    stale_time: Duration,
}

impl std::fmt::Debug for CacheEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheEntry")
            .field("updated_at", &self.updated_at)
            .field("stale_time", &self.stale_time)
            .finish()
    }
}

impl CacheEntry {
    fn is_stale(&self) -> bool {
        self.updated_at.elapsed() > self.stale_time
    }
}

/// Global or scoped query cache instance.
#[derive(Debug, Clone, Default)]
pub struct QueryClient {
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

impl QueryClient {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Retrieve cached value if present and optionally return staleness flag.
    pub fn get<T: Clone + Send + Sync + 'static>(&self, key: &str) -> Option<(T, bool)> {
        let entries = self.entries.read().unwrap();
        if let Some(entry) = entries.get(key) {
            if let Some(typed_data) = entry.data.downcast_ref::<T>() {
                return Some((typed_data.clone(), entry.is_stale()));
            }
        }
        None
    }

    /// Store fresh data for a query key with specified stale duration.
    pub fn set<T: Send + Sync + 'static>(&self, key: impl Into<String>, data: T, stale_time: Duration) {
        let mut entries = self.entries.write().unwrap();
        entries.insert(
            key.into(),
            CacheEntry {
                data: Arc::new(data),
                updated_at: Instant::now(),
                stale_time,
            },
        );
    }

    /// Invalidate a specific query key, marking it as immediately stale.
    pub fn invalidate_queries(&self, key_prefix: &str) {
        let mut entries = self.entries.write().unwrap();
        for (k, entry) in entries.iter_mut() {
            if k.starts_with(key_prefix) {
                // Set stale time to zero to force immediate revalidation
                entry.stale_time = Duration::ZERO;
            }
        }
    }

    /// Clear all cached query data.
    pub fn clear(&self) {
        self.entries.write().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_cache_retrieval_and_invalidation() {
        let client = QueryClient::new();
        client.set("user:42", "Alice".to_string(), Duration::from_secs(60));

        let (data, is_stale) = client.get::<String>("user:42").unwrap();
        assert_eq!(data, "Alice");
        assert!(!is_stale);

        // Invalidate
        client.invalidate_queries("user:42");
        let (_, is_stale_now) = client.get::<String>("user:42").unwrap();
        assert!(is_stale_now);
    }
}
