//! Pure-Rust W3C fetch() Engine with Headers, AbortController, and HTTP Framing.

use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Errors occurring during HTTP fetch requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    InvalidUrl(String),
    NetworkError(String),
    Timeout(String),
    Aborted,
    IoError(String),
    SerializationError(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(s) => write!(f, "Invalid URL: {s}"),
            Self::NetworkError(s) => write!(f, "Network error: {s}"),
            Self::Timeout(s) => write!(f, "Request timed out: {s}"),
            Self::Aborted => write!(f, "Request was aborted by AbortController"),
            Self::IoError(s) => write!(f, "I/O error: {s}"),
            Self::SerializationError(s) => write!(f, "Serialization error: {s}"),
        }
    }
}

impl std::error::Error for FetchError {}

/// W3C AbortSignal signaling cancellation of asynchronous requests.
#[derive(Clone, Default)]
pub struct AbortSignal {
    aborted: Arc<AtomicBool>,
}

impl AbortSignal {
    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }
}

/// W3C AbortController used to cancel in-flight network requests.
#[derive(Default)]
pub struct AbortController {
    signal: AbortSignal,
}

impl AbortController {
    pub fn new() -> Self {
        Self {
            signal: AbortSignal::default(),
        }
    }

    pub fn signal(&self) -> AbortSignal {
        self.signal.clone()
    }

    pub fn abort(&self) {
        self.signal.aborted.store(true, Ordering::SeqCst);
    }
}

/// Case-insensitive HTTP Header map.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Headers {
    map: HashMap<String, String>,
}

impl Headers {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.map.insert(key.to_ascii_lowercase(), value.to_string());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(&key.to_ascii_lowercase()).map(|s| s.as_str())
    }

    pub fn has(&self, key: &str) -> bool {
        self.map.contains_key(&key.to_ascii_lowercase())
    }

    pub fn delete(&mut self, key: &str) {
        self.map.remove(&key.to_ascii_lowercase());
    }

    pub fn entries(&self) -> Vec<(&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }
}

/// Configuration options for an HTTP request.
#[derive(Clone)]
pub struct RequestInit {
    pub method: String,
    pub headers: Headers,
    pub body: Option<Vec<u8>>,
    pub timeout_ms: Option<u64>,
    pub signal: Option<AbortSignal>,
}

impl Default for RequestInit {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            headers: Headers::new(),
            body: None,
            timeout_ms: Some(15000),
            signal: None,
        }
    }
}

impl RequestInit {
    pub fn get() -> Self {
        Self {
            method: "GET".to_string(),
            ..Default::default()
        }
    }

    pub fn post() -> Self {
        Self {
            method: "POST".to_string(),
            ..Default::default()
        }
    }

    pub fn put() -> Self {
        Self {
            method: "PUT".to_string(),
            ..Default::default()
        }
    }

    pub fn delete() -> Self {
        Self {
            method: "DELETE".to_string(),
            ..Default::default()
        }
    }

    pub fn header(mut self, key: &str, val: &str) -> Self {
        self.headers.set(key, val);
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn body_json_str(mut self, json_str: &str) -> Self {
        self.headers.set("Content-Type", "application/json");
        self.body = Some(json_str.as_bytes().to_vec());
        self
    }

    pub fn signal(mut self, signal: AbortSignal) -> Self {
        self.signal = Some(signal);
        self
    }
}

/// HTTP Response representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub headers: Headers,
    body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, status_text: &str, headers: Headers, body: Vec<u8>) -> Self {
        Self {
            status,
            status_text: status_text.to_string(),
            headers,
            body,
        }
    }

    /// Whether HTTP status code is in the range 200..=299.
    pub fn ok(&self) -> bool {
        (200..=299).contains(&self.status)
    }

    /// Returns response body as UTF-8 string.
    pub fn text(&self) -> Result<String, FetchError> {
        String::from_utf8(self.body.clone()).map_err(|e| {
            FetchError::SerializationError(format!("Response body is not valid UTF-8: {e}"))
        })
    }

    /// Raw body bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }
}

/// Multipart form data builder.
#[derive(Debug, Clone, Default)]
pub struct FormData {
    fields: Vec<(String, String)>,
}

impl FormData {
    pub fn new() -> Self {
        Self { fields: Vec::new() }
    }

