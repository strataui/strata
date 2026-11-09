//! Lexical scope environment frame, closure descriptors, and parameter binding.

use super::ast::Stmt;
use crate::JsValue;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Lexical scope environment frame supporting lexical nesting and closures.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    pub parent: Option<Rc<RefCell<Scope>>>,
    pub bindings: HashMap<String, JsValue>,
}

impl Scope {
    pub fn new_root() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            parent: None,
            bindings: HashMap::new(),
        }))
    }

    pub fn new_child(parent: Rc<RefCell<Self>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            parent: Some(parent),
            bindings: HashMap::new(),
        }))
    }

    pub fn get(&self, name: &str) -> Option<JsValue> {
        if let Some(val) = self.bindings.get(name) {
            return Some(val.clone());
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().get(name);
        }
        None
    }

    pub fn has(&self, name: &str) -> bool {
        if self.bindings.contains_key(name) {
            return true;
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().has(name);
        }
        false
    }

    pub fn set(&mut self, name: &str, val: JsValue) -> bool {
        if self.bindings.contains_key(name) {
            self.bindings.insert(name.to_string(), val);
            return true;
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow_mut().set(name, val);
        }
        false
    }

    pub fn declare(&mut self, name: &str, val: JsValue) {
        self.bindings.insert(name.to_string(), val);
    }
}

/// Represents an executable JavaScript function closure capturing its lexical environment.
#[derive(Clone)]
pub struct JsClosure {
    pub id: u64,
    pub name: Option<String>,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub env: Rc<RefCell<Scope>>,
    pub is_async: bool,
    pub is_generator: bool,
}

pub const MAX_CALL_STACK_DEPTH: usize = 128;

pub fn bind_parameters(params: &[String], args: Vec<JsValue>, scope: &Rc<RefCell<Scope>>) {
    let mut arg_iter = args.into_iter();
    for param in params {
        if let Some(rest_name) = param.strip_prefix("...") {
            let rest_vals: Vec<JsValue> = arg_iter.by_ref().collect();
            scope.borrow_mut().declare(rest_name, JsValue::new_array(rest_vals));
            break;
        } else {
            let val = arg_iter.next().unwrap_or(JsValue::Undefined);
            scope.borrow_mut().declare(param, val);
        }
    }
}
