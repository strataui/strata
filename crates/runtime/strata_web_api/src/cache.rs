//! W3C Cache API & In-Memory HTTP Resource Store with Cache-Control and ETag validation.

use crate::fetch::{FetchError, Response};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Cached HTTP response entry with expiration and revalidation metadata.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub response: Response,
    pub stored_at: Instant,
    pub max_age: Option<Duration>,
    pub etag: Option<String>,
    pub no_cache: bool,
}

impl CacheEntry {
    /// Returns whether this cached entry has expired according to its max-age directive.
    pub fn is_fresh(&self) -> bool {
        if self.no_cache {
            return false;
        }
        match self.max_age {
            Some(max_age) => self.stored_at.elapsed() < max_age,
            None => true, // Indefinite if no expiration specified
        }
    }
}

/// A named W3C Cache storing request/response pairs.
#[derive(Debug, Clone, Default)]
pub struct Cache {
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Retrieves a fresh cached Response for the specified URL, if valid.
    pub fn match_url(&self, url: &str) -> Option<Response> {
        let entries = self.entries.read().ok()?;
        let entry = entries.get(url)?;
        if entry.is_fresh() {
            Some(entry.response.clone())
        } else {
            None
        }
    }

    /// Stores a response in the cache, respecting Cache-Control directives (`no-store`, `no-cache`, `max-age`).
    pub fn put(&self, url: &str, response: Response) -> Result<bool, FetchError> {
        // Inspect Cache-Control header
        let cache_control = response.headers.get("cache-control").unwrap_or("");
        let lower_cc = cache_control.to_ascii_lowercase();

        // If no-store is present, must not store in cache per RFC 7234
        if lower_cc.contains("no-store") {
            return Ok(false);
        }

        let no_cache = lower_cc.contains("no-cache");
        let mut max_age = None;

        for part in lower_cc.split(',') {
            let trimmed = part.trim();
            if let Some(sec_str) = trimmed.strip_prefix("max-age=") {
                if let Ok(secs) = sec_str.trim().parse::<u64>() {
                    max_age = Some(Duration::from_secs(secs));
                }
            }
        }

        let etag = response.headers.get("etag").map(|s| s.to_string());

        let entry = CacheEntry {
            response,
            stored_at: Instant::now(),
            max_age,
            etag,
            no_cache,
        };

        if let Ok(mut lock) = self.entries.write() {
            lock.insert(url.to_string(), entry);
            Ok(true)
        } else {
            Err(FetchError::IoError("Cache lock poisoned".into()))
        }
    }

    /// Deletes a cached entry by URL. Returns true if an entry was removed.
    pub fn delete(&self, url: &str) -> bool {
        if let Ok(mut lock) = self.entries.write() {
            lock.remove(url).is_some()
        } else {
            false
        }
    }

    /// Returns a list of all cached URLs in this cache.
    pub fn keys(&self) -> Vec<String> {
        self.entries
            .read()
            .map(|l| l.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Clears all entries from this cache.
    pub fn clear(&self) {
        if let Ok(mut lock) = self.entries.write() {
            lock.clear();
        }
    }
}

/// Global W3C CacheStorage managing multiple named caches.
#[derive(Debug, Clone, Default)]
pub struct CacheStorage {
    caches: Arc<RwLock<HashMap<String, Cache>>>,
}

impl CacheStorage {
    pub fn new() -> Self {
        Self {
            caches: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Opens or retrieves an existing named Cache.
    pub fn open(&self, name: &str) -> Cache {
        let mut lock = self.caches.write().expect("CacheStorage write lock");
        lock.entry(name.to_string()).or_default().clone()
    }

    /// Returns whether a cache with the given name exists.
    pub fn has(&self, name: &str) -> bool {
        self.caches.read().map(|l| l.contains_key(name)).unwrap_or(false)
    }

    /// Deletes a named cache. Returns true if it was present.
    pub fn delete(&self, name: &str) -> bool {
        self.caches.write().map(|mut l| l.remove(name).is_some()).unwrap_or(false)
    }

    /// Returns the names of all open caches.
    pub fn keys(&self) -> Vec<String> {
        self.caches.read().map(|l| l.keys().cloned().collect()).unwrap_or_default()
    }
}
