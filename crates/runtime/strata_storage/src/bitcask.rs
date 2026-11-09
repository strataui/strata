//! Pure-Rust Bitcask Append-Only WAL Storage Engine (Zero External Dependencies).

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::crc32::crc32;

pub const TOMBSTONE_VAL_LEN: u32 = 0xFFFF_FFFF;
pub const MAX_KEY_SIZE: usize = 65535;
pub const MAX_RECORD_SIZE: usize = 64 * 1024 * 1024; // 64 MB limit protecting against OOM bombs

/// In-memory index entry pointing to the location of a value on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyDirEntry {
    pub value_pos: u64,
    pub value_size: u32,
    pub timestamp: u64,
}

#[cfg(unix)]
fn try_lock_exclusive(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;

    let res = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
    if res != 0 {
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::WouldBlock || err.raw_os_error() == Some(11) {
            Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Bitcask WAL is locked by another process",
            ))
        } else {
            Err(err)
        }
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn unlock_file(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    const LOCK_UN: i32 = 8;
    let res = unsafe { flock(file.as_raw_fd(), LOCK_UN) };
    if res != 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn try_lock_exclusive(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    extern "system" {
        fn LockFileEx(
            h_file: *mut std::ffi::c_void,
            dw_flags: u32,
            dw_reserved: u32,
            n_number_of_bytes_to_lock_low: u32,
            n_number_of_bytes_to_lock_high: u32,
            lp_overlapped: *mut std::ffi::c_void,
        ) -> i32;
    }
    const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x0000_0002;
    const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x0000_0001;

    let mut overlapped = std::mem::MaybeUninit::<[u8; 32]>::zeroed();
    let res = unsafe {
        LockFileEx(
            file.as_raw_handle() as *mut _,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            overlapped.as_mut_ptr() as *mut _,
        )
    };
    if res == 0 {
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(33) {
            Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Bitcask WAL is locked by another process",
            ))
        } else {
            Err(err)
        }
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn unlock_file(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    extern "system" {
        fn UnlockFile(
            h_file: *mut std::ffi::c_void,
            dw_file_offset_low: u32,
            dw_file_offset_high: u32,
            n_number_of_bytes_to_unlock_low: u32,
            n_number_of_bytes_to_unlock_high: u32,
        ) -> i32;
    }
    let res = unsafe { UnlockFile(file.as_raw_handle() as *mut _, 0, 0, 1, 0) };
    if res == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn try_lock_exclusive(_file: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn unlock_file(_file: &File) -> io::Result<()> {
    Ok(())
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

        try_lock_exclusive(&file)?;

        let mut key_dir = HashMap::new();
        let valid_offset = Self::replay_wal(&mut file, &mut key_dir)?;
        let file_len = file.metadata()?.len();
        if valid_offset < file_len {
            file.set_len(valid_offset)?;
        }

        // Ensure file cursor is at the very end for subsequent appends
        file.seek(SeekFrom::Start(valid_offset))?;

        Ok(Self {
            wal_path: path,
            wal_file: file,
            key_dir,
        })
    }

    /// Replays the WAL from start to end, verifying CRC32 checksums and indexing active keys.
    /// Returns the last confirmed valid byte offset, allowing recovery and truncation of torn writes.
    fn replay_wal(file: &mut File, key_dir: &mut HashMap<String, KeyDirEntry>) -> io::Result<u64> {
        let file_len = file.metadata()?.len();
        file.seek(SeekFrom::Start(0))?;
        let mut header_buf = [0u8; 18]; // 4 (crc) + 8 (ts) + 2 (klen) + 4 (vlen)
        let mut last_valid_offset = 0u64;

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

            let is_tombstone = val_len_raw == TOMBSTONE_VAL_LEN;
            let val_len = if is_tombstone { 0 } else { val_len_raw as usize };

            // Reject pre-allocation OOM bomb: validate bounds against max record size and remaining file size
            if val_len > MAX_RECORD_SIZE {
                break;
            }

            let total_payload_len = key_len + val_len;
            let remaining_file_bytes = file_len.saturating_sub(offset + 18);
            if (total_payload_len as u64) > remaining_file_bytes {
                break;
            }

            let mut payload = vec![0u8; total_payload_len];
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

            if is_tombstone {
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

            last_valid_offset = file.stream_position()?;
        }

        Ok(last_valid_offset)
    }

    /// Retrieves a value by key.
    pub fn get(&mut self, key: &str) -> io::Result<Option<Vec<u8>>> {
        let entry = match self.key_dir.get(key) {
            Some(e) => *e,
            None => return Ok(None),
        };

        if entry.value_size as usize > MAX_RECORD_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Record exceeds MAX_RECORD_SIZE"));
        }

        let file_len = self.wal_file.metadata()?.len();
        if entry.value_pos.saturating_add(entry.value_size as u64) > file_len {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Record exceeds WAL file length"));
        }

        self.wal_file.seek(SeekFrom::Start(entry.value_pos))?;
        let mut buf = vec![0u8; entry.value_size as usize];
        self.wal_file.read_exact(&mut buf)?;
        Ok(Some(buf))
    }

    /// Stores a key-value pair, appending to the WAL and updating the in-memory index.
    pub fn put(&mut self, key: &str, val: &[u8]) -> io::Result<()> {
        if val.len() > MAX_RECORD_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Value exceeds MAX_RECORD_SIZE"));
        }

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

    /// Returns total file size on disk in bytes.
    pub fn total_bytes(&self) -> io::Result<u64> {
        self.wal_file.metadata().map(|m| m.len())
    }

    /// Returns the sum of bytes utilized by all live keys, values, and frame headers.
    pub fn live_data_bytes(&self) -> u64 {
        self.key_dir
            .iter()
            .map(|(k, entry)| 18 + k.len() as u64 + entry.value_size as u64)
            .sum()
    }

    /// Ratio of dead/reclaimable bytes in the WAL (0.0 to 1.0).
    pub fn dead_bytes_ratio(&self) -> io::Result<f64> {
        let total = self.total_bytes()?;
        if total == 0 {
            return Ok(0.0);
        }
        let live = self.live_data_bytes();
        let dead = total.saturating_sub(live);
        Ok(dead as f64 / total as f64)
    }

    /// Compacts the log if the dead bytes ratio exceeds the specified threshold.
    pub fn maybe_compact(&mut self, threshold: f64) -> io::Result<bool> {
        if self.dead_bytes_ratio()? >= threshold {
            self.compact()?;
            Ok(true)
        } else {
            Ok(false)
        }
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
        drop(compact_file);

        // Replace old WAL with compacted WAL safely across operating systems:
        // On Windows, renaming fails if either the source or target file is open.
        // We unlock and drop the existing WAL handle first by swapping with an ephemeral file.
        let swap_path = self.wal_path.with_extension("swap");
        let swap_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&swap_path)?;

        let old_file = std::mem::replace(&mut self.wal_file, swap_file);
        let _ = unlock_file(&old_file);
        drop(old_file);

        std::fs::rename(&compact_path, &self.wal_path)?;
        let _ = std::fs::remove_file(&swap_path);

        let mut reloaded = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.wal_path)?;
        try_lock_exclusive(&reloaded)?;
        reloaded.seek(SeekFrom::End(0))?;
        self.wal_file = reloaded;

        self.key_dir = new_key_dir;
        Ok(())
    }
}

impl Drop for StrataKV {
    fn drop(&mut self) {
        let _ = unlock_file(&self.wal_file);
    }
}