    pub fn append(&mut self, name: &str, value: &str) {
        self.fields.push((name.to_string(), value.to_string()));
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// Formats multipart form payload with boundary.
    pub fn to_multipart_payload(&self, boundary: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, val) in &self.fields {
            out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            out.extend_from_slice(format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes());
            out.extend_from_slice(val.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        out
    }
}

/// Parsed URL components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUrl {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
}

impl ParsedUrl {
    pub fn parse(url: &str) -> Result<Self, FetchError> {
        let (scheme, rest) = if let Some(stripped) = url.strip_prefix("http://") {
            ("http", stripped)
        } else if let Some(stripped) = url.strip_prefix("https://") {
            ("https", stripped)
        } else {
            return Err(FetchError::InvalidUrl(format!("Unsupported scheme: {url}")));
        };

        let mut parts = rest.splitn(2, '/');
        let host_port = parts.next().unwrap_or("");
        let path = parts.next().map(|p| format!("/{p}")).unwrap_or_else(|| "/".to_string());

        let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
            let parsed_port = p.parse::<u16>().map_err(|_| {
                FetchError::InvalidUrl(format!("Invalid port: {p}"))
            })?;
            (h.to_string(), parsed_port)
        } else {
            let default_port = if scheme == "https" { 443 } else { 80 };
            (host_port.to_string(), default_port)
        };

        if host.is_empty() {
            return Err(FetchError::InvalidUrl(format!("Missing host in URL: {url}")));
        }

        Ok(Self {
            scheme: scheme.to_string(),
            host,
            port,
            path_and_query: path,
        })
    }
}

/// Executes an HTTP fetch request.
pub fn fetch(url: &str, init: Option<RequestInit>) -> Result<Response, FetchError> {
    let req = init.unwrap_or_default();

    // Check cancellation
    if let Some(ref sig) = req.signal {
        if sig.is_aborted() {
            return Err(FetchError::Aborted);
        }
    }

    let parsed = ParsedUrl::parse(url)?;

    // Handle mock test addresses or direct loopback
    let addr = format!("{}:{}", parsed.host, parsed.port);
    let timeout = Duration::from_millis(req.timeout_ms.unwrap_or(15000));

    let mut stream = TcpStream::connect_timeout(
        &addr.parse().map_err(|e| FetchError::InvalidUrl(format!("{e}")))?,
        timeout,
    ).map_err(|e| FetchError::NetworkError(e.to_string()))?;

    stream.set_read_timeout(Some(timeout)).map_err(|e| FetchError::IoError(e.to_string()))?;
    stream.set_write_timeout(Some(timeout)).map_err(|e| FetchError::IoError(e.to_string()))?;

    // Build HTTP/1.1 request
    let mut request_bytes = Vec::new();
    let method = req.method.to_uppercase();
    request_bytes.extend_from_slice(format!("{method} {} HTTP/1.1\r\n", parsed.path_and_query).as_bytes());
    request_bytes.extend_from_slice(format!("Host: {}\r\n", parsed.host).as_bytes());
    request_bytes.extend_from_slice(b"Connection: close\r\n");

    let body_len = req.body.as_ref().map(|b| b.len()).unwrap_or(0);
    if body_len > 0 {
        request_bytes.extend_from_slice(format!("Content-Length: {body_len}\r\n").as_bytes());
    }

    for (k, v) in req.headers.entries() {
        request_bytes.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    request_bytes.extend_from_slice(b"\r\n");

    if let Some(ref body) = req.body {
        request_bytes.extend_from_slice(body);
    }

    stream.write_all(&request_bytes).map_err(|e| FetchError::NetworkError(e.to_string()))?;
    stream.flush().map_err(|e| FetchError::NetworkError(e.to_string()))?;

    // Read response
    let mut raw_response = Vec::new();
    stream.read_to_end(&mut raw_response).map_err(|e| FetchError::NetworkError(e.to_string()))?;

    parse_http_response(&raw_response)
}

/// Parses raw HTTP/1.1 response bytes into a structured `Response`.
pub fn parse_http_response(raw: &[u8]) -> Result<Response, FetchError> {
    let header_end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| FetchError::NetworkError("Malformed HTTP response: no header delimiter".into()))?;

    let header_str = String::from_utf8_lossy(&raw[..header_end]);
    let mut lines = header_str.split("\r\n");

    // Status Line: HTTP/1.1 200 OK
    let status_line = lines.next().ok_or_else(|| {
        FetchError::NetworkError("Missing HTTP status line".into())
    })?;

    let mut status_parts = status_line.splitn(3, ' ');
    let _version = status_parts.next();
    let status_code_str = status_parts.next().unwrap_or("0");
    let status_text = status_parts.next().unwrap_or("");

    let status_code = status_code_str.parse::<u16>().unwrap_or(0);

    let mut headers = Headers::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.set(k.trim(), v.trim());
        }
    }

    let body = raw[header_end + 4..].to_vec();

    Ok(Response::new(status_code, status_text, headers, body))
}
