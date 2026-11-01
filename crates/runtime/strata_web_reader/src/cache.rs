//! Offline article caching engine backed by Strata Storage Bitcask WAL.

use std::path::PathBuf;
use strata_storage::LocalStorage;

/// Key prefix distinguishing cached web reader articles in storage.
const ARTICLE_KEY_PREFIX: &str = "web_reader:article:";

/// Persistent offline cache for transpiled and raw HTML articles.
#[derive(Clone)]
pub struct ArticleCache {
    storage: LocalStorage,
}

impl ArticleCache {
    /// Opens or creates an article cache using the specified WAL storage path.
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let storage = LocalStorage::open(path)?;
        Ok(Self { storage })
    }

    /// Creates an article cache backed by the standard global LocalStorage instance.
    pub fn default_instance() -> Self {
        Self {
            storage: LocalStorage::default_instance(),
        }
    }

    /// Stores raw HTML content for an article slug or URL.
    pub fn store(&self, slug_or_url: &str, html: &str) {
        let key = format!("{}{}", ARTICLE_KEY_PREFIX, slug_or_url);
        self.storage.set(key, html);
    }

    /// Fetches raw HTML content for an article slug or URL if available.
    pub fn fetch(&self, slug_or_url: &str) -> Option<String> {
        let key = format!("{}{}", ARTICLE_KEY_PREFIX, slug_or_url);
        self.storage.get(&key)
    }

    /// Returns whether the specified article slug or URL is cached offline.
    pub fn contains(&self, slug_or_url: &str) -> bool {
        self.fetch(slug_or_url).is_some()
    }

    /// Removes an article from the offline cache.
    pub fn remove(&self, slug_or_url: &str) {
        let key = format!("{}{}", ARTICLE_KEY_PREFIX, slug_or_url);
        self.storage.remove(&key);
    }

    /// Returns a list of all currently cached article slugs/URLs.
    pub fn cached_articles(&self) -> Vec<String> {
        self.storage
            .keys()
            .into_iter()
            .filter_map(|k| k.strip_prefix(ARTICLE_KEY_PREFIX).map(|s| s.to_string()))
            .collect()
    }
}
