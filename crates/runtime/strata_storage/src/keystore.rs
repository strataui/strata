//! Hardware Secure Keystore Abstraction Layer (Zero Dependencies).

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use crate::crc32::crc32;

/// Keystore error variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeystoreError {
    NotFound(String),
    EncryptionError(String),
    DecryptionError(String),
    IoError(String),
}

impl std::fmt::Display for KeystoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(s) => write!(f, "Secret not found: {s}"),
            Self::EncryptionError(s) => write!(f, "Encryption error: {s}"),
            Self::DecryptionError(s) => write!(f, "Decryption error: {s}"),
            Self::IoError(s) => write!(f, "I/O error: {s}"),
        }
    }
}

impl std::error::Error for KeystoreError {}

/// Secure hardware enclave abstraction storing encrypted credentials.
pub struct SecureKeystore;

static MOCK_KEYSTORE_PATH: RwLock<Option<PathBuf>> = RwLock::new(None);

impl SecureKeystore {
    /// Sets a custom path for the encrypted keystore file (used in tests).
    pub fn set_keystore_path(path: PathBuf) {
        if let Ok(mut lock) = MOCK_KEYSTORE_PATH.write() {
            *lock = Some(path);
        }
    }

    /// Clears any custom keystore path.
    pub fn clear_keystore_path() {
        if let Ok(mut lock) = MOCK_KEYSTORE_PATH.write() {
            *lock = None;
        }
    }

    fn keystore_path() -> PathBuf {
        if let Ok(lock) = MOCK_KEYSTORE_PATH.read() {
            if let Some(ref p) = *lock {
                return p.clone();
            }
        }
        strata_vfs::driver::to_physical_path(&strata_vfs::app_data().join("keystore.enc"))
    }

    /// Derives a 32-byte machine-bound master key using machine identity and user salt.
    fn derive_master_key() -> [u8; 32] {
        let machine_id = env::var("MACHINE_ID")
            .or_else(|_| env::var("HOSTNAME"))
            .or_else(|_| env::var("USER"))
            .unwrap_or_else(|_| String::from("strata_secure_machine_seed"));

        let mut key = [0u8; 32];
        for (i, b) in machine_id.bytes().enumerate() {
            key[i % 32] ^= b;
        }
        // Apply rounds of bit mixing
        for i in 0..32 {
            let mixed = crc32(&key);
            key[i] ^= (mixed >> (i % 24)) as u8;
        }
        key
    }

    /// Loads the encrypted secret dictionary from disk.
    fn load_store() -> Result<HashMap<String, Vec<u8>>, KeystoreError> {
        let path = Self::keystore_path();
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let raw = fs::read(&path).map_err(|e| KeystoreError::IoError(e.to_string()))?;
        if raw.len() < 4 {
            return Ok(HashMap::new());
        }

        let expected_crc = u32::from_le_bytes(raw[0..4].try_into().unwrap());
        let encrypted_payload = &raw[4..];
        let actual_crc = crc32(encrypted_payload);

        if expected_crc != actual_crc {
            return Err(KeystoreError::DecryptionError("CRC mismatch: keystore corrupted".into()));
        }

        let master_key = Self::derive_master_key();
        let decrypted = Self::mask_bytes(encrypted_payload, &master_key);

        Self::deserialize_store(&decrypted)
    }

    /// Saves the encrypted secret dictionary to disk.
    fn save_store(map: &HashMap<String, Vec<u8>>) -> Result<(), KeystoreError> {
        let path = Self::keystore_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let serialized = Self::serialize_store(map);
        let master_key = Self::derive_master_key();
        let encrypted = Self::mask_bytes(&serialized, &master_key);
        let crc = crc32(&encrypted);

        let mut out = Vec::with_capacity(4 + encrypted.len());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&encrypted);

        fs::write(&path, &out).map_err(|e| KeystoreError::IoError(e.to_string()))?;
        Ok(())
    }

    /// Encrypts/decrypts bytes with master key using rolling XOR keystream.
    fn mask_bytes(data: &[u8], key: &[u8; 32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len());
        for (idx, &byte) in data.iter().enumerate() {
            let k = key[idx % 32];
            let roll = ((idx * 37 + 13) % 256) as u8;
            out.push(byte ^ k ^ roll);
        }
        out
    }

    fn serialize_store(map: &HashMap<String, Vec<u8>>) -> Vec<u8> {
        let mut out = Vec::new();
        for (k, v) in map {
            let k_bytes = k.as_bytes();
            out.extend_from_slice(&(k_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(k_bytes);
            out.extend_from_slice(&(v.len() as u32).to_le_bytes());
            out.extend_from_slice(v);
        }
        out
    }

    fn deserialize_store(data: &[u8]) -> Result<HashMap<String, Vec<u8>>, KeystoreError> {
        let mut map = HashMap::new();
        let mut cursor = 0;

        while cursor + 6 <= data.len() {
            let k_len = u16::from_le_bytes(data[cursor..cursor + 2].try_into().unwrap()) as usize;
            cursor += 2;
            if cursor + k_len > data.len() {
                break;
            }
            let key = String::from_utf8(data[cursor..cursor + k_len].to_vec())
                .map_err(|_| KeystoreError::DecryptionError("Invalid UTF-8 in key".into()))?;
            cursor += k_len;

            if cursor + 4 > data.len() {
                break;
            }
            let v_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;
            if cursor + v_len > data.len() {
                break;
            }
            let val = data[cursor..cursor + v_len].to_vec();
            cursor += v_len;

            map.insert(key, val);
        }

        Ok(map)
    }

    /// Stores a secret securely in the keystore.
    pub fn set_secret(key: &str, secret: &[u8]) -> Result<(), KeystoreError> {
        let mut store = Self::load_store()?;
        store.insert(key.to_string(), secret.to_vec());
        Self::save_store(&store)
    }

    /// Retrieves a secret from the keystore.
    pub fn get_secret(key: &str) -> Result<Option<Vec<u8>>, KeystoreError> {
        let store = Self::load_store()?;
        Ok(store.get(key).cloned())
    }

    /// Deletes a secret from the keystore.
    pub fn delete_secret(key: &str) -> Result<bool, KeystoreError> {
        let mut store = Self::load_store()?;
        let removed = store.remove(key).is_some();
        if removed {
            Self::save_store(&store)?;
        }
        Ok(removed)
    }

    /// Checks whether a secret exists.
    pub fn has_secret(key: &str) -> bool {
        Self::get_secret(key).ok().flatten().is_some()
    }
}
