//! Strata JS: Porting modern JavaScript frontend paradigms, dynamic typing, and Web APIs into safe Rust.
//!
//! # Examples
//!
//! ```rust
//! use strata_js::json::{parse_json, stringify_json};
//! use strata_js::JsValue;
//!
//! let val = parse_json("{\"name\": \"Strata\"}").unwrap();
//! assert_eq!(val["name"], JsValue::String("Strata".to_string()));
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Index;
use std::rc::Rc;

pub mod bigint;
pub mod engine;
pub mod json;
pub mod promise;
pub mod web;

pub use bigint::JsBigInt;
pub use engine::{
    Document, DomMutationRecord, DomMutationType, DomNode, DomNodeType, Expr, JsEngine, JsParser,
    MutationListener, MutationSink, Stmt,
};

/// Reference handle to an executable JavaScript closure.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClosureHandle {
    pub id: u64,
    pub name: Option<String>,
}

impl ClosureHandle {
    pub const fn new(id: u64, name: Option<String>) -> Self {
        Self { id, name }
    }
}

/// Unique ECMAScript Symbol identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId {
    pub id: u64,
    pub description: Option<String>,
}

impl SymbolId {
    pub const ITERATOR: u64 = 1;
    pub const TO_STRING_TAG: u64 = 2;
    pub const HAS_INSTANCE: u64 = 3;
    pub const TO_PRIMITIVE: u64 = 4;

    pub const fn new(id: u64, description: Option<String>) -> Self {
        Self { id, description }
    }

    pub fn iterator() -> Self {
        Self {
            id: Self::ITERATOR,
            description: Some("Symbol.iterator".to_string()),
        }
    }

    pub fn to_string_tag() -> Self {
        Self {
            id: Self::TO_STRING_TAG,
            description: Some("Symbol.toStringTag".to_string()),
        }
    }

    pub fn has_instance() -> Self {
        Self {
            id: Self::HAS_INSTANCE,
            description: Some("Symbol.hasInstance".to_string()),
        }
    }

    pub fn to_primitive() -> Self {
        Self {
            id: Self::TO_PRIMITIVE,
            description: Some("Symbol.toPrimitive".to_string()),
        }
    }
}

/// Tagged union representing dynamic JavaScript values with authentic ECMAScript reference semantics.
#[derive(Debug, Clone)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    BigInt(JsBigInt),
    Array(Rc<RefCell<Vec<JsValue>>>),
    Object(Rc<RefCell<BTreeMap<String, JsValue>>>),
    Function(ClosureHandle),
    Symbol(SymbolId),
}

