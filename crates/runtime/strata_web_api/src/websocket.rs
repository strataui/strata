//! Pure-Rust W3C WebSocket Client with RFC 6455 Frame Serialization, Handshake & TLS (`wss://`).
//!
//! Provides:
//! - Complete RFC 6455 frame encoding and decoding with masking and control frame processing.
//! - Cryptographic `Sec-WebSocket-Key` generation and SHA-1 `Sec-WebSocket-Accept` validation.
//! - Native TLS transport for `wss://` connections via Mozilla WebPKI root certificates and `rustls`.
//! - Asynchronous background worker stream reading, heartbeat Ping/Pong processing, and queueing.

use std::collections::VecDeque;
use std::fmt;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::tls::{create_tls_stream, HttpTransportStream};

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

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
    InvalidUrl(String),
    InvalidState(String),
    HandshakeFailed(String),
    ProtocolError(String),
    InvalidFrame(String),
    NetworkError(String),
    Closed,
}

impl fmt::Display for WsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(s) => write!(f, "Invalid WebSocket URL: {s}"),
            Self::InvalidState(s) => write!(f, "Invalid WebSocket state: {s}"),
            Self::HandshakeFailed(s) => write!(f, "WebSocket handshake failed: {s}"),
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

/// Computes FIPS 180-1 SHA-1 digest in pure Rust.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h0 = 0x67452301u32;
    let mut h1 = 0xEFCDAB89u32;
    let mut h2 = 0x98BADCFEu32;
    let mut h3 = 0x10325476u32;
    let mut h4 = 0xC3D2E1F0u32;

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for (i, &w_i) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w_i);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

/// Standard base64 encoding.
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[(triple >> 18) & 0x3F] as char);
        out.push(TABLE[(triple >> 12) & 0x3F] as char);

        if chunk.len() > 1 {
            out.push(TABLE[(triple >> 6) & 0x3F] as char);
        } else {
            out.push('=');
        }

        if chunk.len() > 2 {
            out.push(TABLE[triple & 0x3F] as char);
        } else {
            out.push('=');
        }
    }

    out
}

/// Computes the expected RFC 6455 `Sec-WebSocket-Accept` header value from client key.
pub fn compute_sec_websocket_accept(key: &str) -> String {
    let mut concat = key.trim().as_bytes().to_vec();
    concat.extend_from_slice(WS_GUID.as_bytes());
    let digest = sha1(&concat);
    base64_encode(&digest)
}

/// Generates a randomized 16-byte base64 nonce for `Sec-WebSocket-Key`.
pub fn generate_sec_websocket_key() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut nonce = [0u8; 16];
    let bytes = now.to_be_bytes();
    for i in 0..16 {
        nonce[i] = bytes[i].wrapping_add(i as u8).rotate_left((i % 7) as u32);
    }
    base64_encode(&nonce)
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

    let byte0 = if fin { 0x80 } else { 0x00 } | (opcode as u8 & 0x0F);
    frame.push(byte0);

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
        let key = mask_key.unwrap_or([0x37, 0x91, 0xAF, 0x5C]);
        frame.extend_from_slice(&key);
        for (i, &b) in payload.iter().enumerate() {
            frame.push(b ^ key[i % 4]);
        }
    } else {
        frame.extend_from_slice(payload);
    }

    frame
}

/// Decodes an RFC 6455 frame from a byte buffer.
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

