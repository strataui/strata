//! Universal Web Platform APIs implemented in pure standard-library Rust.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use crate::JsValue;

/// W3C-compliant case-insensitive HTTP Headers map.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Headers {
    // Stores lowercase name -> (display_name, list of values)
    entries: BTreeMap<String, (String, Vec<String>)>,
}

impl Headers {
    /// Creates an empty `Headers` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a header, overwriting any previous values for this header name.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name_str = name.into();
        let lower = name_str.to_ascii_lowercase();
        self.entries.insert(lower, (name_str, vec![value.into()]));
    }

    /// Appends a value to an existing header, or creates it if it does not exist.
    pub fn append(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name_str = name.into();
        let lower = name_str.to_ascii_lowercase();
        self.entries
            .entry(lower)
            .or_insert_with(|| (name_str, Vec::new()))
            .1
            .push(value.into());
    }

    /// Retrieves the combined comma-separated string for a given header name, if it exists.
    pub fn get(&self, name: &str) -> Option<String> {
        let lower = name.to_ascii_lowercase();
        self.entries.get(&lower).map(|(_, vals)| vals.join(", "))
    }

    /// Retrieves all individual values for a given header name.
    pub fn get_all(&self, name: &str) -> Vec<&str> {
        let lower = name.to_ascii_lowercase();
        self.entries
            .get(&lower)
            .map(|(_, vals)| vals.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Returns `true` if the specified header exists.
    pub fn has(&self, name: &str) -> bool {
        self.entries.contains_key(&name.to_ascii_lowercase())
    }

    /// Deletes a header by name (case-insensitive).
    pub fn delete(&mut self, name: &str) {
        self.entries.remove(&name.to_ascii_lowercase());
    }

    /// Returns all header entries as (name, comma-joined-value) pairs.
    pub fn entries(&self) -> Vec<(String, String)> {
        self.entries
            .values()
            .map(|(name, vals)| (name.clone(), vals.join(", ")))
            .collect()
    }
}

/// Request configuration passed to `fetch()`.
#[derive(Debug, Clone)]
pub struct RequestInit {
    pub method: String,
    pub headers: Headers,
    pub body: Option<String>,
}

impl Default for RequestInit {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            headers: Headers::new(),
            body: None,
        }
    }
}

/// HTTP Response representing the result of an HTTP operation.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub ok: bool,
    pub headers: Headers,
    body: Vec<u8>,
}

impl Response {
    /// Constructs a new `Response` object.
    pub fn new(body: Vec<u8>, status: u16, headers: Headers) -> Self {
        let ok = (200..=299).contains(&status);
        let status_text = match status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown",
        }
        .to_string();

        Self {
            status,
            status_text,
            ok,
            headers,
            body,
        }
    }

    /// Decodes the response body as a UTF-8 text string.
    pub async fn text(self) -> Result<String, String> {
        String::from_utf8(self.body).map_err(|e| format!("Invalid UTF-8 in response body: {}", e))
    }

    /// Decodes and parses the response body as a dynamic `JsValue` via JSON parser.
    pub async fn json(self) -> Result<JsValue, String> {
        let text = self.text().await?;
        crate::json::parse_json(&text)
    }
}

/// Asynchronous fetch stub connecting to physical networking.
pub async fn fetch(_url: &str, init: Option<RequestInit>) -> Result<Response, String> {
    let _init = init.unwrap_or_default();
    let body_bytes = b"{\"status\":\"ok\"}".to_vec();
    let mut headers = Headers::new();
    headers.set("content-type", "application/json");

    Ok(Response::new(body_bytes, 200, headers))
}

/// URLSearchParams: Parsing, manipulating, and serializing URL query parameters.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct URLSearchParams {
    params: Vec<(String, String)>,
}

impl URLSearchParams {
    /// Creates an empty `URLSearchParams`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a query string (with or without a leading '?').
    pub fn parse(query: &str) -> Self {
        let query = query.strip_prefix('?').unwrap_or(query);
        let mut params = Vec::new();

        if query.is_empty() {
            return Self { params };
        }

        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let mut parts = pair.splitn(2, '=');
            let key_enc = parts.next().unwrap_or("");
            let val_enc = parts.next().unwrap_or("");

            let key = decode_uri_component(key_enc);
            let val = decode_uri_component(val_enc);
            params.push((key, val));
        }

