//! Pure-Rust JavaScript Execution Engine, AST Parser, and W3C DOM Bindings.

pub mod ast;
pub mod dom;
pub mod eval;
pub mod parser;
pub mod scope;

pub use ast::*;
pub use dom::*;
pub use parser::*;
pub use scope::*;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use crate::promise::{JsPromise, PromiseState, RejectFn, ResolveFn};
use crate::{ClosureHandle, JsValue, SymbolId};

pub type WeakObjectMap = HashMap<u64, std::rc::Weak<RefCell<BTreeMap<String, JsValue>>>>;

pub struct JsEngine {
    pub globals: BTreeMap<String, JsValue>,
    pub root_scope: Rc<RefCell<Scope>>,
    pub current_scope: Rc<RefCell<Scope>>,
    pub document: Document,
    pub console_logs: Vec<String>,
    pub user_functions: HashMap<String, JsClosure>,
    pub closures: HashMap<u64, JsClosure>,
    pub next_closure_id: u64,
    pub call_depth: usize,
    pub live_nodes: RefCell<HashMap<u64, DomNode>>,
    pub anon_counter: usize,
    pub classes: HashMap<String, ClassDef>,
    pub promises: HashMap<u64, JsPromise<JsValue>>,
    pub promise_controllers: HashMap<u64, (ResolveFn<JsValue>, RejectFn)>,
    pub next_promise_id: u64,
    pub pending_throw: Option<JsValue>,
    pub next_symbol_id: u64,
    pub symbol_registry: HashMap<String, SymbolId>,
    pub object_prototype: BTreeMap<String, JsValue>,
    pub array_prototype: BTreeMap<String, JsValue>,
    pub function_prototype: BTreeMap<String, JsValue>,
    pub string_prototype: BTreeMap<String, JsValue>,
    pub error_prototype: BTreeMap<String, JsValue>,
    pub memory_limit_bytes: Option<usize>,
    pub allocated_memory_bytes: usize,
    pub freeze_prototypes: bool,
    pub is_strict: bool,
    pub weak_maps: RefCell<HashMap<u64, HashMap<usize, JsValue>>>,
    pub next_weak_id: u64,
    pub weak_sets: RefCell<HashMap<u64, HashSet<usize>>>,
    pub weak_refs: RefCell<WeakObjectMap>,
    pub generators: RefCell<HashMap<u64, GeneratorInstance>>,
    pub next_generator_id: u64,
    pub pending_yield: Option<JsValue>,
}

#[derive(Clone)]
pub struct GeneratorInstance {
    pub closure: JsClosure,
    pub scope: Rc<RefCell<Scope>>,
    pub pc: usize,
    pub done: bool,
}

impl Default for JsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl JsEngine {
    pub fn new() -> Self {
        Self::with_document(Document::new())
    }

