//! Bitcask WAL Persistence Middleware for Strata Store.

use serde::de::DeserializeOwned;
use serde::Serialize;
use strata_storage::LocalStorage;
use crate::store::Store;

/// Configuration options for store persistence.
#[derive(Clone)]
pub struct PersistOptions {
    pub key: String,
    pub storage: Option<LocalStorage>,
}

impl PersistOptions {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            storage: None,
        }
    }

    pub fn with_storage(mut self, storage: LocalStorage) -> Self {
        self.storage = Some(storage);
        self
    }
}

/// Creates a reactive store that automatically rehydrates its state from WAL storage
/// and synchronizes any subsequent updates to disk.
pub fn create_persistent_store<T>(options: PersistOptions, fallback: T) -> Store<T>
where
    T: Serialize + DeserializeOwned + Clone + PartialEq + 'static,
{
    let storage = options.storage.unwrap_or_else(LocalStorage::default_instance);
    let key = options.key;

    // Attempt rehydration
    let initial_val = if let Some(raw_json) = storage.get(&key) {
        serde_json::from_str::<T>(&raw_json).unwrap_or(fallback)
    } else {
        fallback
    };

    let store = Store::new(initial_val);

    // Attach persistence listener
    let store_storage = storage;
    let store_key = key;
    store.subscribe(move |new_state| {
        if let Ok(serialized) = serde_json::to_string(new_state) {
            store_storage.set(&store_key, serialized);
        }
    });

    store
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct UserSettings {
        dark_mode: bool,
        font_size: u32,
    }

    #[test]
    fn test_persistent_store_wal_lifecycle() {
        let temp_dir = std::env::temp_dir().join("strata_test_store_wal");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);
        let wal_path = temp_dir.join("test_store.wal");

        let storage = LocalStorage::open(&wal_path).expect("Failed to open test WAL");

        let opts = PersistOptions::new("settings").with_storage(storage.clone());
        let default_settings = UserSettings {
            dark_mode: false,
            font_size: 14,
        };

        // 1. Initial creation
        let store = create_persistent_store(opts.clone(), default_settings.clone());
        assert!(!store.get().dark_mode);
        assert_eq!(store.get().font_size, 14);

        // 2. Mutate state
        store.update(|s| {
            s.dark_mode = true;
            s.font_size = 18;
        });
        assert!(store.get().dark_mode);
        assert_eq!(store.get().font_size, 18);

        // Verify storage received JSON
        let stored = storage.get("settings").unwrap();
        assert!(stored.contains("\"dark_mode\":true"));

        // 3. Re-open store from same WAL: state is restored!
        let rehydrated_store = create_persistent_store(opts, default_settings);
        assert!(rehydrated_store.get().dark_mode);
        assert_eq!(rehydrated_store.get().font_size, 18);

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
