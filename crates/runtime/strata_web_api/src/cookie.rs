//! RFC 6265 HTTP State Management Mechanism (Cookie Jar & Attributes).
//!
//! Provides:
//! - Full parsing of `Set-Cookie` headers with attributes:
//!   `Expires`, `Max-Age`, `Domain`, `Path`, `Secure`, `HttpOnly`, `SameSite`.
//! - Pure-Rust HTTP date parser (RFC 1123, RFC 850, asctime) with leap-year handling.
//! - RFC 6265 domain matching, path matching, and security domain boundaries.
//! - Thread-safe, serializable `CookieJar` with automatic cookie injection into requests
//!   and response extraction.

use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::fetch::{Headers, ParsedUrl};

/// Cookie SameSite policy attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SameSite {
    Strict,
    #[default]
    Lax,
    None,
}

/// An authentic RFC 6265 HTTP Cookie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<u64>,
    pub max_age: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
    pub host_only: bool,
    pub created_at: u64,
    pub last_accessed: u64,
}

impl Cookie {
    /// Constructs a basic cookie with default path "/" and current timestamp.
    pub fn new(name: impl Into<String>, value: impl Into<String>, domain: impl Into<String>) -> Self {
        let now = current_time_secs();
        Self {
            name: name.into(),
            value: value.into(),
            domain: domain.into().to_ascii_lowercase(),
            path: "/".to_string(),
            expires: None,
            max_age: None,
            secure: false,
            http_only: false,
            same_site: SameSite::Lax,
            host_only: true,
            created_at: now,
            last_accessed: now,
        }
    }

    /// Parses a `Set-Cookie` response header string in the context of an HTTP request URL.
    pub fn parse(header_value: &str, request_url: &str) -> Option<Self> {
        let parsed_url = ParsedUrl::parse(request_url).ok()?;
        Self::parse_with_context(header_value, &parsed_url, current_time_secs())
    }

    /// Parses a `Set-Cookie` string with an explicit timestamp and parsed request URL.
    pub fn parse_with_context(
        header_value: &str,
        parsed_url: &ParsedUrl,
        now: u64,
    ) -> Option<Self> {
        let mut parts = header_value.split(';');
        let name_value = parts.next()?.trim();
        let (name, value) = name_value.split_once('=')?;
        let name = name.trim().to_string();
        let value = value.trim().to_string();
        if name.is_empty() {
            return None;
        }

        let mut domain: Option<String> = None;
        let mut path: Option<String> = None;
        let mut expires: Option<u64> = None;
        let mut max_age: Option<i64> = None;
        let mut secure = false;
        let mut http_only = false;
        let mut same_site = SameSite::Lax;

        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((attr_name, attr_val)) = part.split_once('=') {
                let attr_name = attr_name.trim().to_ascii_lowercase();
                let attr_val = attr_val.trim();
                match attr_name.as_str() {
                    "domain" => {
                        let d = attr_val.trim_start_matches('.').to_ascii_lowercase();
                        if !d.is_empty() && can_set_cookie_domain(&parsed_url.host, &d) {
                            domain = Some(d);
                        }
                    }
                    "path" => {
                        if attr_val.starts_with('/') {
                            path = Some(attr_val.to_string());
                        }
                    }
                    "expires" => {
                        if let Some(exp_secs) = parse_http_date(attr_val) {
                            // Max-Age takes precedence over Expires if both specified (RFC 6265 §5.3)
                            if max_age.is_none() {
                                expires = Some(exp_secs);
                            }
                        }
                    }
                    "max-age" => {
                        if let Ok(delta) = attr_val.parse::<i64>() {
                            max_age = Some(delta);
                            let exp = if delta <= 0 {
                                0
                            } else {
                                now.saturating_add(delta as u64)
                            };
                            expires = Some(exp);
                        }
                    }
                    "samesite" => {
                        let lower = attr_val.to_ascii_lowercase();
                        if lower == "strict" {
                            same_site = SameSite::Strict;
                        } else if lower == "none" {
                            same_site = SameSite::None;
                        } else {
                            same_site = SameSite::Lax;
                        }
                    }
                    _ => {}
                }
            } else {
                let flag = part.to_ascii_lowercase();
                match flag.as_str() {
                    "secure" => secure = true,
                    "httponly" => http_only = true,
                    _ => {}
                }
            }
        }

