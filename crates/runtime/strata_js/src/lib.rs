//! Strata JS: Porting modern JavaScript frontend paradigms, dynamic typing, and Web APIs into safe Rust.

use std::collections::BTreeMap;
use std::ops::Index;

pub mod engine;
pub mod json;
pub mod promise;
pub mod web;

pub use engine::{Document, DomNode, DomNodeType, JsEngine, JsParser, Stmt, Expr};

/// Tagged union representing dynamic JavaScript values.
#[derive(Debug, Clone, PartialEq)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsValue>),
    Object(BTreeMap<String, JsValue>),
}

impl JsValue {
    /// Parses a JSON string into a `JsValue`.
    pub fn from_json_str(s: &str) -> Result<Self, String> {
        json::parse_json(s)
    }

    /// Serializes this `JsValue` into a standard JSON string.
    pub fn to_json_string(&self) -> String {
        json::stringify_json(self)
    }

    /// Returns JavaScript `typeof` string for this value.
    pub fn js_typeof(&self) -> &'static str {
        match self {
            Self::Undefined => "undefined",
            Self::Null => "object", // Canonical JS specification quirk
            Self::Bool(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Array(_) => "object",
            Self::Object(map) => {
                if map.contains_key("__fn__") || map.contains_key("__is_class__") {
                    "function"
                } else {
                    "object"
                }
            }
        }
    }

