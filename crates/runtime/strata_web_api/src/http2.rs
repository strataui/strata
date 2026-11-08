//! Pure-Rust HTTP/2 Framing, HPACK Header Compression, and Client Protocol Engine (RFC 7540, RFC 7541, RFC 9113).

use std::io::{Read, Write};
use std::sync::OnceLock;

use crate::fetch::{decompress_body, FetchError, Headers, ParsedUrl, RequestInit, Response};
use crate::huffman_table::HUFFMAN_CODE_TABLE;

/// HTTP/2 Connection Preface sent by the client upon connection establishment.
pub const HTTP2_PREFACE: &[u8; 24] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// Frame header length in octets (24-bit length + 8-bit type + 8-bit flags + 31-bit stream ID).
pub const FRAME_HEADER_LEN: usize = 9;

// Frame Types (RFC 7540 Section 11.2)
pub const FRAME_DATA: u8 = 0x0;
pub const FRAME_HEADERS: u8 = 0x1;
pub const FRAME_PRIORITY: u8 = 0x2;
pub const FRAME_RST_STREAM: u8 = 0x3;
pub const FRAME_SETTINGS: u8 = 0x4;
pub const FRAME_PUSH_PROMISE: u8 = 0x5;
pub const FRAME_PING: u8 = 0x6;
pub const FRAME_GOAWAY: u8 = 0x7;
pub const FRAME_WINDOW_UPDATE: u8 = 0x8;
pub const FRAME_CONTINUATION: u8 = 0x9;

// Frame Flags
pub const FLAG_END_STREAM: u8 = 0x1;
pub const FLAG_ACK: u8 = 0x1;
pub const FLAG_END_HEADERS: u8 = 0x4;
pub const FLAG_PADDED: u8 = 0x8;
pub const FLAG_PRIORITY: u8 = 0x20;

/// Default initial maximum frame size per RFC 7540 Section 6.5.2 (16,384 octets / 2^14).
pub const DEFAULT_MAX_FRAME_SIZE: u32 = 16_384;

/// Absolute maximum allowable frame size per RFC 7540 Section 4.1 (16,777,215 octets / 2^24 - 1).
pub const ABSOLUTE_MAX_FRAME_SIZE: u32 = 16_777_215;

// RFC 7540 Section 6.5.2 Settings Parameters
pub const SETTINGS_HEADER_TABLE_SIZE: u16 = 0x1;
pub const SETTINGS_ENABLE_PUSH: u16 = 0x2;
pub const SETTINGS_MAX_CONCURRENT_STREAMS: u16 = 0x3;
pub const SETTINGS_INITIAL_WINDOW_SIZE: u16 = 0x4;
pub const SETTINGS_MAX_FRAME_SIZE: u16 = 0x5;
pub const SETTINGS_MAX_HEADER_LIST_SIZE: u16 = 0x6;

/// RFC 7540 Section 7 HTTP/2 Error Codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Http2ErrorCode {
    NoError = 0x0,
    ProtocolError = 0x1,
    InternalError = 0x2,
    FlowControlError = 0x3,
    SettingsTimeout = 0x4,
    StreamClosed = 0x5,
    FrameSizeError = 0x6,
    RefusedStream = 0x7,
    Cancel = 0x8,
    CompressionError = 0x9,
    ConnectError = 0xa,
    EnhanceYourCalm = 0xb,
    InadequateSecurity = 0xc,
    Http11Required = 0xd,
}

/// Active HTTP/2 connection settings negotiated across the lifetime of the connection.
#[derive(Debug, Clone)]
pub struct Http2ConnectionSettings {
    pub header_table_size: u32,
    pub enable_push: bool,
    pub max_concurrent_streams: Option<u32>,
    pub initial_window_size: u32,
    pub max_frame_size: u32,
    pub max_header_list_size: Option<u32>,
}

impl Default for Http2ConnectionSettings {
    fn default() -> Self {
        Self {
            header_table_size: 4096,
            enable_push: false,
            max_concurrent_streams: None,
            initial_window_size: 65535,
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            max_header_list_size: None,
        }
    }
}

