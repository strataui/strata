//! Pure-Rust W3C fetch() Engine with Headers, AbortController, and HTTP Framing.

use std::collections::HashMap;
use std::fmt;
use std::io::{Cursor, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::tls::HttpTransportStream;

/// Errors occurring during HTTP fetch requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    InvalidUrl(String),
    NetworkError(String),
    Timeout(String),
    Aborted,
    IoError(String),
    SerializationError(String),
    CorsError(String),
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
            Self::CorsError(s) => write!(f, "CORS error: {s}"),
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

/// Case-insensitive HTTP Header map conforming to W3C Fetch spec.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Headers {
    map: HashMap<String, String>,
    all: HashMap<String, Vec<String>>,
}

impl Headers {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            all: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        let lower = key.to_ascii_lowercase();
        self.map.insert(lower.clone(), value.to_string());
        self.all.insert(lower, vec![value.to_string()]);
    }

    pub fn append(&mut self, key: &str, value: &str) {
        let lower = key.to_ascii_lowercase();
        if lower == "set-cookie" {
            // Per RFC 6265 §3, Set-Cookie headers must NOT be joined with comma
            self.map.insert(lower.clone(), value.to_string());
        } else if let Some(existing) = self.map.get_mut(&lower) {
            existing.push_str(", ");
            existing.push_str(value);
        } else {
            self.map.insert(lower.clone(), value.to_string());
        }
        self.all.entry(lower).or_default().push(value.to_string());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(&key.to_ascii_lowercase()).map(|s| s.as_str())
    }

    pub fn get_all(&self, key: &str) -> Vec<&str> {
        self.all
            .get(&key.to_ascii_lowercase())
            .map(|vec| vec.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    pub fn has(&self, key: &str) -> bool {
        self.map.contains_key(&key.to_ascii_lowercase())
    }

    pub fn delete(&mut self, key: &str) {
        let lower = key.to_ascii_lowercase();
        self.map.remove(&lower);
        self.all.remove(&lower);
    }

    pub fn entries(&self) -> Vec<(&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }

    pub fn all_entries(&self) -> Vec<(&str, &str)> {
        let mut out = Vec::new();
        for (k, vals) in &self.all {
            for v in vals {
                out.push((k.as_str(), v.as_str()));
            }
        }
        out
    }
}

/// W3C Fetch request mode governing Cross-Origin Resource Sharing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestMode {
    #[default]
    Cors,
    NoCors,
    SameOrigin,
}

/// W3C Fetch credentials mode governing cookie inclusion in cross-origin requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestCredentials {
    #[default]
    SameOrigin,
    Include,
    Omit,
}

/// Configuration options for an HTTP request.
#[derive(Clone)]
pub struct RequestInit {
    pub method: String,
    pub headers: Headers,
    pub body: Option<Vec<u8>>,
    pub timeout_ms: Option<u64>,
    pub signal: Option<AbortSignal>,
    pub cookie_jar: Option<Arc<std::sync::Mutex<crate::cookie::CookieJar>>>,
    pub mode: RequestMode,
    pub credentials: RequestCredentials,
    pub origin: Option<String>,
}

impl Default for RequestInit {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            headers: Headers::new(),
            body: None,
            timeout_ms: Some(15000),
            signal: None,
            cookie_jar: None,
            mode: RequestMode::Cors,
            credentials: RequestCredentials::SameOrigin,
            origin: None,
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

    pub fn cookie_jar(mut self, jar: Arc<std::sync::Mutex<crate::cookie::CookieJar>>) -> Self {
        self.cookie_jar = Some(jar);
        self
    }

