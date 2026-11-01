//! Compact binary message framing for peer-to-peer CRDT state replication.

use crate::clock::LamportClock;

/// Message types exchanged between replicated Strata peers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncMessage {
    RegisterDelta {
        clock: LamportClock,
        key: String,
        payload: Vec<u8>,
    },
    CounterDelta {
        peer: u64,
        positive: u64,
        negative: u64,
    },
    Heartbeat {
        clock: LamportClock,
    },
}

impl SyncMessage {
    /// Encodes sync message into a compact binary byte buffer.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            SyncMessage::RegisterDelta { clock, key, payload } => {
                buf.push(1); // Type 1
                buf.extend_from_slice(&clock.counter.to_le_bytes());
                buf.extend_from_slice(&clock.peer.to_le_bytes());
                let key_bytes = key.as_bytes();
                buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
                buf.extend_from_slice(key_bytes);
                buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                buf.extend_from_slice(payload);
            }
            SyncMessage::CounterDelta { peer, positive, negative } => {
                buf.push(2); // Type 2
                buf.extend_from_slice(&peer.to_le_bytes());
                buf.extend_from_slice(&positive.to_le_bytes());
                buf.extend_from_slice(&negative.to_le_bytes());
            }
            SyncMessage::Heartbeat { clock } => {
                buf.push(3); // Type 3
                buf.extend_from_slice(&clock.counter.to_le_bytes());
                buf.extend_from_slice(&clock.peer.to_le_bytes());
            }
        }
        buf
    }

    /// Decodes a binary byte slice into a SyncMessage.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }

        let msg_type = bytes[0];
        match msg_type {
            1 => {
                if bytes.len() < 1 + 8 + 8 + 2 {
                    return None;
                }
                let mut offset = 1;
                let counter = u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?);
                offset += 8;
                let peer = u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?);
                offset += 8;
                let key_len = u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
                offset += 2;
                if bytes.len() < offset + key_len + 4 {
                    return None;
                }
                let key = String::from_utf8(bytes[offset..offset + key_len].to_vec()).ok()?;
                offset += key_len;
                let payload_len = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
                offset += 4;
                if bytes.len() < offset + payload_len {
                    return None;
                }
                let payload = bytes[offset..offset + payload_len].to_vec();

                Some(SyncMessage::RegisterDelta {
                    clock: LamportClock::new(counter, peer),
                    key,
                    payload,
                })
            }
            2 => {
                if bytes.len() < 1 + 8 + 8 + 8 {
                    return None;
                }
                let peer = u64::from_le_bytes(bytes[1..9].try_into().ok()?);
                let positive = u64::from_le_bytes(bytes[9..17].try_into().ok()?);
                let negative = u64::from_le_bytes(bytes[17..25].try_into().ok()?);

                Some(SyncMessage::CounterDelta { peer, positive, negative })
            }
            3 => {
                if bytes.len() < 1 + 8 + 8 {
                    return None;
                }
                let counter = u64::from_le_bytes(bytes[1..9].try_into().ok()?);
                let peer = u64::from_le_bytes(bytes[9..17].try_into().ok()?);

                Some(SyncMessage::Heartbeat {
                    clock: LamportClock::new(counter, peer),
                })
            }
            _ => None,
        }
    }
}