        let (final_domain, host_only) = match domain {
            Some(d) => (d, false),
            None => (parsed_url.host.to_ascii_lowercase(), true),
        };

        let final_path = match path {
            Some(p) => p,
            None => default_path_for_request(&parsed_url.path_and_query),
        };

        Some(Self {
            name,
            value,
            domain: final_domain,
            path: final_path,
            expires,
            max_age,
            secure,
            http_only,
            same_site,
            host_only,
            created_at: now,
            last_accessed: now,
        })
    }

    /// Evaluates whether this cookie has expired relative to `now` timestamp in seconds.
    pub fn is_expired(&self, now: u64) -> bool {
        if let Some(exp) = self.expires {
            exp <= now
        } else {
            false
        }
    }

    /// Evaluates whether this cookie matches an outgoing request URL per RFC 6265 §5.4.
    pub fn matches_request(&self, parsed_url: &ParsedUrl, now: u64) -> bool {
        if self.is_expired(now) {
            return false;
        }
        if self.secure && parsed_url.scheme != "https" {
            return false;
        }
        if !domain_matches(&parsed_url.host, &self.domain, self.host_only) {
            return false;
        }
        if !path_matches(&parsed_url.path_and_query, &self.path) {
            return false;
        }
        true
    }
}

/// In-memory and persistent cookie storage conforming to RFC 6265.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CookieJar {
    cookies: Vec<Cookie>,
}

impl CookieJar {
    /// Creates a new empty `CookieJar`.
    pub fn new() -> Self {
        Self { cookies: Vec::new() }
    }

    /// Stores a cookie in the jar according to RFC 6265 §5.3 storage model.
    pub fn store(&mut self, cookie: Cookie) {
        let now = current_time_secs();
        if cookie.is_expired(now) {
            self.remove(&cookie.name, &cookie.domain, &cookie.path);
            return;
        }

        if let Some(pos) = self.cookies.iter().position(|c| {
            c.name.eq_ignore_ascii_case(&cookie.name)
                && c.domain.eq_ignore_ascii_case(&cookie.domain)
                && c.path == cookie.path
        }) {
            let mut updated = cookie;
            // Preserve original creation timestamp (RFC 6265 §5.3 step 11)
            updated.created_at = self.cookies[pos].created_at;
            self.cookies[pos] = updated;
        } else {
            self.cookies.push(cookie);
        }
    }

    /// Parses and stores a `Set-Cookie` header value in the context of an HTTP request URL.
    pub fn process_set_cookie(&mut self, set_cookie_str: &str, request_url: &str) {
        if let Some(cookie) = Cookie::parse(set_cookie_str, request_url) {
            self.store(cookie);
        }
    }

    /// Processes all `Set-Cookie` headers from an HTTP response.
    pub fn process_response_headers(&mut self, headers: &Headers, request_url: &str) {
        for set_cookie in headers.get_all("set-cookie") {
            self.process_set_cookie(set_cookie, request_url);
        }
    }