    pub fn with_document(doc: Document) -> Self {
        let mut globals = BTreeMap::new();
        globals.insert("window".to_string(), JsValue::empty_object());
        globals.insert("document".to_string(), JsValue::empty_object());
        globals.insert("console".to_string(), JsValue::empty_object());
        let mut math_obj = BTreeMap::new();
        math_obj.insert("PI".to_string(), JsValue::Number(std::f64::consts::PI));
        math_obj.insert("E".to_string(), JsValue::Number(std::f64::consts::E));
        globals.insert("Math".to_string(), JsValue::new_object(math_obj));
        globals.insert("JSON".to_string(), JsValue::empty_object());

        let mut promise_obj = BTreeMap::new();
        promise_obj.insert("__is_class__".to_string(), JsValue::Bool(true));
        promise_obj.insert("__class_name__".to_string(), JsValue::String("Promise".to_string()));
        globals.insert("Promise".to_string(), JsValue::new_object(promise_obj));

        let mut regex_obj = BTreeMap::new();
        regex_obj.insert("__fn__".to_string(), JsValue::String("RegExp".to_string()));
        globals.insert("RegExp".to_string(), JsValue::new_object(regex_obj));

        let mut fetch_obj = BTreeMap::new();
        fetch_obj.insert("__fn__".to_string(), JsValue::String("fetch".to_string()));
        globals.insert("fetch".to_string(), JsValue::new_object(fetch_obj));

        // Built-in prototypes (W3C / ECMAScript specification)
        let mut object_prototype = BTreeMap::new();
        object_prototype.insert(
            "hasOwnProperty".to_string(),
            JsValue::Function(ClosureHandle::new(0, Some("hasOwnProperty".to_string()))),
        );
        object_prototype.insert(
            "toString".to_string(),
            JsValue::Function(ClosureHandle::new(0, Some("toString".to_string()))),
        );
        object_prototype.insert(
            "valueOf".to_string(),
            JsValue::Function(ClosureHandle::new(0, Some("valueOf".to_string()))),
        );

        let mut array_prototype = BTreeMap::new();
        array_prototype.insert("__proto__".to_string(), JsValue::new_object(object_prototype.clone()));

        let mut function_prototype = BTreeMap::new();
        function_prototype.insert("__proto__".to_string(), JsValue::new_object(object_prototype.clone()));

        let mut string_prototype = BTreeMap::new();
        string_prototype.insert("__proto__".to_string(), JsValue::new_object(object_prototype.clone()));

        let mut error_prototype = BTreeMap::new();
        error_prototype.insert("name".to_string(), JsValue::String("Error".to_string()));
        error_prototype.insert("message".to_string(), JsValue::String(String::new()));
        error_prototype.insert("__proto__".to_string(), JsValue::new_object(object_prototype.clone()));

        // Constructors with .prototype linkage
        let mut obj_ctor = BTreeMap::new();
        obj_ctor.insert("prototype".to_string(), JsValue::new_object(object_prototype.clone()));
        obj_ctor.insert("__fn__".to_string(), JsValue::String("Object".to_string()));
        globals.insert("Object".to_string(), JsValue::new_object(obj_ctor));

        let mut arr_ctor = BTreeMap::new();
        arr_ctor.insert("prototype".to_string(), JsValue::new_object(array_prototype.clone()));
        arr_ctor.insert("__fn__".to_string(), JsValue::String("Array".to_string()));
        globals.insert("Array".to_string(), JsValue::new_object(arr_ctor));

        let mut fn_ctor = BTreeMap::new();
        fn_ctor.insert("prototype".to_string(), JsValue::new_object(function_prototype.clone()));
        fn_ctor.insert("__fn__".to_string(), JsValue::String("Function".to_string()));
        globals.insert("Function".to_string(), JsValue::new_object(fn_ctor));

        let mut str_ctor = BTreeMap::new();
        str_ctor.insert("prototype".to_string(), JsValue::new_object(string_prototype.clone()));
        str_ctor.insert("__fn__".to_string(), JsValue::String("String".to_string()));
        globals.insert("String".to_string(), JsValue::new_object(str_ctor));

        let mut sym_ctor = BTreeMap::new();
        sym_ctor.insert("iterator".to_string(), JsValue::Symbol(SymbolId::iterator()));
        sym_ctor.insert("toStringTag".to_string(), JsValue::Symbol(SymbolId::to_string_tag()));
        sym_ctor.insert("hasInstance".to_string(), JsValue::Symbol(SymbolId::has_instance()));
        sym_ctor.insert("toPrimitive".to_string(), JsValue::Symbol(SymbolId::to_primitive()));
        sym_ctor.insert("__fn__".to_string(), JsValue::String("Symbol".to_string()));
        globals.insert("Symbol".to_string(), JsValue::new_object(sym_ctor));

        // BigInt built-in
        let mut bigint_obj = BTreeMap::new();
        bigint_obj.insert("__fn__".to_string(), JsValue::String("BigInt".to_string()));
        globals.insert("BigInt".to_string(), JsValue::new_object(bigint_obj));

        // Proxy and Reflect built-ins
        let mut proxy_obj = BTreeMap::new();
        proxy_obj.insert("__fn__".to_string(), JsValue::String("Proxy".to_string()));
        proxy_obj.insert("__is_class__".to_string(), JsValue::Bool(true));
        proxy_obj.insert("__class_name__".to_string(), JsValue::String("Proxy".to_string()));
        globals.insert("Proxy".to_string(), JsValue::new_object(proxy_obj));

        let mut reflect_obj = BTreeMap::new();
        reflect_obj.insert("__is_reflect__".to_string(), JsValue::Bool(true));
        globals.insert("Reflect".to_string(), JsValue::new_object(reflect_obj));

        // Weak collections
        for weak_type in &["WeakMap", "WeakSet", "WeakRef"] {
            let mut weak_ctor = BTreeMap::new();
            weak_ctor.insert("__fn__".to_string(), JsValue::String(weak_type.to_string()));
            weak_ctor.insert("__is_class__".to_string(), JsValue::Bool(true));
            weak_ctor.insert("__class_name__".to_string(), JsValue::String(weak_type.to_string()));
            globals.insert(weak_type.to_string(), JsValue::new_object(weak_ctor));
        }

        for err_type in &["Error", "TypeError", "ReferenceError", "SyntaxError", "RangeError"] {
            let mut err_ctor = BTreeMap::new();
            let mut proto = error_prototype.clone();
            proto.insert("name".to_string(), JsValue::String(err_type.to_string()));
            err_ctor.insert("prototype".to_string(), JsValue::new_object(proto));
            err_ctor.insert("__fn__".to_string(), JsValue::String(err_type.to_string()));
            globals.insert(err_type.to_string(), JsValue::new_object(err_ctor));
        }

        let root_scope = Scope::new_root();
        for (k, v) in &globals {
            root_scope.borrow_mut().declare(k, v.clone());
        }

        Self {
            globals,
            root_scope: root_scope.clone(),
            current_scope: root_scope,
            document: doc,
            console_logs: Vec::new(),
            user_functions: HashMap::new(),
            closures: HashMap::new(),
            next_closure_id: 1,
            call_depth: 0,
            live_nodes: RefCell::new(HashMap::new()),
            anon_counter: 0,
            classes: HashMap::new(),
            promises: HashMap::new(),
            promise_controllers: HashMap::new(),
            next_promise_id: 1,
            pending_throw: None,
            next_symbol_id: 10, // 1..=4 reserved for well-known symbols
            symbol_registry: HashMap::new(),
            object_prototype,
            array_prototype,
            function_prototype,
            string_prototype,
            error_prototype,
            memory_limit_bytes: None,
            allocated_memory_bytes: 0,
            freeze_prototypes: false,
            is_strict: false,
            weak_maps: RefCell::new(HashMap::new()),
            next_weak_id: 1,
            weak_sets: RefCell::new(HashMap::new()),
            weak_refs: RefCell::new(HashMap::new()),
            generators: RefCell::new(HashMap::new()),
            next_generator_id: 1,
            pending_yield: None,
        }
    }