    pub fn mode(mut self, mode: RequestMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn credentials(mut self, creds: RequestCredentials) -> Self {
        self.credentials = creds;
        self
    }

    pub fn origin(mut self, origin: impl Into<String>) -> Self {
        self.origin = Some(origin.into());
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

        // Under RFC 3986, the authority segment terminates at the first '/', '?', or '#' (or end of string).
        let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (host_port, path_query_frag) = rest.split_at(authority_end);

        // Strip fragment (#...) from HTTP request path_and_query
        let path_and_query_raw = match path_query_frag.split_once('#') {
            Some((pq, _frag)) => pq,
            None => path_query_frag,
        };

        let path = if path_and_query_raw.is_empty() {
            "/".to_string()
        } else if path_and_query_raw.starts_with('?') {
            format!("/{path_and_query_raw}")
        } else {
            path_and_query_raw.to_string()
        };

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

/// Resolves a redirect location header against the base URL.
pub fn resolve_redirect_url(base: &str, location: &str) -> Result<String, FetchError> {
    let loc_trimmed = location.trim();
    if loc_trimmed.starts_with("http://") || loc_trimmed.starts_with("https://") {
        return Ok(loc_trimmed.to_string());
    }

    let parsed_base = ParsedUrl::parse(base)?;
    let host_part = if (parsed_base.scheme == "http" && parsed_base.port == 80)
        || (parsed_base.scheme == "https" && parsed_base.port == 443)
    {
        parsed_base.host
    } else {
        format!("{}:{}", parsed_base.host, parsed_base.port)
    };

    if let Some(stripped) = loc_trimmed.strip_prefix("//") {
        return Ok(format!("{}://{stripped}", parsed_base.scheme));
    }

    if loc_trimmed.starts_with('/') {
        return Ok(format!("{}://{}{}", parsed_base.scheme, host_part, loc_trimmed));
    }

    let base_dir = match parsed_base.path_and_query.rfind('/') {
        Some(idx) => &parsed_base.path_and_query[..=idx],
        None => "/",
    };
    Ok(format!("{}://{}{}{}", parsed_base.scheme, host_part, base_dir, loc_trimmed))
}

/// Computes the serialized origin (scheme + host + port) for an HTTP/HTTPS URL per RFC 6454 / W3C Fetch.
pub fn compute_origin(url: &str) -> Result<String, FetchError> {
    let parsed = ParsedUrl::parse(url)?;
    let is_default_port = (parsed.scheme == "http" && parsed.port == 80)
        || (parsed.scheme == "https" && parsed.port == 443);
    if is_default_port {
        Ok(format!("{}://{}", parsed.scheme, parsed.host))
    } else {
        Ok(format!("{}://{}:{}", parsed.scheme, parsed.host, parsed.port))
    }
}

/// Checks whether an HTTP request qualifies as a CORS-simple request under W3C Fetch spec.
/// Returns (is_simple, list_of_custom_headers).
pub fn is_cors_simple_request(req: &RequestInit) -> (bool, Vec<String>) {
    let method = req.method.to_uppercase();
    let method_simple = matches!(method.as_str(), "GET" | "HEAD" | "POST");
    let mut custom_headers = Vec::new();

    for (k, v) in req.headers.entries() {
        let key_lower = k.to_ascii_lowercase();
        match key_lower.as_str() {
            "accept" | "accept-language" | "content-language" | "user-agent" | "host"
            | "connection" | "accept-encoding" | "origin" | "content-length" => {}
            "content-type" => {
                let mime = v.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
                if !matches!(
                    mime.as_str(),
                    "application/x-www-form-urlencoded" | "multipart/form-data" | "text/plain"
                ) {
                    custom_headers.push(key_lower);
                }
            }
            _ => {
                custom_headers.push(key_lower);
            }
        }
    }

    let is_simple = method_simple && custom_headers.is_empty();
    (is_simple, custom_headers)
}

/// Executes an HTTP or HTTPS fetch request, following standard HTTP redirects and enforcing W3C CORS rules.
pub fn fetch(url: &str, init: Option<RequestInit>) -> Result<Response, FetchError> {
    let req = init.unwrap_or_default();
    let max_redirects = 5;
    let mut current_url = url.to_string();
    let mut current_req = req;

    let client_origin = current_req
        .origin
        .clone()
        .or_else(|| current_req.headers.get("origin").map(|s| s.to_string()));

    let client_origin_norm = match &client_origin {
        Some(raw) => Some(compute_origin(raw).unwrap_or_else(|_| raw.trim().to_string())),
        None => None,
    };

    for _ in 0..max_redirects {
        let target_origin = compute_origin(&current_url)?;
        let is_same_origin = match &client_origin_norm {
            Some(orig) => orig.eq_ignore_ascii_case(&target_origin),
            None => true,
        };

        if !is_same_origin {
            let origin_str = client_origin_norm.as_deref().unwrap_or("");

            if current_req.mode == RequestMode::SameOrigin {
                return Err(FetchError::CorsError(format!(
                    "Cross-origin request to '{target_origin}' blocked by SameOrigin policy (origin: '{origin_str}')"
                )));
            }

            // In NoCors mode, only simple methods and headers are permitted
            let (is_simple, custom_headers) = is_cors_simple_request(&current_req);
            if current_req.mode == RequestMode::NoCors && !is_simple {
                return Err(FetchError::CorsError(
                    "Invalid request in 'no-cors' mode: method or headers require CORS preflight".into(),
                ));
            }

            // Omit or SameOrigin credentials mode: strip cookie jar on cross-origin requests
            if current_req.credentials != RequestCredentials::Include {
                current_req.cookie_jar = None;
                current_req.headers.delete("cookie");
            }

            // Under Cors mode, execute preflight OPTIONS check if request is not simple
            if current_req.mode == RequestMode::Cors && !is_simple {
                let mut preflight_headers = Headers::new();
                preflight_headers.set("Origin", origin_str);
                preflight_headers.set("Access-Control-Request-Method", &current_req.method.to_uppercase());
                if !custom_headers.is_empty() {
                    preflight_headers.set("Access-Control-Request-Headers", &custom_headers.join(", "));
                }

                let preflight_req = RequestInit {
                    method: "OPTIONS".to_string(),
                    headers: preflight_headers,
                    body: None,
                    timeout_ms: current_req.timeout_ms,
                    signal: current_req.signal.clone(),
                    cookie_jar: None,
                    mode: RequestMode::Cors,
                    credentials: RequestCredentials::Omit,
                    origin: Some(origin_str.to_string()),
                };

                let preflight_resp = fetch_single(&current_url, &preflight_req)?;

                if !(200..=299).contains(&preflight_resp.status) && preflight_resp.status != 204 {
                    return Err(FetchError::CorsError(format!(
                        "CORS preflight failed with HTTP status {}",
                        preflight_resp.status
                    )));
                }

                let allow_origin = preflight_resp
                    .headers
                    .get("access-control-allow-origin")
                    .map(|s| s.trim())
                    .ok_or_else(|| {
                        FetchError::CorsError(
                            "CORS preflight failed: missing 'Access-Control-Allow-Origin' header".into(),
                        )
                    })?;

                if allow_origin == "*" {
                    if current_req.credentials == RequestCredentials::Include {
                        return Err(FetchError::CorsError(
                            "CORS preflight failed: 'Access-Control-Allow-Origin' cannot be wildcard '*' when credentials mode is 'Include'".into(),
                        ));
                    }
                } else if !allow_origin.eq_ignore_ascii_case(origin_str) {
                    return Err(FetchError::CorsError(format!(
                        "CORS preflight failed: 'Access-Control-Allow-Origin' ('{allow_origin}') does not match origin '{origin_str}'"
                    )));
                }

                if current_req.credentials == RequestCredentials::Include {
                    let allow_creds = preflight_resp
                        .headers
                        .get("access-control-allow-credentials")
                        .map(|s| s.trim());
                    if allow_creds != Some("true") {
                        return Err(FetchError::CorsError(
                            "CORS preflight failed: 'Access-Control-Allow-Credentials' must be 'true'".into(),
                        ));
                    }
                }

                let allow_methods = preflight_resp
                    .headers
                    .get("access-control-allow-methods")
                    .unwrap_or("");
                let methods_list: Vec<&str> = allow_methods.split(',').map(|s| s.trim()).collect();
                let req_method = current_req.method.to_uppercase();
                if !methods_list.iter().any(|&m| m == "*" || m.eq_ignore_ascii_case(&req_method)) {
                    return Err(FetchError::CorsError(format!(
                        "CORS preflight failed: method '{req_method}' not permitted by Access-Control-Allow-Methods ('{allow_methods}')"
                    )));
                }

                if !custom_headers.is_empty() {
                    let allow_headers = preflight_resp
                        .headers
                        .get("access-control-allow-headers")
                        .unwrap_or("");
                    let headers_list: Vec<&str> = allow_headers.split(',').map(|s| s.trim()).collect();
                    let wildcard = headers_list.iter().any(|&h| h == "*");
                    if !wildcard {
                        for header in &custom_headers {
                            if !headers_list.iter().any(|&h| h.eq_ignore_ascii_case(header)) {
                                return Err(FetchError::CorsError(format!(
                                    "CORS preflight failed: header '{header}' not permitted by Access-Control-Allow-Headers ('{allow_headers}')"
                                )));
                            }
                        }
                    }
                }
            }

            // Set Origin header on actual request
            current_req.headers.set("Origin", origin_str);
        }

        let resp = fetch_single(&current_url, &current_req)?;

        // If cross-origin in Cors mode, validate response CORS headers
        if !is_same_origin && current_req.mode == RequestMode::Cors {
            let origin_str = client_origin_norm.as_deref().unwrap_or("");
            let allow_origin = resp
                .headers
                .get("access-control-allow-origin")
                .map(|s| s.trim())
                .ok_or_else(|| {
                    FetchError::CorsError(format!(
                        "CORS policy violation: missing 'Access-Control-Allow-Origin' header on response from {current_url}"
                    ))
                })?;

            if allow_origin == "*" {
                if current_req.credentials == RequestCredentials::Include {
                    return Err(FetchError::CorsError(
                        "CORS policy violation: 'Access-Control-Allow-Origin' cannot be wildcard '*' when credentials mode is 'Include'".into(),
                    ));
                }
            } else if !allow_origin.eq_ignore_ascii_case(origin_str) {
                return Err(FetchError::CorsError(format!(
                    "CORS policy violation: Access-Control-Allow-Origin '{allow_origin}' does not match request origin '{origin_str}'"
                )));
            }

            if current_req.credentials == RequestCredentials::Include {
                let allow_creds = resp
                    .headers
                    .get("access-control-allow-credentials")
                    .map(|s| s.trim());
                if allow_creds != Some("true") {
                    return Err(FetchError::CorsError(
                        "CORS policy violation: 'Access-Control-Allow-Credentials' must be 'true' for credentialed requests".into(),
                    ));
                }
            }
        }

        if (301..=303).contains(&resp.status) || resp.status == 307 || resp.status == 308 {
            if let Some(loc) = resp.headers.get("location") {
                let next_url = resolve_redirect_url(&current_url, loc)?;
                current_url = next_url;
                if resp.status == 303 || resp.status == 301 || resp.status == 302 {
                    current_req.method = "GET".to_string();
                    current_req.body = None;
                }
                continue;
            }
        }

        return Ok(resp);
    }

    Err(FetchError::NetworkError("Too many redirects (exceeded 5 hops)".into()))
}

/// Executes a single HTTP or HTTPS fetch request without following redirects.
pub fn fetch_single(url: &str, req: &RequestInit) -> Result<Response, FetchError> {
    // Check cancellation
    if let Some(ref sig) = req.signal {
        if sig.is_aborted() {
            return Err(FetchError::Aborted);
        }
    }

    let parsed = ParsedUrl::parse(url)?;
    let timeout = Duration::from_millis(req.timeout_ms.unwrap_or(15000));

    // Resolve DNS host and establish TCP connection
    let addrs: Vec<std::net::SocketAddr> = match (parsed.host.as_str(), parsed.port).to_socket_addrs() {
        Ok(iter) => iter.collect(),
        Err(e) => {
            return Err(FetchError::NetworkError(format!(
                "DNS resolution failed for {}: {e}",
                parsed.host
            )));
        }
    };

    if addrs.is_empty() {
        return Err(FetchError::NetworkError(format!(
            "No IP addresses resolved for host {}",
            parsed.host
        )));
    }

    let mut tcp_stream_opt = None;
    let mut last_err = None;

    for addr in &addrs {
        match TcpStream::connect_timeout(addr, timeout) {
            Ok(s) => {
                tcp_stream_opt = Some(s);
                break;
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    let tcp_stream = tcp_stream_opt.ok_or_else(|| {
        FetchError::NetworkError(format!(
            "Connection to {}:{} failed: {}",
            parsed.host,
            parsed.port,
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Network unreachable".into())
        ))
    })?;

    tcp_stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| FetchError::IoError(e.to_string()))?;
    tcp_stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| FetchError::IoError(e.to_string()))?;

    // Transparently multiplex between Plain TCP and TLS 1.2/1.3
    let mut transport: HttpTransportStream = if parsed.scheme == "https" {
        crate::tls::create_tls_stream(tcp_stream, &parsed.host)
            .map_err(FetchError::NetworkError)?
    } else {
        HttpTransportStream::Plain(tcp_stream)
    };

    let mut req_effective = req.clone();
    if let Some(ref jar) = req_effective.cookie_jar {
        if !req_effective.headers.has("cookie") {
            if let Ok(mut jar_guard) = jar.lock() {
                if let Some(cookie_val) = jar_guard.get_cookie_header_value(url) {
                    req_effective.headers.set("Cookie", &cookie_val);
                }
            }
        }
    }

    // If ALPN negotiated HTTP/2 ("h2"), dispatch to pure-Rust HTTP/2 client engine
    if transport.alpn_protocol() == Some(b"h2") {
        let resp = crate::http2::execute_http2_request(&mut transport, &req_effective, &parsed)?;
        if let Some(ref jar) = req.cookie_jar {
            if let Ok(mut jar_guard) = jar.lock() {
                jar_guard.process_response_headers(&resp.headers, url);
            }
        }
        return Ok(resp);
    }

    // Build HTTP/1.1 request
    let mut request_bytes = Vec::new();
    let method = req_effective.method.to_uppercase();
    request_bytes.extend_from_slice(
        format!("{method} {} HTTP/1.1\r\n", parsed.path_and_query).as_bytes(),
    );
    request_bytes.extend_from_slice(format!("Host: {}\r\n", parsed.host).as_bytes());
    request_bytes.extend_from_slice(b"Connection: close\r\n");

    let body_len = req_effective.body.as_ref().map(|b| b.len()).unwrap_or(0);
    if body_len > 0 {
        request_bytes.extend_from_slice(format!("Content-Length: {body_len}\r\n").as_bytes());
    }

    if !req_effective.headers.has("Accept-Encoding") {
        request_bytes.extend_from_slice(b"Accept-Encoding: gzip, deflate, br\r\n");
    }

    if !req_effective.headers.has("User-Agent") && !req_effective.headers.has("user-agent") {
        request_bytes.extend_from_slice(b"User-Agent: Mozilla/5.0 (compatible; Onyx/1.0; +https://strataui.org)\r\n");
    }

    for (k, v) in req_effective.headers.entries() {
        request_bytes.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    request_bytes.extend_from_slice(b"\r\n");

    if let Some(ref body) = req_effective.body {
        request_bytes.extend_from_slice(body);
    }

    transport
        .write_all(&request_bytes)
        .map_err(|e| FetchError::NetworkError(e.to_string()))?;
    transport
        .flush()
        .map_err(|e| FetchError::NetworkError(e.to_string()))?;

    // Read response with RFC 7230 framing (terminates on Content-Length or chunked without waiting for EOF)
    let resp = read_http_response(&mut transport)?;
    if let Some(ref jar) = req.cookie_jar {
        if let Ok(mut jar_guard) = jar.lock() {
            jar_guard.process_response_headers(&resp.headers, url);
        }
    }
    Ok(resp)
}

/// Decodes HTTP/1.1 chunked transfer encoded payloads.
pub fn decode_chunked_body(raw_body: &[u8]) -> Result<Vec<u8>, FetchError> {
    let mut decoded = Vec::new();
    let mut cursor = 0;

    while cursor < raw_body.len() {
        let slice = &raw_body[cursor..];
        let Some(newline_pos) = slice.windows(2).position(|w| w == b"\r\n") else {
            break;
        };

        let size_str = std::str::from_utf8(&slice[..newline_pos])
            .map_err(|e| FetchError::NetworkError(format!("Invalid chunk size encoding: {e}")))?
            .trim();

        let size_hex = size_str.split(';').next().unwrap_or(size_str).trim();
        let chunk_size = usize::from_str_radix(size_hex, 16)
            .map_err(|e| FetchError::NetworkError(format!("Invalid hex chunk size '{size_hex}': {e}")))?;

        cursor += newline_pos + 2;

        if chunk_size == 0 {
            break;
        }

        if cursor + chunk_size > raw_body.len() {
            return Err(FetchError::NetworkError("Truncated chunked HTTP payload".into()));
        }

        decoded.extend_from_slice(&raw_body[cursor..cursor + chunk_size]);
        cursor += chunk_size;

        if cursor + 2 <= raw_body.len() && &raw_body[cursor..cursor + 2] == b"\r\n" {
            cursor += 2;
        }
    }

    Ok(decoded)
}

/// Maximum allowable HTTP header size in bytes (RFC 7230 / Slowloris DoS prevention).
pub const MAX_HEADER_SIZE: usize = 64 * 1024; // 64 KB

/// Maximum allowable HTTP response body size in bytes (128 MB).
pub const MAX_RESPONSE_BODY_SIZE: usize = 128 * 1024 * 1024; // 128 MB

/// Maximum allowable decompressed payload size in bytes (Decompression bomb protection).
pub const MAX_DECOMPRESSED_SIZE: usize = 128 * 1024 * 1024; // 128 MB

/// Decompresses body bytes based on HTTP Content-Encoding (gzip, deflate, br) with
/// strict decompression bomb expansion limits.
pub fn decompress_body(raw_body: &[u8], encoding: &str) -> Result<Vec<u8>, FetchError> {
    let lower = encoding.trim().to_ascii_lowercase();
    match lower.as_str() {
        "gzip" => {
            let mut decoder = flate2::read::GzDecoder::new(raw_body);
            let mut decompressed = Vec::new();
            let mut limited = (&mut decoder).take((MAX_DECOMPRESSED_SIZE + 1) as u64);
            limited
                .read_to_end(&mut decompressed)
                .map_err(|e| FetchError::NetworkError(format!("Gzip decompression failed: {e}")))?;
            if decompressed.len() > MAX_DECOMPRESSED_SIZE {
                return Err(FetchError::NetworkError(format!(
                    "Decompressed payload exceeded maximum limit of {MAX_DECOMPRESSED_SIZE} bytes (decompression bomb protection)"
                )));
            }
            Ok(decompressed)
        }
        "deflate" => {
            let mut decoder = flate2::read::ZlibDecoder::new(raw_body);
            let mut decompressed = Vec::new();
            let mut limited = (&mut decoder).take((MAX_DECOMPRESSED_SIZE + 1) as u64);
            if limited.read_to_end(&mut decompressed).is_ok() {
                if decompressed.len() > MAX_DECOMPRESSED_SIZE {
                    return Err(FetchError::NetworkError(format!(
                        "Decompressed payload exceeded maximum limit of {MAX_DECOMPRESSED_SIZE} bytes (decompression bomb protection)"
                    )));
                }
                Ok(decompressed)
            } else {
                let mut raw_decoder = flate2::read::DeflateDecoder::new(raw_body);
                let mut raw_decompressed = Vec::new();
                let mut raw_limited = (&mut raw_decoder).take((MAX_DECOMPRESSED_SIZE + 1) as u64);
                raw_limited
                    .read_to_end(&mut raw_decompressed)
                    .map_err(|e| FetchError::NetworkError(format!("Deflate decompression failed: {e}")))?;
                if raw_decompressed.len() > MAX_DECOMPRESSED_SIZE {
                    return Err(FetchError::NetworkError(format!(
                        "Decompressed payload exceeded maximum limit of {MAX_DECOMPRESSED_SIZE} bytes (decompression bomb protection)"
                    )));
                }
                Ok(raw_decompressed)
            }
        }
        "br" | "brotli" => {
            let mut decompressor = brotli::Decompressor::new(raw_body, 4096);
            let mut decompressed = Vec::new();
            let mut limited = (&mut decompressor).take((MAX_DECOMPRESSED_SIZE + 1) as u64);
            limited
                .read_to_end(&mut decompressed)
                .map_err(|e| FetchError::NetworkError(format!("Brotli decompression failed: {e:?}")))?;
            if decompressed.len() > MAX_DECOMPRESSED_SIZE {
                return Err(FetchError::NetworkError(format!(
                    "Decompressed payload exceeded maximum limit of {MAX_DECOMPRESSED_SIZE} bytes (decompression bomb protection)"
                )));
            }
            Ok(decompressed)
        }
        "identity" | "" => Ok(raw_body.to_vec()),
        _ => Ok(raw_body.to_vec()),
    }
}

/// Reads an HTTP/1.1 response from a synchronous byte stream, enforcing RFC 7230 framing.
/// Terminates immediately upon reading `Content-Length` bytes or the terminal chunk `0\r\n\r\n`,
/// preventing hangs on persistent keep-alive connections.
pub fn read_http_response<R: Read>(reader: &mut R) -> Result<Response, FetchError> {
    let mut buffer = Vec::with_capacity(4096);
    let mut header_end = None;

    // 1. Read headers incrementally until \r\n\r\n with strict 64 KB limit
    let mut temp = [0u8; 1024];
    while header_end.is_none() {
        let n = reader
            .read(&mut temp)
            .map_err(|e| FetchError::NetworkError(e.to_string()))?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..n]);
        if buffer.len() > MAX_HEADER_SIZE {
            return Err(FetchError::NetworkError(format!(
                "HTTP header section exceeded {MAX_HEADER_SIZE} bytes limit (RFC 7230 DoS protection)"
            )));
        }
        header_end = buffer.windows(4).position(|w| w == b"\r\n\r\n");
    }

    let Some(header_end_pos) = header_end else {
        return Err(FetchError::NetworkError("Malformed HTTP response: no header delimiter".into()));
    };

    let header_str = String::from_utf8_lossy(&buffer[..header_end_pos]);
    let mut lines = header_str.split("\r\n");

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
            headers.append(k.trim(), v.trim());
        }
    }

    // Residual bytes already read into buffer after the \r\n\r\n delimiter
    let initial_body = buffer[header_end_pos + 4..].to_vec();

    // Responses to 1xx, 204 No Content, 304 Not Modified have no body per RFC 7230 §3.3.3
    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
        return Ok(Response::new(status_code, status_text, headers, Vec::new()));
    }

