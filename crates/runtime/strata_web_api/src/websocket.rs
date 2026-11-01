//! Pure-Rust W3C WebSocket Client with RFC 6455 Frame Serialization and State Machine.

use std::fmt;
use std::sync::{Arc, Mutex};

/// RFC 6455 WebSocket Opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsOpcode {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}

impl WsOpcode {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x0 => Some(Self::Continuation),
            0x1 => Some(Self::Text),
            0x2 => Some(Self::Binary),
            0x8 => Some(Self::Close),
            0x9 => Some(Self::Ping),
            0xA => Some(Self::Pong),
            _ => None,
        }
    }
}

/// Errors occurring during WebSocket communication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsError {
    InvalidState(String),
    ProtocolError(String),
    InvalidFrame(String),
    NetworkError(String),
    Closed,
}

impl fmt::Display for WsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState(s) => write!(f, "Invalid WebSocket state: {s}"),
            Self::ProtocolError(s) => write!(f, "WebSocket protocol error: {s}"),
            Self::InvalidFrame(s) => write!(f, "Invalid WebSocket frame: {s}"),
            Self::NetworkError(s) => write!(f, "WebSocket network error: {s}"),
            Self::Closed => write!(f, "WebSocket is already closed"),
        }
    }
}

impl std::error::Error for WsError {}

/// State of the WebSocket connection per W3C specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsState {
    Connecting = 0,
    Open = 1,
    Closing = 2,
    Closed = 3,
}

/// A decoded RFC 6455 WebSocket frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsFrame {
    pub fin: bool,
    pub opcode: WsOpcode,
    pub payload: Vec<u8>,
}

/// High-level WebSocket message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsMessage {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<u16>, String),
}