    /// Sets a strict memory allocation threshold in bytes (e.g. 64MB for sandbox).
    pub fn with_memory_limit(mut self, limit_bytes: usize) -> Self {
        self.memory_limit_bytes = Some(limit_bytes);
        self
    }

    /// Enables or disables prototype freezing to prevent prototype pollution.
    pub fn set_freeze_prototypes(&mut self, freeze: bool) {
        self.freeze_prototypes = freeze;
    }

    /// Tracks heap memory allocations and enforces memory limit quotas.
    pub fn allocate_bytes(&mut self, bytes: usize) -> Result<(), String> {
        if let Some(limit) = self.memory_limit_bytes {
            if self.allocated_memory_bytes.saturating_add(bytes) > limit {
                return Err("RangeError: Script memory allocation quota exceeded".to_string());
            }
        }
        self.allocated_memory_bytes = self.allocated_memory_bytes.saturating_add(bytes);
        Ok(())
    }

    pub fn prune_dead_closures(&mut self) {
        let mut visited_closure_ids = HashSet::new();
        let mut visited_scopes = HashSet::new();

        // 1. Mark from globals
        for val in self.globals.values() {
            self.mark_value_for_closures(val, &mut visited_closure_ids, &mut visited_scopes);
        }

        // 2. Mark from root_scope and current_scope
        self.mark_scope_for_closures(&self.root_scope, &mut visited_closure_ids, &mut visited_scopes);
        self.mark_scope_for_closures(&self.current_scope, &mut visited_closure_ids, &mut visited_scopes);

        // 3. Mark from user_functions
        for closure in self.user_functions.values() {
            if closure.id != 0 {
                visited_closure_ids.insert(closure.id);
            }
            self.mark_scope_for_closures(&closure.env, &mut visited_closure_ids, &mut visited_scopes);
        }

        // 4. Mark from document nodes and live_nodes
        self.mark_dom_node_closures(&self.document.html, &mut visited_closure_ids, &mut visited_scopes);
        self.mark_dom_node_closures(&self.document.head, &mut visited_closure_ids, &mut visited_scopes);
        self.mark_dom_node_closures(&self.document.body, &mut visited_closure_ids, &mut visited_scopes);
        for node in self.live_nodes.borrow().values() {
            self.mark_dom_node_closures(node, &mut visited_closure_ids, &mut visited_scopes);
        }

        // 5. Mark from promises
        for promise in self.promises.values() {
            match promise.state() {
                PromiseState::Fulfilled(ref v) | PromiseState::Rejected(ref v) => {
                    self.mark_value_for_closures(v, &mut visited_closure_ids, &mut visited_scopes);
                }
                PromiseState::Pending => {}
            }
        }

        // Retain only reachable closures (plus any newly allocated within a grace threshold of 8 IDs)
        let min_grace_id = self.next_closure_id.saturating_sub(8);
        self.closures.retain(|id, _| *id >= min_grace_id || visited_closure_ids.contains(id));
    }

