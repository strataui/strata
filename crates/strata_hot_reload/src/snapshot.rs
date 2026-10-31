//! Reactive state snapshots for state-preserving hot reload.

use std::collections::HashMap;

/// Serialized state of an individual reactive signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalSnapshot {
    pub signal_id: u64,
    pub type_name: String,
    pub value_bytes: Vec<u8>,
}

/// Serialized state of a component instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentSnapshot {
    pub key: String,
    pub memo_index: usize,
    pub state_data: HashMap<String, String>,
}

/// Comprehensive state snapshot of the active application.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HotReloadSnapshot {
    pub epoch: u64,
    pub signals: HashMap<u64, SignalSnapshot>,
    pub components: HashMap<String, ComponentSnapshot>,
}

impl HotReloadSnapshot {
    pub fn new(epoch: u64) -> Self {
        Self {
            epoch,
            signals: HashMap::new(),
            components: HashMap::new(),
        }
    }

    pub fn insert_signal(&mut self, signal_id: u64, type_name: impl Into<String>, value_bytes: Vec<u8>) {
        self.signals.insert(
            signal_id,
            SignalSnapshot {
                signal_id,
                type_name: type_name.into(),
                value_bytes,
            },
        );
    }

    pub fn insert_component(&mut self, key: impl Into<String>, memo_index: usize, state: HashMap<String, String>) {
        let k = key.into();
        self.components.insert(
            k.clone(),
            ComponentSnapshot {
                key: k,
                memo_index,
                state_data: state,
            },
        );
    }

    /// Serializes snapshot into a compact byte payload.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Epoch
        buf.extend_from_slice(&self.epoch.to_le_bytes());

        // Signals count
        buf.extend_from_slice(&(self.signals.len() as u32).to_le_bytes());
        for s in self.signals.values() {
            buf.extend_from_slice(&s.signal_id.to_le_bytes());
            let type_bytes = s.type_name.as_bytes();
            buf.extend_from_slice(&(type_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(type_bytes);
            buf.extend_from_slice(&(s.value_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(&s.value_bytes);
        }

        // Components count
        buf.extend_from_slice(&(self.components.len() as u32).to_le_bytes());
        for c in self.components.values() {
            let key_bytes = c.key.as_bytes();
            buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(key_bytes);
            buf.extend_from_slice(&(c.memo_index as u64).to_le_bytes());
            buf.extend_from_slice(&(c.state_data.len() as u16).to_le_bytes());
            for (k, v) in &c.state_data {
                let kb = k.as_bytes();
                buf.extend_from_slice(&(kb.len() as u16).to_le_bytes());
                buf.extend_from_slice(kb);
                let vb = v.as_bytes();
                buf.extend_from_slice(&(vb.len() as u16).to_le_bytes());
                buf.extend_from_slice(vb);
            }
        }

        buf
    }

    /// Deserializes a byte slice into a HotReloadSnapshot.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 + 4 {
            return None;
        }
        let mut offset = 0;
        let epoch = u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?);
        offset += 8;

        let num_signals = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
        offset += 4;

        let mut signals = HashMap::with_capacity(num_signals);
        for _ in 0..num_signals {
            if bytes.len() < offset + 8 + 2 {
                return None;
            }
            let signal_id = u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?);
            offset += 8;
            let type_len = u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
            offset += 2;
            if bytes.len() < offset + type_len + 4 {
                return None;
            }
            let type_name = String::from_utf8(bytes[offset..offset + type_len].to_vec()).ok()?;
            offset += type_len;
            let val_len = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
            offset += 4;
            if bytes.len() < offset + val_len {
                return None;
            }
            let value_bytes = bytes[offset..offset + val_len].to_vec();
            offset += val_len;

            signals.insert(
                signal_id,
                SignalSnapshot {
                    signal_id,
                    type_name,
                    value_bytes,
                },
            );
        }

        if bytes.len() < offset + 4 {
            return None;
        }
        let num_components = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
        offset += 4;

        let mut components = HashMap::with_capacity(num_components);
        for _ in 0..num_components {
            if bytes.len() < offset + 2 {
                return None;
            }
            let key_len = u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
            offset += 2;
            if bytes.len() < offset + key_len + 8 + 2 {
                return None;
            }
            let key = String::from_utf8(bytes[offset..offset + key_len].to_vec()).ok()?;
            offset += key_len;
            let memo_index = u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?) as usize;
            offset += 8;
            let num_entries = u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
            offset += 2;

            let mut state_data = HashMap::with_capacity(num_entries);
            for _ in 0..num_entries {
                if bytes.len() < offset + 2 {
                    return None;
                }
                let k_len = u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
                offset += 2;
                if bytes.len() < offset + k_len + 2 {
                    return None;
                }
                let k = String::from_utf8(bytes[offset..offset + k_len].to_vec()).ok()?;
                offset += k_len;
                let v_len = u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
                offset += 2;
                if bytes.len() < offset + v_len {
                    return None;
                }
                let v = String::from_utf8(bytes[offset..offset + v_len].to_vec()).ok()?;
                offset += v_len;
                state_data.insert(k, v);
            }

            components.insert(
                key.clone(),
                ComponentSnapshot {
                    key,
                    memo_index,
                    state_data,
                },
            );
        }

        Some(HotReloadSnapshot {
            epoch,
            signals,
            components,
        })
    }
}