    let is_chunked = headers
        .get("transfer-encoding")
        .map(|v| v.eq_ignore_ascii_case("chunked"))
        .unwrap_or(false);

    let raw_body = if is_chunked {
        read_chunked_stream(reader, initial_body)?
    } else if let Some(cl_str) = headers.get("content-length") {
        let content_length = cl_str.trim().parse::<usize>().map_err(|e| {
            FetchError::NetworkError(format!("Invalid Content-Length '{cl_str}': {e}"))
        })?;
        read_fixed_length_body(reader, initial_body, content_length)?
    } else {
        // Delimited by connection closure (RFC 7230 Section 3.3.3 #7)
        let mut full_body = initial_body;
        let mut eof_buf = [0u8; 4096];
        loop {
            match reader.read(&mut eof_buf) {
                Ok(0) => break,
                Ok(n) => {
                    if full_body.len() + n > MAX_RESPONSE_BODY_SIZE {
                        return Err(FetchError::NetworkError(format!(
                            "Response body exceeded maximum allowable size ({MAX_RESPONSE_BODY_SIZE} bytes)"
                        )));
                    }
                    full_body.extend_from_slice(&eof_buf[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(FetchError::NetworkError(e.to_string())),
            }
        }
        full_body
    };

    let body = if let Some(encoding) = headers.get("content-encoding") {
        decompress_body(&raw_body, encoding)?
    } else {
        raw_body
    };

    Ok(Response::new(status_code, status_text, headers, body))
}

fn read_fixed_length_body<R: Read>(
    reader: &mut R,
    mut initial: Vec<u8>,
    content_length: usize,
) -> Result<Vec<u8>, FetchError> {
    if content_length > MAX_RESPONSE_BODY_SIZE {
        return Err(FetchError::NetworkError(format!(
            "Content-Length {content_length} exceeds maximum allowable body size ({MAX_RESPONSE_BODY_SIZE} bytes)"
        )));
    }
    if initial.len() >= content_length {
        initial.truncate(content_length);
        return Ok(initial);
    }

    let remaining = content_length - initial.len();
    let mut temp = [0u8; 16384];
    let mut bytes_read = 0;
    while bytes_read < remaining {
        let to_read = (remaining - bytes_read).min(temp.len());
        let n = reader
            .read(&mut temp[..to_read])
            .map_err(|e| FetchError::NetworkError(format!("Error reading body: {e}")))?;
        if n == 0 {
            return Err(FetchError::NetworkError(format!(
                "Truncated HTTP body: expected {content_length} bytes, received {}",
                initial.len() + bytes_read
            )));
        }
        initial.extend_from_slice(&temp[..n]);
        bytes_read += n;
    }
    Ok(initial)
}

fn read_chunked_stream<R: Read>(
    reader: &mut R,
    initial: Vec<u8>,
) -> Result<Vec<u8>, FetchError> {
    let mut decoded = Vec::new();
    let mut stream_buf = initial;
    let mut temp = [0u8; 4096];

    loop {
        let newline_pos = loop {
            if let Some(pos) = stream_buf.windows(2).position(|w| w == b"\r\n") {
                break pos;
            }
            let n = reader
                .read(&mut temp)
                .map_err(|e| FetchError::NetworkError(e.to_string()))?;
            if n == 0 {
                return Err(FetchError::NetworkError(
                    "Unexpected EOF while reading chunk size".into(),
                ));
            }
            stream_buf.extend_from_slice(&temp[..n]);
        };

        let size_str = std::str::from_utf8(&stream_buf[..newline_pos])
            .map_err(|e| FetchError::NetworkError(format!("Invalid chunk size UTF-8: {e}")))?
            .trim();
        let size_hex = size_str.split(';').next().unwrap_or(size_str).trim();
        let chunk_size = usize::from_str_radix(size_hex, 16).map_err(|e| {
            FetchError::NetworkError(format!("Invalid hex chunk size '{size_hex}': {e}"))
        })?;

        stream_buf.drain(..newline_pos + 2);

        if chunk_size == 0 {
            // Drain trailing CRLF / trailers
            while !stream_buf.starts_with(b"\r\n") {
                if stream_buf.is_empty() {
                    let n = reader
                        .read(&mut temp)
                        .map_err(|e| FetchError::NetworkError(e.to_string()))?;
                    if n == 0 {
                        break;
                    }
                    stream_buf.extend_from_slice(&temp[..n]);
                } else if let Some(end) = stream_buf.windows(2).position(|w| w == b"\r\n") {
                    stream_buf.drain(..end + 2);
                    break;
                } else {
                    break;
                }
            }
            if stream_buf.starts_with(b"\r\n") {
                stream_buf.drain(..2);
            }
            break;
        }

        if chunk_size > MAX_RESPONSE_BODY_SIZE || decoded.len() + chunk_size > MAX_RESPONSE_BODY_SIZE {
            return Err(FetchError::NetworkError(format!(
                "Chunked response body exceeded maximum allowable size ({MAX_RESPONSE_BODY_SIZE} bytes)"
            )));
        }

        let total_needed = chunk_size + 2;
        while stream_buf.len() < total_needed {
            let n = reader
                .read(&mut temp)
                .map_err(|e| FetchError::NetworkError(e.to_string()))?;
            if n == 0 {
                return Err(FetchError::NetworkError(
                    "Unexpected EOF while reading chunk payload".into(),
                ));
            }
            stream_buf.extend_from_slice(&temp[..n]);
        }

        decoded.extend_from_slice(&stream_buf[..chunk_size]);
        stream_buf.drain(..total_needed);
    }

    Ok(decoded)
}

/// Parses raw HTTP/1.1 response bytes into a structured `Response`.
pub fn parse_http_response(raw: &[u8]) -> Result<Response, FetchError> {
    let mut cursor = Cursor::new(raw);
    read_http_response(&mut cursor)
}