    fn mark_dom_node_closures(
        &self,
        node: &DomNode,
        visited_ids: &mut HashSet<u64>,
        visited_scopes: &mut HashSet<usize>,
    ) {
        let inner = node.inner.borrow();
        for listeners in inner.event_listeners.values() {
            for cb in listeners {
                self.mark_value_for_closures(cb, visited_ids, visited_scopes);
            }
        }
        for child in &inner.children {
            self.mark_dom_node_closures(child, visited_ids, visited_scopes);
        }
    }

    fn mark_value_for_closures(
        &self,
        val: &JsValue,
        visited_ids: &mut HashSet<u64>,
        visited_scopes: &mut HashSet<usize>,
    ) {
        match val {
            JsValue::Function(handle) => {
                if visited_ids.insert(handle.id) {
                    if let Some(closure) = self.closures.get(&handle.id) {
                        self.mark_scope_for_closures(&closure.env, visited_ids, visited_scopes);
                    }
                }
            }
            JsValue::Object(map) => {
                let b = map.borrow();
                if let Some(JsValue::Number(cid)) = b.get("__closure_id__") {
                    let id = *cid as u64;
                    if visited_ids.insert(id) {
                        if let Some(closure) = self.closures.get(&id) {
                            self.mark_scope_for_closures(&closure.env, visited_ids, visited_scopes);
                        }
                    }
                }
                for v in b.values() {
                    self.mark_value_for_closures(v, visited_ids, visited_scopes);
                }
            }
            JsValue::Array(items) => {
                for item in items.borrow().iter() {
                    self.mark_value_for_closures(item, visited_ids, visited_scopes);
                }
            }
            _ => {}
        }
    }

    fn mark_scope_for_closures(
        &self,
        scope_rc: &Rc<RefCell<Scope>>,
        visited_ids: &mut HashSet<u64>,
        visited_scopes: &mut HashSet<usize>,
    ) {
        let ptr = Rc::as_ptr(scope_rc) as usize;
        if !visited_scopes.insert(ptr) {
            return;
        }

        let scope = scope_rc.borrow();
        for val in scope.bindings.values() {
            self.mark_value_for_closures(val, visited_ids, visited_scopes);
        }
        if let Some(ref parent) = scope.parent {
            self.mark_scope_for_closures(parent, visited_ids, visited_scopes);
        }
    }

    pub fn maybe_prune_closures(&mut self) {
        if self.closures.len() > 64 && self.anon_counter.is_multiple_of(32) {
            self.prune_dead_closures();
        }
    }