    /// Generates the `Cookie` request header string for an outgoing request URL.
    ///
    /// Cookies are ordered per RFC 6265 §5.4: longer paths first, then earlier creation timestamps.
    pub fn get_cookie_header_value(&mut self, request_url: &str) -> Option<String> {
        let parsed = ParsedUrl::parse(request_url).ok()?;
        let now = current_time_secs();

        // Evict expired cookies
        self.cookies.retain(|c| !c.is_expired(now));

        // Find matching cookies
        let mut matching: Vec<(usize, &mut Cookie)> = self
            .cookies
            .iter_mut()
            .enumerate()
            .filter(|(_, c)| c.matches_request(&parsed, now))
            .collect();

        if matching.is_empty() {
            return None;
        }

        // Sort by path length descending, then created_at ascending
        matching.sort_by(|(_, a), (_, b)| {
            b.path
                .len()
                .cmp(&a.path.len())
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        let mut pairs = Vec::with_capacity(matching.len());
        for (_, cookie) in matching {
            cookie.last_accessed = now;
            pairs.push(format!("{}={}", cookie.name, cookie.value));
        }

        Some(pairs.join("; "))
    }

    /// Retrieves a specific cookie by exact name, domain, and path.
    pub fn get(&self, name: &str, domain: &str, path: &str) -> Option<&Cookie> {
        self.cookies.iter().find(|c| {
            c.name.eq_ignore_ascii_case(name)
                && c.domain.eq_ignore_ascii_case(domain)
                && c.path == path
        })
    }

    /// Removes a specific cookie matching name, domain, and path.
    pub fn remove(&mut self, name: &str, domain: &str, path: &str) {
        self.cookies.retain(|c| {
            !(c.name.eq_ignore_ascii_case(name)
                && c.domain.eq_ignore_ascii_case(domain)
                && c.path == path)
        });
    }

    /// Clears all cookies from the jar.
    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    /// Evicts all expired cookies from the jar.
    pub fn clear_expired(&mut self) {
        let now = current_time_secs();
        self.cookies.retain(|c| !c.is_expired(now));
    }

    /// Returns the number of cookies currently in the jar.
    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    /// Returns `true` if the jar contains no cookies.
    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    /// Returns an iterator over all cookies currently stored in the jar.
    pub fn all_cookies(&self) -> &[Cookie] {
        &self.cookies
    }

    /// Serializes the cookie jar into a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes a cookie jar from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Persists all stored cookies to disk as a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Loads persisted cookies from a JSON file on disk.
    pub fn load_from_file(path: &Path) -> Result<Self, std::io::Error> {
        let data = std::fs::read_to_string(path)?;
        serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

/// Computes the default path for a cookie given a request URI path per RFC 6265 §5.1.4.
pub fn default_path_for_request(request_path: &str) -> String {
    let path = if request_path.is_empty() {
        "/"
    } else {
        request_path
    };
    if !path.starts_with('/') {
        return "/".to_string();
    }
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(idx) => path[..idx].to_string(),
        None => "/".to_string(),
    }
}

/// Verifies whether `request_host` domain-matches `cookie_domain` per RFC 6265 §5.1.3.
pub fn domain_matches(request_host: &str, cookie_domain: &str, host_only: bool) -> bool {
    let req = request_host.trim().to_ascii_lowercase();
    let cookie = cookie_domain.trim().trim_start_matches('.').to_ascii_lowercase();

    if host_only {
        return req == cookie;
    }

    if req == cookie {
        return true;
    }

    if req.ends_with(&format!(".{cookie}")) {
        return true;
    }

    false
}

/// Checks whether an origin host is authorized to set a cookie for `cookie_domain`.
pub fn can_set_cookie_domain(request_host: &str, cookie_domain: &str) -> bool {
    let req = request_host.trim().to_ascii_lowercase();
    let domain = cookie_domain.trim().trim_start_matches('.').to_ascii_lowercase();
    if domain.is_empty() {
        return false;
    }
    // Prevent setting cookies on public suffixes or top-level domains without internal dots
    if !domain.contains('.') {
        return false;
    }
    req == domain || req.ends_with(&format!(".{domain}"))
}

/// Evaluates path matching between request path and cookie path per RFC 6265 §5.1.4.
pub fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    let req = if request_path.is_empty() { "/" } else { request_path };
    let c_path = if cookie_path.is_empty() { "/" } else { cookie_path };

    if req == c_path {
        return true;
    }

    if req.starts_with(c_path) {
        if c_path.ends_with('/') {
            return true;
        }
        if req.as_bytes().get(c_path.len()) == Some(&b'/') {
            return true;
        }
    }

    false
}

/// Parses an HTTP date string (RFC 1123, RFC 850, or asctime) into seconds since the UNIX epoch.
pub fn parse_http_date(date_str: &str) -> Option<u64> {
    let trimmed = date_str.trim();
    if trimmed.is_empty() {
        return None;
    }

    let s = if let Some((_, rest)) = trimmed.split_once(',') {
        rest.trim()
    } else {
        trimmed
    };

    let mut tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    // Strip leading day-of-week if present without comma (e.g. asctime "Wed Jun  9 10:18:14 2021")
    if tokens.len() >= 5 && parse_month_name(tokens[0]).is_none() && tokens[0].parse::<u32>().is_err() {
        tokens.remove(0);
    }

    let (day, month, year, time_str) = if tokens.len() >= 2 && tokens[0].contains('-') {
        // RFC 850: "06-Nov-94 08:49:37 GMT"
        let mut dash_parts = tokens[0].split('-');
        let d = dash_parts.next()?.parse::<u32>().ok()?;
        let m = parse_month_name(dash_parts.next()?)?;
        let y_raw = dash_parts.next()?.parse::<i32>().ok()?;
        let y = if y_raw < 70 { 2000 + y_raw } else { 1900 + y_raw };
        (d, m, y, tokens[1])
    } else if tokens.len() >= 4 && parse_month_name(tokens[0]).is_some() {
        // asctime: "Nov  6 08:49:37 1994"
        let m = parse_month_name(tokens[0])?;
        let d = tokens[1].parse::<u32>().ok()?;
        let time = tokens[2];
        let y = tokens[3].parse::<i32>().ok()?;
        (d, m, y, time)
    } else if tokens.len() >= 4 {
        // RFC 1123: "06 Nov 1994 08:49:37 GMT"
        let d = tokens[0].parse::<u32>().ok()?;
        let m = parse_month_name(tokens[1])?;
        let y = tokens[2].parse::<i32>().ok()?;
        (d, m, y, tokens[3])
    } else {
        return None;
    };

    let mut time_parts = time_str.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let min = time_parts.next()?.parse::<u32>().ok()?;
    let sec = time_parts.next()?.parse::<u32>().ok()?;

    if day == 0 || day > 31 || month == 0 || month > 12 || hour >= 24 || min >= 60 || sec >= 60 || year < 1970 {
        return None;
    }

    let days = days_from_1970(year, month, day)?;
    Some(days as u64 * 86400 + hour as u64 * 3600 + min as u64 * 60 + sec as u64)
}

fn parse_month_name(s: &str) -> Option<u32> {
    match &s[..3.min(s.len())].to_ascii_lowercase()[..] {
        "jan" => Some(1),
        "feb" => Some(2),
        "mar" => Some(3),
        "apr" => Some(4),
        "may" => Some(5),
        "jun" => Some(6),
        "jul" => Some(7),
        "aug" => Some(8),
        "sep" => Some(9),
        "oct" => Some(10),
        "nov" => Some(11),
        "dec" => Some(12),
        _ => None,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_from_1970(year: i32, month: u32, day: u32) -> Option<i64> {
    if year < 1970 {
        return None;
    }
    let mut total_days: i64 = 0;
    for y in 1970..year {
        total_days += if is_leap_year(y) { 366 } else { 365 };
    }

    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        total_days += month_days[(m - 1) as usize] as i64;
    }
    if month > 2 && is_leap_year(year) {
        total_days += 1;
    }
    total_days += (day as i64) - 1;
    Some(total_days)
}

fn current_time_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::from_secs(0))
        .as_secs()
}
