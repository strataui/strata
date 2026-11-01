//! Strata Storage: Pure-Rust Bitcask WAL Engine, In-Memory Storage, and Hardware Keystore.

pub mod crc32;
pub mod bitcask;
pub mod storage;
pub mod keystore;

pub use bitcask::{KeyDirEntry, StrataKV};
pub use storage::{LocalStorage, SessionStorage};
pub use keystore::{KeystoreError, SecureKeystore};

/// Returns the default global LocalStorage instance backed by WAL.
pub fn local() -> LocalStorage {
    LocalStorage::default_instance()
}

/// Returns a new volatile in-memory SessionStorage instance.
pub fn session() -> SessionStorage {
    SessionStorage::new()
}

pub mod prelude {
    pub use super::bitcask::{KeyDirEntry, StrataKV};
    pub use super::keystore::{KeystoreError, SecureKeystore};
    pub use super::storage::{LocalStorage, SessionStorage};
    pub use super::{local, session};
}