    pub fn compile_regex(&self, pattern: &str, flags: &str) -> Result<regex::Regex, String> {
        let mut builder = regex::RegexBuilder::new(pattern);
        if flags.contains('i') {
            builder.case_insensitive(true);
        }
        if flags.contains('m') {
            builder.multi_line(true);
        }
        if flags.contains('s') {
            builder.dot_matches_new_line(true);
        }
        builder.build().map_err(|e| format!("SyntaxError: Invalid regular expression: /{pattern}/{flags}: {e}"))
    }

    pub fn create_regex_value(&self, pattern: &str, flags: &str) -> Result<JsValue, String> {
        let _ = self.compile_regex(pattern, flags)?;

        let mut map = BTreeMap::new();
        map.insert("__is_regex__".to_string(), JsValue::Bool(true));
        map.insert("source".to_string(), JsValue::String(pattern.to_string()));
        map.insert("flags".to_string(), JsValue::String(flags.to_string()));
        map.insert("global".to_string(), JsValue::Bool(flags.contains('g')));
        map.insert("ignoreCase".to_string(), JsValue::Bool(flags.contains('i')));
        map.insert("multiline".to_string(), JsValue::Bool(flags.contains('m')));
        map.insert("dotAll".to_string(), JsValue::Bool(flags.contains('s')));
        map.insert("unicode".to_string(), JsValue::Bool(flags.contains('u')));
        map.insert("sticky".to_string(), JsValue::Bool(flags.contains('y')));
        map.insert("lastIndex".to_string(), JsValue::Number(0.0));
        Ok(JsValue::new_object(map))
    }

    pub fn create_promise_value(&mut self, promise: JsPromise<JsValue>) -> JsValue {
        let pid = self.next_promise_id;
        self.next_promise_id += 1;
        self.promises.insert(pid, promise);
        let mut map = BTreeMap::new();
        map.insert("__is_promise__".to_string(), JsValue::Bool(true));
        map.insert("__promise_id__".to_string(), JsValue::Number(pid as f64));
        JsValue::new_object(map)
    }

    pub fn resolve_constructor(&self, class_name: &str) -> Option<(Vec<String>, Vec<Stmt>, Option<String>)> {
        let cls = self.classes.get(class_name)?;
        if let Some(ref ctor) = cls.constructor {
            Some((ctor.0.clone(), ctor.1.clone(), cls.super_class.clone()))
        } else if let Some(ref super_name) = cls.super_class {
            self.resolve_constructor(super_name)
        } else {
            None
        }
    }

    pub fn resolve_method(&self, class_name: &str, method_name: &str) -> Option<(Vec<String>, Vec<Stmt>, Option<String>)> {
        let mut curr_class = Some(class_name.to_string());
        while let Some(cls_name) = curr_class {
            if let Some(cls) = self.classes.get(&cls_name) {
                if let Some(m) = cls.methods.get(method_name) {
                    return Some((m.0.clone(), m.1.clone(), cls.super_class.clone()));
                }
                curr_class = cls.super_class.clone();
            } else {
                break;
            }
        }
        None
    }

    pub fn is_instance_of(&self, instance: &JsValue, class_name: &str) -> bool {
        if class_name == "Object" {
            return matches!(instance, JsValue::Object(_) | JsValue::Array(_));
        }
        if class_name == "Array" {
            return matches!(instance, JsValue::Array(_));
        }
        if let JsValue::Object(map) = instance {
            let b = map.borrow();
            if class_name == "Error"
                || class_name == "TypeError"
                || class_name == "ReferenceError"
                || class_name == "SyntaxError"
                || class_name == "RangeError"
            {
                if let Some(JsValue::String(name)) = b.get("name") {
                    if name == class_name || class_name == "Error" {
                        return true;
                    }
                }
            }

            if let Some(JsValue::String(inst_class)) = b.get("__class__") {
                let mut curr = Some(inst_class.clone());
                while let Some(c) = curr {
                    if c == class_name {
                        return true;
                    }
                    curr = self.classes.get(&c).and_then(|cd| cd.super_class.clone());
                }
            }

            // Walk __proto__ chain
            let mut cur_proto = b.get("__proto__").cloned();
            let mut depth = 0;
            while let Some(JsValue::Object(p_map)) = cur_proto {
                depth += 1;
                if depth > 64 {
                    break;
                }
                let pb = p_map.borrow();
                if let Some(JsValue::String(p_cls)) = pb.get("__class__") {
                    if p_cls == class_name {
                        return true;
                    }
                }
                if let Some(JsValue::String(p_name)) = pb.get("name") {
                    if p_name == class_name || (class_name == "Error" && p_name.ends_with("Error")) {
                        return true;
                    }
                }
                cur_proto = pb.get("__proto__").cloned();
            }
        }
        false
    }