/// Encodes an RFC 6455 frame into bytes.
pub fn encode_frame(
    fin: bool,
    opcode: WsOpcode,
    payload: &[u8],
    masked: bool,
    mask_key: Option<[u8; 4]>,
) -> Vec<u8> {
    let mut frame = Vec::new();

    // Byte 0: FIN (bit 7) + RSV1-3 (bits 4-6 = 0) + Opcode (bits 0-3)
    let byte0 = if fin { 0x80 } else { 0x00 } | (opcode as u8 & 0x0F);
    frame.push(byte0);

    // Byte 1: Mask bit (bit 7) + Payload length indicator
    let mask_bit = if masked { 0x80 } else { 0x00 };
    let len = payload.len();

    if len < 126 {
        frame.push(mask_bit | (len as u8));
    } else if len <= 0xFFFF {
        frame.push(mask_bit | 126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(mask_bit | 127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }

    if masked {
        let key = mask_key.unwrap_or([0x12, 0x34, 0x56, 0x78]);
        frame.extend_from_slice(&key);
        // Apply XOR masking
        for (i, &b) in payload.iter().enumerate() {
            frame.push(b ^ key[i % 4]);
        }
    } else {
        frame.extend_from_slice(payload);
    }

    frame
}

/// Decodes an RFC 6455 frame from a byte buffer.
/// Returns Ok(Some((frame, bytes_consumed))) if a complete frame was parsed,
/// Ok(None) if more data is needed, or Err(WsError) on malformed framing.
pub fn decode_frame(buf: &[u8]) -> Result<Option<(WsFrame, usize)>, WsError> {
    if buf.len() < 2 {
        return Ok(None);
    }

    let byte0 = buf[0];
    let fin = (byte0 & 0x80) != 0;
    let opcode_raw = byte0 & 0x0F;
    let opcode = WsOpcode::from_u8(opcode_raw)
        .ok_or_else(|| WsError::InvalidFrame(format!("Unknown opcode: 0x{opcode_raw:X}")))?;

    let byte1 = buf[1];
    let masked = (byte1 & 0x80) != 0;
    let payload_indicator = byte1 & 0x7F;

    let mut cursor = 2;

    let payload_len: usize = if payload_indicator < 126 {
        payload_indicator as usize
    } else if payload_indicator == 126 {
        if buf.len() < cursor + 2 {
            return Ok(None);
        }
        let len_bytes: [u8; 2] = [buf[cursor], buf[cursor + 1]];
        cursor += 2;
        u16::from_be_bytes(len_bytes) as usize
    } else {
        if buf.len() < cursor + 8 {
            return Ok(None);
        }
        let mut len_bytes = [0u8; 8];
        len_bytes.copy_from_slice(&buf[cursor..cursor + 8]);
        cursor += 8;
        u64::from_be_bytes(len_bytes) as usize
    };

    let mask_key = if masked {
        if buf.len() < cursor + 4 {
            return Ok(None);
        }
        let key = [buf[cursor], buf[cursor + 1], buf[cursor + 2], buf[cursor + 3]];
        cursor += 4;
        Some(key)
    } else {
        None
    };

    if buf.len() < cursor + payload_len {
        return Ok(None);
    }

    let mut payload = buf[cursor..cursor + payload_len].to_vec();
    if let Some(key) = mask_key {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= key[i % 4];
        }
    }

    cursor += payload_len;

    Ok(Some((
        WsFrame {
            fin,
            opcode,
            payload,
        },
        cursor,
    )))
}

/// W3C WebSocket client interface.
#[derive(Clone)]
pub struct WebSocket {
    pub url: String,
    state: Arc<Mutex<WsState>>,
    outgoing_queue: Arc<Mutex<Vec<Vec<u8>>>>,
    received_messages: Arc<Mutex<Vec<WsMessage>>>,
}

impl WebSocket {
    /// Creates a new WebSocket instance pointing to the given URL.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            state: Arc::new(Mutex::new(WsState::Connecting)),
            outgoing_queue: Arc::new(Mutex::new(Vec::new())),
            received_messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Mark the connection as Open.
    pub fn set_open(&self) {
        if let Ok(mut s) = self.state.lock() {
            *s = WsState::Open;
        }
    }

    /// Current connection state.
    pub fn ready_state(&self) -> WsState {
        self.state.lock().map(|s| *s).unwrap_or(WsState::Closed)
    }

    /// Sends a text message over the WebSocket.
    pub fn send_text(&self, text: &str) -> Result<(), WsError> {
        let state = self.ready_state();
        if state != WsState::Open {
            return Err(WsError::InvalidState(format!("Cannot send in state {state:?}")));
        }

        let frame = encode_frame(true, WsOpcode::Text, text.as_bytes(), true, None);
        if let Ok(mut q) = self.outgoing_queue.lock() {
            q.push(frame);
        }
        Ok(())
    }

    /// Sends binary data over the WebSocket.
    pub fn send_binary(&self, data: &[u8]) -> Result<(), WsError> {
        let state = self.ready_state();
        if state != WsState::Open {
            return Err(WsError::InvalidState(format!("Cannot send in state {state:?}")));
        }

        let frame = encode_frame(true, WsOpcode::Binary, data, true, None);
        if let Ok(mut q) = self.outgoing_queue.lock() {
            q.push(frame);
        }
        Ok(())
    }

    /// Sends a Ping frame.
    pub fn send_ping(&self, payload: &[u8]) -> Result<(), WsError> {
        let frame = encode_frame(true, WsOpcode::Ping, payload, true, None);
        if let Ok(mut q) = self.outgoing_queue.lock() {
            q.push(frame);
        }
        Ok(())
    }

    /// Closes the WebSocket connection with status code and reason.
    pub fn close(&self, code: u16, reason: &str) -> Result<(), WsError> {
        if let Ok(mut s) = self.state.lock() {
            if *s == WsState::Closed || *s == WsState::Closing {
                return Ok(());
            }
            *s = WsState::Closing;
        }

        let mut payload = code.to_be_bytes().to_vec();
        payload.extend_from_slice(reason.as_bytes());
        let frame = encode_frame(true, WsOpcode::Close, &payload, true, None);
        if let Ok(mut q) = self.outgoing_queue.lock() {
            q.push(frame);
        }

        if let Ok(mut s) = self.state.lock() {
            *s = WsState::Closed;
        }

        Ok(())
    }

    /// Ingests received raw bytes and decodes messages into the internal buffer.
    pub fn ingest_incoming_bytes(&self, mut buf: &[u8]) -> Result<usize, WsError> {
        let mut total_consumed = 0;
        while let Some((frame, consumed)) = decode_frame(buf)? {
            buf = &buf[consumed..];
            total_consumed += consumed;

            let msg = match frame.opcode {
                WsOpcode::Text => {
                    let s = String::from_utf8(frame.payload).map_err(|e| {
                        WsError::ProtocolError(format!("Invalid UTF-8 in text frame: {e}"))
                    })?;
                    WsMessage::Text(s)
                }
                WsOpcode::Binary => WsMessage::Binary(frame.payload),
                WsOpcode::Ping => WsMessage::Ping(frame.payload),
                WsOpcode::Pong => WsMessage::Pong(frame.payload),
                WsOpcode::Close => {
                    let (code, reason) = if frame.payload.len() >= 2 {
                        let code = u16::from_be_bytes([frame.payload[0], frame.payload[1]]);
                        let r = String::from_utf8_lossy(&frame.payload[2..]).to_string();
                        (Some(code), r)
                    } else {
                        (None, String::new())
                    };
                    WsMessage::Close(code, reason)
                }
                WsOpcode::Continuation => continue,
            };

            if let Ok(mut msgs) = self.received_messages.lock() {
                msgs.push(msg);
            }
        }
        Ok(total_consumed)
    }

    /// Pops the next received message from the queue, if any.
    pub fn poll_message(&self) -> Option<WsMessage> {
        self.received_messages.lock().ok()?.pop()
    }

    /// Drains all currently queued outgoing encoded frames.
    pub fn drain_outgoing_frames(&self) -> Vec<Vec<u8>> {
        self.outgoing_queue
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }
}