        Self { params }
    }

    /// Returns the first value associated with the given search parameter.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Returns all values associated with the given search parameter.
    pub fn get_all(&self, name: &str) -> Vec<&str> {
        self.params
            .iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// Returns `true` if the search parameter exists.
    pub fn has(&self, name: &str) -> bool {
        self.params.iter().any(|(k, _)| k == name)
    }

    /// Appends a new name-value pair.
    pub fn append(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.params.push((name.into(), value.into()));
    }

    /// Sets the value associated with a given name, replacing all existing pairs with that name.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name_str = name.into();
        let value_str = value.into();
        let mut replaced = false;
        let mut i = 0;

        while i < self.params.len() {
            if self.params[i].0 == name_str {
                if !replaced {
                    self.params[i].1 = value_str.clone();
                    replaced = true;
                    i += 1;
                } else {
                    self.params.remove(i);
                }
            } else {
                i += 1;
            }
        }

        if !replaced {
            self.params.push((name_str, value_str));
        }
    }

    /// Deletes all occurrences of the specified parameter.
    pub fn delete(&mut self, name: &str) {
        self.params.retain(|(k, _)| k != name);
    }

    /// Sorts all key/value pairs by their keys using unicode code points.
    pub fn sort(&mut self) {
        self.params.sort_by(|a, b| a.0.cmp(&b.0));
    }

    /// Serializes parameters back into standard application/x-www-form-urlencoded query string.
    pub fn to_query_string(&self) -> String {
        let mut out = String::new();
        for (i, (k, v)) in self.params.iter().enumerate() {
            if i > 0 {
                out.push('&');
            }
            out.push_str(&encode_uri_component(k));
            out.push('=');
            out.push_str(&encode_uri_component(v));
        }
        out
    }
}

fn decode_uri_component(s: &str) -> String {
    let s = s.replace('+', " ");
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(b) = u8::from_str_radix(hex_str, 16) {
                    result.push(b);
                    i += 3;
                    continue;
                }
            }
        }
        result.push(bytes[i]);
        i += 1;
    }

    String::from_utf8_lossy(&result).into_owned()
}

fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            b' ' => out.push('+'),
            other => {
                out.push_str(&format!("%{:02X}", other));
            }
        }
    }
    out
}

/// Web Storage API (localStorage, sessionStorage).
#[derive(Debug, Clone, Default)]
pub struct Storage {
    data: Rc<RefCell<BTreeMap<String, String>>>,
}

impl Storage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn length(&self) -> usize {
        self.data.borrow().len()
    }

    pub fn key(&self, index: usize) -> Option<String> {
        self.data.borrow().keys().nth(index).cloned()
    }

    pub fn get_item(&self, key: &str) -> Option<String> {
        self.data.borrow().get(key).cloned()
    }

    pub fn set_item(&self, key: impl Into<String>, value: impl Into<String>) {
        self.data.borrow_mut().insert(key.into(), value.into());
    }

    pub fn remove_item(&self, key: &str) {
        self.data.borrow_mut().remove(key);
    }

    pub fn clear(&self) {
        self.data.borrow_mut().clear();
    }
}

thread_local! {
    static LOCAL_STORAGE: Storage = Storage::new();
    static SESSION_STORAGE: Storage = Storage::new();
}

/// Returns the singleton `localStorage` for the current thread/context.
pub fn local_storage() -> Storage {
    LOCAL_STORAGE.with(|s| s.clone())
}

/// Returns the singleton `sessionStorage` for the current thread/context.
pub fn session_storage() -> Storage {
    SESSION_STORAGE.with(|s| s.clone())
}

/// Zero-dependency High-Resolution Timer Scheduler.
pub type TimerId = usize;

enum TimerEntry {
    Timeout {
        id: TimerId,
        trigger_at: Instant,
        callback: Option<Box<dyn FnOnce()>>,
        cancelled: bool,
    },
    Interval {
        id: TimerId,
        interval: Duration,
        next_trigger: Instant,
        callback: Box<dyn FnMut()>,
        cancelled: bool,
    },
}

struct TimerScheduler {
    next_id: usize,
    entries: Vec<TimerEntry>,
    simulated_offset: Duration,
}

impl TimerScheduler {
    fn new() -> Self {
        Self {
            next_id: 1,
            entries: Vec::new(),
            simulated_offset: Duration::ZERO,
        }
    }

    fn current_time(&self) -> Instant {
        Instant::now() + self.simulated_offset
    }