impl PartialEq for JsValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Undefined, Self::Undefined) => true,
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => {
                if a.is_nan() && b.is_nan() {
                    true
                } else {
                    a == b
                }
            }
            (Self::String(a), Self::String(b)) => a == b,
            (Self::BigInt(a), Self::BigInt(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => a.id == b.id,
            (Self::Symbol(a), Self::Symbol(b)) => a.id == b.id,
            (Self::Array(a), Self::Array(b)) => {
                if Rc::ptr_eq(a, b) {
                    true
                } else {
                    *a.borrow() == *b.borrow()
                }
            }
            (Self::Object(a), Self::Object(b)) => {
                if Rc::ptr_eq(a, b) {
                    true
                } else {
                    *a.borrow() == *b.borrow()
                }
            }
            _ => false,
        }
    }
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
            Self::BigInt(_) => "bigint",
            Self::Array(_) => "object",
            Self::Function(_) => "function",
            Self::Symbol(_) => "symbol",
            Self::Object(cell) => {
                let map = cell.borrow();
                if map.contains_key("__fn__") || map.contains_key("__is_class__") {
                    "function"
                } else {
                    "object"
                }
            }
        }
    }

    /// Strict equality comparison (`===`) adhering to ECMAScript standard.
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
            (Self::BigInt(a), Self::BigInt(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => a.id == b.id,
            (Self::Symbol(a), Self::Symbol(b)) => a.id == b.id,
            (Self::Array(a), Self::Array(b)) => Rc::ptr_eq(a, b),
            (Self::Object(a), Self::Object(b)) => Rc::ptr_eq(a, b),
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
            | (Self::BigInt(_), Self::BigInt(_))
            | (Self::Array(_), Self::Array(_))
            | (Self::Object(_), Self::Object(_))
            | (Self::Function(_), Self::Function(_))
            | (Self::Symbol(_), Self::Symbol(_)) => self.strict_eq(other),

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

            // BigInt == String
            (Self::BigInt(bi), Self::String(s)) | (Self::String(s), Self::BigInt(bi)) => {
                if let Ok(parsed) = JsBigInt::parse(s) {
                    &parsed == bi
                } else {
                    false
                }
            }

            // BigInt == Number
            (Self::BigInt(bi), Self::Number(n)) | (Self::Number(n), Self::BigInt(bi)) => {
                if n.fract() == 0.0 && !n.is_nan() && !n.is_infinite() {
                    bi.to_decimal_string() == format!("{n}")
                } else {
                    false
                }
            }

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
                    .borrow()
                    .iter()
                    .map(|v| match v {
                        JsValue::String(s) => s.clone(),
                        JsValue::Number(n) => format!("{}", n),
                        JsValue::Bool(b) => format!("{}", b),
                        JsValue::BigInt(bi) => bi.to_decimal_string(),
                        _ => String::new(),
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                self.loose_eq(&JsValue::String(joined))
            }
            (Self::Array(arr), Self::String(_)) | (Self::Array(arr), Self::Number(_)) => {
                let joined = arr
                    .borrow()
                    .iter()
                    .map(|v| match v {
                        JsValue::String(s) => s.clone(),
                        JsValue::Number(n) => format!("{}", n),
                        JsValue::Bool(b) => format!("{}", b),
                        JsValue::BigInt(bi) => bi.to_decimal_string(),
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
            Self::BigInt(bi) => !bi.is_zero(),
            Self::Array(_) | Self::Object(_) | Self::Function(_) | Self::Symbol(_) => true,
        }
    }

    pub fn is_function(&self) -> bool {
        matches!(self, Self::Function(_))
    }

    pub fn is_symbol(&self) -> bool {
        matches!(self, Self::Symbol(_))
    }

    pub fn new_object(map: BTreeMap<String, JsValue>) -> Self {
        Self::Object(Rc::new(RefCell::new(map)))
    }

    pub fn new_array(items: Vec<JsValue>) -> Self {
        Self::Array(Rc::new(RefCell::new(items)))
    }

    pub fn empty_object() -> Self {
        Self::Object(Rc::new(RefCell::new(BTreeMap::new())))
    }

    pub fn empty_array() -> Self {
        Self::Array(Rc::new(RefCell::new(Vec::new())))
    }

    pub fn has_prop(&self, key: &str) -> bool {
        match self {
            Self::Object(c) => c.borrow().contains_key(key),
            Self::Array(arr) => {
                if key == "length" {
                    return true;
                }
                if let Ok(idx) = key.parse::<usize>() {
                    idx < arr.borrow().len()
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub fn get_prop(&self, key: &str) -> Option<JsValue> {
        match self {
            Self::Object(c) => c.borrow().get(key).cloned(),
            Self::Array(arr) => {
                if key == "length" {
                    return Some(JsValue::Number(arr.borrow().len() as f64));
                }
                if let Ok(idx) = key.parse::<usize>() {
                    arr.borrow().get(idx).cloned()
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn set_prop(&self, key: impl Into<String>, val: impl Into<JsValue>) -> bool {
        let k = key.into();
        let v = val.into();
        match self {
            Self::Object(c) => {
                c.borrow_mut().insert(k, v);
                true
            }
            Self::Array(arr) => {
                if let Ok(idx) = k.parse::<usize>() {
                    let mut b = arr.borrow_mut();
                    if idx >= b.len() {
                        b.resize(idx + 1, JsValue::Undefined);
                    }
                    b[idx] = v;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub fn get_property(&self, key: &str) -> Option<JsValue> {
        self.get_prop(key)
    }

    pub fn set_property(&self, key: impl Into<String>, val: impl Into<JsValue>) -> bool {
        self.set_prop(key, val)
    }

    pub fn as_closure_handle(&self) -> Option<&ClosureHandle> {
        match self {
            Self::Function(h) => Some(h),
            _ => None,
        }
    }

    pub fn as_symbol(&self) -> Option<&SymbolId> {
        match self {
            Self::Symbol(s) => Some(s),
            _ => None,
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

    pub fn as_bigint(&self) -> Option<&JsBigInt> {
        match self {
            Self::BigInt(b) => Some(b),
            _ => None,
        }
    }

    pub fn remove_property(&mut self, key: &str) -> Option<JsValue> {
        match self {
            Self::Object(cell) => cell.borrow_mut().remove(key),
            _ => None,
        }
    }

    pub fn get_index(&self, index: usize) -> Option<JsValue> {
        match self {
            Self::Array(cell) => cell.borrow().get(index).cloned(),
            _ => None,
        }
    }

    pub fn set_index(&mut self, index: usize, val: impl Into<JsValue>) {
        if let Self::Array(cell) = self {
            let mut arr = cell.borrow_mut();
            if index >= arr.len() {
                arr.resize(index + 1, JsValue::Undefined);
            }
            arr[index] = val.into();
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Array(cell) => cell.borrow().len(),
            Self::String(s) => s.len(),
            Self::Object(cell) => cell.borrow().len(),
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
            Self::Array(cell) => Self::new_array(cell.borrow().iter().map(&mut f).collect()),
            _ => Self::empty_array(),
        }
    }

    pub fn filter<F>(&self, mut f: F) -> Self
    where
        F: FnMut(&JsValue) -> bool,
    {
        match self {
            Self::Array(cell) => Self::new_array(cell.borrow().iter().filter(|item| f(item)).cloned().collect()),
            _ => Self::empty_array(),
        }
    }

    pub fn reduce<F>(&self, f: F, initial: JsValue) -> JsValue
    where
        F: FnMut(JsValue, &JsValue) -> JsValue,
    {
        match self {
            Self::Array(cell) => cell.borrow().iter().fold(initial, f),
            _ => initial,
        }
    }

    pub fn find<F>(&self, f: F) -> Option<JsValue>
    where
        F: Fn(&JsValue) -> bool,
    {
        match self {
            Self::Array(cell) => cell.borrow().iter().find(|item| f(item)).cloned(),
            _ => None,
        }
    }

    pub fn find_index<F>(&self, f: F) -> Option<usize>
    where
        F: Fn(&JsValue) -> bool,
    {
        match self {
            Self::Array(cell) => cell.borrow().iter().position(f),
            _ => None,
        }
    }

    pub fn some<F>(&self, f: F) -> bool
    where
        F: Fn(&JsValue) -> bool,
    {
        match self {
            Self::Array(cell) => cell.borrow().iter().any(f),
            _ => false,
        }
    }

    pub fn every<F>(&self, f: F) -> bool
    where
        F: Fn(&JsValue) -> bool,
    {
        match self {
            Self::Array(cell) => cell.borrow().iter().all(f),
            _ => false,
        }
    }

    pub fn includes(&self, item: &JsValue) -> bool {
        match self {
            Self::Array(cell) => cell.borrow().iter().any(|x| x.strict_eq(item)),
            _ => false,
        }
    }

    pub fn slice(&self, start: isize, end: Option<isize>) -> Self {
        match self {
            Self::Array(cell) => {
                let arr = cell.borrow();
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
                    Self::empty_array()
                } else {
                    Self::new_array(arr[actual_start..actual_end].to_vec())
                }
            }
            _ => Self::empty_array(),
        }
    }

    pub fn join(&self, separator: &str) -> String {
        match self {
            Self::Array(cell) => cell
                .borrow()
                .iter()
                .map(|item| match item {
                    JsValue::String(s) => s.clone(),
                    JsValue::Number(n) => format!("{}", n),
                    JsValue::Bool(b) => format!("{}", b),
                    JsValue::BigInt(bi) => bi.to_decimal_string(),
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
            *self = Self::empty_array();
        }
        if let Self::Array(cell) = self {
            cell.borrow_mut().push(val.into());
        }
    }

    pub fn pop(&mut self) -> Option<JsValue> {
        match self {
            Self::Array(cell) => cell.borrow_mut().pop(),
            _ => None,
        }
    }

    pub fn concat(&self, other: &JsValue) -> Self {
        let mut result = match self {
            Self::Array(cell) => cell.borrow().clone(),
            other_self => vec![other_self.clone()],
        };

        match other {
            Self::Array(other_cell) => result.extend(other_cell.borrow().iter().cloned()),
            other_val => result.push(other_val.clone()),
        }

        Self::new_array(result)
    }

    pub fn reverse(&mut self) {
        if let Self::Array(cell) = self {
            cell.borrow_mut().reverse();
        }
    }

    pub fn sort_by<F>(&mut self, f: F)
    where
        F: FnMut(&JsValue, &JsValue) -> std::cmp::Ordering,
    {
        if let Self::Array(cell) = self {
            cell.borrow_mut().sort_by(f);
        }
    }

    pub fn flat(&self, depth: usize) -> Self {
        match self {
            Self::Array(cell) => {
                let mut out = Vec::new();
                flatten_recursive(&cell.borrow(), depth, &mut out);
                Self::new_array(out)
            }
            _ => self.clone(),
        }
    }
}

fn flatten_recursive(arr: &[JsValue], depth: usize, out: &mut Vec<JsValue>) {
    for item in arr {
        if depth > 0 {
            if let JsValue::Array(inner) = item {
                flatten_recursive(&inner.borrow(), depth - 1, out);
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
            JsValue::Object(cell) => cell.borrow().keys().cloned().collect(),
            JsValue::Array(cell) => (0..cell.borrow().len()).map(|i| i.to_string()).collect(),
            _ => Vec::new(),
        }
    }

    pub fn values(val: &JsValue) -> Vec<JsValue> {
        match val {
            JsValue::Object(cell) => cell.borrow().values().cloned().collect(),
            JsValue::Array(cell) => cell.borrow().clone(),
            _ => Vec::new(),
        }
    }

    pub fn entries(val: &JsValue) -> Vec<(String, JsValue)> {
        match val {
            JsValue::Object(cell) => cell.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            JsValue::Array(cell) => cell
                .borrow()
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn assign(target: &mut JsValue, source: &JsValue) {
        if !matches!(target, JsValue::Object(_)) {
            *target = JsValue::empty_object();
        }

        if let (JsValue::Object(target_cell), JsValue::Object(source_cell)) = (target, source) {
            let src = source_cell.borrow().clone();
            let mut tgt = target_cell.borrow_mut();
            for (k, v) in src {
                tgt.insert(k, v);
            }
        }
    }
}

struct SyncJsValue(JsValue);
unsafe impl Sync for SyncJsValue {}
static UNDEFINED: SyncJsValue = SyncJsValue(JsValue::Undefined);

impl Index<&str> for JsValue {
    type Output = JsValue;

    fn index(&self, key: &str) -> &Self::Output {
        match self {
            Self::Object(cell) => {
                let ptr = cell.as_ptr();
                unsafe { (&*ptr).get(key).unwrap_or(&UNDEFINED.0) }
            }
            _ => &UNDEFINED.0,
        }
    }
}

impl Index<usize> for JsValue {
    type Output = JsValue;

    fn index(&self, index: usize) -> &Self::Output {
        match self {
            Self::Array(cell) => {
                let ptr = cell.as_ptr();
                unsafe { (&*ptr).get(index).unwrap_or(&UNDEFINED.0) }
            }
            _ => &UNDEFINED.0,
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
        Self::new_array(v)
    }
}

impl From<BTreeMap<String, JsValue>> for JsValue {
    fn from(v: BTreeMap<String, JsValue>) -> Self {
        Self::new_object(v)
    }
}

impl From<JsBigInt> for JsValue {
    fn from(v: JsBigInt) -> Self {
        Self::BigInt(v)
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
            Self::BigInt(b) => write!(f, "{}", b.to_decimal_string()),
            Self::Array(arr) => {
                let items: Vec<String> = arr.borrow().iter().map(|item| item.to_string()).collect();
                write!(f, "{}", items.join(","))
            }
            Self::Object(_) => write!(f, "[object Object]"),
            Self::Function(h) => {
                let name = h.name.as_deref().unwrap_or("anonymous");
                write!(f, "function {name}() {{ [native code] }}")
            }
            Self::Symbol(s) => {
                if let Some(ref d) = s.description {
                    write!(f, "Symbol({d})")
                } else {
                    write!(f, "Symbol()")
                }
            }
        }
    }
}

pub mod prelude {
    pub use super::bigint::JsBigInt;
    pub use super::engine::{Document, DomNode, DomNodeType, JsEngine};
    pub use super::json::{parse_json, stringify_json};
    pub use super::promise::{drain_microtasks, pending_microtasks_count, queue_microtask, JsPromise, PromiseState};
    pub use super::web::*;
    pub use super::{ClosureHandle, JsObject, JsValue, SymbolId};
}