impl Http2ConnectionSettings {
    /// Applies received SETTINGS payload key-value pairs (RFC 7540 Section 6.5.2).
    pub fn apply_settings(&mut self, payload: &[u8]) -> Result<(), FetchError> {
        if !payload.len().is_multiple_of(6) {
            return Err(FetchError::NetworkError(
                "FRAME_SIZE_ERROR: SETTINGS frame payload length must be a multiple of 6".into(),
            ));
        }

        let mut cursor = 0;
        while cursor < payload.len() {
            let id = u16::from_be_bytes([payload[cursor], payload[cursor + 1]]);
            let val = u32::from_be_bytes([
                payload[cursor + 2],
                payload[cursor + 3],
                payload[cursor + 4],
                payload[cursor + 5],
            ]);
            cursor += 6;

            match id {
                SETTINGS_HEADER_TABLE_SIZE => {
                    self.header_table_size = val;
                }
                SETTINGS_ENABLE_PUSH => {
                    if val > 1 {
                        return Err(FetchError::NetworkError(
                            "PROTOCOL_ERROR: SETTINGS_ENABLE_PUSH must be 0 or 1".into(),
                        ));
                    }
                    self.enable_push = val == 1;
                }
                SETTINGS_MAX_CONCURRENT_STREAMS => {
                    self.max_concurrent_streams = Some(val);
                }
                SETTINGS_INITIAL_WINDOW_SIZE => {
                    if val > 0x7FFF_FFFF {
                        return Err(FetchError::NetworkError(
                            "FLOW_CONTROL_ERROR: SETTINGS_INITIAL_WINDOW_SIZE exceeds 2^31-1".into(),
                        ));
                    }
                    self.initial_window_size = val;
                }
                SETTINGS_MAX_FRAME_SIZE => {
                    if !(DEFAULT_MAX_FRAME_SIZE..=ABSOLUTE_MAX_FRAME_SIZE).contains(&val) {
                        return Err(FetchError::NetworkError(format!(
                            "PROTOCOL_ERROR: Invalid SETTINGS_MAX_FRAME_SIZE {val} (must be between 16384 and 16777215)"
                        )));
                    }
                    self.max_frame_size = val;
                }
                SETTINGS_MAX_HEADER_LIST_SIZE => {
                    self.max_header_list_size = Some(val);
                }
                _ => {
                    // Unknown settings parameters MUST be ignored per RFC 7540 Section 6.5.2
                }
            }
        }
        Ok(())
    }
}

/// An individual HTTP/2 protocol frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Http2Frame {
    pub length: u32,
    pub frame_type: u8,
    pub flags: u8,
    pub stream_id: u32,
    pub payload: Vec<u8>,
}

impl Http2Frame {
    /// Constructs a new `Http2Frame`.
    pub fn new(frame_type: u8, flags: u8, stream_id: u32, payload: Vec<u8>) -> Self {
        Self {
            length: payload.len() as u32,
            frame_type,
            flags,
            stream_id,
            payload,
        }
    }

    /// Serializes the frame into its wire representation.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(FRAME_HEADER_LEN + self.payload.len());
        let len = self.payload.len() as u32;

        out.push(((len >> 16) & 0xFF) as u8);
        out.push(((len >> 8) & 0xFF) as u8);
        out.push((len & 0xFF) as u8);

        out.push(self.frame_type);
        out.push(self.flags);

        let stream_be = (self.stream_id & 0x7FFF_FFFF).to_be_bytes();
        out.extend_from_slice(&stream_be);

