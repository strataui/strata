//! Pure-Rust Bitcask Append-Only WAL Storage Engine (Zero External Dependencies).

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::crc32::crc32;

pub const TOMBSTONE_VAL_LEN: u32 = 0xFFFF_FFFF;

/// In-memory index entry pointing to the location of a value on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyDirEntry {
    pub value_pos: u64,
    pub value_size: u32,
    pub timestamp: u64,
}

/// Pure-Rust Bitcask key-value store with append-only WAL.
pub struct StrataKV {
    wal_path: PathBuf,
    wal_file: File,
    key_dir: HashMap<String, KeyDirEntry>,
}

impl StrataKV {
    /// Opens or creates a StrataKV database at the specified WAL file path.
    pub fn open(wal_path: impl AsRef<Path>) -> io::Result<Self> {
        let path = wal_path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        let mut key_dir = HashMap::new();
        Self::replay_wal(&mut file, &mut key_dir)?;

        // Ensure file cursor is at the very end for subsequent appends
        file.seek(SeekFrom::End(0))?;

        Ok(Self {
            wal_path: path,
            wal_file: file,
            key_dir,
        })
    }

    /// Replays the WAL from start to end, verifying CRC32 checksums and indexing active keys.
    fn replay_wal(file: &mut File, key_dir: &mut HashMap<String, KeyDirEntry>) -> io::Result<()> {
        file.seek(SeekFrom::Start(0))?;
        let mut header_buf = [0u8; 18]; // 4 (crc) + 8 (ts) + 2 (klen) + 4 (vlen)

        loop {
            let offset = file.stream_position()?;
            match file.read_exact(&mut header_buf) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }

            let expected_crc = u32::from_le_bytes(header_buf[0..4].try_into().unwrap());
            let timestamp = u64::from_le_bytes(header_buf[4..12].try_into().unwrap());
            let key_len = u16::from_le_bytes(header_buf[12..14].try_into().unwrap()) as usize;
            let val_len_raw = u32::from_le_bytes(header_buf[14..18].try_into().unwrap());

            let mut payload = vec![0u8; key_len + if val_len_raw == TOMBSTONE_VAL_LEN { 0 } else { val_len_raw as usize }];
            file.read_exact(&mut payload)?;

            // Verify checksum over timestamp + key_len + val_len + payload
            let mut crc_check_data = Vec::with_capacity(14 + payload.len());
            crc_check_data.extend_from_slice(&header_buf[4..18]);
            crc_check_data.extend_from_slice(&payload);
            let actual_crc = crc32(&crc_check_data);

            if actual_crc != expected_crc {
                // Detected corrupted record, stop replay at last valid offset
                break;
            }

            let key_str = match String::from_utf8(payload[0..key_len].to_vec()) {
                Ok(k) => k,
                Err(_) => continue,
            };

            if val_len_raw == TOMBSTONE_VAL_LEN {
                key_dir.remove(&key_str);
            } else {
                let value_pos = offset + 18 + (key_len as u64);
                key_dir.insert(
                    key_str,
                    KeyDirEntry {
                        value_pos,
                        value_size: val_len_raw,
                        timestamp,
                    },
                );
            }
        }

