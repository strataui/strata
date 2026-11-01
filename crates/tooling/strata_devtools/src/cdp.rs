//! Remote WebSocket Chrome DevTools Protocol (CDP) Bridge and RFC 6455 Framing.

/// RFC 6455 WebSocket Frame Opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSocketOpcode {
    Continuation = 0x00,
    Text = 0x01,
    Binary = 0x02,
    Close = 0x08,
    Ping = 0x09,
    Pong = 0x0A,
}

impl WebSocketOpcode {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b & 0x0F {
            0x00 => Some(Self::Continuation),
            0x01 => Some(Self::Text),
            0x02 => Some(Self::Binary),
            0x08 => Some(Self::Close),
            0x09 => Some(Self::Ping),
            0x0A => Some(Self::Pong),
            _ => None,
        }
    }
}

/// Decoded RFC 6455 WebSocket frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketFrame {
    pub fin: bool,
    pub opcode: WebSocketOpcode,
    pub payload: Vec<u8>,
}

/// Encoder & Decoder for RFC 6455 WebSocket communication.
pub struct WebSocketCodec;

impl WebSocketCodec {
    /// Encodes a server-to-client unmasked WebSocket frame.
    pub fn encode_frame(opcode: WebSocketOpcode, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(payload.len() + 10);
        let first_byte = 0x80 | (opcode as u8); // FIN bit set
        out.push(first_byte);

        let len = payload.len();
        if len <= 125 {
            out.push(len as u8); // Unmasked (Mask bit = 0)
        } else if len <= 65535 {
            out.push(126);
            out.push((len >> 8) as u8);
            out.push((len & 0xFF) as u8);
        } else {
            out.push(127);
            for i in (0..8).rev() {
                out.push(((len as u64) >> (i * 8)) as u8);
            }
        }

        out.extend_from_slice(payload);
        out
    }

    /// Decodes a client-to-server (potentially masked) or server frame.
    /// Returns the decoded frame and total bytes consumed from the buffer.
    pub fn decode_frame(buf: &[u8]) -> Result<(WebSocketFrame, usize), &'static str> {
        if buf.len() < 2 {
            return Err("Incomplete frame header");
        }

        let first = buf[0];
        let second = buf[1];

        let fin = (first & 0x80) != 0;
        let opcode = WebSocketOpcode::from_u8(first).ok_or("Invalid opcode")?;
        let masked = (second & 0x80) != 0;
        let mut payload_len = (second & 0x7F) as usize;

        let mut offset = 2;
        if payload_len == 126 {
            if buf.len() < offset + 2 {
                return Err("Incomplete 16-bit payload length");
            }
            payload_len = ((buf[offset] as usize) << 8) | (buf[offset + 1] as usize);
            offset += 2;
        } else if payload_len == 127 {
            if buf.len() < offset + 8 {
                return Err("Incomplete 64-bit payload length");
            }
            let mut l: u64 = 0;
            for i in 0..8 {
                l = (l << 8) | (buf[offset + i] as u64);
            }
            payload_len = l as usize;
            offset += 8;
        }

        let mask_key = if masked {
            if buf.len() < offset + 4 {
                return Err("Incomplete masking key");
            }
            let key = [buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]];
            offset += 4;
            Some(key)
        } else {
            None
        };

        if buf.len() < offset + payload_len {
            return Err("Incomplete frame payload");
        }

        let mut payload = buf[offset..offset + payload_len].to_vec();
        if let Some(key) = mask_key {
            for i in 0..payload.len() {
                payload[i] ^= key[i % 4];
            }
        }

        let consumed = offset + payload_len;
        Ok((
            WebSocketFrame {
                fin,
                opcode,
                payload,
            },
            consumed,
        ))
    }
}

/// Incoming Chrome DevTools Protocol JSON-RPC request.
#[derive(Debug, Clone, PartialEq)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    pub params: String,
}

