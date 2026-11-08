//! Concurrent In-Memory Cache with Bitcask WAL Persistence.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use crate::bitcask::StrataKV;

/// Persistent LocalStorage with instant in-memory reads and background WAL durability.
#[derive(Clone)]
pub struct LocalStorage {
    cache: Arc<RwLock<HashMap<String, String>>>,
    kv: Arc<Mutex<StrataKV>>,
}

impl LocalStorage {
    /// Opens or initializes LocalStorage at the specified WAL file path.
    pub fn open(wal_path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let mut kv = StrataKV::open(wal_path.into())?;
        let mut cache = HashMap::new();

        // Hydrate in-memory cache from WAL keys
        for key in kv.keys() {
            if let Ok(Some(bytes)) = kv.get(&key) {
                if let Ok(s) = String::from_utf8(bytes) {
                    cache.insert(key, s);
                }
            }
        }

        Ok(Self {
            cache: Arc::new(RwLock::new(cache)),
            kv: Arc::new(Mutex::new(kv)),
        })
    }

    /// Default global LocalStorage instance anchored to `app_data/storage.wal`.
    pub fn default_instance() -> Self {
        let wal_path = strata_vfs::driver::to_physical_path(&strata_vfs::app_data().join("storage.wal"))
            .unwrap_or_else(|_| {
                std::env::temp_dir().join("strata_default_storage.wal")
            });
        Self::open(wal_path).unwrap_or_else(|_| {
            let tmp_path = std::env::temp_dir().join("strata_default_storage.wal");
            Self::open(tmp_path).expect("Failed to open fallback storage WAL")
        })
    }

    /// Instant $O(1)$ zero-latency read from in-memory cache.
    pub fn get(&self, key: &str) -> Option<String> {
        if let Ok(lock) = self.cache.read() {
            lock.get(key).cloned()
        } else {
            None
        }
    }

    /// Stores a value, updating in-memory cache and appending to WAL.
    pub fn set(&self, key: impl Into<String>, val: impl Into<String>) {
        let k = key.into();
        let v = val.into();

        if let Ok(mut lock) = self.cache.write() {
            lock.insert(k.clone(), v.clone());
        }

        if let Ok(mut kv_lock) = self.kv.lock() {
            let _ = kv_lock.put(&k, v.as_bytes());
        }
    }

    /// Removes a key from both in-memory cache and WAL.
    pub fn remove(&self, key: &str) -> bool {
        let removed = if let Ok(mut lock) = self.cache.write() {
            lock.remove(key).is_some()
        } else {
            false
        };

        if removed {
            if let Ok(mut kv_lock) = self.kv.lock() {
                let _ = kv_lock.delete(key);
            }
        }

        removed
    }

    /// Clears all stored key-value pairs.
    pub fn clear(&self) {
        let keys_to_delete: Vec<String> = if let Ok(mut lock) = self.cache.write() {
            let keys = lock.keys().cloned().collect();
            lock.clear();
            keys
        } else {
            Vec::new()
        };

        if let Ok(mut kv_lock) = self.kv.lock() {
            for key in keys_to_delete {
                let _ = kv_lock.delete(&key);
            }
        }
    }

    /// Number of active items in storage.
    pub fn len(&self) -> usize {
        if let Ok(lock) = self.cache.read() {
            lock.len()
        } else {
            0
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a list of all keys in storage.
    pub fn keys(&self) -> Vec<String> {
        if let Ok(lock) = self.cache.read() {
            lock.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Explicitly trigger WAL compaction.
    pub fn compact(&self) -> std::io::Result<()> {
        if let Ok(mut kv_lock) = self.kv.lock() {
            kv_lock.compact()?;
        }
        Ok(())
    }
}

/// Volatile in-memory SessionStorage (discarded on application exit).
#[derive(Clone, Default)]
pub struct SessionStorage {
    cache: Arc<RwLock<HashMap<String, String>>>,
}

impl SessionStorage {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if let Ok(lock) = self.cache.read() {
            lock.get(key).cloned()
        } else {
            None
        }
    }

    pub fn set(&self, key: impl Into<String>, val: impl Into<String>) {
        if let Ok(mut lock) = self.cache.write() {
            lock.insert(key.into(), val.into());
        }
    }

    pub fn remove(&self, key: &str) -> bool {
        if let Ok(mut lock) = self.cache.write() {
            lock.remove(key).is_some()
        } else {
            false
        }
    }

    pub fn clear(&self) {
        if let Ok(mut lock) = self.cache.write() {
            lock.clear();
        }
    }

    pub fn len(&self) -> usize {
        if let Ok(lock) = self.cache.read() {
            lock.len()
        } else {
            0
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn keys(&self) -> Vec<String> {
        if let Ok(lock) = self.cache.read() {
            lock.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }
}