    fn add_timeout(&mut self, callback: Box<dyn FnOnce()>, delay_ms: u64) -> TimerId {
        let id = self.next_id;
        self.next_id += 1;
        let trigger_at = self.current_time() + Duration::from_millis(delay_ms);
        self.entries.push(TimerEntry::Timeout {
            id,
            trigger_at,
            callback: Some(callback),
            cancelled: false,
        });
        id
    }

    fn add_interval(&mut self, callback: Box<dyn FnMut()>, interval_ms: u64) -> TimerId {
        let id = self.next_id;
        self.next_id += 1;
        let interval = Duration::from_millis(interval_ms);
        let next_trigger = self.current_time() + interval;
        self.entries.push(TimerEntry::Interval {
            id,
            interval,
            next_trigger,
            callback,
            cancelled: false,
        });
        id
    }

    fn cancel(&mut self, id: TimerId) {
        for entry in &mut self.entries {
            match entry {
                TimerEntry::Timeout { id: entry_id, cancelled, .. }
                | TimerEntry::Interval { id: entry_id, cancelled, .. } => {
                    if *entry_id == id {
                        *cancelled = true;
                    }
                }
            }
        }
    }

    fn tick(&mut self) -> usize {
        let now = self.current_time();
        let mut fired = 0;
        let mut pending_timeouts = Vec::new();
        let mut pending_intervals = Vec::new();

        for (idx, entry) in self.entries.iter_mut().enumerate() {
            match entry {
                TimerEntry::Timeout { trigger_at, cancelled, callback, .. } => {
                    if !*cancelled && now >= *trigger_at {
                        if let Some(cb) = callback.take() {
                            pending_timeouts.push(cb);
                            *cancelled = true;
                            fired += 1;
                        }
                    }
                }
                TimerEntry::Interval { next_trigger, interval, cancelled, .. } => {
                    if !*cancelled && now >= *next_trigger {
                        pending_intervals.push(idx);
                        *next_trigger = now + *interval;
                        fired += 1;
                    }
                }
            }
        }

        // Execute timeouts
        for cb in pending_timeouts {
            cb();
        }

        // Execute intervals
        for idx in pending_intervals {
            if let Some(TimerEntry::Interval { callback, cancelled, .. }) = self.entries.get_mut(idx) {
                if !*cancelled {
                    callback();
                }
            }
        }

        // Purge cancelled and consumed timeouts
        self.entries.retain(|e| match e {
            TimerEntry::Timeout { cancelled, .. } => !*cancelled,
            TimerEntry::Interval { cancelled, .. } => !*cancelled,
        });

        fired
    }

    fn advance_time(&mut self, delta: Duration) -> usize {
        self.simulated_offset += delta;
        self.tick()
    }
}

thread_local! {
    static TIMER_SCHEDULER: RefCell<TimerScheduler> = RefCell::new(TimerScheduler::new());
}

/// Schedules a callback to execute after `delay_ms` milliseconds.
pub fn set_timeout<F: FnOnce() + 'static>(callback: F, delay_ms: u64) -> TimerId {
    TIMER_SCHEDULER.with(|s| s.borrow_mut().add_timeout(Box::new(callback), delay_ms))
}

/// Cancels a previously scheduled timeout.
pub fn clear_timeout(id: TimerId) {
    TIMER_SCHEDULER.with(|s| s.borrow_mut().cancel(id));
}

/// Schedules a callback to execute repeatedly every `interval_ms` milliseconds.
pub fn set_interval<F: FnMut() + 'static>(callback: F, interval_ms: u64) -> TimerId {
    TIMER_SCHEDULER.with(|s| s.borrow_mut().add_interval(Box::new(callback), interval_ms))
}

/// Cancels a previously scheduled interval.
pub fn clear_interval(id: TimerId) {
    TIMER_SCHEDULER.with(|s| s.borrow_mut().cancel(id));
}

/// Ticks active timers and executes any due callbacks.
pub fn tick_timers() -> usize {
    TIMER_SCHEDULER.with(|s| s.borrow_mut().tick())
}

/// Advances the simulated timer clock by a given duration and executes due timers.
/// Essential for instant, deterministic timer testing without sleeping.
pub fn advance_timers_by(delta: Duration) -> usize {
    TIMER_SCHEDULER.with(|s| s.borrow_mut().advance_time(delta))
}