    pub fn instantiate_class(&mut self, class_name: &str, args: Vec<JsValue>) -> Result<JsValue, String> {
        let _cls = self.classes.get(class_name).cloned().ok_or_else(|| {
            format!("ReferenceError: {class_name} is not defined")
        })?;

        let mut instance_map = BTreeMap::new();
        instance_map.insert("__class__".to_string(), JsValue::String(class_name.to_string()));
        let instance_obj = JsValue::new_object(instance_map);

        if let Some((params, body, super_opt)) = self.resolve_constructor(class_name) {
            let ctor_scope = Scope::new_child(self.current_scope.clone());
            ctor_scope.borrow_mut().declare("this", instance_obj);
            if let Some(super_name) = super_opt {
                ctor_scope.borrow_mut().declare("__super_class__", JsValue::String(super_name));
            }
            ctor_scope.borrow_mut().declare("__current_class__", JsValue::String(class_name.to_string()));

            bind_parameters(&params, args, &ctor_scope);

            let saved_scope = self.current_scope.clone();
            self.current_scope = ctor_scope.clone();

            let mut ret_val = None;
            let mut err = None;
            for stmt in &body {
                match self.execute_stmt_flow(stmt) {
                    Ok(StatementFlow::Normal(_)) => {}
                    Ok(StatementFlow::Return(v)) => {
                        if let JsValue::Object(_) = v {
                            ret_val = Some(v);
                        }
                        break;
                    }
                    Ok(StatementFlow::Throw(v)) => {
                        err = Some(format!("Uncaught exception: {v}"));
                        break;
                    }
                    Ok(StatementFlow::Break(_)) => {
                        err = Some("SyntaxError: Illegal break statement".to_string());
                        break;
                    }
                    Ok(StatementFlow::Continue(_)) => {
                        err = Some("SyntaxError: Illegal continue statement".to_string());
                        break;
                    }
                    Ok(StatementFlow::Yield(v)) => {
                        return Ok(v);
                    }
                    Err(e) => {
                        err = Some(e);
                        break;
                    }
                }
            }

            self.current_scope = saved_scope;
            if let Some(e) = err {
                return Err(e);
            }

            if let Some(ret) = ret_val {
                return Ok(ret);
            }

            let final_this = ctor_scope.borrow().get("this").unwrap_or(JsValue::Undefined);
            Ok(final_this)
        } else {
            Ok(instance_obj)
        }
    }

    pub fn eval(&mut self, script: &str) -> Result<JsValue, String> {
        let mut parser = JsParser::new(script);
        let stmts = parser.parse_program()?;

        for (k, v) in &self.globals {
            self.root_scope.borrow_mut().declare(k, v.clone());
        }
        self.current_scope = self.root_scope.clone();

        let mut result = JsValue::Undefined;
        for stmt in &stmts {
            self.maybe_prune_closures();
            match self.execute_stmt_flow(stmt)? {
                StatementFlow::Normal(v) => result = v,
                StatementFlow::Return(v) | StatementFlow::Yield(v) => {
                    result = v;
                    break;
                }
                StatementFlow::Throw(err) => {
                    return Err(format!("Uncaught exception: {err}"));
                }
                StatementFlow::Break(_) => {
                    return Err("SyntaxError: Illegal break statement".to_string());
                }
                StatementFlow::Continue(_) => {
                    return Err("SyntaxError: Illegal continue statement".to_string());
                }
            }
        }

        for (k, v) in &self.root_scope.borrow().bindings {
            self.globals.insert(k.clone(), v.clone());
        }

        self.prune_dead_closures();

        Ok(result)
    }