impl CdpRequest {
    /// Zero-dependency parser for standard CDP JSON-RPC requests: `{"id": 1, "method": "DOM.getDocument", ...}`.
    pub fn parse(raw_json: &str) -> Result<Self, &'static str> {
        let trimmed = raw_json.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return Err("Invalid JSON object");
        }

        let id = Self::extract_u64_field(trimmed, "\"id\"").unwrap_or(0);
        let method = Self::extract_string_field(trimmed, "\"method\"").unwrap_or_default();

        Ok(Self {
            id,
            method,
            params: trimmed.to_string(),
        })
    }

    fn extract_string_field(json: &str, field_key: &str) -> Option<String> {
        let key_idx = json.find(field_key)?;
        let rest = &json[key_idx + field_key.len()..];
        let colon_idx = rest.find(':')?;
        let val_rest = rest[colon_idx + 1..].trim_start();
        if !val_rest.starts_with('"') {
            return None;
        }
        let end_quote = val_rest[1..].find('"')?;
        Some(val_rest[1..1 + end_quote].to_string())
    }

    fn extract_u64_field(json: &str, field_key: &str) -> Option<u64> {
        let key_idx = json.find(field_key)?;
        let rest = &json[key_idx + field_key.len()..];
        let colon_idx = rest.find(':')?;
        let val_rest = rest[colon_idx + 1..].trim_start();
        let mut digits = String::new();
        for ch in val_rest.chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
            } else if !digits.is_empty() {
                break;
            }
        }
        digits.parse::<u64>().ok()
    }
}

/// Outgoing Chrome DevTools Protocol JSON-RPC response.
#[derive(Debug, Clone, PartialEq)]
pub struct CdpResponse {
    pub id: u64,
    pub result: Option<String>,
    pub error: Option<String>,
}

impl CdpResponse {
    pub fn success(id: u64, result_json: impl Into<String>) -> Self {
        Self {
            id,
            result: Some(result_json.into()),
            error: None,
        }
    }

    pub fn error(id: u64, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(message.into()),
        }
    }

    /// Serializes response to standard JSON.
    pub fn to_json(&self) -> String {
        if let Some(err) = &self.error {
            format!(
                r#"{{"id":{},"error":{{"code":-32601,"message":"{}"}}}}"#,
                self.id, err
            )
        } else {
            let res = self.result.as_deref().unwrap_or("{}");
            format!(r#"{{"id":{},"result":{}}}"#, self.id, res)
        }
    }
}

/// Chrome DevTools Protocol Domain Dispatcher.
#[derive(Debug, Clone, Default)]
pub struct CdpDispatcher;

impl CdpDispatcher {
    pub fn new() -> Self {
        Self
    }

    /// Dispatches incoming CDP requests to supported domains (`DOM`, `CSS`, `Performance`, `Runtime`).
    pub fn handle_request(&self, request: &CdpRequest) -> CdpResponse {
        match request.method.as_str() {
            "DOM.getDocument" => CdpResponse::success(
                request.id,
                r##"{"root":{"nodeId":1,"nodeType":9,"nodeName":"#document","children":[{"nodeId":2,"nodeType":1,"nodeName":"strata-root","childNodeCount":0}]}}"##,
            ),
            "DOM.highlightNode" => CdpResponse::success(request.id, r#"{"highlighted":true}"#),
            "CSS.getComputedStyleForNode" => CdpResponse::success(
                request.id,
                r#"{"computedStyle":[{"name":"display","value":"flex"},{"name":"box-sizing","value":"border-box"}]}"#,
            ),
            "Performance.getMetrics" => CdpResponse::success(
                request.id,
                r#"{"metrics":[{"name":"LayoutCount","value":1},{"name":"RecalcStyleCount","value":1}]}"#,
            ),
            "Runtime.evaluate" => CdpResponse::success(
                request.id,
                r#"{"result":{"type":"string","value":"Strata Reactive Engine v0.1.0"}}"#,
            ),
            other => CdpResponse::error(request.id, format!("Method '{}' not implemented", other)),
        }
    }

    /// Handles a raw WebSocket frame containing a CDP JSON-RPC text message and returns the response frame.
    pub fn handle_ws_frame(&self, frame: &WebSocketFrame) -> Option<WebSocketFrame> {
        if frame.opcode != WebSocketOpcode::Text {
            return None;
        }

        let text = std::str::from_utf8(&frame.payload).ok()?;
        let req = CdpRequest::parse(text).ok()?;
        let resp = self.handle_request(&req);
        let resp_json = resp.to_json();

        Some(WebSocketFrame {
            fin: true,
            opcode: WebSocketOpcode::Text,
            payload: resp_json.into_bytes(),
        })
    }
}