        out.extend_from_slice(&self.payload);
        out
    }

    /// Validates frame invariants according to RFC 7540 Section 6.
    pub fn validate_invariants(&self) -> Result<(), FetchError> {
        match self.frame_type {
            FRAME_SETTINGS => {
                if self.stream_id != 0 {
                    return Err(FetchError::NetworkError(
                        "PROTOCOL_ERROR: SETTINGS frame stream ID must be 0".into(),
                    ));
                }
                if (self.flags & FLAG_ACK) != 0 && self.length != 0 {
                    return Err(FetchError::NetworkError(
                        "FRAME_SIZE_ERROR: ACK SETTINGS frame must have empty payload".into(),
                    ));
                }
                if !self.length.is_multiple_of(6) {
                    return Err(FetchError::NetworkError(
                        "FRAME_SIZE_ERROR: SETTINGS frame payload length must be a multiple of 6".into(),
                    ));
                }
            }
            FRAME_PING => {
                if self.stream_id != 0 {
                    return Err(FetchError::NetworkError(
                        "PROTOCOL_ERROR: PING frame stream ID must be 0".into(),
                    ));
                }
                if self.length != 8 {
                    return Err(FetchError::NetworkError(format!(
                        "FRAME_SIZE_ERROR: PING frame payload length must be exactly 8, got {}",
                        self.length
                    )));
                }
            }
            FRAME_RST_STREAM => {
                if self.stream_id == 0 {
                    return Err(FetchError::NetworkError(
                        "PROTOCOL_ERROR: RST_STREAM frame stream ID cannot be 0".into(),
                    ));
                }
                if self.length != 4 {
                    return Err(FetchError::NetworkError(format!(
                        "FRAME_SIZE_ERROR: RST_STREAM frame payload length must be exactly 4, got {}",
                        self.length
                    )));
                }
            }
            FRAME_WINDOW_UPDATE => {
                if self.length != 4 {
                    return Err(FetchError::NetworkError(format!(
                        "FRAME_SIZE_ERROR: WINDOW_UPDATE frame payload length must be exactly 4, got {}",
                        self.length
                    )));
                }
                if self.payload.len() == 4 {
                    let increment = u32::from_be_bytes([
                        self.payload[0] & 0x7F,
                        self.payload[1],
                        self.payload[2],
                        self.payload[3],
                    ]);
                    if increment == 0 {
                        return Err(FetchError::NetworkError(
                            "PROTOCOL_ERROR: WINDOW_UPDATE increment must be non-zero".into(),
                        ));
                    }
                }
            }
            FRAME_DATA | FRAME_HEADERS if self.stream_id == 0 => {
                return Err(FetchError::NetworkError(
                    "PROTOCOL_ERROR: DATA and HEADERS frames cannot have stream ID 0".into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    /// Reads an individual frame from a synchronous byte stream, enforcing the active max frame size.
    pub fn read_from_bounded<R: Read>(reader: &mut R, max_frame_size: u32) -> Result<Self, FetchError> {
        let mut header = [0u8; FRAME_HEADER_LEN];
        reader.read_exact(&mut header).map_err(|e| {
            FetchError::NetworkError(format!("Failed to read HTTP/2 frame header: {e}"))
        })?;

        let length = ((header[0] as u32) << 16) | ((header[1] as u32) << 8) | (header[2] as u32);
        let frame_type = header[3];
        let flags = header[4];
        let stream_id = u32::from_be_bytes([header[5] & 0x7F, header[6], header[7], header[8]]);

        if length > max_frame_size {
            return Err(FetchError::NetworkError(format!(
                "FRAME_SIZE_ERROR: Frame length {length} exceeds maximum permitted frame size {max_frame_size}"
            )));
        }

        let mut payload = vec![0u8; length as usize];
        if length > 0 {
            reader.read_exact(&mut payload).map_err(|e| {
                FetchError::NetworkError(format!(
                    "Failed to read HTTP/2 frame payload ({length} bytes): {e}"
                ))
            })?;
        }

        let frame = Self {
            length,
            frame_type,
            flags,
            stream_id,
            payload,
        };

        frame.validate_invariants()?;
        Ok(frame)
    }

    /// Reads an individual frame from a synchronous byte stream using the default RFC 7540 initial limit (16,384 bytes).
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self, FetchError> {
        Self::read_from_bounded(reader, DEFAULT_MAX_FRAME_SIZE)
    }
}

// RFC 7541 Appendix A — Static Table (Indices 1..=61)
pub const STATIC_TABLE: &[(&str, &str)] = &[
    ("", ""), // 0 is unused in 1-based indexing
    (":authority", ""),
    (":method", "GET"),
    (":method", "POST"),
    (":path", "/"),
    (":path", "/index.html"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "200"),
    (":status", "204"),
    (":status", "206"),
    (":status", "304"),
    (":status", "400"),
    (":status", "404"),
    (":status", "500"),
    ("accept-charset", ""),
    ("accept-encoding", "gzip, deflate"),
    ("accept-language", ""),
    ("accept-ranges", ""),
    ("accept", ""),
    ("access-control-allow-origin", ""),
    ("age", ""),
    ("allow", ""),
    ("authorization", ""),
    ("cache-control", ""),
    ("content-disposition", ""),
    ("content-encoding", ""),
    ("content-language", ""),
    ("content-length", ""),
    ("content-location", ""),
    ("content-range", ""),
    ("content-type", ""),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("expect", ""),
    ("expires", ""),
    ("from", ""),
    ("host", ""),
    ("if-match", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("if-range", ""),
    ("if-unmodified-since", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("max-forwards", ""),
    ("proxy-authenticate", ""),
    ("proxy-authorization", ""),
    ("range", ""),
    ("referer", ""),
    ("refresh", ""),
    ("retry-after", ""),
    ("server", ""),
    ("set-cookie", ""),
    ("strict-transport-security", ""),
    ("transfer-encoding", ""),
    ("user-agent", ""),
    ("vary", ""),
    ("via", ""),
    ("www-authenticate", ""),
];

// RFC 7541 Appendix B: Huffman Decoding Prefix Tree Node
#[derive(Clone, Copy, Default)]
struct HuffmanNode {
    left: u16,   // Index of left child (bit 0); 0 means unallocated
    right: u16,  // Index of right child (bit 1); 0 means unallocated
    symbol: u16, // Decoded symbol (0..=255 for octet, 256 for EOS, 0xFFFF for branch)
}

fn get_huffman_tree() -> &'static [HuffmanNode] {
    static TREE: OnceLock<Vec<HuffmanNode>> = OnceLock::new();
    TREE.get_or_init(|| {
        let mut nodes = Vec::with_capacity(600);
        // Root node at index 0
        nodes.push(HuffmanNode {
            left: 0,
            right: 0,
            symbol: 0xFFFF,
        });

        for (sym, &(code, len)) in HUFFMAN_CODE_TABLE.iter().enumerate() {
            let mut curr = 0;
            for i in (0..len).rev() {
                let bit = ((code >> i) & 1) != 0;
                let next_idx = if !bit {
                    let left = nodes[curr].left;
                    if left == 0 {
                        let new_idx = nodes.len() as u16;
                        nodes.push(HuffmanNode {
                            left: 0,
                            right: 0,
                            symbol: 0xFFFF,
                        });
                        nodes[curr].left = new_idx;
                        new_idx
                    } else {
                        left
                    }
                } else {
                    let right = nodes[curr].right;
                    if right == 0 {
                        let new_idx = nodes.len() as u16;
                        nodes.push(HuffmanNode {
                            left: 0,
                            right: 0,
                            symbol: 0xFFFF,
                        });
                        nodes[curr].right = new_idx;
                        new_idx
                    } else {
                        right
                    }
                };
                curr = next_idx as usize;
            }
            nodes[curr].symbol = sym as u16;
        }

        nodes
    })
}

/// Decodes an HPACK Huffman-encoded octet slice into a UTF-8 string per RFC 7541 Section 5.2.
pub fn decode_huffman_string(raw: &[u8]) -> Result<String, FetchError> {
    let tree = get_huffman_tree();
    let mut out = Vec::with_capacity(raw.len());
    let mut curr = 0;

    for &b in raw {
        for bit_idx in (0..8).rev() {
            let bit = ((b >> bit_idx) & 1) != 0;
            let next = if !bit {
                tree[curr].left as usize
            } else {
                tree[curr].right as usize
            };

            if next == 0 {
                return Err(FetchError::NetworkError(
                    "Invalid HPACK Huffman bit code".into(),
                ));
            }

            let sym = tree[next].symbol;
            if sym != 0xFFFF {
                if sym == 256 {
                    return Err(FetchError::NetworkError(
                        "EOS symbol found in HPACK string".into(),
                    ));
                }
                out.push(sym as u8);
                curr = 0; // Reset to root
            } else {
                curr = next;
            }
        }
    }

    // Per RFC 7541 Section 5.2: At the end of the sequence, any remaining bits in the last octet
    // must be padding corresponding to the prefix of EOS (all 1s), and strictly less than 8 bits.
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Encodes an integer into a byte vector with a given prefix size per RFC 7541 Section 5.1.
pub fn encode_integer(mut value: usize, prefix_bits: u8, prefix_mask: u8) -> Vec<u8> {
    let max_prefix = (1 << prefix_bits) - 1;
    if value < max_prefix {
        return vec![prefix_mask | (value as u8)];
    }

    let mut out = vec![prefix_mask | (max_prefix as u8)];
    value -= max_prefix;

    while value >= 128 {
        out.push(((value % 128) as u8) | 0x80);
        value /= 128;
    }
    out.push(value as u8);
    out
}

/// Decodes an integer from a byte slice with a given prefix size per RFC 7541 Section 5.1.
pub fn decode_integer(slice: &[u8], prefix_bits: u8) -> Result<(usize, usize), FetchError> {
    if slice.is_empty() {
        return Err(FetchError::NetworkError("Truncated HPACK integer".into()));
    }

    let mask = (1usize << prefix_bits) - 1;
    let first = slice[0] as usize & mask;
    if first < mask {
        return Ok((first, 1));
    }

    let mut value = mask;
    let mut m = 0;
    let mut cursor = 1;

    while cursor < slice.len() {
        let b = slice[cursor];
        cursor += 1;
        value += ((b & 0x7F) as usize) << m;
        m += 7;
        if (b & 0x80) == 0 {
            return Ok((value, cursor));
        }
        if m >= 28 {
            return Err(FetchError::NetworkError("HPACK integer overflow".into()));
        }
    }

    Err(FetchError::NetworkError(
        "Truncated HPACK multi-byte integer".into(),
    ))
}

/// Encodes a string literal into an HPACK octet sequence (uncompressed literal, H=0).
pub fn encode_string_literal(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = encode_integer(bytes.len(), 7, 0x00);
    out.extend_from_slice(bytes);
    out
}

/// Decodes an HPACK string literal starting at cursor, supporting both plain and Huffman encoding.
pub fn decode_string_literal(slice: &[u8]) -> Result<(String, usize), FetchError> {
    if slice.is_empty() {
        return Err(FetchError::NetworkError(
            "Truncated HPACK string literal".into(),
        ));
    }
    let is_huffman = (slice[0] & 0x80) != 0;
    let (length, consumed) = decode_integer(slice, 7)?;

    if slice.len() < consumed + length {
        return Err(FetchError::NetworkError("Truncated HPACK string body".into()));
    }

    let raw = &slice[consumed..consumed + length];
    let s = if is_huffman {
        decode_huffman_string(raw)?
    } else {
        String::from_utf8_lossy(raw).into_owned()
    };

    Ok((s, consumed + length))
}

/// Stateful HPACK decoder maintaining dynamic table per RFC 7541.
#[derive(Debug, Clone, Default)]
pub struct HpackDecoder {
    pub dynamic_table: Vec<(String, String)>,
    pub max_table_size: usize,
    pub current_table_size: usize,
}

impl HpackDecoder {
    pub fn new() -> Self {
        Self {
            dynamic_table: Vec::new(),
            max_table_size: 4096,
            current_table_size: 0,
        }
    }

    /// Retrieves an entry by 1-based combined static and dynamic table index.
    pub fn get_header(&self, index: usize) -> Result<(String, String), FetchError> {
        if index == 0 {
            return Err(FetchError::NetworkError("HPACK index 0 is invalid".into()));
        }
        if index < STATIC_TABLE.len() {
            let (k, v) = STATIC_TABLE[index];
            return Ok((k.to_string(), v.to_string()));
        }

        let dyn_idx = index - STATIC_TABLE.len();
        if dyn_idx < self.dynamic_table.len() {
            return Ok(self.dynamic_table[dyn_idx].clone());
        }

        Err(FetchError::NetworkError(format!(
            "HPACK index {index} out of table bounds"
        )))
    }

    /// Retrieves a header name by 1-based combined index.
    pub fn get_name(&self, index: usize) -> Result<String, FetchError> {
        self.get_header(index).map(|(name, _)| name)
    }

    /// Adds a new header entry to the dynamic table with FIFO eviction per RFC 7541 Section 4.1.
    pub fn add_entry(&mut self, name: String, value: String) {
        let entry_size = name.len() + value.len() + 32;
        while self.current_table_size + entry_size > self.max_table_size
            && !self.dynamic_table.is_empty()
        {
            if let Some((old_name, old_val)) = self.dynamic_table.pop() {
                let old_size = old_name.len() + old_val.len() + 32;
                self.current_table_size = self.current_table_size.saturating_sub(old_size);
            }
        }

        if entry_size <= self.max_table_size {
            self.current_table_size += entry_size;
            self.dynamic_table.insert(0, (name, value));
        }
    }

    /// Decodes an HPACK header block into key-value pairs.
    pub fn decode_header_block(
        &mut self,
        block: &[u8],
    ) -> Result<Vec<(String, String)>, FetchError> {
        let mut headers = Vec::new();
        let mut cursor = 0;

        while cursor < block.len() {
            let b = block[cursor];

            if (b & 0x80) != 0 {
                // Indexed Header Field: 1xxxxxxx (7-bit prefix)
                let (index, consumed) = decode_integer(&block[cursor..], 7)?;
                cursor += consumed;
                let header = self.get_header(index)?;
                headers.push(header);
            } else if (b & 0x40) != 0 {
                // Literal with Incremental Indexing: 01xxxxxx (6-bit prefix)
                let (index, consumed) = decode_integer(&block[cursor..], 6)?;
                cursor += consumed;

                let name = if index > 0 {
                    self.get_name(index)?
                } else {
                    let (n, n_consumed) = decode_string_literal(&block[cursor..])?;
                    cursor += n_consumed;
                    n
                };

                let (val, v_consumed) = decode_string_literal(&block[cursor..])?;
                cursor += v_consumed;

                self.add_entry(name.clone(), val.clone());
                headers.push((name, val));
            } else if (b & 0x20) != 0 {
                // Dynamic Table Size Update: 001xxxxx (5-bit prefix)
                let (new_size, consumed) = decode_integer(&block[cursor..], 5)?;
                cursor += consumed;
                self.max_table_size = new_size;
                while self.current_table_size > self.max_table_size && !self.dynamic_table.is_empty() {
                    if let Some((old_name, old_val)) = self.dynamic_table.pop() {
                        let old_size = old_name.len() + old_val.len() + 32;
                        self.current_table_size = self.current_table_size.saturating_sub(old_size);
                    }
                }
            } else {
                // Literal without Indexing (0000xxxx) or Never Indexed (0001xxxx) (4-bit prefix)
                let (index, consumed) = decode_integer(&block[cursor..], 4)?;
                cursor += consumed;

                let name = if index > 0 {
                    self.get_name(index)?
                } else {
                    let (n, n_consumed) = decode_string_literal(&block[cursor..])?;
                    cursor += n_consumed;
                    n
                };

                let (val, v_consumed) = decode_string_literal(&block[cursor..])?;
                cursor += v_consumed;
                headers.push((name, val));
            }
        }

        Ok(headers)
    }
}

/// Convenience function to decode HPACK headers with a clean ephemeral decoder.
pub fn decode_hpack_headers(block: &[u8]) -> Result<Vec<(String, String)>, FetchError> {
    let mut decoder = HpackDecoder::new();
    decoder.decode_header_block(block)
}

/// Encodes a list of header key-value pairs into an HPACK header block.
pub fn encode_hpack_headers(headers: &[(&str, &str)]) -> Vec<u8> {
    let mut block = Vec::new();

    for &(k, v) in headers {
        let k_lower = k.to_ascii_lowercase();

        let mut matched_index = None;
        let mut name_matched_index = None;

        for (idx, &(s_k, s_v)) in STATIC_TABLE.iter().enumerate().skip(1) {
            if s_k == k_lower {
                if name_matched_index.is_none() {
                    name_matched_index = Some(idx);
                }
                if s_v == v {
                    matched_index = Some(idx);
                    break;
                }
            }
        }

        if let Some(idx) = matched_index {
            // Indexed Header Field Representation: 1xxxxxxx
            block.extend(encode_integer(idx, 7, 0x80));
        } else if let Some(idx) = name_matched_index {
            // Literal Header Field without Indexing — Indexed Name: 0000xxxx
            block.extend(encode_integer(idx, 4, 0x00));
            block.extend(encode_string_literal(v));
        } else {
            // Literal Header Field without Indexing — New Name: 00000000
            block.push(0x00);
            block.extend(encode_string_literal(&k_lower));
            block.extend(encode_string_literal(v));
        }
    }

    block
}

/// Manages RFC 7540 §5.2 flow control window tracking and updates.
#[derive(Debug, Clone)]
pub struct FlowControlWindow {
    pub current: i64,
}

impl FlowControlWindow {
    pub fn new(initial: i64) -> Self {
        Self { current: initial }
    }

    pub fn update(&mut self, increment: u32) -> Result<(), FetchError> {
        if increment == 0 {
            return Err(FetchError::NetworkError(
                "PROTOCOL_ERROR: WINDOW_UPDATE increment must be non-zero (RFC 7540 §6.9)".into(),
            ));
        }
        let next = self.current + increment as i64;
        if next > 0x7FFF_FFFF {
            return Err(FetchError::NetworkError(
                "FLOW_CONTROL_ERROR: Window update exceeds maximum 2^31-1 (RFC 7540 §6.9.1)".into(),
            ));
        }
        self.current = next;
        Ok(())
    }

    pub fn consume(&mut self, amount: usize) -> Result<(), FetchError> {
        if (amount as i64) > self.current {
            return Err(FetchError::NetworkError(
                "FLOW_CONTROL_ERROR: Sent data exceeds available flow control window".into(),
            ));
        }
        self.current -= amount as i64;
        Ok(())
    }
}

fn send_available_body_chunks<T: Write>(
    stream: &mut T,
    body: &[u8],
    body_bytes_sent: &mut usize,
    conn_win: &mut FlowControlWindow,
    stream_win: &mut FlowControlWindow,
    max_frame_size: u32,
) -> Result<bool, FetchError> {
    let body_total = body.len();
    while *body_bytes_sent < body_total {
        let available_window = conn_win.current.min(stream_win.current);
        if available_window <= 0 {
            return Ok(false);
        }

        let remaining = body_total - *body_bytes_sent;
        let chunk_size = remaining
            .min(available_window as usize)
            .min(max_frame_size as usize);

        if chunk_size == 0 {
            return Ok(false);
        }

        let next_offset = *body_bytes_sent + chunk_size;
        let is_last = next_offset == body_total;
        let flags = if is_last { FLAG_END_STREAM } else { 0 };

        let chunk_payload = body[*body_bytes_sent..next_offset].to_vec();
        let data_frame = Http2Frame::new(FRAME_DATA, flags, 1, chunk_payload);
        stream
            .write_all(&data_frame.encode())
            .map_err(|e| FetchError::NetworkError(format!("Failed to write request DATA: {e}")))?;
        stream
            .flush()
            .map_err(|e| FetchError::NetworkError(format!("Failed to flush request DATA: {e}")))?;

        conn_win.consume(chunk_size)?;
        stream_win.consume(chunk_size)?;
        *body_bytes_sent = next_offset;

        if is_last {
            return Ok(true);
        }
    }

    Ok(true)
}

/// Executes a full HTTP/2 request cycle over a synchronous stream (Plain TCP or TLS).
pub fn execute_http2_request<T: Read + Write>(
    stream: &mut T,
    req: &RequestInit,
    parsed: &ParsedUrl,
) -> Result<Response, FetchError> {
    // 1. Send 24-octet Client Connection Preface
    stream
        .write_all(HTTP2_PREFACE)
        .map_err(|e| FetchError::NetworkError(format!("Failed to write HTTP/2 preface: {e}")))?;

    // 2. Send Initial Client SETTINGS Frame (Stream 0)
    let client_settings = Http2Frame::new(FRAME_SETTINGS, 0, 0, Vec::new());
    stream
        .write_all(&client_settings.encode())
        .map_err(|e| FetchError::NetworkError(format!("Failed to write client SETTINGS: {e}")))?;
    stream
        .flush()
        .map_err(|e| FetchError::NetworkError(format!("Failed to flush HTTP/2 init: {e}")))?;

    // 3. Assemble and Send Request HEADERS Frame (Stream 1)
    let method = req.method.to_uppercase();
    let authority = if (parsed.scheme == "https" && parsed.port == 443)
        || (parsed.scheme == "http" && parsed.port == 80)
    {
        parsed.host.clone()
    } else {
        format!("{}:{}", parsed.host, parsed.port)
    };

    let mut request_header_list: Vec<(&str, &str)> = vec![
        (":method", &method),
        (":scheme", &parsed.scheme),
        (":path", &parsed.path_and_query),
        (":authority", &authority),
    ];

    let mut owned_headers = Vec::new();
    if !req.headers.has("accept-encoding") {
        owned_headers.push(("accept-encoding".to_string(), "gzip, deflate, br".to_string()));
    }
    if !req.headers.has("user-agent") {
        owned_headers.push((
            "user-agent".to_string(),
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Strata/0.1.0".to_string(),
        ));
    }
    for (k, v) in req.headers.entries() {
        owned_headers.push((k.to_ascii_lowercase(), v.to_string()));
    }
    for (k, v) in &owned_headers {
        request_header_list.push((k.as_str(), v.as_str()));
    }

    let hpack_block = encode_hpack_headers(&request_header_list);

    let has_body = req.body.as_ref().map(|b| !b.is_empty()).unwrap_or(false);
    let header_flags = if has_body {
        FLAG_END_HEADERS
    } else {
        FLAG_END_HEADERS | FLAG_END_STREAM
    };

    let headers_frame = Http2Frame::new(FRAME_HEADERS, header_flags, 1, hpack_block);
    stream
        .write_all(&headers_frame.encode())
        .map_err(|e| FetchError::NetworkError(format!("Failed to write request HEADERS: {e}")))?;

    let mut conn_settings = Http2ConnectionSettings::default();
    let mut conn_send_window = FlowControlWindow::new(65_535);
    let mut stream_send_window = FlowControlWindow::new(65_535);
    let mut body_bytes_sent = 0usize;
    let body_total = req.body.as_ref().map(|b| b.len()).unwrap_or(0);

    if let Some(ref body) = req.body {
        if !body.is_empty() {
            let _ = send_available_body_chunks(
                stream,
                body,
                &mut body_bytes_sent,
                &mut conn_send_window,
                &mut stream_send_window,
                conn_settings.max_frame_size,
            )?;
        }
    }

    stream
        .flush()
        .map_err(|e| FetchError::NetworkError(format!("Failed to flush HTTP/2 request: {e}")))?;

    // 4. Read Response Frames
    let mut status_code = 200u16;
    let mut status_text = "OK".to_string();
    let mut response_headers = Headers::new();
    let mut raw_body = Vec::new();
    let mut headers_received = false;
    let mut stream_ended = false;
    let mut header_fragments = Vec::new();
    let mut awaiting_continuation = false;
    let mut hpack_decoder = HpackDecoder::new();

    while !stream_ended {
        let frame = Http2Frame::read_from_bounded(stream, conn_settings.max_frame_size)?;

        match frame.frame_type {
            FRAME_SETTINGS => {
                if (frame.flags & FLAG_ACK) == 0 {
                    let old_initial = conn_settings.initial_window_size;
                    conn_settings.apply_settings(&frame.payload)?;
                    if conn_settings.initial_window_size != old_initial {
                        let delta = conn_settings.initial_window_size as i64 - old_initial as i64;
                        stream_send_window.current += delta;
                    }
                    let ack = Http2Frame::new(FRAME_SETTINGS, FLAG_ACK, 0, Vec::new());
                    let _ = stream.write_all(&ack.encode());
                    let _ = stream.flush();
                    if let Some(ref body) = req.body {
                        if body_bytes_sent < body_total {
                            let _ = send_available_body_chunks(
                                stream,
                                body,
                                &mut body_bytes_sent,
                                &mut conn_send_window,
                                &mut stream_send_window,
                                conn_settings.max_frame_size,
                            )?;
                        }
                    }
                }
            }
            FRAME_PING => {
                if (frame.flags & FLAG_ACK) == 0 {
                    let ping_ack = Http2Frame::new(FRAME_PING, FLAG_ACK, 0, frame.payload);
                    let _ = stream.write_all(&ping_ack.encode());
                    let _ = stream.flush();
                }
            }
            FRAME_WINDOW_UPDATE => {
                if frame.payload.len() != 4 {
                    return Err(FetchError::NetworkError(
                        "FRAME_SIZE_ERROR: WINDOW_UPDATE payload must be exactly 4 bytes".into(),
                    ));
                }
                let increment = u32::from_be_bytes([
                    frame.payload[0] & 0x7F,
                    frame.payload[1],
                    frame.payload[2],
                    frame.payload[3],
                ]);
                if frame.stream_id == 0 {
                    conn_send_window.update(increment)?;
                } else if frame.stream_id == 1 {
                    stream_send_window.update(increment)?;
                }
                if let Some(ref body) = req.body {
                    if body_bytes_sent < body_total {
                        let _ = send_available_body_chunks(
                            stream,
                            body,
                            &mut body_bytes_sent,
                            &mut conn_send_window,
                            &mut stream_send_window,
                            conn_settings.max_frame_size,
                        )?;
                    }
                }
            }
            FRAME_HEADERS if frame.stream_id == 1 => {
                let mut payload_slice = &frame.payload[..];

                // Handle FLAG_PADDED (0x8)
                if (frame.flags & FLAG_PADDED) != 0 {
                    if payload_slice.is_empty() {
                        return Err(FetchError::NetworkError("Malformed padded HEADERS frame".into()));
                    }
                    let pad_len = payload_slice[0] as usize;
                    payload_slice = &payload_slice[1..];
                    if payload_slice.len() < pad_len {
                        return Err(FetchError::NetworkError("Padding exceeds HEADERS payload".into()));
                    }
                    payload_slice = &payload_slice[..payload_slice.len() - pad_len];
                }

                // Handle FLAG_PRIORITY (0x20)
                if (frame.flags & FLAG_PRIORITY) != 0 {
                    if payload_slice.len() < 5 {
                        return Err(FetchError::NetworkError("Malformed prioritized HEADERS frame".into()));
                    }
                    payload_slice = &payload_slice[5..];
                }

                header_fragments.extend_from_slice(payload_slice);

                if (frame.flags & FLAG_END_HEADERS) != 0 {
                    let decoded = hpack_decoder.decode_header_block(&header_fragments)?;
                    for (k, v) in decoded {
                        if k == ":status" {
                            if let Ok(code) = v.parse::<u16>() {
                                status_code = code;
                                status_text = match code {
                                    200 => "OK",
                                    201 => "Created",
                                    204 => "No Content",
                                    301 => "Moved Permanently",
                                    302 => "Found",
                                    303 => "See Other",
                                    304 => "Not Modified",
                                    307 => "Temporary Redirect",
                                    308 => "Permanent Redirect",
                                    400 => "Bad Request",
                                    401 => "Unauthorized",
                                    403 => "Forbidden",
                                    404 => "Not Found",
                                    500 => "Internal Server Error",
                                    502 => "Bad Gateway",
                                    503 => "Service Unavailable",
                                    _ => "HTTP/2 Response",
                                }
                                .to_string();
                            }
                        } else if !k.starts_with(':') {
                            response_headers.set(&k, &v);
                        }
                    }
                    headers_received = true;
                    header_fragments.clear();
                    awaiting_continuation = false;
                } else {
                    awaiting_continuation = true;
                }

                if (frame.flags & FLAG_END_STREAM) != 0 {
                    stream_ended = true;
                }
            }
            FRAME_CONTINUATION if frame.stream_id == 1 && awaiting_continuation => {
                header_fragments.extend_from_slice(&frame.payload);
                if (frame.flags & FLAG_END_HEADERS) != 0 {
                    let decoded = hpack_decoder.decode_header_block(&header_fragments)?;
                    for (k, v) in decoded {
                        if k == ":status" {
                            if let Ok(code) = v.parse::<u16>() {
                                status_code = code;
                            }
                        } else if !k.starts_with(':') {
                            response_headers.set(&k, &v);
                        }
                    }
                    headers_received = true;
                    header_fragments.clear();
                    awaiting_continuation = false;
                }
                if (frame.flags & FLAG_END_STREAM) != 0 {
                    stream_ended = true;
                }
            }
            FRAME_DATA if frame.stream_id == 1 => {
                let mut data_payload = &frame.payload[..];
                if (frame.flags & FLAG_PADDED) != 0 {
                    if data_payload.is_empty() {
                        return Err(FetchError::NetworkError(
                            "PROTOCOL_ERROR: Padded DATA frame missing pad length byte".into(),
                        ));
                    }
                    let pad_len = data_payload[0] as usize;
                    data_payload = &data_payload[1..];
                    if data_payload.len() < pad_len {
                        return Err(FetchError::NetworkError(
                            "PROTOCOL_ERROR: DATA frame padding length exceeds or equals payload length".into(),
                        ));
                    }
                    data_payload = &data_payload[..data_payload.len() - pad_len];
                }
                raw_body.extend_from_slice(data_payload);

                if (frame.flags & FLAG_END_STREAM) != 0 {
                    stream_ended = true;
                } else if !data_payload.is_empty() {
                    // Send WINDOW_UPDATE to maintain stream and connection flow control while stream is active
                    let delta = data_payload.len() as u32;
                    let win_conn = Http2Frame::new(FRAME_WINDOW_UPDATE, 0, 0, delta.to_be_bytes().to_vec());
                    let _ = stream.write_all(&win_conn.encode());
                    let win_stream = Http2Frame::new(FRAME_WINDOW_UPDATE, 0, 1, delta.to_be_bytes().to_vec());
                    let _ = stream.write_all(&win_stream.encode());
                    let _ = stream.flush();
                }
            }
            FRAME_RST_STREAM if frame.stream_id == 1 => {
                return Err(FetchError::NetworkError(format!(
                    "HTTP/2 stream 1 reset by peer (error code {:?})",
                    frame.payload
                )));
            }
            FRAME_GOAWAY => {
                if !headers_received {
                    return Err(FetchError::NetworkError(
                        "Received GOAWAY before HTTP/2 headers".into(),
                    ));
                }
                stream_ended = true;
            }
            _ => {
                // Ignore other stream IDs or unknown extension frames
            }
        }
    }

    // 5. Decompress Payload based on Content-Encoding
    let body = if let Some(encoding) = response_headers.get("content-encoding") {
        decompress_body(&raw_body, encoding)?
    } else {
        raw_body
    };

    Ok(Response::new(
        status_code,
        &status_text,
        response_headers,
        body,
    ))
}