    pub fn dom_node_to_js(&self, node: &DomNode) -> JsValue {
        self.live_nodes.borrow_mut().insert(node.uid(), node.clone());
        let mut map = BTreeMap::new();
        map.insert("__uid__".to_string(), JsValue::Number(node.uid() as f64));
        let id_val = node.id().unwrap_or_else(|| format!("dom_node_{}", node.uid()));
        map.insert("__dom_id__".to_string(), JsValue::String(id_val));
        map.insert(
            "id".to_string(),
            JsValue::String(node.id().unwrap_or_default()),
        );
        map.insert(
            "tagName".to_string(),
            JsValue::String(node.tag_name().unwrap_or_default()),
        );
        map.insert(
            "className".to_string(),
            JsValue::String(node.class_name()),
        );
        map.insert(
            "innerHTML".to_string(),
            JsValue::String(node.inner_html()),
        );
        map.insert(
            "textContent".to_string(),
            JsValue::String(node.text_content()),
        );
        map.insert(
            "value".to_string(),
            JsValue::String(node.get_attribute("value").unwrap_or_default()),
        );
        JsValue::new_object(map)
    }

    pub fn create_error_from_string(&self, err_msg: &str) -> JsValue {
        let (err_name, msg) = if let Some(idx) = err_msg.find(": ") {
            let prefix = &err_msg[..idx];
            if matches!(prefix, "TypeError" | "ReferenceError" | "SyntaxError" | "RangeError" | "Error") {
                (prefix.to_string(), err_msg[idx + 2..].to_string())
            } else {
                ("Error".to_string(), err_msg.to_string())
            }
        } else {
            ("Error".to_string(), err_msg.to_string())
        };

        let mut map = BTreeMap::new();
        map.insert("__is_error__".to_string(), JsValue::Bool(true));
        map.insert("name".to_string(), JsValue::String(err_name.clone()));
        map.insert("message".to_string(), JsValue::String(msg.clone()));
        map.insert("stack".to_string(), JsValue::String(format!("{err_name}: {msg}\n    at strata_js")));
        map.insert("__proto__".to_string(), JsValue::new_object(self.error_prototype.clone()));
        JsValue::new_object(map)
    }
}

pub fn days_from_civil(mut y: i64, m: u64, d: u64) -> i64 {
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m_adj = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * m_adj + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + (doe as i64) - 719468
}

pub fn civil_from_days(z: i64) -> (i64, u64, u64) {
    let z_adj = z + 719468;
    let era = if z_adj >= 0 { z_adj } else { z_adj - 146096 } / 146097;
    let doe = (z_adj - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    (y, m, d)
}

pub fn ms_to_date(ms: f64) -> (i64, u64, u64, u64, u64, u64, u64) {
    if ms.is_nan() || ms.is_infinite() {
        return (1970, 0, 1, 0, 0, 0, 0);
    }
    let total_ms = ms as i64;
    let days = total_ms.div_euclid(86_400_000);
    let rem_ms = total_ms.rem_euclid(86_400_000);
    let (y, m, d) = civil_from_days(days);
    let hours = (rem_ms / 3_600_000) as u64;
    let rem_hours = rem_ms % 3_600_000;
    let minutes = (rem_hours / 60_000) as u64;
    let rem_min = rem_hours % 60_000;
    let seconds = (rem_min / 1_000) as u64;
    let millis = (rem_min % 1_000) as u64;
    (y, m.saturating_sub(1), d, hours, minutes, seconds, millis)
}

pub fn date_to_ms(year: f64, month: f64, day: f64, hours: f64, minutes: f64, seconds: f64, ms: f64) -> f64 {
    let y = year as i64;
    let m = (month as u64) + 1;
    let d = (day as u64).max(1);
    let days = days_from_civil(y, m, d);
    let total_ms = days * 86_400_000
        + (hours as i64) * 3_600_000
        + (minutes as i64) * 60_000
        + (seconds as i64) * 1_000
        + (ms as i64);
    total_ms as f64
}
