//! Cryptographically Secure Keystore using RFC 8439 ChaCha20-Poly1305 AEAD.
//!
//! Replaces vulnerable XOR keystreams and world-readable seeds with authenticated encryption
//! with associated data (AEAD) using 256-bit CSPRNG keys and 96-bit per-write nonces.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use ring::aead::{
    Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey, CHACHA20_POLY1305,
    NONCE_LEN,
};
use ring::rand::{SecureRandom, SystemRandom};

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

struct SingleNonceSequence(Option<Nonce>);

impl NonceSequence for SingleNonceSequence {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        self.0.take().ok_or(ring::error::Unspecified)
    }
}

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
            .unwrap_or_else(|_| {
                let base = strata_vfs::resolver::resolve_root_dir(strata_vfs::VfsRoot::AppData);
                base.join("keystore.enc")
            })
    }

    fn key_file_path() -> PathBuf {
        let store_path = Self::keystore_path();
        store_path.with_extension("key")
    }

    /// Retrieves or generates a 256-bit cryptographically secure random master key.
    fn get_or_create_master_key() -> Result<[u8; 32], KeystoreError> {
        let key_path = Self::key_file_path();
        if key_path.exists() {
            if let Ok(bytes) = fs::read(&key_path) {
                if bytes.len() == 32 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&bytes);
                    return Ok(key);
                }
            }
        }

        // Generate 32 bytes of cryptographically secure randomness from system CSPRNG
        let rng = SystemRandom::new();
        let mut key = [0u8; 32];
        rng.fill(&mut key)
            .map_err(|_| KeystoreError::EncryptionError("Failed to generate secure random key".into()))?;

        if let Some(parent) = key_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        fs::write(&key_path, key).map_err(|e| KeystoreError::IoError(e.to_string()))?;

        // Restrict key file to user read/write only (0600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600));
        }

        Ok(key)
    }

    /// Loads the encrypted secret dictionary from disk using ChaCha20-Poly1305 AEAD.
    fn load_store() -> Result<HashMap<String, Vec<u8>>, KeystoreError> {
        let path = Self::keystore_path();
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let raw = fs::read(&path).map_err(|e| KeystoreError::IoError(e.to_string()))?;
        if raw.len() < 16 + NONCE_LEN + 16 {
            return Ok(HashMap::new());
        }

        if &raw[..16] != b"STRATA_KEYSTORE_" {
            return Err(KeystoreError::DecryptionError(
                "Invalid keystore file format magic".into(),
            ));
        }

        let nonce_bytes = &raw[16..16 + NONCE_LEN];
        let mut in_out = raw[16 + NONCE_LEN..].to_vec();

        let master_key = Self::get_or_create_master_key()?;
        let unbound_key = UnboundKey::new(&CHACHA20_POLY1305, &master_key)
            .map_err(|_| KeystoreError::DecryptionError("Invalid master key".into()))?;

        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| KeystoreError::DecryptionError("Corrupted nonce".into()))?;

        let mut opening_key = OpeningKey::new(unbound_key, SingleNonceSequence(Some(nonce)));
        let plaintext = opening_key
            .open_in_place(Aad::empty(), &mut in_out)
            .map_err(|_| {
                KeystoreError::DecryptionError(
                    "Cryptographic authentication failed: ciphertext tampered or invalid key".into(),
                )
            })?;

        Self::deserialize_store(plaintext)
    }

    /// Saves the secret dictionary to disk encrypted with ChaCha20-Poly1305 AEAD.
    fn save_store(map: &HashMap<String, Vec<u8>>) -> Result<(), KeystoreError> {
        let path = Self::keystore_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let serialized = Self::serialize_store(map);
        let master_key = Self::get_or_create_master_key()?;

        // Generate 12-byte CSPRNG nonce
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| KeystoreError::EncryptionError("Failed to generate secure nonce".into()))?;

        let unbound_key = UnboundKey::new(&CHACHA20_POLY1305, &master_key)
            .map_err(|_| KeystoreError::EncryptionError("Invalid master key length".into()))?;

        let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
            .map_err(|_| KeystoreError::EncryptionError("Failed to initialize nonce".into()))?;

        let mut sealing_key = SealingKey::new(unbound_key, SingleNonceSequence(Some(nonce)));
        let mut ciphertext = serialized;
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut ciphertext)
            .map_err(|_| KeystoreError::EncryptionError("AEAD encryption sealing failed".into()))?;

        let mut output = Vec::with_capacity(16 + NONCE_LEN + ciphertext.len());
        output.extend_from_slice(b"STRATA_KEYSTORE_");
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);

        fs::write(&path, output).map_err(|e| KeystoreError::IoError(e.to_string()))?;

        // Restrict keystore file to owner read/write only (0600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }

        Ok(())
    }

    fn serialize_store(map: &HashMap<String, Vec<u8>>) -> Vec<u8> {
        let mut out = Vec::new();
        for (k, v) in map {
            let k_bytes = k.as_bytes();
            out.extend_from_slice(&(k_bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(k_bytes);
            out.extend_from_slice(&(v.len() as u32).to_le_bytes());
            out.extend_from_slice(v);
        }
        out
    }

    fn deserialize_store(data: &[u8]) -> Result<HashMap<String, Vec<u8>>, KeystoreError> {
        let mut map = HashMap::new();
        let mut cursor = 0;

        while cursor + 8 <= data.len() {
            let k_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;
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

    /// Stores a secret key/value pair securely into the enclave.
    pub fn set_secret(key: &str, value: &[u8]) -> Result<(), KeystoreError> {
        let mut store = Self::load_store()?;
        store.insert(key.to_string(), value.to_vec());
        Self::save_store(&store)
    }

    /// Retrieves an existing secret from the enclave.
    pub fn get_secret(key: &str) -> Result<Option<Vec<u8>>, KeystoreError> {
        let store = Self::load_store()?;
        Ok(store.get(key).cloned())
    }

    /// Deletes a secret key/value pair from the enclave.
    pub fn delete_secret(key: &str) -> Result<bool, KeystoreError> {
        let mut store = Self::load_store()?;
        let existed = store.remove(key).is_some();
        if existed {
            Self::save_store(&store)?;
        }
        Ok(existed)
    }

    /// Checks if a secret identifier exists in the enclave.
    pub fn has_secret(key: &str) -> bool {
        Self::load_store().map(|m| m.contains_key(key)).unwrap_or(false)
    }
}