    /// Strict equality comparison (`===`).
    pub fn strict_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Undefined, Self::Undefined) => true,
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => {
                if a.is_nan() || b.is_nan() {
                    false
                } else {
                    a == b
                }
            }
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.strict_eq(y))
            }
            (Self::Object(a), Self::Object(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| {
                        b.get(k).is_some_and(|other_v| v.strict_eq(other_v))
                    })
            }
            _ => false,
        }
    }

    /// Abstract loose equality comparison (`==`) adhering to ECMAScript standard.
    pub fn loose_eq(&self, other: &Self) -> bool {
        match (self, other) {
            // Same type: delegates to strict equality
            (Self::Undefined, Self::Undefined)
            | (Self::Null, Self::Null)
            | (Self::Bool(_), Self::Bool(_))
            | (Self::Number(_), Self::Number(_))
            | (Self::String(_), Self::String(_))
            | (Self::Array(_), Self::Array(_))
            | (Self::Object(_), Self::Object(_)) => self.strict_eq(other),

            // null == undefined -> true
            (Self::Null, Self::Undefined) | (Self::Undefined, Self::Null) => true,

            // Number == String: convert string to number
            (Self::Number(n), Self::String(s)) => match s.trim().parse::<f64>() {
                Ok(parsed) => !n.is_nan() && !parsed.is_nan() && n == &parsed,
                Err(_) => false,
            },
            (Self::String(s), Self::Number(n)) => match s.trim().parse::<f64>() {
                Ok(parsed) => !n.is_nan() && !parsed.is_nan() && &parsed == n,
                Err(_) => false,
            },

            // Boolean == Any: convert boolean to 1.0 or 0.0
            (Self::Bool(b), other) => {
                let num = if *b { 1.0 } else { 0.0 };
                JsValue::Number(num).loose_eq(other)
            }
            (this, Self::Bool(b)) => {
                let num = if *b { 1.0 } else { 0.0 };
                this.loose_eq(&JsValue::Number(num))
            }

            // String/Number == Array: unwrap single-element or join
            (Self::String(_), Self::Array(arr)) | (Self::Number(_), Self::Array(arr)) => {
                let joined = arr
                    .iter()
                    .map(|v| match v {
                        JsValue::String(s) => s.clone(),
                        JsValue::Number(n) => format!("{}", n),
                        JsValue::Bool(b) => format!("{}", b),
                        _ => String::new(),
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                self.loose_eq(&JsValue::String(joined))
            }
            (Self::Array(arr), Self::String(_)) | (Self::Array(arr), Self::Number(_)) => {
                let joined = arr
                    .iter()
                    .map(|v| match v {
                        JsValue::String(s) => s.clone(),
                        JsValue::Number(n) => format!("{}", n),
                        JsValue::Bool(b) => format!("{}", b),
                        _ => String::new(),
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                JsValue::String(joined).loose_eq(other)
            }

            _ => false,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, Self::Undefined)
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(b) => *b,
            Self::Number(n) => *n != 0.0 && !n.is_nan(),
            Self::String(s) => !s.is_empty(),
            Self::Array(_) | Self::Object(_) => true,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[JsValue]> {
        match self {
            Self::Array(arr) => Some(arr.as_slice()),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<JsValue>> {
        match self {
            Self::Array(arr) => Some(arr),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, JsValue>> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut BTreeMap<String, JsValue>> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&JsValue> {
        match self {
            Self::Object(map) => map.get(key),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut JsValue> {
        match self {
            Self::Object(map) => map.get_mut(key),
            _ => None,
        }
    }

    pub fn set_property(&mut self, key: impl Into<String>, val: impl Into<JsValue>) {
        if !matches!(self, Self::Object(_)) {
            *self = Self::Object(BTreeMap::new());
        }
        if let Self::Object(map) = self {
            map.insert(key.into(), val.into());
        }
    }

    pub fn remove_property(&mut self, key: &str) -> Option<JsValue> {
        match self {
            Self::Object(map) => map.remove(key),
            _ => None,
        }
    }

    pub fn get_index(&self, index: usize) -> Option<&JsValue> {
        match self {
            Self::Array(arr) => arr.get(index),
            _ => None,
        }
    }

    pub fn get_index_mut(&mut self, index: usize) -> Option<&mut JsValue> {
        match self {
            Self::Array(arr) => arr.get_mut(index),
            _ => None,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Array(arr) => arr.len(),
            Self::String(s) => s.len(),
            Self::Object(obj) => obj.len(),
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // ==========================================
    // JavaScript Array Prototype Methods
    // ==========================================

    pub fn map<F>(&self, mut f: F) -> Self
    where
        F: FnMut(&JsValue) -> JsValue,
    {
        match self {
            Self::Array(arr) => Self::Array(arr.iter().map(&mut f).collect()),
            _ => Self::Array(Vec::new()),
        }
    }

    pub fn filter<F>(&self, mut f: F) -> Self
    where
        F: FnMut(&JsValue) -> bool,
    {
        match self {
            Self::Array(arr) => Self::Array(arr.iter().filter(|item| f(item)).cloned().collect()),
            _ => Self::Array(Vec::new()),
        }
    }

    pub fn reduce<F>(&self, f: F, initial: JsValue) -> JsValue
    where
        F: FnMut(JsValue, &JsValue) -> JsValue,
    {
        match self {
            Self::Array(arr) => arr.iter().fold(initial, f),
            _ => initial,
        }
    }

    pub fn find<F>(&self, f: F) -> Option<&JsValue>
    where
        F: Fn(&JsValue) -> bool,
    {
        match self {
            Self::Array(arr) => arr.iter().find(|item| f(item)),
            _ => None,
        }
    }

    pub fn find_index<F>(&self, f: F) -> Option<usize>
    where
        F: Fn(&JsValue) -> bool,
    {
        match self {
            Self::Array(arr) => arr.iter().position(f),
            _ => None,
        }
    }

    pub fn some<F>(&self, f: F) -> bool
    where
        F: Fn(&JsValue) -> bool,
    {
        match self {
            Self::Array(arr) => arr.iter().any(f),
            _ => false,
        }
    }

    pub fn every<F>(&self, f: F) -> bool
    where
        F: Fn(&JsValue) -> bool,
    {
        match self {
            Self::Array(arr) => arr.iter().all(f),
            _ => false,
        }
    }

    pub fn includes(&self, item: &JsValue) -> bool {
        match self {
            Self::Array(arr) => arr.iter().any(|x| x.strict_eq(item)),
            _ => false,
        }
    }

    pub fn slice(&self, start: isize, end: Option<isize>) -> Self {
        match self {
            Self::Array(arr) => {
                let len = arr.len() as isize;
                let actual_start = if start < 0 {
                    (len + start).max(0) as usize
                } else {
                    start.min(len) as usize
                };

                let actual_end = match end {
                    Some(e) => {
                        if e < 0 {
                            (len + e).max(0) as usize
                        } else {
                            e.min(len) as usize
                        }
                    }
                    None => len as usize,
                };

                if actual_start >= actual_end {
                    Self::Array(Vec::new())
                } else {
                    Self::Array(arr[actual_start..actual_end].to_vec())
                }
            }
            _ => Self::Array(Vec::new()),
        }
    }

    pub fn join(&self, separator: &str) -> String {
        match self {
            Self::Array(arr) => arr
                .iter()
                .map(|item| match item {
                    JsValue::String(s) => s.clone(),
                    JsValue::Number(n) => format!("{}", n),
                    JsValue::Bool(b) => format!("{}", b),
                    JsValue::Null | JsValue::Undefined => String::new(),
                    other => json::stringify_json(other),
                })
                .collect::<Vec<_>>()
                .join(separator),
            _ => String::new(),
        }
    }

    pub fn push(&mut self, val: impl Into<JsValue>) {
        if !matches!(self, Self::Array(_)) {
            *self = Self::Array(Vec::new());
        }
        if let Self::Array(arr) = self {
            arr.push(val.into());
        }
    }

    pub fn pop(&mut self) -> Option<JsValue> {
        match self {
            Self::Array(arr) => arr.pop(),
            _ => None,
        }
    }

    pub fn concat(&self, other: &JsValue) -> Self {
        let mut result = match self {
            Self::Array(arr) => arr.clone(),
            other_self => vec![other_self.clone()],
        };

        match other {
            Self::Array(other_arr) => result.extend(other_arr.iter().cloned()),
            other_val => result.push(other_val.clone()),
        }

        Self::Array(result)
    }

    pub fn reverse(&mut self) {
        if let Self::Array(arr) = self {
            arr.reverse();
        }
    }

    pub fn sort_by<F>(&mut self, f: F)
    where
        F: FnMut(&JsValue, &JsValue) -> std::cmp::Ordering,
    {
        if let Self::Array(arr) = self {
            arr.sort_by(f);
        }
    }

    pub fn flat(&self, depth: usize) -> Self {
        match self {
            Self::Array(arr) => {
                let mut out = Vec::new();
                flatten_recursive(arr, depth, &mut out);
                Self::Array(out)
            }
            _ => self.clone(),
        }
    }
}

fn flatten_recursive(arr: &[JsValue], depth: usize, out: &mut Vec<JsValue>) {
    for item in arr {
        if depth > 0 {
            if let JsValue::Array(inner) = item {
                flatten_recursive(inner, depth - 1, out);
                continue;
            }
        }
        out.push(item.clone());
    }
}

/// Static JavaScript `Object` utility functions.
pub struct JsObject;

impl JsObject {
    pub fn keys(val: &JsValue) -> Vec<String> {
        match val {
            JsValue::Object(map) => map.keys().cloned().collect(),
            JsValue::Array(arr) => (0..arr.len()).map(|i| i.to_string()).collect(),
            _ => Vec::new(),
        }
    }

    pub fn values(val: &JsValue) -> Vec<JsValue> {
        match val {
            JsValue::Object(map) => map.values().cloned().collect(),
            JsValue::Array(arr) => arr.clone(),
            _ => Vec::new(),
        }
    }

    pub fn entries(val: &JsValue) -> Vec<(String, JsValue)> {
        match val {
            JsValue::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            JsValue::Array(arr) => arr
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn assign(target: &mut JsValue, source: &JsValue) {
        if !matches!(target, JsValue::Object(_)) {
            *target = JsValue::Object(BTreeMap::new());
        }

        if let (JsValue::Object(target_map), JsValue::Object(source_map)) = (target, source) {
            for (k, v) in source_map {
                target_map.insert(k.clone(), v.clone());
            }
        }
    }
}

impl Index<&str> for JsValue {
    type Output = JsValue;

    fn index(&self, key: &str) -> &Self::Output {
        static UNDEFINED: JsValue = JsValue::Undefined;
        match self {
            Self::Object(map) => map.get(key).unwrap_or(&UNDEFINED),
            _ => &UNDEFINED,
        }
    }
}

impl Index<usize> for JsValue {
    type Output = JsValue;

    fn index(&self, index: usize) -> &Self::Output {
        static UNDEFINED: JsValue = JsValue::Undefined;
        match self {
            Self::Array(arr) => arr.get(index).unwrap_or(&UNDEFINED),
            _ => &UNDEFINED,
        }
    }
}

impl From<bool> for JsValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<f64> for JsValue {
    fn from(v: f64) -> Self {
        Self::Number(v)
    }
}

impl From<f32> for JsValue {
    fn from(v: f32) -> Self {
        Self::Number(v as f64)
    }
}

impl From<i32> for JsValue {
    fn from(v: i32) -> Self {
        Self::Number(v as f64)
    }
}

impl From<u32> for JsValue {
    fn from(v: u32) -> Self {
        Self::Number(v as f64)
    }
}

impl From<i64> for JsValue {
    fn from(v: i64) -> Self {
        Self::Number(v as f64)
    }
}

impl From<u64> for JsValue {
    fn from(v: u64) -> Self {
        Self::Number(v as f64)
    }
}

impl From<usize> for JsValue {
    fn from(v: usize) -> Self {
        Self::Number(v as f64)
    }
}

impl From<isize> for JsValue {
    fn from(v: isize) -> Self {
        Self::Number(v as f64)
    }
}

impl From<&str> for JsValue {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}

impl From<String> for JsValue {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<&String> for JsValue {
    fn from(v: &String) -> Self {
        Self::String(v.clone())
    }
}

impl From<Vec<JsValue>> for JsValue {
    fn from(v: Vec<JsValue>) -> Self {
        Self::Array(v)
    }
}

impl From<BTreeMap<String, JsValue>> for JsValue {
    fn from(v: BTreeMap<String, JsValue>) -> Self {
        Self::Object(v)
    }
}

impl<T: Into<JsValue>> From<Option<T>> for JsValue {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(val) => val.into(),
            None => Self::Null,
        }
    }
}

impl std::fmt::Display for JsValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Undefined => write!(f, "undefined"),
            Self::Null => write!(f, "null"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Number(n) => {
                if n.is_nan() {
                    write!(f, "NaN")
                } else if n.is_infinite() {
                    if *n < 0.0 {
                        write!(f, "-Infinity")
                    } else {
                        write!(f, "Infinity")
                    }
                } else if n.fract() == 0.0 && n.abs() <= i64::MAX as f64 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{n}")
                }
            }
            Self::String(s) => write!(f, "{s}"),
            Self::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|item| item.to_string()).collect();
                write!(f, "{}", items.join(","))
            }
            Self::Object(_) => write!(f, "[object Object]"),
        }
    }
}

pub mod prelude {
    pub use super::engine::{Document, DomNode, DomNodeType, JsEngine};
    pub use super::json::{parse_json, stringify_json};
    pub use super::promise::{drain_microtasks, pending_microtasks_count, queue_microtask, JsPromise, PromiseState};
    pub use super::web::*;
    pub use super::{JsObject, JsValue};
}
