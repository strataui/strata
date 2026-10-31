# Virtual Filesystem & Storage

Modern cross-platform applications often struggle with filesystem differences across operating systems: POSIX permissions, Windows path separators, Android Scoped Storage, iOS sandboxes, and Web browser sandboxes (OPFS/IndexedDB).

Strata solves this with a unified virtual filesystem (`strata_vfs`) and a zero-dependency append-only storage engine (`strata_storage`).

---

## Universal Virtual Filesystem (`strata_vfs`)

`strata_vfs` exposes an asynchronous, unified filesystem API without requiring developers to scatter `#[cfg(...)]` conditionals throughout application code.

### Standard Virtual Roots
- **`app_data()`**: Persistent configuration and application database.
  - Linux: `~/.config/<app>/`
  - macOS: `~/Library/Application Support/<app>/`
  - Windows: `%APPDATA%/<app>/`
  - Web: Origin Private File System (OPFS)
- **`cache()`**: Disposable cached assets and precompiled shaders.
- **`documents()`**: User-facing document exports and saved workspaces.
- **`temp()`**: Ephemeral scratch directory automatically cleaned on process exit.

```rust
use strata_vfs::prelude::*;

async fn save_user_preferences(prefs: &UserPrefs) -> Result<(), VfsError> {
    let vfs = StrataVfs::new();
    let config_path = vfs.app_data().join("preferences.json");

    let bytes = serde_json::to_vec_pretty(prefs)?;
    vfs.write(&config_path, &bytes).await?;

    Ok(())
}
```

---

## Bitcask WAL Storage Engine (`StrataKV`)

Rather than bundling bulky C libraries such as SQLite (which adds 1.2MB of binary size) or RocksDB (which adds complex C++ dependencies), Strata implements a pure-Rust key-value engine modeled after **Bitcask**:

### Principles of Bitcask
1. **Append-Only Write-Ahead Log (WAL)**: All writes, updates, and deletes are appended sequentially to the active disk log. Sequential disk writes saturate NVMe and SSD bandwidth with zero random write overhead.
2. **In-Memory Hash Index**: Key pointers (file offset and payload length) are stored in an in-memory hash table.
3. **Instant $O(1)$ Reads**: Fetching any record requires exactly one disk seek at the cached byte offset.
4. **Crash Safety**: In the event of a sudden power outage, unwritten buffers are recovered automatically by replaying the append-only log up to the last valid checksum.

```rust
use strata_storage::StrataKV;

async fn track_session() -> Result<(), StorageError> {
    let kv = StrataKV::open("session_store").await?;

    // Atomic sequential append
    kv.put(b"last_login", b"2026-11-01T00:30:00Z").await?;

    // O(1) direct retrieval
    if let Some(val) = kv.get(b"last_login").await? {
        println!("Last login: {}", String::from_utf8_lossy(&val));
    }

    Ok(())
}
```

---

## Hardware Secure Keystore

For sensitive credentials, encryption keys, and authentication tokens, `strata_storage` provides direct access to platform hardware security modules:

- **Apple**: macOS Keychain / iOS Secure Enclave (`Security.framework`).
- **Android**: Android Keystore Provider with hardware-backed TEE / StrongBox.
- **Windows**: Data Protection API (DPAPI) and Windows Hello TPM.
- **Linux**: FreeDesktop Secret Service DBus interface (GNOME Keyring / KWallet).
- **Web**: Web Cryptography API (`crypto.subtle`).

```rust
use strata_storage::keystore::SecureKeystore;

async fn store_secret(token: &str) -> Result<(), KeystoreError> {
    let keystore = SecureKeystore::new("com.larvance.strata")?;
    keystore.set_secret("auth_bearer_token", token.as_bytes())?;
    Ok(())
}
```
