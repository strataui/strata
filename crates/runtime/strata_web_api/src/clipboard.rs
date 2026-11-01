//! W3C Clipboard API in Pure Idiomatic Rust.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

/// Errors occurring during Clipboard read or write operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    Unavailable(String),
    PermissionDenied(String),
    InvalidData(String),
    IoError(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(s) => write!(f, "Clipboard unavailable: {s}"),
            Self::PermissionDenied(s) => write!(f, "Clipboard permission denied: {s}"),
            Self::InvalidData(s) => write!(f, "Invalid clipboard data: {s}"),
            Self::IoError(s) => write!(f, "Clipboard I/O error: {s}"),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// W3C Clipboard interface.
#[derive(Clone)]
pub struct Clipboard {
    text_buffer: Arc<RwLock<String>>,
    data_buffer: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard {
    /// Creates a new Clipboard instance with thread-safe internal backing.
    pub fn new() -> Self {
        Self {
            text_buffer: Arc::new(RwLock::new(String::new())),
            data_buffer: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Writes plain text to the clipboard.
    pub fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        let mut buf = self.text_buffer.write().map_err(|e| {
            ClipboardError::IoError(format!("Failed to acquire text write lock: {e}"))
        })?;
        *buf = text.to_string();

        let mut data = self.data_buffer.write().map_err(|e| {
            ClipboardError::IoError(format!("Failed to acquire data write lock: {e}"))
        })?;
        data.insert("text/plain".to_string(), text.as_bytes().to_vec());
        Ok(())
    }

    /// Reads plain text from the clipboard.
    pub fn read_text(&self) -> Result<String, ClipboardError> {
        let buf = self.text_buffer.read().map_err(|e| {
            ClipboardError::IoError(format!("Failed to acquire text read lock: {e}"))
        })?;
        Ok(buf.clone())
    }

    /// Writes arbitrary MIME-typed data to the clipboard.
    pub fn write_data(&self, mime: &str, data: &[u8]) -> Result<(), ClipboardError> {
        let mut map = self.data_buffer.write().map_err(|e| {
            ClipboardError::IoError(format!("Failed to acquire data write lock: {e}"))
        })?;
        map.insert(mime.to_string(), data.to_vec());

        if mime == "text/plain" {
            if let Ok(text) = String::from_utf8(data.to_vec()) {
                if let Ok(mut tbuf) = self.text_buffer.write() {
                    *tbuf = text;
                }
            }
        }
        Ok(())
    }

    /// Reads MIME-typed data from the clipboard, returning None if the type is not present.
    pub fn read_data(&self, mime: &str) -> Result<Option<Vec<u8>>, ClipboardError> {
        let map = self.data_buffer.read().map_err(|e| {
            ClipboardError::IoError(format!("Failed to acquire data read lock: {e}"))
        })?;
        Ok(map.get(mime).cloned())
    }

    /// Clears all clipboard data.
    pub fn clear(&self) -> Result<(), ClipboardError> {
        if let Ok(mut tbuf) = self.text_buffer.write() {
            tbuf.clear();
        }
        if let Ok(mut dbuf) = self.data_buffer.write() {
            dbuf.clear();
        }
        Ok(())
    }
}