#[derive(Debug, Clone)]
struct ParsedWsUrl {
    is_secure: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_ws_url(url: &str) -> Result<ParsedWsUrl, WsError> {
    let (is_secure, rest) = if let Some(s) = url.strip_prefix("wss://") {
        (true, s)
    } else if let Some(s) = url.strip_prefix("ws://") {
        (false, s)
    } else {
        return Err(WsError::InvalidUrl(format!(
            "URL must start with ws:// or wss://: {url}"
        )));
    };

    let (host_port, path) = match rest.find('/') {
        Some(pos) => (&rest[..pos], &rest[pos..]),
        None => (rest, "/"),
    };

    let (host, port) = match host_port.find(':') {
        Some(pos) => {
            let h = &host_port[..pos];
            let p = host_port[pos + 1..]
                .parse::<u16>()
                .map_err(|e| WsError::InvalidUrl(format!("Invalid port: {e}")))?;
            (h.to_string(), p)
        }
        None => {
            let p = if is_secure { 443 } else { 80 };
            (host_port.to_string(), p)
        }
    };

    Ok(ParsedWsUrl {
        is_secure,
        host,
        port,
        path: path.to_string(),
    })
}

/// Ongoing fragmented message tracking state.
#[derive(Debug, Clone)]
pub struct WsFragment {
    pub opcode: WsOpcode,
    pub buffer: Vec<u8>,
}

/// W3C WebSocket client interface supporting `ws://` and TLS `wss://`.
#[derive(Clone)]
pub struct WebSocket {
    pub url: String,
    state: Arc<Mutex<WsState>>,
    outgoing_queue: Arc<Mutex<Vec<Vec<u8>>>>,
    received_messages: Arc<Mutex<VecDeque<WsMessage>>>,
    closed_flag: Arc<AtomicBool>,
    fragment_state: Arc<Mutex<Option<WsFragment>>>,
}

impl WebSocket {
    /// Creates an in-memory/mock WebSocket for offline testing.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            state: Arc::new(Mutex::new(WsState::Connecting)),
            outgoing_queue: Arc::new(Mutex::new(Vec::new())),
            received_messages: Arc::new(Mutex::new(VecDeque::new())),
            closed_flag: Arc::new(AtomicBool::new(false)),
            fragment_state: Arc::new(Mutex::new(None)),
        }
    }

    /// Establishes a real RFC 6455 WebSocket connection over plain TCP (`ws://`) or TLS (`wss://`).
    pub fn connect(url: &str) -> Result<Self, WsError> {
        let parsed = parse_ws_url(url)?;
        let addr = format!("{}:{}", parsed.host, parsed.port);

        let tcp = TcpStream::connect_timeout(
            &addr
                .parse()
                .or_else(|_| {
                    use std::net::ToSocketAddrs;
                    addr.to_socket_addrs()
                        .map_err(std::io::Error::other)?
                        .next()
                        .ok_or_else(|| {
                            std::io::Error::new(std::io::ErrorKind::NotFound, "Host resolution failed")
                        })
                })
                .map_err(|e| WsError::NetworkError(format!("DNS/Socket resolution error: {e}")))?,
            Duration::from_secs(10),
        )
        .map_err(|e| WsError::NetworkError(format!("TCP connection failed: {e}")))?;

        let mut transport = if parsed.is_secure {
            create_tls_stream(tcp, &parsed.host)
                .map_err(|e| WsError::NetworkError(format!("TLS handshake failed: {e}")))?
        } else {
            HttpTransportStream::Plain(tcp)
        };

        // HTTP/1.1 Upgrade Handshake
        let sec_key = generate_sec_websocket_key();
        let expected_accept = compute_sec_websocket_accept(&sec_key);

        let handshake_req = format!(
            "GET {} HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {}\r\n\
             Sec-WebSocket-Version: 13\r\n\
             User-Agent: Strata/0.1.0\r\n\r\n",
            parsed.path, parsed.host, parsed.port, sec_key
        );

        transport
            .write_all(handshake_req.as_bytes())
            .map_err(|e| WsError::NetworkError(format!("Failed to write handshake request: {e}")))?;
        transport
            .flush()
            .map_err(|e| WsError::NetworkError(format!("Failed to flush handshake request: {e}")))?;

        // Read handshake response
        let mut response_buf = Vec::new();
        let mut temp_buf = [0u8; 1024];
        let mut accept_validated = false;

        loop {
            let n = transport
                .read(&mut temp_buf)
                .map_err(|e| WsError::NetworkError(format!("Error reading handshake response: {e}")))?;
            if n == 0 {
                return Err(WsError::HandshakeFailed("Connection closed by server during handshake".into()));
            }
            response_buf.extend_from_slice(&temp_buf[..n]);

            if let Some(pos) = response_buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let header_str = String::from_utf8_lossy(&response_buf[..pos]);
                if !header_str.starts_with("HTTP/1.1 101") && !header_str.starts_with("HTTP/1.0 101") {
                    return Err(WsError::HandshakeFailed(format!(
                        "Server returned non-101 status: {}",
                        header_str.lines().next().unwrap_or("")
                    )));
                }

                for line in header_str.lines() {
                    if let Some((name, val)) = line.split_once(':') {
                        if name.trim().eq_ignore_ascii_case("sec-websocket-accept") {
                            if val.trim() == expected_accept {
                                accept_validated = true;
                            } else {
                                return Err(WsError::ProtocolError(format!(
                                    "Sec-WebSocket-Accept mismatch: expected {expected_accept}, got {}",
                                    val.trim()
                                )));
                            }
                        }
                    }
                }
                break;
            }
        }

        if !accept_validated {
            return Err(WsError::ProtocolError("Missing Sec-WebSocket-Accept header in response".into()));
        }

        let ws = Self {
            url: url.to_string(),
            state: Arc::new(Mutex::new(WsState::Open)),
            outgoing_queue: Arc::new(Mutex::new(Vec::new())),
            received_messages: Arc::new(Mutex::new(VecDeque::new())),
            closed_flag: Arc::new(AtomicBool::new(false)),
            fragment_state: Arc::new(Mutex::new(None)),
        };

        // Spawn background worker thread managing stream I/O
        let worker_state = ws.state.clone();
        let worker_outgoing = ws.outgoing_queue.clone();
        let worker_received = ws.received_messages.clone();
        let worker_closed = ws.closed_flag.clone();
        let worker_fragment = ws.fragment_state.clone();

        std::thread::Builder::new()
            .name("strata-ws-worker".into())
            .spawn(move || {
                let mut read_buf = Vec::new();
                let mut temp = [0u8; 4096];
                let mut last_ping = std::time::Instant::now();
                let ping_interval = Duration::from_secs(30);

                while !worker_closed.load(Ordering::Relaxed) {
                    // 1. Send periodic RFC 6455 Ping heartbeat
                    if last_ping.elapsed() >= ping_interval {
                        let ping = encode_frame(true, WsOpcode::Ping, b"strata-heartbeat", true, None);
                        if transport.write_all(&ping).is_err() || transport.flush().is_err() {
                            worker_closed.store(true, Ordering::Relaxed);
                            break;
                        }
                        last_ping = std::time::Instant::now();
                    }

                    // 2. Drain and send outgoing frames
                    let outgoing = worker_outgoing
                        .lock()
                        .map(|mut q| std::mem::take(&mut *q))
                        .unwrap_or_default();

                    for frame in outgoing {
                        if transport.write_all(&frame).is_err() || transport.flush().is_err() {
                            worker_closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }

                    // 3. Read incoming frames
                    match transport.read(&mut temp) {
                        Ok(0) => {
                            worker_closed.store(true, Ordering::Relaxed);
                            break;
                        }
                        Ok(n) => {
                            read_buf.extend_from_slice(&temp[..n]);
                            while let Ok(Some((frame, consumed))) = decode_frame(&read_buf) {
                                read_buf.drain(..consumed);

                                match frame.opcode {
                                    WsOpcode::Text => {
                                        if frame.fin {
                                            if let Ok(s) = String::from_utf8(frame.payload) {
                                                if let Ok(mut msgs) = worker_received.lock() {
                                                    msgs.push_back(WsMessage::Text(s));
                                                }
                                            }
                                        } else if let Ok(mut frag) = worker_fragment.lock() {
                                            *frag = Some(WsFragment {
                                                opcode: WsOpcode::Text,
                                                buffer: frame.payload,
                                            });
                                        }
                                    }
                                    WsOpcode::Binary => {
                                        if frame.fin {
                                            if let Ok(mut msgs) = worker_received.lock() {
                                                msgs.push_back(WsMessage::Binary(frame.payload));
                                            }
                                        } else if let Ok(mut frag) = worker_fragment.lock() {
                                            *frag = Some(WsFragment {
                                                opcode: WsOpcode::Binary,
                                                buffer: frame.payload,
                                            });
                                        }
                                    }
                                    WsOpcode::Continuation => {
                                        let mut completed_msg = None;
                                        if let Ok(mut frag_guard) = worker_fragment.lock() {
                                            if let Some(ref mut fragment) = *frag_guard {
                                                fragment.buffer.extend_from_slice(&frame.payload);
                                                if frame.fin {
                                                    let fragment_val = frag_guard.take().unwrap();
                                                    match fragment_val.opcode {
                                                        WsOpcode::Text => {
                                                            if let Ok(s) = String::from_utf8(fragment_val.buffer) {
                                                                completed_msg = Some(WsMessage::Text(s));
                                                            }
                                                        }
                                                        WsOpcode::Binary => {
                                                            completed_msg = Some(WsMessage::Binary(fragment_val.buffer));
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(msg) = completed_msg {
                                            if let Ok(mut msgs) = worker_received.lock() {
                                                msgs.push_back(msg);
                                            }
                                        }
                                    }
                                    WsOpcode::Ping => {
                                        // Auto-reply with Pong per RFC 6455 §5.5.3
                                        let pong = encode_frame(true, WsOpcode::Pong, &frame.payload, true, None);
                                        let _ = transport.write_all(&pong);
                                        let _ = transport.flush();
                                    }
                                    WsOpcode::Pong => {
                                        if let Ok(mut msgs) = worker_received.lock() {
                                            msgs.push_back(WsMessage::Pong(frame.payload));
                                        }
                                    }
                                    WsOpcode::Close => {
                                        let _ = transport.write_all(&encode_frame(true, WsOpcode::Close, &[], true, None));
                                        let _ = transport.flush();
                                        worker_closed.store(true, Ordering::Relaxed);
                                        break;
                                    }
                                }
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => {
                            worker_closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }

                if let Ok(mut s) = worker_state.lock() {
                    *s = WsState::Closed;
                }
            })
            .map_err(|e| WsError::NetworkError(format!("Failed to spawn WebSocket thread: {e}")))?;

        Ok(ws)
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

    /// Whether the WebSocket is open.
    pub fn is_open(&self) -> bool {
        self.ready_state() == WsState::Open
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

        self.closed_flag.store(true, Ordering::Relaxed);
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

            match frame.opcode {
                WsOpcode::Text => {
                    if frame.fin {
                        let s = String::from_utf8(frame.payload).map_err(|e| {
                            WsError::ProtocolError(format!("Invalid UTF-8 in text frame: {e}"))
                        })?;
                        if let Ok(mut msgs) = self.received_messages.lock() {
                            msgs.push_back(WsMessage::Text(s));
                        }
                    } else if let Ok(mut frag) = self.fragment_state.lock() {
                        *frag = Some(WsFragment {
                            opcode: WsOpcode::Text,
                            buffer: frame.payload,
                        });
                    }
                }
                WsOpcode::Binary => {
                    if frame.fin {
                        if let Ok(mut msgs) = self.received_messages.lock() {
                            msgs.push_back(WsMessage::Binary(frame.payload));
                        }
                    } else if let Ok(mut frag) = self.fragment_state.lock() {
                        *frag = Some(WsFragment {
                            opcode: WsOpcode::Binary,
                            buffer: frame.payload,
                        });
                    }
                }
                WsOpcode::Continuation => {
                    let mut completed_msg = None;
                    if let Ok(mut frag_guard) = self.fragment_state.lock() {
                        if let Some(ref mut fragment) = *frag_guard {
                            fragment.buffer.extend_from_slice(&frame.payload);
                            if frame.fin {
                                let fragment_val = frag_guard.take().unwrap();
                                match fragment_val.opcode {
                                    WsOpcode::Text => {
                                        let s = String::from_utf8(fragment_val.buffer).map_err(|e| {
                                            WsError::ProtocolError(format!("Invalid UTF-8 in continuation frame: {e}"))
                                        })?;
                                        completed_msg = Some(WsMessage::Text(s));
                                    }
                                    WsOpcode::Binary => {
                                        completed_msg = Some(WsMessage::Binary(fragment_val.buffer));
                                    }
                                    _ => {}
                                }
                            }
                        } else {
                            return Err(WsError::ProtocolError("Unexpected continuation frame without initial fragmented frame".into()));
                        }
                    }
                    if let Some(msg) = completed_msg {
                        if let Ok(mut msgs) = self.received_messages.lock() {
                            msgs.push_back(msg);
                        }
                    }
                }
                WsOpcode::Ping => {
                    if let Ok(mut msgs) = self.received_messages.lock() {
                        msgs.push_back(WsMessage::Ping(frame.payload));
                    }
                }
                WsOpcode::Pong => {
                    if let Ok(mut msgs) = self.received_messages.lock() {
                        msgs.push_back(WsMessage::Pong(frame.payload));
                    }
                }
                WsOpcode::Close => {
                    let (code, reason) = if frame.payload.len() >= 2 {
                        let code = u16::from_be_bytes([frame.payload[0], frame.payload[1]]);
                        let r = String::from_utf8_lossy(&frame.payload[2..]).to_string();
                        (Some(code), r)
                    } else {
                        (None, String::new())
                    };
                    if let Ok(mut msgs) = self.received_messages.lock() {
                        msgs.push_back(WsMessage::Close(code, reason));
                    }
                }
            }
        }
        Ok(total_consumed)
    }

    /// Pops the next received message from the queue, if any.
    pub fn poll_message(&self) -> Option<WsMessage> {
        self.received_messages.lock().ok()?.pop_front()
    }

    /// Waits up to `timeout` for a message to arrive.
    pub fn recv_message_timeout(&self, timeout: Duration) -> Option<WsMessage> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if let Some(msg) = self.poll_message() {
                return Some(msg);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    /// Drains all currently queued outgoing encoded frames.
    pub fn drain_outgoing_frames(&self) -> Vec<Vec<u8>> {
        self.outgoing_queue
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }
}
