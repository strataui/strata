//! Comprehensive integration tests for strata_storage.

use strata_storage::prelude::*;

#[test]
fn test_bitcask_wal_persistence_and_recovery() {
    let test_wal = std::env::temp_dir().join("strata_test_bitcask.wal");
    let _ = std::fs::remove_file(&test_wal);

    // Initial session
    {
        let mut kv = StrataKV::open(&test_wal).expect("Failed to open StrataKV");
        assert_eq!(kv.len(), 0);

        kv.put("cluster_id", b"us-east-42").expect("Put failed");
        kv.put("api_token", b"secret_tok_99").expect("Put failed");
        kv.put("retry_count", b"3").expect("Put failed");
        assert_eq!(kv.len(), 3);

        let val = kv.get("cluster_id").expect("Get failed");
        assert_eq!(val, Some(b"us-east-42".to_vec()));

        // Update key
        kv.put("retry_count", b"4").expect("Update failed");
        assert_eq!(kv.get("retry_count").unwrap(), Some(b"4".to_vec()));

        // Delete key
        assert!(kv.delete("api_token").expect("Delete failed"));
        assert_eq!(kv.get("api_token").unwrap(), None);
        assert_eq!(kv.len(), 2);
    }

    // Recovery session: re-open from same WAL file
    {
        let mut kv = StrataKV::open(&test_wal).expect("Failed to re-open StrataKV");
        assert_eq!(kv.len(), 2);
        assert_eq!(kv.get("cluster_id").unwrap(), Some(b"us-east-42".to_vec()));
        assert_eq!(kv.get("retry_count").unwrap(), Some(b"4".to_vec()));
        assert_eq!(kv.get("api_token").unwrap(), None);
    }

    let _ = std::fs::remove_file(&test_wal);
}

#[test]
fn test_bitcask_compaction() {
    let test_wal = std::env::temp_dir().join("strata_test_compact.wal");
    let _ = std::fs::remove_file(&test_wal);

    let mut kv = StrataKV::open(&test_wal).expect("Failed to open StrataKV");

    for i in 0..50 {
        kv.put(&format!("key_{i}"), format!("val_{i}").as_bytes()).unwrap();
    }
    // Overwrite half
    for i in 0..25 {
        kv.put(&format!("key_{i}"), b"overwritten").unwrap();
    }
    // Delete some
    for i in 25..35 {
        kv.delete(&format!("key_{i}")).unwrap();
    }

    assert_eq!(kv.len(), 40); // 50 - 10 deleted

    // Trigger compaction
    kv.compact().expect("Compaction failed");
    assert_eq!(kv.len(), 40);

    assert_eq!(kv.get("key_0").unwrap(), Some(b"overwritten".to_vec()));
    assert_eq!(kv.get("key_26").unwrap(), None); // deleted
    assert_eq!(kv.get("key_40").unwrap(), Some(b"val_40".to_vec()));

    let _ = std::fs::remove_file(&test_wal);
}

#[test]
fn test_local_storage_and_session_storage() {
    let test_wal = std::env::temp_dir().join("strata_test_ls.wal");
    let _ = std::fs::remove_file(&test_wal);

    let storage = LocalStorage::open(&test_wal).expect("Failed to open LocalStorage");
    assert_eq!(storage.len(), 0);

    storage.set("theme", "dark");
    storage.set("lang", "en-US");
    assert_eq!(storage.len(), 2);
    assert_eq!(storage.get("theme"), Some("dark".to_string()));
    assert_eq!(storage.get("lang"), Some("en-US".to_string()));

    storage.remove("lang");
    assert_eq!(storage.len(), 1);
    assert_eq!(storage.get("lang"), None);

    storage.clear();
    assert_eq!(storage.len(), 0);

    // SessionStorage
    let session = SessionStorage::new();
    session.set("user_id", "usr_1234");
    assert_eq!(session.get("user_id"), Some("usr_1234".to_string()));
    assert_eq!(session.len(), 1);

    let _ = std::fs::remove_file(&test_wal);
}

#[test]
fn test_secure_keystore() {
    let test_enc = std::env::temp_dir().join("strata_test_keystore.enc");
    let test_key = test_enc.with_extension("key");
    let _ = std::fs::remove_file(&test_enc);
    let _ = std::fs::remove_file(&test_key);

    SecureKeystore::set_keystore_path(test_enc.clone());

    assert!(!SecureKeystore::has_secret("jwt_refresh"));

    let secret_bytes = b"super_secret_oauth_refresh_token_2026";
    SecureKeystore::set_secret("jwt_refresh", secret_bytes).expect("Failed to save secret");
    assert!(SecureKeystore::has_secret("jwt_refresh"));

    let retrieved = SecureKeystore::get_secret("jwt_refresh").expect("Failed to retrieve secret");
    assert_eq!(retrieved, Some(secret_bytes.to_vec()));

    // Verify disk file is encrypted and does not contain plain text
    let disk_raw = std::fs::read(&test_enc).expect("Failed to read raw encrypted file");
    let disk_str = String::from_utf8_lossy(&disk_raw);
    assert!(!disk_str.contains("super_secret_oauth_refresh_token_2026"));

    // Verify 0600 permissions on Unix for both keystore and keyfile
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let enc_mode = std::fs::metadata(&test_enc).unwrap().permissions().mode() & 0o777;
        let key_mode = std::fs::metadata(&test_key).unwrap().permissions().mode() & 0o777;
        assert_eq!(enc_mode, 0o600, "Keystore file must be restricted to 0600 permissions");
        assert_eq!(key_mode, 0o600, "Key file must be restricted to 0600 permissions");
    }

    // Tampering test: AEAD authentication tag validation must reject forged/modified ciphertext
    let mut tampered = disk_raw.clone();
    let last_idx = tampered.len() - 1;
    tampered[last_idx] ^= 0x01; // flip 1 bit
    std::fs::write(&test_enc, &tampered).unwrap();
    let tamper_result = SecureKeystore::get_secret("jwt_refresh");
    assert!(
        matches!(tamper_result, Err(KeystoreError::DecryptionError(_))),
        "AEAD must detect corrupted or forged ciphertext"
    );

    // Restore valid ciphertext and continue
    std::fs::write(&test_enc, &disk_raw).unwrap();

    assert!(SecureKeystore::delete_secret("jwt_refresh").unwrap());
    assert!(!SecureKeystore::has_secret("jwt_refresh"));

    // Test large key (>64KB) support without u16 overflow truncation
    let large_key = "K".repeat(70_000);
    let large_val = b"large_payload_value";
    SecureKeystore::set_secret(&large_key, large_val).expect("Failed to store large key");
    assert!(SecureKeystore::has_secret(&large_key));
    let retrieved_large = SecureKeystore::get_secret(&large_key).expect("Failed to get large key");
    assert_eq!(retrieved_large, Some(large_val.to_vec()));

    assert!(SecureKeystore::delete_secret(&large_key).unwrap());

    let _ = std::fs::remove_file(&test_enc);
    let _ = std::fs::remove_file(&test_key);
    SecureKeystore::clear_keystore_path();
}