        Ok(())
    }

    /// Retrieves a value by key.
    pub fn get(&mut self, key: &str) -> io::Result<Option<Vec<u8>>> {
        let entry = match self.key_dir.get(key) {
            Some(e) => *e,
            None => return Ok(None),
        };

        self.wal_file.seek(SeekFrom::Start(entry.value_pos))?;
        let mut buf = vec![0u8; entry.value_size as usize];
        self.wal_file.read_exact(&mut buf)?;
        Ok(Some(buf))
    }

    /// Stores a key-value pair, appending to the WAL and updating the in-memory index.
    pub fn put(&mut self, key: &str, val: &[u8]) -> io::Result<()> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let key_bytes = key.as_bytes();
        let key_len = key_bytes.len() as u16;
        let val_len = val.len() as u32;

        let mut crc_data = Vec::with_capacity(14 + key_bytes.len() + val.len());
        crc_data.extend_from_slice(&ts.to_le_bytes());
        crc_data.extend_from_slice(&key_len.to_le_bytes());
        crc_data.extend_from_slice(&val_len.to_le_bytes());
        crc_data.extend_from_slice(key_bytes);
        crc_data.extend_from_slice(val);
        let crc = crc32(&crc_data);

        // Append to WAL
        let record_pos = self.wal_file.seek(SeekFrom::End(0))?;
        self.wal_file.write_all(&crc.to_le_bytes())?;
        self.wal_file.write_all(&crc_data)?;
        self.wal_file.flush()?;

        let value_pos = record_pos + 18 + (key_len as u64);
        self.key_dir.insert(
            key.to_string(),
            KeyDirEntry {
                value_pos,
                value_size: val_len,
                timestamp: ts,
            },
        );

        Ok(())
    }

    /// Deletes a key by appending a tombstone record.
    pub fn delete(&mut self, key: &str) -> io::Result<bool> {
        if !self.key_dir.contains_key(key) {
            return Ok(false);
        }

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let key_bytes = key.as_bytes();
        let key_len = key_bytes.len() as u16;
        let val_len = TOMBSTONE_VAL_LEN;

        let mut crc_data = Vec::with_capacity(14 + key_bytes.len());
        crc_data.extend_from_slice(&ts.to_le_bytes());
        crc_data.extend_from_slice(&key_len.to_le_bytes());
        crc_data.extend_from_slice(&val_len.to_le_bytes());
        crc_data.extend_from_slice(key_bytes);
        let crc = crc32(&crc_data);

        self.wal_file.seek(SeekFrom::End(0))?;
        self.wal_file.write_all(&crc.to_le_bytes())?;
        self.wal_file.write_all(&crc_data)?;
        self.wal_file.flush()?;

        self.key_dir.remove(key);
        Ok(true)
    }

    /// Number of active non-deleted keys.
    pub fn len(&self) -> usize {
        self.key_dir.len()
    }

    /// Whether the database is empty.
    pub fn is_empty(&self) -> bool {
        self.key_dir.is_empty()
    }

    /// Returns a list of all active keys.
    pub fn keys(&self) -> Vec<String> {
        self.key_dir.keys().cloned().collect()
    }

    /// Compacts the log by writing all current active entries into a fresh compacted file.
    pub fn compact(&mut self) -> io::Result<()> {
        let compact_path = self.wal_path.with_extension("compact");
        let mut compact_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&compact_path)?;

        let mut new_key_dir = HashMap::new();

        // Read each live entry from current file and append to compact file
        for (key, entry) in &self.key_dir {
            self.wal_file.seek(SeekFrom::Start(entry.value_pos))?;
            let mut val_buf = vec![0u8; entry.value_size as usize];
            self.wal_file.read_exact(&mut val_buf)?;

            let key_bytes = key.as_bytes();
            let key_len = key_bytes.len() as u16;
            let val_len = val_buf.len() as u32;

            let mut crc_data = Vec::with_capacity(14 + key_bytes.len() + val_buf.len());
            crc_data.extend_from_slice(&entry.timestamp.to_le_bytes());
            crc_data.extend_from_slice(&key_len.to_le_bytes());
            crc_data.extend_from_slice(&val_len.to_le_bytes());
            crc_data.extend_from_slice(key_bytes);
            crc_data.extend_from_slice(&val_buf);
            let crc = crc32(&crc_data);

            let record_pos = compact_file.seek(SeekFrom::End(0))?;
            compact_file.write_all(&crc.to_le_bytes())?;
            compact_file.write_all(&crc_data)?;

            let value_pos = record_pos + 18 + (key_len as u64);
            new_key_dir.insert(
                key.clone(),
                KeyDirEntry {
                    value_pos,
                    value_size: val_len,
                    timestamp: entry.timestamp,
                },
            );
        }

        compact_file.flush()?;

        // Replace old WAL with compacted WAL
        drop(compact_file);
        self.wal_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&compact_path)?;
        std::fs::rename(&compact_path, &self.wal_path)?;

        self.key_dir = new_key_dir;
        Ok(())
    }
}
