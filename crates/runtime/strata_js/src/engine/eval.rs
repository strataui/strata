//! JavaScript Statement, Expression, and Function Evaluator.

use super::ast::*;
use super::dom::*;
use super::scope::*;
use super::{JsEngine, MAX_EVAL_DEPTH};
use crate::json::{parse_json, stringify_json};
use crate::promise::{drain_microtasks, JsPromise, PromiseState};
use crate::{ClosureHandle, JsValue, SymbolId};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Maximum loop iterations before triggering execution timeout guard to prevent infinite loop DoS.
pub const MAX_LOOP_ITERATIONS: usize = 1_000_000;

impl JsEngine {
    pub fn execute_statement(&mut self, stmt: &Stmt) -> Result<JsValue, String> {
        match self.execute_stmt_flow(stmt)? {
            StatementFlow::Normal(v) | StatementFlow::Return(v) | StatementFlow::Yield(v) => Ok(v),
            StatementFlow::Throw(err) => Err(format!("Uncaught exception: {err}")),
            StatementFlow::Break(_) => Err("SyntaxError: Illegal break statement".to_string()),
            StatementFlow::Continue(_) => {
                Err("SyntaxError: Illegal continue statement".to_string())
            }
        }
    }

    fn execute_block_flow(&mut self, stmts: &[Stmt]) -> Result<StatementFlow, String> {
        let mut res = JsValue::Undefined;
        for s in stmts {
            match self.execute_stmt_flow(s)? {
                StatementFlow::Normal(v) => res = v,
                early => return Ok(early),
            }
        }
        Ok(StatementFlow::Normal(res))
    }

    pub fn execute_stmt_flow(&mut self, stmt: &Stmt) -> Result<StatementFlow, String> {
        match stmt {
            Stmt::Directive(ref d) => {
                if d == "use strict" {
                    self.is_strict = true;
                }
                Ok(StatementFlow::Normal(JsValue::String(d.clone())))
            }
            Stmt::VarDecl { name, init } => {
                let val = match init {
                    Some(expr) => self.eval_expr(expr)?,
                    None => JsValue::Undefined,
                };
                if let Some(err) = self.pending_throw.take() {
                    return Ok(StatementFlow::Throw(err));
                }
                self.current_scope.borrow_mut().declare(name, val.clone());
                if self.current_scope.borrow().parent.is_none() {
                    self.globals.insert(name.clone(), val);
                }
                Ok(StatementFlow::Normal(JsValue::Undefined))
            }
            Stmt::DestructureDecl { pattern, init } => {
                let val = self.eval_expr(init)?;
                if let Some(err) = self.pending_throw.take() {
                    return Ok(StatementFlow::Throw(err));
                }
                match pattern {
                    DestructurePattern::Object { properties, rest } => {
                        let mut matched_keys = HashSet::new();
                        for prop in properties {
                            matched_keys.insert(prop.source_key.clone());
                            let mut prop_val = match &val {
                                JsValue::Object(cell) => cell
                                    .borrow()
                                    .get(&prop.source_key)
                                    .cloned()
                                    .unwrap_or(JsValue::Undefined),
                                _ => JsValue::Undefined,
                            };
                            if matches!(prop_val, JsValue::Undefined) {
                                if let Some(ref def_expr) = prop.default_value {
                                    prop_val = self.eval_expr(def_expr)?;
                                }
                            }
                            self.current_scope
                                .borrow_mut()
                                .declare(&prop.target_name, prop_val.clone());
                            if self.current_scope.borrow().parent.is_none() {
                                self.globals.insert(prop.target_name.clone(), prop_val);
                            }
                        }
                        if let Some(rest_name) = rest {
                            let mut rest_map = BTreeMap::new();
                            if let JsValue::Object(cell) = &val {
                                for (k, v) in cell.borrow().iter() {
                                    if !matched_keys.contains(k) && k != "__proto__" {
                                        rest_map.insert(k.clone(), v.clone());
                                    }
                                }
                            }
                            let rest_val = JsValue::new_object(rest_map);
                            self.current_scope
                                .borrow_mut()
                                .declare(rest_name, rest_val.clone());
                            if self.current_scope.borrow().parent.is_none() {
                                self.globals.insert(rest_name.clone(), rest_val);
                            }
                        }
                    }
                    DestructurePattern::Array { elements, rest } => {
                        let mut idx = 0;
                        for elem_opt in elements {
                            if let Some(elem) = elem_opt {
                                let mut elem_val = match &val {
                                    JsValue::Array(cell) => cell
                                        .borrow()
                                        .get(idx)
                                        .cloned()
                                        .unwrap_or(JsValue::Undefined),
                                    JsValue::String(s) => s
                                        .chars()
                                        .nth(idx)
                                        .map(|c| JsValue::String(c.to_string()))
                                        .unwrap_or(JsValue::Undefined),
                                    _ => JsValue::Undefined,
                                };
                                if matches!(elem_val, JsValue::Undefined) {
                                    if let Some(ref def_expr) = elem.default_value {
                                        elem_val = self.eval_expr(def_expr)?;
                                    }
                                }
                                self.current_scope
                                    .borrow_mut()
                                    .declare(&elem.target_name, elem_val.clone());
                                if self.current_scope.borrow().parent.is_none() {
                                    self.globals.insert(elem.target_name.clone(), elem_val);
                                }
                            }
                            idx += 1;
                        }
                        if let Some(rest_name) = rest {
                            let rest_items = match &val {
                                JsValue::Array(cell) => {
                                    let b = cell.borrow();
                                    if idx < b.len() {
                                        b[idx..].to_vec()
                                    } else {
                                        Vec::new()
                                    }
                                }
                                _ => Vec::new(),
                            };
                            let rest_val = JsValue::new_array(rest_items);
                            self.current_scope
                                .borrow_mut()
                                .declare(rest_name, rest_val.clone());
                            if self.current_scope.borrow().parent.is_none() {
                                self.globals.insert(rest_name.clone(), rest_val);
                            }
                        }
                    }
                }
                Ok(StatementFlow::Normal(JsValue::Undefined))
            }
            Stmt::Assign { target, value } => {
                let val = self.eval_expr(value)?;
                if let Some(err) = self.pending_throw.take() {
                    return Ok(StatementFlow::Throw(err));
                }
                self.assign_target(target, val.clone())?;
                Ok(StatementFlow::Normal(val))
            }
            Stmt::Expr(expr) => {
                let val = self.eval_expr(expr)?;
                if let Some(err) = self.pending_throw.take() {
                    return Ok(StatementFlow::Throw(err));
                }
                Ok(StatementFlow::Normal(val))
            }
            Stmt::Block(stmts) => self.execute_block_flow(stmts),
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.eval_expr(cond)?;
                if c.is_truthy() {
                    self.execute_stmt_flow(then_branch)
                } else if let Some(e) = else_branch {
                    self.execute_stmt_flow(e)
                } else {
                    Ok(StatementFlow::Normal(JsValue::Undefined))
                }
            }
            Stmt::While { cond, body } => {
                let mut res = JsValue::Undefined;
                let mut iterations = 0;
                while self.eval_expr(cond)?.is_truthy() {
                    iterations += 1;
                    if iterations > MAX_LOOP_ITERATIONS {
                        return Err("RangeError: Maximum loop iteration limit exceeded (execution timed out)".to_string());
                    }
                    match self.execute_stmt_flow(body)? {
                        StatementFlow::Normal(v) => res = v,
                        StatementFlow::Continue(_) => continue,
                        StatementFlow::Break(_) => break,
                        early => return Ok(early),
                    }
                }
                Ok(StatementFlow::Normal(res))
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                if let Some(i) = init {
                    match self.execute_stmt_flow(i)? {
                        StatementFlow::Normal(_) => {}
                        early => return Ok(early),
                    }
                }
                let mut res = JsValue::Undefined;
                let mut iterations = 0;
                loop {
                    iterations += 1;
                    if iterations > MAX_LOOP_ITERATIONS {
                        return Err("RangeError: Maximum loop iteration limit exceeded (execution timed out)".to_string());
                    }
                    if let Some(c) = cond {
                        if !self.eval_expr(c)?.is_truthy() {
                            break;
                        }
                    }
                    match self.execute_stmt_flow(body)? {
                        StatementFlow::Normal(v) => res = v,
                        StatementFlow::Continue(_) => {
                            if let Some(s) = step {
                                self.eval_expr(s)?;
                            }
                            continue;
                        }
                        StatementFlow::Break(_) => break,
                        early => return Ok(early),
                    }
                    if let Some(s) = step {
                        self.eval_expr(s)?;
                    }
                    self.maybe_prune_closures();
                }
                Ok(StatementFlow::Normal(res))
            }
            Stmt::FunctionDecl {
                name,
                params,
                body,
                is_async,
                is_generator,
            } => {
                let closure_id = self.next_closure_id;
                self.next_closure_id += 1;
                let closure = JsClosure {
                    id: closure_id,
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: body.clone(),
                    env: self.current_scope.clone(),
                    is_async: *is_async,
                    is_generator: *is_generator,
                };
                let fn_val = JsValue::Function(ClosureHandle::new(closure_id, Some(name.clone())));
                self.closures.insert(closure_id, closure.clone());
                self.current_scope
                    .borrow_mut()
                    .declare(name, fn_val.clone());
                if self.current_scope.borrow().parent.is_none() {
                    self.globals.insert(name.clone(), fn_val);
                    self.user_functions.insert(name.clone(), closure);
                }
                Ok(StatementFlow::Normal(JsValue::Undefined))
            }
            Stmt::ClassDecl {
                name,
                super_class,
                constructor,
                methods,
            } => {
                let mut methods_map = HashMap::new();
                for (m_name, params, body) in methods {
                    methods_map.insert(m_name.clone(), (params.clone(), body.clone()));
                }
                self.classes.insert(
                    name.clone(),
                    ClassDef {
                        name: name.clone(),
                        super_class: super_class.clone(),
                        constructor: constructor.clone(),
                        methods: methods_map,
                    },
                );
                let mut class_meta = BTreeMap::new();
                class_meta.insert("__class_name__".to_string(), JsValue::String(name.clone()));
                class_meta.insert("__is_class__".to_string(), JsValue::Bool(true));
                let class_val = JsValue::new_object(class_meta);
                self.current_scope
                    .borrow_mut()
                    .declare(name, class_val.clone());
                if self.current_scope.borrow().parent.is_none() {
                    self.globals.insert(name.clone(), class_val);
                }
                Ok(StatementFlow::Normal(JsValue::Undefined))
            }
            Stmt::Return(expr_opt) => {
                let val = match expr_opt {
                    Some(e) => self.eval_expr(e)?,
                    None => JsValue::Undefined,
                };
                if let Some(err) = self.pending_throw.take() {
                    return Ok(StatementFlow::Throw(err));
                }
                Ok(StatementFlow::Return(val))
            }
            Stmt::Throw(expr) => {
                let val = self.eval_expr(expr)?;
                Ok(StatementFlow::Throw(val))
            }
            Stmt::TryCatch {
                try_block,
                catch_param,
                catch_block,
                finally_block,
            } => {
                let mut flow = match self.execute_block_flow(try_block) {
                    Ok(f) => f,
                    Err(e) => {
                        let err_obj = self.create_error_from_string(&e);
                        StatementFlow::Throw(err_obj)
                    }
                };
                if let StatementFlow::Throw(ref thrown_val) = flow {
                    let thrown_val = thrown_val.clone();
                    if let Some(catch_stmts) = catch_block {
                        let catch_scope = Scope::new_child(self.current_scope.clone());
                        if let Some(ref param_name) = catch_param {
                            catch_scope.borrow_mut().declare(param_name, thrown_val);
                        }
                        let saved_scope = self.current_scope.clone();
                        self.current_scope = catch_scope;
                        let catch_flow = self.execute_block_flow(catch_stmts);
                        self.current_scope = saved_scope;
                        flow = catch_flow?;
                    }
                }
                if let Some(finally_stmts) = finally_block {
                    let finally_flow = self.execute_block_flow(finally_stmts)?;
                    if let StatementFlow::Return(_) | StatementFlow::Throw(_) = finally_flow {
                        flow = finally_flow;
                    }
                }
                Ok(flow)
            }
            Stmt::Break(label) => Ok(StatementFlow::Break(label.clone())),
            Stmt::Continue(label) => Ok(StatementFlow::Continue(label.clone())),
            Stmt::Switch {
                discriminant,
                cases,
            } => {
                let disc_val = self.eval_expr(discriminant)?;
                let mut matched_index = None;
                let mut default_index = None;

                for (i, case) in cases.iter().enumerate() {
                    if let Some(test_expr) = &case.test {
                        let test_val = self.eval_expr(test_expr)?;
                        if disc_val.strict_eq(&test_val) {
                            matched_index = Some(i);
                            break;
                        }
                    } else if default_index.is_none() {
                        default_index = Some(i);
                    }
                }

                let start_index = matched_index.or(default_index);
                let mut res = JsValue::Undefined;

                if let Some(start) = start_index {
                    for case in &cases[start..] {
                        for stmt in &case.consequent {
                            match self.execute_stmt_flow(stmt)? {
                                StatementFlow::Normal(v) => res = v,
                                StatementFlow::Break(_) => return Ok(StatementFlow::Normal(res)),
                                StatementFlow::Continue(lbl) => {
                                    return Ok(StatementFlow::Continue(lbl))
                                }
                                StatementFlow::Return(v) => return Ok(StatementFlow::Return(v)),
                                StatementFlow::Throw(v) => return Ok(StatementFlow::Throw(v)),
                                StatementFlow::Yield(v) => return Ok(StatementFlow::Yield(v)),
                            }
                        }
                    }
                }

                Ok(StatementFlow::Normal(res))
            }
        }
    }

    fn assign_target(&mut self, target: &Expr, val: JsValue) -> Result<(), String> {
        match target {
            Expr::Identifier(name) => {
                if self.is_strict && !self.current_scope.borrow().has(name) {
                    return Err(format!("ReferenceError: {name} is not defined"));
                }
                if !self.current_scope.borrow_mut().set(name, val.clone()) {
                    self.current_scope.borrow_mut().declare(name, val.clone());
                }
                if self.current_scope.borrow().parent.is_none() {
                    self.globals.insert(name.clone(), val);
                }
                Ok(())
            }
            Expr::Member {
                object,
                property,
                computed,
            } => {
                let key = if let Some(comp_expr) = computed {
                    let prop_val = self.eval_expr(comp_expr)?;
                    match prop_val {
                        JsValue::String(s) => s,
                        JsValue::Number(n) => format!("{n}"),
                        JsValue::BigInt(bi) => bi.to_decimal_string(),
                        other => other.to_string(),
                    }
                } else {
                    property.clone()
                };

                // Check built-in prototype mutation guard
                if let Expr::Member {
                    object: ref base,
                    property: ref base_prop,
                    ..
                } = &**object
                {
                    if let Expr::Identifier(ref ctor_id) = &**base {
                        if base_prop == "prototype" {
                            if self.freeze_prototypes {
                                return Err(format!(
                                    "TypeError: Cannot modify frozen prototype {ctor_id}.prototype"
                                ));
                            }
                            if !self.allow_prototype_mutation
                                && matches!(
                                    ctor_id.as_str(),
                                    "Object" | "Array" | "Function" | "String" | "Error"
                                )
                            {
                                return Err(format!(
                                    "TypeError: Cannot modify sealed prototype {ctor_id}.prototype"
                                ));
                            }
                            match ctor_id.as_str() {
                                "Array" => {
                                    self.array_prototype.insert(key.clone(), val.clone());
                                }
                                "Object" => {
                                    self.object_prototype.insert(key.clone(), val.clone());
                                }
                                "Function" => {
                                    self.function_prototype.insert(key.clone(), val.clone());
                                }
                                "String" => {
                                    self.string_prototype.insert(key.clone(), val.clone());
                                }
                                "Error" => {
                                    self.error_prototype.insert(key.clone(), val.clone());
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // Document / DOM element shortcuts
                if let Expr::Identifier(id) = &**object {
                    if id == "document" && key == "title" {
                        if let Some(s) = val.as_str() {
                            *self.document.title.borrow_mut() = s.to_string();
                            self.document.mark_mutated();
                        }
                        return Ok(());
                    }
                }
                if let Expr::Member {
                    object: parent_obj,
                    property: parent_prop,
                    ..
                } = &**object
                {
                    if let Expr::Identifier(id) = &**parent_obj {
                        if id == "document" && parent_prop == "body" {
                            if key == "innerHTML" {
                                if let Some(s) = val.as_str() {
                                    self.document.body.set_inner_html(s);
                                    self.document.mark_mutated();
                                }
                                return Ok(());
                            } else if key == "textContent" {
                                if let Some(s) = val.as_str() {
                                    self.document.body.set_text_content(s);
                                    self.document.mark_mutated();
                                }
                                return Ok(());
                            }
                        }
                    }
                }

                let obj = self.eval_expr(object)?;

                // Check DOM node on object
                if let JsValue::Object(ref cell) = obj {
                    if let Some(node) = self.resolve_dom_node(&cell.borrow()) {
                        if key == "innerHTML" {
                            if let Some(s) = val.as_str() {
                                node.set_inner_html(s);
                                self.document.mark_mutated();
                            }
                            return Ok(());
                        } else if key == "textContent" {
                            if let Some(s) = val.as_str() {
                                node.set_text_content(s);
                                self.document.mark_mutated();
                            }
                            return Ok(());
                        } else if key == "className" {
                            if let Some(s) = val.as_str() {
                                node.set_class_name(s);
                                self.document.mark_mutated();
                            }
                            return Ok(());
                        } else if key == "id" {
                            if let Some(s) = val.as_str() {
                                node.set_attribute("id", s);
                                self.document.mark_mutated();
                            }
                            return Ok(());
                        } else if key == "value" {
                            if let Some(s) = val.as_str() {
                                node.set_attribute("value", s);
                                self.document.mark_mutated();
                            }
                            return Ok(());
                        } else if key.starts_with("on") && key.len() > 2 {
                            let ev_name = key[2..].to_ascii_lowercase();
                            node.add_event_listener(&ev_name, val);
                            return Ok(());
                        }
                    }
                }

                if key == "__proto__" && (self.freeze_prototypes || !self.allow_prototype_mutation)
                {
                    return Err("TypeError: Cannot modify prototype in sealed mode".to_string());
                }

                // Check Proxy set trap
                if let JsValue::Object(ref cell) = obj {
                    let proxy_info = {
                        let b = cell.borrow();
                        if b.get("__is_proxy__") == Some(&JsValue::Bool(true)) {
                            let target = b.get("__proxy_target__").cloned();
                            let handler = b.get("__proxy_handler__").cloned();
                            Some((target, handler))
                        } else {
                            None
                        }
                    };
                    if let Some((Some(target), Some(handler))) = proxy_info {
                        if let Some(set_trap) = handler.get_prop("set") {
                            if set_trap.is_function() {
                                let receiver = obj.clone();
                                let trap_args =
                                    vec![target.clone(), JsValue::String(key), val, receiver];
                                let _ =
                                    self.call_value_with_this(&set_trap, trap_args, Some(handler))?;
                                return Ok(());
                            }
                        }
                        target.set_prop(key, val);
                        return Ok(());
                    }
                }

                // Mutate Object
                if let JsValue::Object(ref cell) = obj {
                    cell.borrow_mut().insert(key, val);
                    return Ok(());
                }

                // Mutate Array
                if let JsValue::Array(ref cell) = obj {
                    if key == "length" {
                        if let Some(new_len) = val.as_f64() {
                            let mut b = cell.borrow_mut();
                            b.resize(new_len as usize, JsValue::Undefined);
                        }
                        return Ok(());
                    }
                    if let Ok(idx) = key.parse::<usize>() {
                        let mut b = cell.borrow_mut();
                        if idx >= b.len() {
                            b.resize(idx + 1, JsValue::Undefined);
                        }
                        b[idx] = val;
                        return Ok(());
                    }
                    return Ok(());
                }

                if matches!(obj, JsValue::Null | JsValue::Undefined) {
                    return Err(format!(
                        "TypeError: Cannot set properties of {} (setting '{}')",
                        obj.js_typeof(),
                        key
                    ));
                }

                Ok(())
            }
            Expr::Array(items) => {
                let mut idx = 0;
                for item in items {
                    if matches!(item, Expr::Literal(JsValue::Undefined)) {
                        idx += 1;
                        continue;
                    }
                    if let Expr::Spread(inner) = item {
                        let rest_items = match &val {
                            JsValue::Array(cell) => {
                                let b = cell.borrow();
                                if idx < b.len() {
                                    b[idx..].to_vec()
                                } else {
                                    Vec::new()
                                }
                            }
                            _ => Vec::new(),
                        };
                        self.assign_target(inner, JsValue::new_array(rest_items))?;
                        break;
                    } else {
                        let item_val = match &val {
                            JsValue::Array(cell) => cell
                                .borrow()
                                .get(idx)
                                .cloned()
                                .unwrap_or(JsValue::Undefined),
                            _ => JsValue::Undefined,
                        };
                        self.assign_target(item, item_val)?;
                        idx += 1;
                    }
                }
                Ok(())
            }
            Expr::Object(pairs) => {
                let mut matched_keys = HashSet::new();
                for (k, target_expr) in pairs {
                    if let Expr::Spread(inner) = target_expr {
                        let mut rest_map = BTreeMap::new();
                        if let JsValue::Object(cell) = &val {
                            for (mk, mv) in cell.borrow().iter() {
                                if !matched_keys.contains(mk) && mk != "__proto__" {
                                    rest_map.insert(mk.clone(), mv.clone());
                                }
                            }
                        }
                        self.assign_target(inner, JsValue::new_object(rest_map))?;
                    } else {
                        matched_keys.insert(k.clone());
                        let item_val = match &val {
                            JsValue::Object(cell) => {
                                cell.borrow().get(k).cloned().unwrap_or(JsValue::Undefined)
                            }
                            _ => JsValue::Undefined,
                        };
                        self.assign_target(target_expr, item_val)?;
                    }
                }
                Ok(())
            }
            _ => Err("Invalid assignment target".to_string()),
        }
    }

    pub fn call_value(&mut self, callee: &JsValue, args: Vec<JsValue>) -> Result<JsValue, String> {
        self.call_value_with_this(callee, args, None)
    }

    pub fn call_value_with_this(
        &mut self,
        callee: &JsValue,
        args: Vec<JsValue>,
        this_val: Option<JsValue>,
    ) -> Result<JsValue, String> {
        match callee {
            JsValue::Function(handle) => {
                if let Some(closure) = self.closures.get(&handle.id).cloned() {
                    return self.call_closure_with_this(&closure, args, this_val);
                }
                if let Some(ref name) = handle.name {
                    if let Some(closure) = self.user_functions.get(name).cloned() {
                        return self.call_closure_with_this(&closure, args, this_val);
                    }
                    return self.call_function(name, args);
                }
                Err(format!(
                    "TypeError: Function #{} is not callable",
                    handle.id
                ))
            }
            JsValue::String(name) => self.call_function(name, args),
            JsValue::Object(map) => {
                let (cid, fn_name) = {
                    let b = map.borrow();
                    let cid = b
                        .get("__closure_id__")
                        .and_then(|v| v.as_f64())
                        .map(|n| n as u64);
                    let fn_name = b
                        .get("__fn__")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    (cid, fn_name)
                };
                if let Some(id) = cid {
                    if let Some(closure) = self.closures.get(&id).cloned() {
                        return self.call_closure_with_this(&closure, args, this_val);
                    }
                }
                if let Some(name) = fn_name {
                    self.call_function(&name, args)
                } else {
                    Ok(JsValue::Undefined)
                }
            }
            _ => Ok(JsValue::Undefined),
        }
    }

    pub fn eval_expr(&mut self, expr: &Expr) -> Result<JsValue, String> {
        if self.eval_depth >= MAX_EVAL_DEPTH {
            return Err(
                "RangeError: Maximum call stack size exceeded (eval recursion limit)".to_string(),
            );
        }
        self.eval_depth += 1;
        let res = self.eval_expr_inner(expr);
        self.eval_depth -= 1;
        res
    }

    fn eval_expr_inner(&mut self, expr: &Expr) -> Result<JsValue, String> {
        match expr {
            Expr::Literal(val) => Ok(val.clone()),
            Expr::Assign { target, value } => {
                let val = self.eval_expr(value)?;
                self.assign_target(target, val.clone())?;
                Ok(val)
            }
            Expr::Update { op, target, prefix } => {
                let cur_val = self.eval_expr(target)?;
                let num = cur_val.as_f64().unwrap_or(0.0);
                let new_num = if *op == UpdateOp::Increment {
                    num + 1.0
                } else {
                    num - 1.0
                };
                self.assign_target(target, JsValue::Number(new_num))?;
                if *prefix {
                    Ok(JsValue::Number(new_num))
                } else {
                    Ok(JsValue::Number(num))
                }
            }
            Expr::Identifier(name) => {
                if let Some(val) = self.current_scope.borrow().get(name) {
                    return Ok(val);
                }
                if let Some(val) = self.globals.get(name) {
                    return Ok(val.clone());
                }
                match name.as_str() {
                    "undefined" => Ok(JsValue::Undefined),
                    "null" => Ok(JsValue::Null),
                    "document" => Ok(JsValue::empty_object()),
                    "window" => Ok(JsValue::empty_object()),
                    _ => Err(format!("ReferenceError: {name} is not defined")),
                }
            }
            Expr::Binary { op, left, right } => match op {
                BinaryOp::Or => {
                    let l_val = self.eval_expr(left)?;
                    if l_val.is_truthy() {
                        return Ok(l_val);
                    }
                    self.eval_expr(right)
                }
                BinaryOp::And => {
                    let l_val = self.eval_expr(left)?;
                    if !l_val.is_truthy() {
                        return Ok(l_val);
                    }
                    self.eval_expr(right)
                }
                BinaryOp::NullishCoalescing => {
                    let l_val = self.eval_expr(left)?;
                    if !matches!(l_val, JsValue::Null | JsValue::Undefined) {
                        return Ok(l_val);
                    }
                    self.eval_expr(right)
                }
                BinaryOp::InstanceOf => {
                    let l_val = self.eval_expr(left)?;
                    let class_name = match &**right {
                        Expr::Identifier(ref id) => id.clone(),
                        Expr::Literal(JsValue::String(ref s)) => s.clone(),
                        other => {
                            let rv = self.eval_expr(other)?;
                            if let JsValue::Object(map) = rv {
                                map.borrow()
                                    .get("__class_name__")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string()
                            } else {
                                String::new()
                            }
                        }
                    };
                    Ok(JsValue::Bool(self.is_instance_of(&l_val, &class_name)))
                }
                _ => {
                    let l_val = self.eval_expr(left)?;
                    let r_val = self.eval_expr(right)?;
                    self.eval_binary(op, &l_val, &r_val)
                }
            },
            Expr::Unary { op, expr } => {
                if *op == UnaryOp::Typeof {
                    if let Expr::Identifier(ref name) = &**expr {
                        if let Some(val) = self.current_scope.borrow().get(name) {
                            return Ok(JsValue::String(val.js_typeof().to_string()));
                        }
                        if let Some(val) = self.globals.get(name) {
                            return Ok(JsValue::String(val.js_typeof().to_string()));
                        }
                        return Ok(JsValue::String("undefined".to_string()));
                    }
                }
                let val = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Not => Ok(JsValue::Bool(!val.is_truthy())),
                    UnaryOp::Neg => match val {
                        JsValue::BigInt(bi) => Ok(JsValue::BigInt(bi.neg())),
                        _ => {
                            let n = val.as_f64().unwrap_or(0.0);
                            Ok(JsValue::Number(-n))
                        }
                    },
                    UnaryOp::Pos => match val {
                        JsValue::BigInt(_) => {
                            Err("TypeError: Cannot convert a BigInt value to a number".to_string())
                        }
                        _ => {
                            let n = val.as_f64().unwrap_or(0.0);
                            Ok(JsValue::Number(n))
                        }
                    },
                    UnaryOp::BitNot => match val {
                        JsValue::BigInt(bi) => Ok(JsValue::BigInt(bi.bit_not())),
                        _ => {
                            let n = val.as_f64().unwrap_or(0.0) as i64 as i32;
                            Ok(JsValue::Number((!n) as f64))
                        }
                    },
                    UnaryOp::Typeof => Ok(JsValue::String(val.js_typeof().to_string())),
                }
            }
            Expr::TemplateLiteral(parts) => {
                let mut out = String::new();
                for part in parts {
                    match part {
                        TemplatePart::Literal(s) => out.push_str(s),
                        TemplatePart::Expr(e) => {
                            let v = self.eval_expr(e)?;
                            match v {
                                JsValue::String(s) => out.push_str(&s),
                                JsValue::Number(n) => out.push_str(&format!("{n}")),
                                JsValue::BigInt(bi) => out.push_str(&bi.to_decimal_string()),
                                JsValue::Bool(b) => out.push_str(&format!("{b}")),
                                JsValue::Null => out.push_str("null"),
                                JsValue::Undefined => out.push_str("undefined"),
                                other => out.push_str(
                                    &stringify_json(&other)
                                        .unwrap_or_else(|_| "[object Object]".to_string()),
                                ),
                            }
                        }
                    }
                }
                Ok(JsValue::String(out))
            }
            Expr::ArrowFunction {
                params,
                body,
                is_async,
            } => {
                let closure_id = self.next_closure_id;
                self.next_closure_id += 1;
                self.anon_counter += 1;
                let fn_name = format!("__anon_fn_{closure_id}");
                let stmts = match body {
                    ArrowBody::Expr(e) => vec![Stmt::Return(Some((**e).clone()))],
                    ArrowBody::Block(s) => s.clone(),
                };
                let closure = JsClosure {
                    id: closure_id,
                    name: None,
                    params: params.clone(),
                    body: stmts,
                    env: self.current_scope.clone(),
                    is_async: *is_async,
                    is_generator: false,
                };
                self.closures.insert(closure_id, closure);
                Ok(JsValue::Function(ClosureHandle::new(
                    closure_id,
                    Some(fn_name),
                )))
            }
            Expr::Await(inner) => {
                let val = self.eval_expr(inner)?;
                drain_microtasks();

                let promise_opt = match &val {
                    JsValue::Promise(p) => Some(p.clone()),
                    _ => None,
                };

                if let Some(promise) = promise_opt {
                    // Cooperative event loop progression until promise settles or progress halts
                    let max_wait = std::time::Duration::from_secs(5);
                    let start = std::time::Instant::now();
                    loop {
                        match promise.state() {
                            PromiseState::Fulfilled(res) => return Ok(res),
                            PromiseState::Rejected(err) => {
                                self.pending_throw = Some(err);
                                return Ok(JsValue::Undefined);
                            }
                            PromiseState::Pending => {
                                let drained = drain_microtasks();
                                let fired = self.tick_engine_timers();
                                if drained > 0 || fired > 0 {
                                    continue;
                                }
                                if let Some(wait_dur) = crate::web::time_until_next_timer() {
                                    if start.elapsed() > max_wait {
                                        break;
                                    }
                                    let sleep_chunk =
                                        wait_dur.min(std::time::Duration::from_millis(5));
                                    if sleep_chunk > std::time::Duration::ZERO {
                                        std::thread::sleep(sleep_chunk);
                                    } else {
                                        std::thread::yield_now();
                                    }
                                } else {
                                    if start.elapsed().as_millis() > 50 {
                                        break;
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(1));
                                }
                            }
                        }
                    }

                    match promise.state() {
                        PromiseState::Fulfilled(res) => Ok(res),
                        PromiseState::Rejected(err) => {
                            self.pending_throw = Some(err);
                            Ok(JsValue::Undefined)
                        }
                        PromiseState::Pending => {
                            Err("AsyncError: Awaited Promise remained pending after event loop exhaustion".to_string())
                        }
                    }
                } else {
                    Ok(val)
                }
            }
            Expr::Member {
                object,
                property,
                computed,
            } => {
                let obj_val = self.eval_expr(object)?;
                self.eval_member_access(object, &obj_val, property, computed.as_deref())
            }
            Expr::OptionalMember {
                object,
                property,
                computed,
            } => {
                let obj_val = self.eval_expr(object)?;
                if matches!(obj_val, JsValue::Null | JsValue::Undefined) {
                    return Ok(JsValue::Undefined);
                }
                self.eval_member_access(object, &obj_val, property, computed.as_deref())
            }
            Expr::Call { callee, args } => {
                if let Expr::OptionalMember { object, .. } = &**callee {
                    let base = self.eval_expr(object)?;
                    if matches!(base, JsValue::Null | JsValue::Undefined) {
                        return Ok(JsValue::Undefined);
                    }
                }

                let mut evaluated_args = Vec::new();
                for a in args {
                    if let Expr::Spread(inner) = a {
                        let spread_val = self.eval_expr(inner)?;
                        match spread_val {
                            JsValue::Array(items) => evaluated_args.extend(items.borrow().clone()),
                            JsValue::String(s) => {
                                for ch in s.chars() {
                                    evaluated_args.push(JsValue::String(ch.to_string()));
                                }
                            }
                            other => evaluated_args.push(other),
                        }
                    } else {
                        evaluated_args.push(self.eval_expr(a)?);
                    }
                }

                if let Expr::Identifier(id) = &**callee {
                    if id == "super" {
                        let super_class_opt = self.current_scope.borrow().get("__super_class__");
                        if let Some(JsValue::String(super_name)) = super_class_opt {
                            let super_ctor = self.resolve_constructor(&super_name);
                            if let Some((params, body, next_super)) = super_ctor {
                                let current_this = self
                                    .current_scope
                                    .borrow()
                                    .get("this")
                                    .unwrap_or(JsValue::Undefined);
                                let super_scope = Scope::new_child(self.current_scope.clone());
                                super_scope.borrow_mut().declare("this", current_this);
                                if let Some(ns) = next_super {
                                    super_scope
                                        .borrow_mut()
                                        .declare("__super_class__", JsValue::String(ns));
                                }
                                super_scope
                                    .borrow_mut()
                                    .declare("__current_class__", JsValue::String(super_name));

                                bind_parameters(&params, evaluated_args, &super_scope);

                                let saved_scope = self.current_scope.clone();
                                self.current_scope = super_scope.clone();

                                let mut err = None;
                                for stmt in &body {
                                    match self.execute_stmt_flow(stmt) {
                                        Ok(StatementFlow::Normal(_)) => {}
                                        Ok(StatementFlow::Return(_)) => break,
                                        Ok(StatementFlow::Throw(v)) => {
                                            err = Some(format!("Uncaught exception: {v}"));
                                            break;
                                        }
                                        Ok(StatementFlow::Break(_)) => {
                                            err = Some(
                                                "SyntaxError: Illegal break statement".to_string(),
                                            );
                                            break;
                                        }
                                        Ok(StatementFlow::Continue(_)) => {
                                            err = Some(
                                                "SyntaxError: Illegal continue statement"
                                                    .to_string(),
                                            );
                                            break;
                                        }
                                        Ok(StatementFlow::Yield(_)) => break,
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

                                if let Some(updated_this) = super_scope.borrow().get("this") {
                                    self.current_scope.borrow_mut().set("this", updated_this);
                                }
                                return Ok(JsValue::Undefined);
                            }
                        }
                    }
                }

                if let Expr::Member {
                    object,
                    property,
                    computed,
                }
                | Expr::OptionalMember {
                    object,
                    property,
                    computed,
                } = &**callee
                {
                    if computed.is_none() {
                        if let Expr::Identifier(id) = &**object {
                            if id == "super" {
                                let current_class_opt =
                                    self.current_scope.borrow().get("__current_class__");
                                if let Some(JsValue::String(curr_cls)) = current_class_opt {
                                    let super_class_opt = self
                                        .classes
                                        .get(&curr_cls)
                                        .and_then(|c| c.super_class.clone());
                                    if let Some(super_name) = super_class_opt {
                                        if let Some((params, body, _)) =
                                            self.resolve_method(&super_name, property)
                                        {
                                            let current_this = self
                                                .current_scope
                                                .borrow()
                                                .get("this")
                                                .unwrap_or(JsValue::Undefined);
                                            let super_scope =
                                                Scope::new_child(self.current_scope.clone());
                                            super_scope.borrow_mut().declare("this", current_this);
                                            super_scope.borrow_mut().declare(
                                                "__current_class__",
                                                JsValue::String(super_name),
                                            );

                                            bind_parameters(&params, evaluated_args, &super_scope);

                                            let saved_scope = self.current_scope.clone();
                                            self.current_scope = super_scope.clone();

                                            let mut ret = JsValue::Undefined;
                                            let mut err = None;
                                            for stmt in &body {
                                                match self.execute_stmt_flow(stmt) {
                                                    Ok(StatementFlow::Normal(v)) => ret = v,
                                                    Ok(StatementFlow::Return(v)) => {
                                                        ret = v;
                                                        break;
                                                    }
                                                    Ok(StatementFlow::Throw(v)) => {
                                                        err = Some(format!(
                                                            "Uncaught exception: {v}"
                                                        ));
                                                        break;
                                                    }
                                                    Ok(StatementFlow::Break(_)) => {
                                                        err = Some(
                                                            "SyntaxError: Illegal break statement"
                                                                .to_string(),
                                                        );
                                                        break;
                                                    }
                                                    Ok(StatementFlow::Continue(_)) => {
                                                        err = Some("SyntaxError: Illegal continue statement".to_string());
                                                        break;
                                                    }
                                                    Ok(StatementFlow::Yield(v)) => {
                                                        ret = v;
                                                        break;
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

                                            if let Some(updated_this) =
                                                super_scope.borrow().get("this")
                                            {
                                                self.current_scope
                                                    .borrow_mut()
                                                    .set("this", updated_this);
                                            }
                                            return Ok(ret);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Expr::Identifier(id) = &**object {
                        if id == "document" {
                            match property.as_str() {
                                "getElementById" => {
                                    if let Some(target_id) =
                                        evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        if let Some(elem) =
                                            self.document.get_element_by_id(target_id)
                                        {
                                            return Ok(self.dom_node_to_js(&elem));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "querySelector" => {
                                    if let Some(sel) =
                                        evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        if let Some(elem) = self.document.query_selector(sel) {
                                            return Ok(self.dom_node_to_js(&elem));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "querySelectorAll" => {
                                    if let Some(sel) =
                                        evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        let all = self.document.query_selector_all(sel);
                                        let list: Vec<JsValue> =
                                            all.iter().map(|n| self.dom_node_to_js(n)).collect();
                                        return Ok(JsValue::new_array(list));
                                    }
                                    return Ok(JsValue::empty_array());
                                }
                                "createElement" => {
                                    if let Some(tag) =
                                        evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        let elem = self.document.create_element(tag);
                                        return Ok(self.dom_node_to_js(&elem));
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "createTextNode" => {
                                    if let Some(text) =
                                        evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        let elem = self.document.create_text_node(text);
                                        return Ok(self.dom_node_to_js(&elem));
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "createComment" => {
                                    if let Some(text) =
                                        evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        let elem = self.document.create_comment(text);
                                        return Ok(self.dom_node_to_js(&elem));
                                    }
                                    return Ok(JsValue::Null);
                                }
                                _ => {}
                            }
                        } else if id == "console" {
                            let msg = evaluated_args
                                .iter()
                                .map(|v| match v {
                                    JsValue::String(s) => s.clone(),
                                    JsValue::Number(n) => n.to_string(),
                                    JsValue::Bool(b) => b.to_string(),
                                    JsValue::Null => "null".to_string(),
                                    JsValue::Undefined => "undefined".to_string(),
                                    other => stringify_json(other)
                                        .unwrap_or_else(|_| "[object Object]".to_string()),
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            self.console_logs.push(msg);
                            return Ok(JsValue::Undefined);
                        } else if id == "JSON" {
                            if property == "stringify" {
                                if let Some(first) = evaluated_args.first() {
                                    return stringify_json(first).map(JsValue::String);
                                }
                            } else if property == "parse" {
                                if let Some(first) = evaluated_args.first().and_then(|v| v.as_str())
                                {
                                    return parse_json(first);
                                }
                            }
                        } else if id == "Math" {
                            match property.as_str() {
                                "floor" => {
                                    let n = evaluated_args
                                        .first()
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0);
                                    return Ok(JsValue::Number(n.floor()));
                                }
                                "ceil" => {
                                    let n = evaluated_args
                                        .first()
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0);
                                    return Ok(JsValue::Number(n.ceil()));
                                }
                                "round" => {
                                    let n = evaluated_args
                                        .first()
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0);
                                    return Ok(JsValue::Number(n.round()));
                                }
                                "abs" => {
                                    let n = evaluated_args
                                        .first()
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0);
                                    return Ok(JsValue::Number(n.abs()));
                                }
                                "min" => {
                                    let mut m = f64::INFINITY;
                                    for a in &evaluated_args {
                                        if let Some(n) = a.as_f64() {
                                            if n < m {
                                                m = n;
                                            }
                                        }
                                    }
                                    return Ok(JsValue::Number(m));
                                }
                                "max" => {
                                    let mut m = f64::NEG_INFINITY;
                                    for a in &evaluated_args {
                                        if let Some(n) = a.as_f64() {
                                            if n > m {
                                                m = n;
                                            }
                                        }
                                    }
                                    return Ok(JsValue::Number(m));
                                }
                                "sqrt" => {
                                    let n = evaluated_args
                                        .first()
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0);
                                    return Ok(JsValue::Number(n.sqrt()));
                                }
                                _ => {}
                            }
                        } else if id == "Promise" {
                            match property.as_str() {
                                "resolve" => {
                                    let v = evaluated_args
                                        .first()
                                        .cloned()
                                        .unwrap_or(JsValue::Undefined);
                                    let p = JsPromise::resolve(v);
                                    return Ok(self.create_promise_value(p));
                                }
                                "reject" => {
                                    let v = evaluated_args
                                        .first()
                                        .cloned()
                                        .unwrap_or(JsValue::Undefined);
                                    let p = JsPromise::reject(v);
                                    return Ok(self.create_promise_value(p));
                                }
                                "all" => {
                                    let first = evaluated_args
                                        .first()
                                        .cloned()
                                        .unwrap_or_else(JsValue::empty_array);
                                    if let JsValue::Array(items) = first {
                                        let mut promise_list = Vec::new();
                                        for item in items.borrow().iter() {
                                            match item {
                                                JsValue::Promise(p) => {
                                                    promise_list.push(p.clone());
                                                }
                                                _ => {
                                                    promise_list
                                                        .push(JsPromise::resolve(item.clone()));
                                                }
                                            }
                                        }
                                        let all_p = JsPromise::all(promise_list);
                                        drain_microtasks();
                                        let mapped_p = all_p.then(JsValue::new_array);
                                        drain_microtasks();
                                        return Ok(self.create_promise_value(mapped_p));
                                    }
                                    let p = JsPromise::resolve(JsValue::empty_array());
                                    return Ok(self.create_promise_value(p));
                                }
                                _ => {}
                            }
                        } else if id == "Object" {
                            match property.as_str() {
                                "create" => {
                                    let proto =
                                        evaluated_args.first().cloned().unwrap_or(JsValue::Null);
                                    let mut new_obj = BTreeMap::new();
                                    if !matches!(proto, JsValue::Null) {
                                        new_obj.insert("__proto__".to_string(), proto);
                                    }
                                    return Ok(JsValue::new_object(new_obj));
                                }
                                "getPrototypeOf" => {
                                    if let Some(JsValue::Object(m)) = evaluated_args.first() {
                                        if let Some(p) = m.borrow().get("__proto__") {
                                            return Ok(p.clone());
                                        }
                                        return Ok(JsValue::new_object(
                                            self.object_prototype.clone(),
                                        ));
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "setPrototypeOf" => {
                                    if self.freeze_prototypes || !self.allow_prototype_mutation {
                                        return Err(
                                            "TypeError: Cannot modify prototype in sealed mode"
                                                .to_string(),
                                        );
                                    }
                                    let proto =
                                        evaluated_args.get(1).cloned().unwrap_or(JsValue::Null);
                                    if let Some(JsValue::Object(m)) = evaluated_args.first() {
                                        m.borrow_mut().insert("__proto__".to_string(), proto);
                                        return Ok(JsValue::Object(m.clone()));
                                    }
                                    return Ok(JsValue::Undefined);
                                }
                                "keys" => {
                                    if let Some(JsValue::Object(m)) = evaluated_args.first() {
                                        let keys: Vec<JsValue> = m
                                            .borrow()
                                            .keys()
                                            .filter(|k| !k.starts_with("__"))
                                            .map(|k| JsValue::String(k.clone()))
                                            .collect();
                                        return Ok(JsValue::new_array(keys));
                                    }
                                    return Ok(JsValue::empty_array());
                                }
                                "values" => {
                                    if let Some(JsValue::Object(m)) = evaluated_args.first() {
                                        let vals: Vec<JsValue> = m
                                            .borrow()
                                            .iter()
                                            .filter(|(k, _)| !k.starts_with("__"))
                                            .map(|(_, v)| v.clone())
                                            .collect();
                                        return Ok(JsValue::new_array(vals));
                                    }
                                    return Ok(JsValue::empty_array());
                                }
                                "entries" => {
                                    if let Some(JsValue::Object(m)) = evaluated_args.first() {
                                        let entries: Vec<JsValue> = m
                                            .borrow()
                                            .iter()
                                            .filter(|(k, _)| !k.starts_with("__"))
                                            .map(|(k, v)| {
                                                JsValue::new_array(vec![
                                                    JsValue::String(k.clone()),
                                                    v.clone(),
                                                ])
                                            })
                                            .collect();
                                        return Ok(JsValue::new_array(entries));
                                    }
                                    return Ok(JsValue::empty_array());
                                }
                                "assign" => {
                                    if let Some(JsValue::Object(target)) = evaluated_args.first() {
                                        for src in evaluated_args.iter().skip(1) {
                                            if let JsValue::Object(s_map) = src {
                                                for (k, v) in s_map.borrow().iter() {
                                                    if (k == "__proto__"
                                                        || k == "constructor"
                                                        || k == "prototype")
                                                        && !self.allow_prototype_mutation
                                                    {
                                                        continue;
                                                    }
                                                    target
                                                        .borrow_mut()
                                                        .insert(k.clone(), v.clone());
                                                }
                                            }
                                        }
                                        return Ok(JsValue::Object(target.clone()));
                                    }
                                    return Ok(JsValue::Undefined);
                                }
                                "fromEntries" => {
                                    let mut map = BTreeMap::new();
                                    if let Some(JsValue::Array(items)) = evaluated_args.first() {
                                        for item in items.borrow().iter() {
                                            if let JsValue::Array(pair) = item {
                                                let p = pair.borrow();
                                                if let (Some(k), Some(v)) = (p.first(), p.get(1)) {
                                                    map.insert(k.to_string(), v.clone());
                                                }
                                            }
                                        }
                                    }
                                    return Ok(JsValue::new_object(map));
                                }
                                _ => {}
                            }
                        } else if id == "Reflect" {
                            match property.as_str() {
                                "get" => {
                                    let target = evaluated_args
                                        .first()
                                        .cloned()
                                        .unwrap_or(JsValue::Undefined);
                                    let prop_key = evaluated_args
                                        .get(1)
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    return Ok(target
                                        .get_prop(&prop_key)
                                        .unwrap_or(JsValue::Undefined));
                                }
                                "set" => {
                                    if evaluated_args.len() >= 3 {
                                        let target = &evaluated_args[0];
                                        let prop_key = evaluated_args[1].to_string();
                                        let val = evaluated_args[2].clone();
                                        target.set_prop(prop_key, val);
                                        return Ok(JsValue::Bool(true));
                                    }
                                    return Ok(JsValue::Bool(false));
                                }
                                "has" => {
                                    let target = evaluated_args
                                        .first()
                                        .cloned()
                                        .unwrap_or(JsValue::Undefined);
                                    let prop_key = evaluated_args
                                        .get(1)
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    return Ok(JsValue::Bool(target.has_prop(&prop_key)));
                                }
                                "deleteProperty" => {
                                    if let Some(JsValue::Object(c)) = evaluated_args.first() {
                                        let prop_key = evaluated_args
                                            .get(1)
                                            .map(|v| v.to_string())
                                            .unwrap_or_default();
                                        return Ok(JsValue::Bool(
                                            c.borrow_mut().remove(&prop_key).is_some(),
                                        ));
                                    }
                                    return Ok(JsValue::Bool(false));
                                }
                                "ownKeys" => {
                                    if let Some(JsValue::Object(c)) = evaluated_args.first() {
                                        let keys: Vec<JsValue> = c
                                            .borrow()
                                            .keys()
                                            .filter(|k| !k.starts_with("__"))
                                            .map(|k| JsValue::String(k.clone()))
                                            .collect();
                                        return Ok(JsValue::new_array(keys));
                                    }
                                    return Ok(JsValue::empty_array());
                                }
                                _ => {}
                            }
                        } else if id == "Date" {
                            match property.as_str() {
                                "now" => {
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as f64)
                                        .unwrap_or(0.0);
                                    return Ok(JsValue::Number(now));
                                }
                                "parse" => {
                                    if let Some(date_str) =
                                        evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        if let Ok(ts) = date_str.parse::<f64>() {
                                            return Ok(JsValue::Number(ts));
                                        }
                                    }
                                    return Ok(JsValue::Number(f64::NAN));
                                }
                                _ => {}
                            }
                        } else if id == "BigInt" {
                            if let Some(val) = evaluated_args.first() {
                                return self.to_bigint(val);
                            }
                            return Err(
                                "TypeError: Cannot convert undefined to a BigInt".to_string()
                            );
                        } else if id == "Symbol" {
                            match property.as_str() {
                                "for" => {
                                    let key = evaluated_args
                                        .first()
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    if let Some(sym_id) = self.symbol_registry.get(&key) {
                                        return Ok(JsValue::Symbol(sym_id.clone()));
                                    }
                                    let id = self.next_symbol_id;
                                    self.next_symbol_id += 1;
                                    let sym = SymbolId::new(id, Some(key.clone()));
                                    self.symbol_registry.insert(key, sym.clone());
                                    return Ok(JsValue::Symbol(sym));
                                }
                                "keyFor" => {
                                    if let Some(JsValue::Symbol(sym)) = evaluated_args.first() {
                                        for (k, v) in &self.symbol_registry {
                                            if v == sym {
                                                return Ok(JsValue::String(k.clone()));
                                            }
                                        }
                                    }
                                    return Ok(JsValue::Undefined);
                                }
                                _ => {}
                            }
                        }
                    }

                    // Array prototype method calls: arr.push, arr.map, arr.filter, arr.join, etc.
                    let obj_val = self.eval_expr(object)?;
                    let _obj_ident: Option<String> = if let Expr::Identifier(ref id) = &**object {
                        Some(id.clone())
                    } else {
                        None
                    };

                    if let JsValue::Array(ref arr) = obj_val {
                        match property.as_str() {
                            "push" => {
                                self.allocate_bytes(evaluated_args.len().saturating_mul(16))?;
                                let mut b = arr.borrow_mut();
                                b.extend(evaluated_args);
                                return Ok(JsValue::Number(b.len() as f64));
                            }
                            "pop" => {
                                let popped = arr.borrow_mut().pop().unwrap_or(JsValue::Undefined);
                                return Ok(popped);
                            }
                            "map" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let mut out = Vec::new();
                                    let b = arr.borrow();
                                    for (i, item) in b.iter().enumerate() {
                                        let res = self.call_value(
                                            &cb,
                                            vec![
                                                item.clone(),
                                                JsValue::Number(i as f64),
                                                obj_val.clone(),
                                            ],
                                        )?;
                                        out.push(res);
                                    }
                                    return Ok(JsValue::new_array(out));
                                }
                                return Ok(JsValue::empty_array());
                            }
                            "filter" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let mut out = Vec::new();
                                    let b = arr.borrow();
                                    for (i, item) in b.iter().enumerate() {
                                        let res = self.call_value(
                                            &cb,
                                            vec![
                                                item.clone(),
                                                JsValue::Number(i as f64),
                                                obj_val.clone(),
                                            ],
                                        )?;
                                        if res.is_truthy() {
                                            out.push(item.clone());
                                        }
                                    }
                                    return Ok(JsValue::new_array(out));
                                }
                                return Ok(JsValue::empty_array());
                            }
                            "find" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let b = arr.borrow();
                                    for (i, item) in b.iter().enumerate() {
                                        let res = self.call_value(
                                            &cb,
                                            vec![
                                                item.clone(),
                                                JsValue::Number(i as f64),
                                                obj_val.clone(),
                                            ],
                                        )?;
                                        if res.is_truthy() {
                                            return Ok(item.clone());
                                        }
                                    }
                                }
                                return Ok(JsValue::Undefined);
                            }
                            "findIndex" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let b = arr.borrow();
                                    for (i, item) in b.iter().enumerate() {
                                        let res = self.call_value(
                                            &cb,
                                            vec![
                                                item.clone(),
                                                JsValue::Number(i as f64),
                                                obj_val.clone(),
                                            ],
                                        )?;
                                        if res.is_truthy() {
                                            return Ok(JsValue::Number(i as f64));
                                        }
                                    }
                                }
                                return Ok(JsValue::Number(-1.0));
                            }
                            "some" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    for (i, item) in arr.borrow().iter().enumerate() {
                                        let res = self.call_value(
                                            &cb,
                                            vec![
                                                item.clone(),
                                                JsValue::Number(i as f64),
                                                obj_val.clone(),
                                            ],
                                        )?;
                                        if res.is_truthy() {
                                            return Ok(JsValue::Bool(true));
                                        }
                                    }
                                }
                                return Ok(JsValue::Bool(false));
                            }
                            "every" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    for (i, item) in arr.borrow().iter().enumerate() {
                                        let res = self.call_value(
                                            &cb,
                                            vec![
                                                item.clone(),
                                                JsValue::Number(i as f64),
                                                obj_val.clone(),
                                            ],
                                        )?;
                                        if !res.is_truthy() {
                                            return Ok(JsValue::Bool(false));
                                        }
                                    }
                                    return Ok(JsValue::Bool(true));
                                }
                                return Ok(JsValue::Bool(true));
                            }
                            "reduce" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let b = arr.borrow();
                                    let mut iter = b.iter();
                                    let mut accum = if evaluated_args.len() >= 2 {
                                        evaluated_args[1].clone()
                                    } else if let Some(first) = iter.next() {
                                        first.clone()
                                    } else {
                                        return Err("TypeError: Reduce of empty array with no initial value".to_string());
                                    };
                                    for (i, item) in iter.enumerate() {
                                        accum = self.call_value(
                                            &cb,
                                            vec![
                                                accum,
                                                item.clone(),
                                                JsValue::Number(i as f64),
                                                obj_val.clone(),
                                            ],
                                        )?;
                                    }
                                    return Ok(accum);
                                }
                                return Ok(JsValue::Undefined);
                            }
                            "reduceRight" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let b = arr.borrow();
                                    let mut iter = b.iter().rev();
                                    let mut accum = if evaluated_args.len() >= 2 {
                                        evaluated_args[1].clone()
                                    } else if let Some(first) = iter.next() {
                                        first.clone()
                                    } else {
                                        return Err("TypeError: Reduce of empty array with no initial value".to_string());
                                    };
                                    let len = b.len();
                                    for (i, item) in iter.enumerate() {
                                        let actual_idx = len - 1 - (i + 1);
                                        accum = self.call_value(
                                            &cb,
                                            vec![
                                                accum,
                                                item.clone(),
                                                JsValue::Number(actual_idx as f64),
                                                obj_val.clone(),
                                            ],
                                        )?;
                                    }
                                    return Ok(accum);
                                }
                                return Ok(JsValue::Undefined);
                            }
                            "join" => {
                                let sep = evaluated_args
                                    .first()
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(",");
                                return Ok(JsValue::String(obj_val.join(sep)));
                            }
                            "slice" => {
                                let start = evaluated_args
                                    .first()
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0)
                                    as isize;
                                let end = evaluated_args
                                    .get(1)
                                    .and_then(|v| v.as_f64())
                                    .map(|n| n as isize);
                                return Ok(obj_val.slice(start, end));
                            }
                            "concat" => {
                                let mut result = arr.borrow().clone();
                                for arg in evaluated_args {
                                    match arg {
                                        JsValue::Array(items) => {
                                            result.extend(items.borrow().clone())
                                        }
                                        other => result.push(other),
                                    }
                                }
                                return Ok(JsValue::new_array(result));
                            }
                            "includes" => {
                                if let Some(item) = evaluated_args.first() {
                                    return Ok(JsValue::Bool(obj_val.includes(item)));
                                }
                                return Ok(JsValue::Bool(false));
                            }
                            "indexOf" => {
                                if let Some(target) = evaluated_args.first() {
                                    for (i, item) in arr.borrow().iter().enumerate() {
                                        if item.strict_eq(target) {
                                            return Ok(JsValue::Number(i as f64));
                                        }
                                    }
                                }
                                return Ok(JsValue::Number(-1.0));
                            }
                            "reverse" => {
                                arr.borrow_mut().reverse();
                                return Ok(obj_val.clone());
                            }
                            "flat" => {
                                let depth = evaluated_args
                                    .first()
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(1.0)
                                    as usize;
                                return Ok(obj_val.flat(depth));
                            }
                            "flatMap" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let mut out = Vec::new();
                                    let b = arr.borrow();
                                    for (i, item) in b.iter().enumerate() {
                                        let res = self.call_value(
                                            &cb,
                                            vec![
                                                item.clone(),
                                                JsValue::Number(i as f64),
                                                obj_val.clone(),
                                            ],
                                        )?;
                                        if let JsValue::Array(inner) = res {
                                            out.extend(inner.borrow().clone());
                                        } else {
                                            out.push(res);
                                        }
                                    }
                                    return Ok(JsValue::new_array(out));
                                }
                                return Ok(JsValue::empty_array());
                            }
                            "at" => {
                                let b = arr.borrow();
                                let idx_f = evaluated_args
                                    .first()
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0);
                                let idx = if idx_f < 0.0 {
                                    (b.len() as f64 + idx_f) as isize
                                } else {
                                    idx_f as isize
                                };
                                if idx >= 0 && (idx as usize) < b.len() {
                                    return Ok(b[idx as usize].clone());
                                }
                                return Ok(JsValue::Undefined);
                            }
                            "sort" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let mut sort_err = None;
                                    let mut cloned = arr.borrow().clone();
                                    cloned.sort_by(|a, b| {
                                        if sort_err.is_some() {
                                            return std::cmp::Ordering::Equal;
                                        }
                                        match self.call_value(&cb, vec![a.clone(), b.clone()]) {
                                            Ok(JsValue::Number(n)) => {
                                                if n < 0.0 {
                                                    std::cmp::Ordering::Less
                                                } else if n > 0.0 {
                                                    std::cmp::Ordering::Greater
                                                } else {
                                                    std::cmp::Ordering::Equal
                                                }
                                            }
                                            Ok(_) => std::cmp::Ordering::Equal,
                                            Err(e) => {
                                                sort_err = Some(e);
                                                std::cmp::Ordering::Equal
                                            }
                                        }
                                    });
                                    if let Some(err) = sort_err {
                                        return Err(err);
                                    }
                                    *arr.borrow_mut() = cloned;
                                } else {
                                    arr.borrow_mut().sort_by_key(|a| a.to_string());
                                }
                                return Ok(obj_val.clone());
                            }
                            _ => {}
                        }
                    }

                    // RegExp method calls: re.test, re.exec, re.toString
                    if let JsValue::Object(ref map) = obj_val {
                        let is_re = {
                            let b = map.borrow();
                            b.get("__is_regex__") == Some(&JsValue::Bool(true))
                        };
                        if is_re {
                            let (source, flags) = {
                                let b = map.borrow();
                                let s = b
                                    .get("source")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let f = b
                                    .get("flags")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                (s, f)
                            };
                            let re = self.compile_regex(&source, &flags)?;

                            match property.as_str() {
                                "test" => {
                                    let input = evaluated_args
                                        .first()
                                        .map(|v| match v {
                                            JsValue::String(s) => s.clone(),
                                            other => self.to_display_string(other),
                                        })
                                        .unwrap_or_default();
                                    return Ok(JsValue::Bool(re.is_match(&input)));
                                }
                                "exec" => {
                                    let input = evaluated_args
                                        .first()
                                        .map(|v| match v {
                                            JsValue::String(s) => s.clone(),
                                            other => self.to_display_string(other),
                                        })
                                        .unwrap_or_default();
                                    if let Some(caps) = re.captures(&input) {
                                        let mut elems = Vec::new();
                                        for cap_opt in caps.iter() {
                                            match cap_opt {
                                                Some(m) => elems
                                                    .push(JsValue::String(m.as_str().to_string())),
                                                None => elems.push(JsValue::Undefined),
                                            }
                                        }
                                        if let Some(m0) = caps.get(0) {
                                            let last_idx = (m0.start() + m0.as_str().len()) as f64;
                                            map.borrow_mut().insert(
                                                "lastIndex".to_string(),
                                                JsValue::Number(last_idx),
                                            );
                                        }
                                        return Ok(JsValue::new_array(elems));
                                    } else {
                                        return Ok(JsValue::Null);
                                    }
                                }
                                "toString" => {
                                    return Ok(JsValue::String(format!("/{source}/{flags}")));
                                }
                                _ => {}
                            }
                        }
                    }

                    // String prototype method calls: str.match, str.replace, str.replaceAll, str.search, str.split, etc.
                    if let JsValue::String(ref s) = obj_val {
                        match property.as_str() {
                            "match" => {
                                if let Some(arg) = evaluated_args.first() {
                                    let (pat, flags) = match arg {
                                        JsValue::Object(m) => {
                                            let b = m.borrow();
                                            if b.get("__is_regex__") == Some(&JsValue::Bool(true)) {
                                                let p = b
                                                    .get("source")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let f = b
                                                    .get("flags")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                (p, f)
                                            } else {
                                                (regex::escape(&arg.to_string()), String::new())
                                            }
                                        }
                                        other => (regex::escape(&other.to_string()), String::new()),
                                    };
                                    let re = self.compile_regex(&pat, &flags)?;
                                    if flags.contains('g') {
                                        let matches: Vec<JsValue> = re
                                            .find_iter(s)
                                            .map(|m| JsValue::String(m.as_str().to_string()))
                                            .collect();
                                        if matches.is_empty() {
                                            return Ok(JsValue::Null);
                                        }
                                        return Ok(JsValue::new_array(matches));
                                    } else if let Some(caps) = re.captures(s) {
                                        let mut elems = Vec::new();
                                        for cap_opt in caps.iter() {
                                            match cap_opt {
                                                Some(m) => elems
                                                    .push(JsValue::String(m.as_str().to_string())),
                                                None => elems.push(JsValue::Undefined),
                                            }
                                        }
                                        return Ok(JsValue::new_array(elems));
                                    } else {
                                        return Ok(JsValue::Null);
                                    }
                                }
                                return Ok(JsValue::Null);
                            }
                            "search" => {
                                if let Some(arg) = evaluated_args.first() {
                                    let (pat, flags) = match arg {
                                        JsValue::Object(m) => {
                                            let b = m.borrow();
                                            if b.get("__is_regex__") == Some(&JsValue::Bool(true)) {
                                                let p = b
                                                    .get("source")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let f = b
                                                    .get("flags")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                (p, f)
                                            } else {
                                                (regex::escape(&arg.to_string()), String::new())
                                            }
                                        }
                                        other => (regex::escape(&other.to_string()), String::new()),
                                    };
                                    let re = self.compile_regex(&pat, &flags)?;
                                    if let Some(m) = re.find(s) {
                                        return Ok(JsValue::Number(m.start() as f64));
                                    }
                                }
                                return Ok(JsValue::Number(-1.0));
                            }
                            "replace" => {
                                if evaluated_args.len() >= 2 {
                                    let target_arg = &evaluated_args[0];
                                    let replacement = match &evaluated_args[1] {
                                        JsValue::String(rs) => rs.clone(),
                                        other => self.to_display_string(other),
                                    };

                                    let re_info = match target_arg {
                                        JsValue::Object(m) => {
                                            let b = m.borrow();
                                            if b.get("__is_regex__") == Some(&JsValue::Bool(true)) {
                                                let p = b
                                                    .get("source")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let f = b
                                                    .get("flags")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                Some((p, f))
                                            } else {
                                                None
                                            }
                                        }
                                        _ => None,
                                    };
                                    if let Some((p, f)) = re_info {
                                        let re = self.compile_regex(&p, &f)?;
                                        if f.contains('g') {
                                            let res = re.replace_all(s, replacement.as_str());
                                            return Ok(JsValue::String(res.to_string()));
                                        } else {
                                            let res = re.replace(s, replacement.as_str());
                                            return Ok(JsValue::String(res.to_string()));
                                        }
                                    } else {
                                        let search_str = target_arg.to_string();
                                        let res = s.replacen(&search_str, &replacement, 1);
                                        return Ok(JsValue::String(res));
                                    }
                                }
                                return Ok(JsValue::String(s.clone()));
                            }
                            "replaceAll" => {
                                if evaluated_args.len() >= 2 {
                                    let target_arg = &evaluated_args[0];
                                    let replacement = match &evaluated_args[1] {
                                        JsValue::String(rs) => rs.clone(),
                                        other => self.to_display_string(other),
                                    };

                                    let re_info = match target_arg {
                                        JsValue::Object(m) => {
                                            let b = m.borrow();
                                            if b.get("__is_regex__") == Some(&JsValue::Bool(true)) {
                                                let p = b
                                                    .get("source")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let f = b
                                                    .get("flags")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                Some((p, f))
                                            } else {
                                                None
                                            }
                                        }
                                        _ => None,
                                    };
                                    if let Some((p, f)) = re_info {
                                        let re = self.compile_regex(&p, &f)?;
                                        let res = re.replace_all(s, replacement.as_str());
                                        return Ok(JsValue::String(res.to_string()));
                                    } else {
                                        let search_str = target_arg.to_string();
                                        let res = s.replace(&search_str, &replacement);
                                        return Ok(JsValue::String(res));
                                    }
                                }
                                return Ok(JsValue::String(s.clone()));
                            }
                            "split" => {
                                let sep_arg = evaluated_args.first();
                                let re_info = match sep_arg {
                                    Some(JsValue::Object(m)) => {
                                        let b = m.borrow();
                                        if b.get("__is_regex__") == Some(&JsValue::Bool(true)) {
                                            let p = b
                                                .get("source")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("")
                                                .to_string();
                                            let f = b
                                                .get("flags")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("")
                                                .to_string();
                                            Some((p, f))
                                        } else {
                                            None
                                        }
                                    }
                                    _ => None,
                                };
                                if let Some((p, f)) = re_info {
                                    let re = self.compile_regex(&p, &f)?;
                                    let parts: Vec<JsValue> = re
                                        .split(s)
                                        .map(|p| JsValue::String(p.to_string()))
                                        .collect();
                                    return Ok(JsValue::new_array(parts));
                                } else if let Some(other) = sep_arg {
                                    let sep = other.to_string();
                                    if sep.is_empty() {
                                        let chars: Vec<JsValue> = s
                                            .chars()
                                            .map(|c| JsValue::String(c.to_string()))
                                            .collect();
                                        return Ok(JsValue::new_array(chars));
                                    }
                                    let parts: Vec<JsValue> = s
                                        .split(&sep)
                                        .map(|p| JsValue::String(p.to_string()))
                                        .collect();
                                    return Ok(JsValue::new_array(parts));
                                } else {
                                    return Ok(JsValue::new_array(vec![JsValue::String(
                                        s.clone(),
                                    )]));
                                }
                            }
                            "toLowerCase" => {
                                return Ok(JsValue::String(s.to_lowercase()));
                            }
                            "toUpperCase" => {
                                return Ok(JsValue::String(s.to_uppercase()));
                            }
                            "trim" => {
                                return Ok(JsValue::String(s.trim().to_string()));
                            }
                            "trimStart" | "trimLeft" => {
                                return Ok(JsValue::String(s.trim_start().to_string()));
                            }
                            "trimEnd" | "trimRight" => {
                                return Ok(JsValue::String(s.trim_end().to_string()));
                            }
                            "includes" => {
                                let search = evaluated_args
                                    .first()
                                    .map(|v| v.to_string())
                                    .unwrap_or_default();
                                return Ok(JsValue::Bool(s.contains(&search)));
                            }
                            "indexOf" => {
                                let search = evaluated_args
                                    .first()
                                    .map(|v| v.to_string())
                                    .unwrap_or_default();
                                if let Some(idx) = s.find(&search) {
                                    return Ok(JsValue::Number(idx as f64));
                                }
                                return Ok(JsValue::Number(-1.0));
                            }
                            "startsWith" => {
                                let prefix = evaluated_args
                                    .first()
                                    .map(|v| v.to_string())
                                    .unwrap_or_default();
                                return Ok(JsValue::Bool(s.starts_with(&prefix)));
                            }
                            "endsWith" => {
                                let suffix = evaluated_args
                                    .first()
                                    .map(|v| v.to_string())
                                    .unwrap_or_default();
                                return Ok(JsValue::Bool(s.ends_with(&suffix)));
                            }
                            "charAt" => {
                                let idx = evaluated_args
                                    .first()
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0)
                                    as usize;
                                let ch = s
                                    .chars()
                                    .nth(idx)
                                    .map(|c| c.to_string())
                                    .unwrap_or_default();
                                return Ok(JsValue::String(ch));
                            }
                            "charCodeAt" => {
                                let idx = evaluated_args
                                    .first()
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0)
                                    as usize;
                                if let Some(c) = s.chars().nth(idx) {
                                    return Ok(JsValue::Number(c as u32 as f64));
                                }
                                return Ok(JsValue::Number(f64::NAN));
                            }
                            "slice" | "substring" => {
                                let start = evaluated_args
                                    .first()
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0)
                                    as isize;
                                let end = evaluated_args
                                    .get(1)
                                    .and_then(|v| v.as_f64())
                                    .map(|n| n as isize);
                                let len = s.chars().count() as isize;
                                let s_idx = if start < 0 {
                                    (len + start).max(0) as usize
                                } else {
                                    start.min(len) as usize
                                };
                                let e_idx = match end {
                                    Some(e) => {
                                        if e < 0 {
                                            (len + e).max(0) as usize
                                        } else {
                                            e.min(len) as usize
                                        }
                                    }
                                    None => len as usize,
                                };
                                if s_idx >= e_idx {
                                    return Ok(JsValue::String(String::new()));
                                }
                                let sub: String =
                                    s.chars().skip(s_idx).take(e_idx - s_idx).collect();
                                return Ok(JsValue::String(sub));
                            }
                            _ => {}
                        }
                    }

                    // DOM Node method calls: elem.appendChild, elem.insertBefore, elem.removeChild, etc.
                    if let JsValue::Object(ref map) = obj_val {
                        if let Some(node) = self.resolve_dom_node(&map.borrow()) {
                            match property.as_str() {
                                "setAttribute" => {
                                    if evaluated_args.len() >= 2 {
                                        if let (Some(k), Some(v)) =
                                            (evaluated_args[0].as_str(), evaluated_args[1].as_str())
                                        {
                                            node.set_attribute(k, v);
                                            self.document.mark_mutated();
                                        }
                                    }
                                    return Ok(JsValue::Undefined);
                                }
                                "getAttribute" => {
                                    if let Some(k) = evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        if let Some(v) = node.get_attribute(k) {
                                            return Ok(JsValue::String(v));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "removeAttribute" => {
                                    if let Some(k) = evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        node.remove_attribute(k);
                                        self.document.mark_mutated();
                                    }
                                    return Ok(JsValue::Undefined);
                                }
                                "hasAttribute" => {
                                    if let Some(k) = evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        return Ok(JsValue::Bool(node.has_attribute(k)));
                                    }
                                    return Ok(JsValue::Bool(false));
                                }
                                "appendChild" => {
                                    if let Some(JsValue::Object(child_map)) = evaluated_args.first()
                                    {
                                        if let Some(child_node) =
                                            self.resolve_dom_node(&child_map.borrow())
                                        {
                                            node.append_child(child_node.clone());
                                            self.document.mark_mutated();
                                            return Ok(self.dom_node_to_js(&child_node));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "prepend" => {
                                    if let Some(JsValue::Object(child_map)) = evaluated_args.first()
                                    {
                                        if let Some(child_node) =
                                            self.resolve_dom_node(&child_map.borrow())
                                        {
                                            node.prepend_child(child_node.clone());
                                            self.document.mark_mutated();
                                            return Ok(self.dom_node_to_js(&child_node));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "insertBefore" => {
                                    if let Some(JsValue::Object(new_map)) = evaluated_args.first() {
                                        if let Some(new_node) =
                                            self.resolve_dom_node(&new_map.borrow())
                                        {
                                            let ref_node = if evaluated_args.len() >= 2 {
                                                if let Some(JsValue::Object(ref_map)) =
                                                    evaluated_args.get(1)
                                                {
                                                    self.resolve_dom_node(&ref_map.borrow())
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            };
                                            let inserted = node.insert_before(
                                                new_node.clone(),
                                                ref_node.as_ref(),
                                            )?;
                                            self.document.mark_mutated();
                                            return Ok(self.dom_node_to_js(&inserted));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "replaceChild" => {
                                    if evaluated_args.len() >= 2 {
                                        if let (
                                            Some(JsValue::Object(new_map)),
                                            Some(JsValue::Object(old_map)),
                                        ) = (evaluated_args.first(), evaluated_args.get(1))
                                        {
                                            if let (Some(new_node), Some(old_node)) = (
                                                self.resolve_dom_node(&new_map.borrow()),
                                                self.resolve_dom_node(&old_map.borrow()),
                                            ) {
                                                let replaced =
                                                    node.replace_child(new_node, &old_node)?;
                                                self.document.mark_mutated();
                                                return Ok(self.dom_node_to_js(&replaced));
                                            }
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "removeChild" => {
                                    if let Some(JsValue::Object(child_map)) = evaluated_args.first()
                                    {
                                        if let Some(child_node) =
                                            self.resolve_dom_node(&child_map.borrow())
                                        {
                                            let removed = node.remove_child(&child_node);
                                            if removed {
                                                self.document.mark_mutated();
                                                return Ok(self.dom_node_to_js(&child_node));
                                            }
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "remove" => {
                                    node.remove();
                                    self.document.mark_mutated();
                                    return Ok(JsValue::Undefined);
                                }
                                "cloneNode" => {
                                    let deep = evaluated_args
                                        .first()
                                        .map(|v| v.is_truthy())
                                        .unwrap_or(false);
                                    let cloned = node.clone_node(deep);
                                    return Ok(self.dom_node_to_js(&cloned));
                                }
                                "contains" => {
                                    if let Some(JsValue::Object(other_map)) = evaluated_args.first()
                                    {
                                        if let Some(other_node) =
                                            self.resolve_dom_node(&other_map.borrow())
                                        {
                                            return Ok(JsValue::Bool(node.contains(&other_node)));
                                        }
                                    }
                                    return Ok(JsValue::Bool(false));
                                }
                                "querySelector" => {
                                    if let Some(sel) =
                                        evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        if let Some(found) = node.query_selector(sel) {
                                            return Ok(self.dom_node_to_js(&found));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "querySelectorAll" => {
                                    if let Some(sel) =
                                        evaluated_args.first().and_then(|v| v.as_str())
                                    {
                                        let found = node.query_selector_all(sel);
                                        let list: Vec<JsValue> =
                                            found.iter().map(|n| self.dom_node_to_js(n)).collect();
                                        return Ok(JsValue::new_array(list));
                                    }
                                    return Ok(JsValue::empty_array());
                                }
                                "addEventListener" => {
                                    if evaluated_args.len() >= 2 {
                                        if let Some(event) = evaluated_args[0].as_str() {
                                            node.add_event_listener(
                                                event,
                                                evaluated_args[1].clone(),
                                            );
                                        }
                                    }
                                    return Ok(JsValue::Undefined);
                                }
                                _ => {}
                            }
                        }
                    }
                }

                if let Expr::Member {
                    object,
                    property,
                    computed: None,
                }
                | Expr::OptionalMember {
                    object,
                    property,
                    computed: None,
                } = &**callee
                {
                    let obj_val = self.eval_expr(object)?;

                    // Promise .then / .catch / .finally with authentic JsValue::Promise support
                    let promise_opt = match &obj_val {
                        JsValue::Promise(p) => Some(p.clone()),
                        _ => None,
                    };
                    if let Some(promise) = promise_opt {
                        match property.as_str() {
                            "then" => {
                                let cb = evaluated_args.first().cloned();
                                let (next_promise, resolve, reject) =
                                    JsPromise::<JsValue>::new_pending();
                                let mut next_val = JsValue::Undefined;
                                let mut had_err = None;

                                match promise.state() {
                                    PromiseState::Fulfilled(v) => {
                                        if let Some(ref callback) = cb {
                                            match self.call_value(callback, vec![v]) {
                                                Ok(res) => next_val = res,
                                                Err(e) => had_err = Some(e),
                                            }
                                        } else {
                                            next_val = v;
                                        }
                                    }
                                    PromiseState::Rejected(err) => {
                                        had_err = Some(err.to_string());
                                    }
                                    PromiseState::Pending => {}
                                }

                                if let Some(e) = had_err {
                                    reject(JsValue::String(e));
                                } else {
                                    if let JsValue::Promise(ref sub_p) = next_val {
                                        match sub_p.state() {
                                            PromiseState::Fulfilled(sv) => resolve(sv),
                                            PromiseState::Rejected(se) => reject(se),
                                            PromiseState::Pending => resolve(next_val.clone()),
                                        }
                                        drain_microtasks();
                                        return Ok(self.create_promise_value(next_promise));
                                    }
                                    resolve(next_val);
                                }
                                drain_microtasks();
                                return Ok(self.create_promise_value(next_promise));
                            }
                            "catch" => {
                                let cb = evaluated_args.first().cloned();
                                let (next_promise, resolve, reject) =
                                    JsPromise::<JsValue>::new_pending();

                                match promise.state() {
                                    PromiseState::Rejected(err) => {
                                        if let Some(ref callback) = cb {
                                            match self.call_value(callback, vec![err]) {
                                                Ok(recovered) => resolve(recovered),
                                                Err(e) => reject(JsValue::String(e)),
                                            }
                                        } else {
                                            reject(err);
                                        }
                                    }
                                    PromiseState::Fulfilled(v) => {
                                        resolve(v);
                                    }
                                    PromiseState::Pending => {}
                                }
                                drain_microtasks();
                                return Ok(self.create_promise_value(next_promise));
                            }
                            "finally" => {
                                let cb = evaluated_args.first().cloned();
                                if let Some(ref callback) = cb {
                                    let _ = self.call_value(callback, Vec::new());
                                }
                                drain_microtasks();
                                return Ok(obj_val.clone());
                            }
                            _ => {}
                        }
                    }

                    if let JsValue::Object(ref map) = obj_val {
                        // Generator .next()
                        let gen_id_opt = {
                            let b = map.borrow();
                            if b.get("__is_generator__") == Some(&JsValue::Bool(true)) {
                                b.get("__gen_id__")
                                    .and_then(|v| v.as_f64())
                                    .map(|n| n as u64)
                            } else {
                                None
                            }
                        };
                        if let Some(gid) = gen_id_opt {
                            if property == "next" {
                                return self.step_generator(gid, evaluated_args.first().cloned());
                            }
                        }

                        // Date methods
                        let date_ms_opt = {
                            let b = map.borrow();
                            if b.get("__is_date__") == Some(&JsValue::Bool(true)) {
                                b.get("__date_ms__").and_then(|v| v.as_f64())
                            } else {
                                None
                            }
                        };
                        if let Some(ms) = date_ms_opt {
                            return self.eval_date_method(property, ms, &evaluated_args);
                        }

                        // WeakMap methods
                        let weak_map_id_opt = {
                            let b = map.borrow();
                            if b.get("__is_weak_map__") == Some(&JsValue::Bool(true)) {
                                b.get("__weak_id__")
                                    .and_then(|v| v.as_f64())
                                    .map(|n| n as u64)
                            } else {
                                None
                            }
                        };
                        if let Some(wid) = weak_map_id_opt {
                            return self.eval_weak_map_method(wid, property, &evaluated_args);
                        }

                        // WeakSet methods
                        let weak_set_id_opt = {
                            let b = map.borrow();
                            if b.get("__is_weak_set__") == Some(&JsValue::Bool(true)) {
                                b.get("__weak_id__")
                                    .and_then(|v| v.as_f64())
                                    .map(|n| n as u64)
                            } else {
                                None
                            }
                        };
                        if let Some(wid) = weak_set_id_opt {
                            return self.eval_weak_set_method(wid, property, &evaluated_args);
                        }

                        // WeakRef methods
                        let weak_ref_id_opt = {
                            let b = map.borrow();
                            if b.get("__is_weak_ref__") == Some(&JsValue::Bool(true)) {
                                b.get("__weak_id__")
                                    .and_then(|v| v.as_f64())
                                    .map(|n| n as u64)
                            } else {
                                None
                            }
                        };
                        if let Some(wid) = weak_ref_id_opt {
                            if property == "deref" {
                                let target_opt = self
                                    .weak_refs
                                    .borrow()
                                    .get(&wid)
                                    .and_then(|w| w.upgrade())
                                    .map(JsValue::Object);
                                return Ok(target_opt.unwrap_or(JsValue::Undefined));
                            }
                        }

                        // Response methods
                        let is_resp =
                            map.borrow().get("__is_response__") == Some(&JsValue::Bool(true));
                        if is_resp {
                            let body_str = map
                                .borrow()
                                .get("__body__")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            match property.as_str() {
                                "json" => {
                                    let parsed =
                                        crate::json::parse_json(&body_str).unwrap_or(JsValue::Null);
                                    let p = JsPromise::resolve(parsed);
                                    return Ok(self.create_promise_value(p));
                                }
                                "text" => {
                                    let p = JsPromise::resolve(JsValue::String(body_str));
                                    return Ok(self.create_promise_value(p));
                                }
                                _ => {}
                            }
                        }

                        if property == "get" && evaluated_args.len() == 1 {
                            if let Some(key) = evaluated_args[0].as_str() {
                                let b = map.borrow();
                                if let Some(val) = b.get(key) {
                                    return Ok(val.clone());
                                }
                                let key_lower = key.to_ascii_lowercase();
                                if let Some(val) = b.get(&key_lower) {
                                    return Ok(val.clone());
                                }
                            }
                            return Ok(JsValue::Null);
                        }
                        if property == "hasOwnProperty" && evaluated_args.len() == 1 {
                            let key = match &evaluated_args[0] {
                                JsValue::String(s) => s.clone(),
                                JsValue::Number(n) => format!("{n}"),
                                other => other.to_string(),
                            };
                            return Ok(JsValue::Bool(map.borrow().contains_key(&key)));
                        }
                        if property == "toString" && evaluated_args.is_empty() {
                            return Ok(JsValue::String("[object Object]".to_string()));
                        }

                        let cls_opt = map.borrow().get("__class__").and_then(|v| match v {
                            JsValue::String(s) => Some(s.clone()),
                            _ => None,
                        });
                        if let Some(cls_name) = cls_opt {
                            if let Some((params, body, super_class_opt)) =
                                self.resolve_method(&cls_name, property)
                            {
                                let method_scope = Scope::new_child(self.current_scope.clone());
                                method_scope.borrow_mut().declare("this", obj_val.clone());
                                method_scope.borrow_mut().declare(
                                    "__current_class__",
                                    JsValue::String(cls_name.clone()),
                                );
                                if let Some(s_name) = super_class_opt {
                                    method_scope
                                        .borrow_mut()
                                        .declare("__super_class__", JsValue::String(s_name));
                                }

                                bind_parameters(&params, evaluated_args, &method_scope);

                                let saved_scope = self.current_scope.clone();
                                self.current_scope = method_scope.clone();

                                let mut ret = JsValue::Undefined;
                                let mut err = None;
                                for stmt in &body {
                                    match self.execute_stmt_flow(stmt) {
                                        Ok(StatementFlow::Normal(v)) => ret = v,
                                        Ok(StatementFlow::Return(v)) => {
                                            ret = v;
                                            break;
                                        }
                                        Ok(StatementFlow::Throw(v)) => {
                                            err = Some(format!("Uncaught exception: {v}"));
                                            break;
                                        }
                                        Ok(StatementFlow::Break(_)) => {
                                            err = Some(
                                                "SyntaxError: Illegal break statement".to_string(),
                                            );
                                            break;
                                        }
                                        Ok(StatementFlow::Continue(_)) => {
                                            err = Some(
                                                "SyntaxError: Illegal continue statement"
                                                    .to_string(),
                                            );
                                            break;
                                        }
                                        Ok(StatementFlow::Yield(v)) => {
                                            ret = v;
                                            break;
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

                                return Ok(ret);
                            }
                        }
                    }
                }

                // Identifier calls: check lexical scope chain -> globals -> legacy user_functions
                if let Expr::Identifier(fn_name) = &**callee {
                    let scope_val = self.current_scope.borrow().get(fn_name);
                    if let Some(val) = scope_val {
                        return self.call_value(&val, evaluated_args);
                    }
                    if let Some(val) = self.globals.get(fn_name).cloned() {
                        return self.call_value(&val, evaluated_args);
                    }
                    if let Some(closure) = self.user_functions.get(fn_name).cloned() {
                        return self.call_closure(&closure, evaluated_args);
                    }
                }

                let this_val = match &**callee {
                    Expr::Member { object, .. } | Expr::OptionalMember { object, .. } => {
                        self.eval_expr(object).ok()
                    }
                    _ => None,
                };
                let callee_val = self.eval_expr(callee)?;
                self.call_value_with_this(&callee_val, evaluated_args, this_val)
            }
            Expr::OptionalCall { callee, args } => {
                if let Expr::OptionalMember { object, .. } | Expr::Member { object, .. } = &**callee
                {
                    let base = self.eval_expr(object)?;
                    if matches!(base, JsValue::Null | JsValue::Undefined) {
                        return Ok(JsValue::Undefined);
                    }
                }
                let callee_val = self.eval_expr(callee)?;
                if matches!(callee_val, JsValue::Null | JsValue::Undefined) {
                    return Ok(JsValue::Undefined);
                }
                let temp_call = Expr::Call {
                    callee: callee.clone(),
                    args: args.clone(),
                };
                self.eval_expr(&temp_call)
            }
            Expr::New { callee, args } => {
                let mut evaluated_args = Vec::new();
                for a in args {
                    if let Expr::Spread(inner) = a {
                        let spread_val = self.eval_expr(inner)?;
                        match spread_val {
                            JsValue::Array(items) => evaluated_args.extend(items.borrow().clone()),
                            JsValue::String(s) => {
                                for ch in s.chars() {
                                    evaluated_args.push(JsValue::String(ch.to_string()));
                                }
                            }
                            other => evaluated_args.push(other),
                        }
                    } else {
                        evaluated_args.push(self.eval_expr(a)?);
                    }
                }
                match &**callee {
                    Expr::Identifier(name) => {
                        if name == "Promise" {
                            let pid = self.next_promise_id;
                            self.next_promise_id += 1;
                            let (promise, resolve_fn, reject_fn) =
                                JsPromise::<JsValue>::new_pending();
                            self.promises.insert(pid, promise.clone());
                            self.promise_controllers
                                .insert(pid, (resolve_fn, reject_fn));

                            if let Some(executor) = evaluated_args.first().cloned() {
                                let mut res_map = BTreeMap::new();
                                res_map.insert(
                                    "__fn__".to_string(),
                                    JsValue::String(format!("__promise_res_{pid}")),
                                );
                                let res_val = JsValue::new_object(res_map);

                                let mut rej_map = BTreeMap::new();
                                rej_map.insert(
                                    "__fn__".to_string(),
                                    JsValue::String(format!("__promise_rej_{pid}")),
                                );
                                let rej_val = JsValue::new_object(rej_map);

                                self.call_value(&executor, vec![res_val, rej_val])?;
                            }
                            drain_microtasks();

                            Ok(JsValue::Promise(promise))
                        } else if matches!(
                            name.as_str(),
                            "Error" | "TypeError" | "ReferenceError" | "SyntaxError" | "RangeError"
                        ) {
                            let msg = evaluated_args
                                .first()
                                .map(|v| v.to_string())
                                .unwrap_or_default();
                            let mut map = BTreeMap::new();
                            map.insert("__is_error__".to_string(), JsValue::Bool(true));
                            map.insert("name".to_string(), JsValue::String(name.clone()));
                            map.insert("message".to_string(), JsValue::String(msg.clone()));
                            map.insert(
                                "stack".to_string(),
                                JsValue::String(format!("{name}: {msg}\n    at strata_js")),
                            );
                            map.insert(
                                "__proto__".to_string(),
                                JsValue::new_object(self.error_prototype.clone()),
                            );
                            Ok(JsValue::new_object(map))
                        } else if name == "Proxy" {
                            if evaluated_args.len() < 2 {
                                return Err(
                                    "TypeError: Cannot create Proxy with fewer than 2 arguments"
                                        .to_string(),
                                );
                            }
                            let target = evaluated_args[0].clone();
                            let handler = evaluated_args[1].clone();
                            if !matches!(
                                target,
                                JsValue::Object(_) | JsValue::Array(_) | JsValue::Function(_)
                            ) {
                                return Err(
                                    "TypeError: Cannot create proxy with a non-object as target"
                                        .to_string(),
                                );
                            }
                            if !matches!(handler, JsValue::Object(_)) {
                                return Err(
                                    "TypeError: Cannot create proxy with a non-object as handler"
                                        .to_string(),
                                );
                            }
                            let mut p_map = BTreeMap::new();
                            p_map.insert("__is_proxy__".to_string(), JsValue::Bool(true));
                            p_map.insert("__proxy_target__".to_string(), target);
                            p_map.insert("__proxy_handler__".to_string(), handler);
                            Ok(JsValue::new_object(p_map))
                        } else if name == "Date" {
                            let ms = if evaluated_args.is_empty() {
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as f64)
                                    .unwrap_or(0.0)
                            } else if evaluated_args.len() == 1 {
                                match &evaluated_args[0] {
                                    JsValue::Number(n) => *n,
                                    JsValue::String(s) => s.parse::<f64>().unwrap_or(f64::NAN),
                                    _ => f64::NAN,
                                }
                            } else {
                                let year = evaluated_args[0].as_f64().unwrap_or(0.0);
                                let month = evaluated_args
                                    .get(1)
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0);
                                let day = evaluated_args
                                    .get(2)
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(1.0);
                                let hours = evaluated_args
                                    .get(3)
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0);
                                let minutes = evaluated_args
                                    .get(4)
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0);
                                let seconds = evaluated_args
                                    .get(5)
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0);
                                let ms = evaluated_args
                                    .get(6)
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0);
                                crate::engine::date_to_ms(
                                    year, month, day, hours, minutes, seconds, ms,
                                )
                            };
                            let mut d_map = BTreeMap::new();
                            d_map.insert("__is_date__".to_string(), JsValue::Bool(true));
                            d_map.insert("__date_ms__".to_string(), JsValue::Number(ms));
                            d_map.insert(
                                "__proto__".to_string(),
                                JsValue::new_object(self.object_prototype.clone()),
                            );
                            Ok(JsValue::new_object(d_map))
                        } else if name == "BigInt" {
                            Err("TypeError: BigInt is not a constructor".to_string())
                        } else if name == "WeakMap" {
                            let wid = self.next_weak_id;
                            self.next_weak_id += 1;
                            self.weak_maps
                                .borrow_mut()
                                .insert(wid, std::collections::HashMap::new());
                            let mut w_map = BTreeMap::new();
                            w_map.insert("__is_weak_map__".to_string(), JsValue::Bool(true));
                            w_map.insert("__weak_id__".to_string(), JsValue::Number(wid as f64));
                            w_map.insert(
                                "__proto__".to_string(),
                                JsValue::new_object(self.object_prototype.clone()),
                            );
                            Ok(JsValue::new_object(w_map))
                        } else if name == "WeakSet" {
                            let wid = self.next_weak_id;
                            self.next_weak_id += 1;
                            self.weak_sets
                                .borrow_mut()
                                .insert(wid, std::collections::HashSet::new());
                            let mut w_map = BTreeMap::new();
                            w_map.insert("__is_weak_set__".to_string(), JsValue::Bool(true));
                            w_map.insert("__weak_id__".to_string(), JsValue::Number(wid as f64));
                            w_map.insert(
                                "__proto__".to_string(),
                                JsValue::new_object(self.object_prototype.clone()),
                            );
                            Ok(JsValue::new_object(w_map))
                        } else if name == "WeakRef" {
                            let target = evaluated_args
                                .first()
                                .cloned()
                                .unwrap_or(JsValue::Undefined);
                            let target_obj = match target {
                                JsValue::Object(ref o) => std::rc::Rc::downgrade(o),
                                _ => {
                                    return Err(
                                        "TypeError: \"target\" must be an object".to_string()
                                    )
                                }
                            };
                            let wid = self.next_weak_id;
                            self.next_weak_id += 1;
                            self.weak_refs.borrow_mut().insert(wid, target_obj);
                            let mut w_map = BTreeMap::new();
                            w_map.insert("__is_weak_ref__".to_string(), JsValue::Bool(true));
                            w_map.insert("__weak_id__".to_string(), JsValue::Number(wid as f64));
                            w_map.insert(
                                "__proto__".to_string(),
                                JsValue::new_object(self.object_prototype.clone()),
                            );
                            Ok(JsValue::new_object(w_map))
                        } else if name == "Symbol" {
                            Err("TypeError: Symbol is not a constructor".to_string())
                        } else if name == "RegExp" {
                            let pat = evaluated_args
                                .first()
                                .map(|v| match v {
                                    JsValue::Object(m)
                                        if m.borrow().get("__is_regex__")
                                            == Some(&JsValue::Bool(true)) =>
                                    {
                                        m.borrow()
                                            .get("source")
                                            .and_then(|s| s.as_str())
                                            .unwrap_or("")
                                            .to_string()
                                    }
                                    _ => v.to_string(),
                                })
                                .unwrap_or_default();
                            let flg = if evaluated_args.len() > 1 {
                                evaluated_args[1].to_string()
                            } else {
                                String::new()
                            };
                            self.create_regex_value(&pat, &flg)
                        } else if name == "Array" {
                            let arr = if evaluated_args.len() == 1 {
                                if let Some(JsValue::Number(n)) = evaluated_args.first() {
                                    if *n >= 0.0 && n.fract() == 0.0 {
                                        let count = *n as usize;
                                        self.allocate_bytes(count.saturating_mul(16))?;
                                        vec![JsValue::Undefined; count]
                                    } else {
                                        return Err("RangeError: Invalid array length".to_string());
                                    }
                                } else {
                                    self.allocate_bytes(16)?;
                                    evaluated_args
                                }
                            } else {
                                self.allocate_bytes(evaluated_args.len().saturating_mul(16))?;
                                evaluated_args
                            };
                            Ok(JsValue::new_array(arr))
                        } else if name == "Object" {
                            Ok(JsValue::empty_object())
                        } else if self.classes.contains_key(name) {
                            self.instantiate_class(name, evaluated_args)
                        } else {
                            Err(format!("TypeError: {name} is not a constructor"))
                        }
                    }
                    other => {
                        let callee_val = self.eval_expr(other)?;
                        if let JsValue::Object(ref map) = callee_val {
                            let cls_opt =
                                map.borrow().get("__class_name__").and_then(|v| match v {
                                    JsValue::String(s) => Some(s.clone()),
                                    _ => None,
                                });
                            if let Some(cls_name) = cls_opt {
                                if cls_name == "Promise" {
                                    let pid = self.next_promise_id;
                                    self.next_promise_id += 1;
                                    let (promise, resolve_fn, reject_fn) =
                                        JsPromise::<JsValue>::new_pending();
                                    self.promises.insert(pid, promise.clone());
                                    self.promise_controllers
                                        .insert(pid, (resolve_fn, reject_fn));

                                    if let Some(executor) = evaluated_args.first().cloned() {
                                        let mut res_map = BTreeMap::new();
                                        res_map.insert(
                                            "__fn__".to_string(),
                                            JsValue::String(format!("__promise_res_{pid}")),
                                        );
                                        let res_val = JsValue::new_object(res_map);

                                        let mut rej_map = BTreeMap::new();
                                        rej_map.insert(
                                            "__fn__".to_string(),
                                            JsValue::String(format!("__promise_rej_{pid}")),
                                        );
                                        let rej_val = JsValue::new_object(rej_map);

                                        self.call_value(&executor, vec![res_val, rej_val])?;
                                    }
                                    drain_microtasks();

                                    return Ok(JsValue::Promise(promise));
                                }
                                if self.classes.contains_key(&cls_name) {
                                    return self.instantiate_class(&cls_name, evaluated_args);
                                }
                            }
                        }
                        Err("TypeError: Value is not a constructor".to_string())
                    }
                }
            }
            Expr::Spread(inner) => self.eval_expr(inner),
            Expr::Array(items) => {
                let mut vals = Vec::new();
                for item in items {
                    if let Expr::Spread(inner) = item {
                        let spread_val = self.eval_expr(inner)?;
                        match spread_val {
                            JsValue::Array(inner_items) => {
                                vals.extend(inner_items.borrow().clone())
                            }
                            JsValue::String(s) => {
                                for ch in s.chars() {
                                    vals.push(JsValue::String(ch.to_string()));
                                }
                            }
                            other => vals.push(other),
                        }
                    } else {
                        vals.push(self.eval_expr(item)?);
                    }
                }
                Ok(JsValue::new_array(vals))
            }
            Expr::Object(pairs) => {
                let mut map = BTreeMap::new();
                for (k, v) in pairs {
                    if let Expr::Spread(inner) = v {
                        let spread_val = self.eval_expr(inner)?;
                        if let JsValue::Object(inner_map) = spread_val {
                            for (ik, iv) in inner_map.borrow().iter() {
                                map.insert(ik.clone(), iv.clone());
                            }
                        }
                    } else {
                        map.insert(k.clone(), self.eval_expr(v)?);
                    }
                }
                Ok(JsValue::new_object(map))
            }
            Expr::RegExpLiteral { pattern, flags } => self.create_regex_value(pattern, flags),
            Expr::Yield(inner) => {
                let val = if let Some(expr) = inner {
                    self.eval_expr(expr)?
                } else {
                    JsValue::Undefined
                };
                self.pending_yield = Some(val.clone());
                Ok(val)
            }
        }
    }

    fn eval_member_access(
        &mut self,
        object: &Expr,
        obj_val: &JsValue,
        property: &str,
        computed: Option<&Expr>,
    ) -> Result<JsValue, String> {
        if matches!(obj_val, JsValue::Null | JsValue::Undefined) {
            return Err(format!(
                "TypeError: Cannot read properties of {} (reading '{}')",
                obj_val.js_typeof(),
                property
            ));
        }

        // Check Proxy get trap
        if let JsValue::Object(ref map) = obj_val {
            let is_proxy = {
                let b = map.borrow();
                b.get("__is_proxy__") == Some(&JsValue::Bool(true))
            };
            if is_proxy {
                let (target, handler) = {
                    let b = map.borrow();
                    (
                        b.get("__proxy_target__")
                            .cloned()
                            .unwrap_or(JsValue::Undefined),
                        b.get("__proxy_handler__")
                            .cloned()
                            .unwrap_or(JsValue::Undefined),
                    )
                };
                if let JsValue::Object(ref h_cell) = handler {
                    let get_trap = h_cell.borrow().get("get").cloned();
                    if let Some(trap_fn) = get_trap {
                        let prop_key = match computed {
                            Some(comp_expr) => self.eval_expr(comp_expr)?,
                            None => JsValue::String(property.to_string()),
                        };
                        return self.call_value(&trap_fn, vec![target, prop_key, obj_val.clone()]);
                    }
                }
                return self.eval_member_access(object, &target, property, computed);
            }
        }

        if let JsValue::Promise(_) = obj_val {
            let key = match computed {
                Some(comp_expr) => {
                    let prop_val = self.eval_expr(comp_expr)?;
                    prop_val.to_string()
                }
                None => property.to_string(),
            };
            if matches!(key.as_str(), "then" | "catch" | "finally") {
                let mut fn_map = BTreeMap::new();
                fn_map.insert(
                    "__fn__".to_string(),
                    JsValue::String(format!("__promise_method_{key}")),
                );
                return Ok(JsValue::new_object(fn_map));
            }
            if key == "Symbol(Symbol.toStringTag)" || key == "toStringTag" {
                return Ok(JsValue::String("Promise".to_string()));
            }
            if let Some(proto_val) = self.object_prototype.get(&key) {
                return Ok(proto_val.clone());
            }
            return Ok(JsValue::Undefined);
        }

        if let Some(comp_expr) = computed {
            let prop_val = self.eval_expr(comp_expr)?;
            let key = match &prop_val {
                JsValue::String(s) => s.clone(),
                JsValue::Number(n) => format!("{n}"),
                JsValue::Symbol(s) => format!("Symbol({})", s.description.as_deref().unwrap_or("")),
                other => other.to_string(),
            };
            if let JsValue::Array(ref arr) = obj_val {
                if let Some(idx) = prop_val.as_f64() {
                    return Ok(arr
                        .borrow()
                        .get(idx as usize)
                        .cloned()
                        .unwrap_or(JsValue::Undefined));
                }
                if let Some(proto_val) = self.array_prototype.get(&key) {
                    return Ok(proto_val.clone());
                }
                if let Some(proto_val) = self.object_prototype.get(&key) {
                    return Ok(proto_val.clone());
                }
            }
            if let JsValue::Object(ref map) = obj_val {
                let b = map.borrow();
                if let Some(v) = b.get(&key) {
                    return Ok(v.clone());
                }
                let mut cur_proto = b.get("__proto__").cloned();
                let mut depth = 0;
                while let Some(JsValue::Object(p_map)) = cur_proto {
                    depth += 1;
                    if depth > 64 {
                        break;
                    }
                    let pb = p_map.borrow();
                    if let Some(v) = pb.get(&key) {
                        return Ok(v.clone());
                    }
                    cur_proto = pb.get("__proto__").cloned();
                }
                if let Some(v) = self.object_prototype.get(&key) {
                    return Ok(v.clone());
                }
                return Ok(JsValue::Undefined);
            }
            if let JsValue::String(ref s) = obj_val {
                if let Some(idx) = prop_val.as_f64() {
                    return Ok(s
                        .chars()
                        .nth(idx as usize)
                        .map(|c| JsValue::String(c.to_string()))
                        .unwrap_or(JsValue::Undefined));
                }
                if let Some(proto_val) = self.string_prototype.get(&key) {
                    return Ok(proto_val.clone());
                }
                if let Some(proto_val) = self.object_prototype.get(&key) {
                    return Ok(proto_val.clone());
                }
            }
            return Ok(JsValue::Undefined);
        }

        if let Expr::Identifier(id) = object {
            if id == "document" {
                match property {
                    "body" => {
                        let mut map = BTreeMap::new();
                        map.insert("__is_body__".to_string(), JsValue::Bool(true));
                        map.insert(
                            "innerHTML".to_string(),
                            JsValue::String(self.document.body.inner_html()),
                        );
                        map.insert(
                            "textContent".to_string(),
                            JsValue::String(self.document.body.text_content()),
                        );
                        return Ok(JsValue::new_object(map));
                    }
                    "title" => {
                        return Ok(JsValue::String(self.document.title.borrow().clone()));
                    }
                    _ => {}
                }
            } else if id == "Symbol" {
                match property {
                    "iterator" => return Ok(JsValue::Symbol(SymbolId::iterator())),
                    "toStringTag" => return Ok(JsValue::Symbol(SymbolId::to_string_tag())),
                    "hasInstance" => return Ok(JsValue::Symbol(SymbolId::has_instance())),
                    "toPrimitive" => return Ok(JsValue::Symbol(SymbolId::to_primitive())),
                    _ => {}
                }
            } else if id == "Object" && property == "prototype" {
                return Ok(JsValue::new_object(self.object_prototype.clone()));
            } else if id == "Array" && property == "prototype" {
                return Ok(JsValue::new_object(self.array_prototype.clone()));
            } else if id == "Function" && property == "prototype" {
                return Ok(JsValue::new_object(self.function_prototype.clone()));
            } else if id == "String" && property == "prototype" {
                return Ok(JsValue::new_object(self.string_prototype.clone()));
            } else if id == "Error" && property == "prototype" {
                return Ok(JsValue::new_object(self.error_prototype.clone()));
            }
        }

        if property == "length" {
            match obj_val {
                JsValue::Array(arr) => return Ok(JsValue::Number(arr.borrow().len() as f64)),
                JsValue::String(s) => return Ok(JsValue::Number(s.len() as f64)),
                _ => {}
            }
        }

        if let JsValue::Symbol(ref sym) = obj_val {
            if property == "description" {
                return Ok(sym
                    .description
                    .as_ref()
                    .map(|s| JsValue::String(s.clone()))
                    .unwrap_or(JsValue::Undefined));
            }
            return Ok(JsValue::Undefined);
        }

        if let JsValue::Array(arr) = obj_val {
            if let Ok(idx) = property.parse::<usize>() {
                return Ok(arr.borrow().get(idx).cloned().unwrap_or(JsValue::Undefined));
            }
            if let Some(proto_val) = self.array_prototype.get(property) {
                return Ok(proto_val.clone());
            }
            if let Some(proto_val) = self.object_prototype.get(property) {
                return Ok(proto_val.clone());
            }
        }

        if let JsValue::String(_) = obj_val {
            if let Some(proto_val) = self.string_prototype.get(property) {
                return Ok(proto_val.clone());
            }
            if let Some(proto_val) = self.object_prototype.get(property) {
                return Ok(proto_val.clone());
            }
        }

        if let JsValue::Function(_) = obj_val {
            if let Some(proto_val) = self.function_prototype.get(property) {
                return Ok(proto_val.clone());
            }
            if let Some(proto_val) = self.object_prototype.get(property) {
                return Ok(proto_val.clone());
            }
        }

        if let JsValue::Object(ref map) = obj_val {
            let b = map.borrow();
            if let Some(node) = self.resolve_dom_node(&b) {
                match property {
                    "parentNode" => {
                        return Ok(node
                            .parent_node()
                            .map(|p| self.dom_node_to_js(&p))
                            .unwrap_or(JsValue::Null));
                    }
                    "parentElement" => {
                        return Ok(node
                            .parent_element()
                            .map(|p| self.dom_node_to_js(&p))
                            .unwrap_or(JsValue::Null));
                    }
                    "firstChild" => {
                        return Ok(node
                            .first_child()
                            .map(|c| self.dom_node_to_js(&c))
                            .unwrap_or(JsValue::Null));
                    }
                    "lastChild" => {
                        return Ok(node
                            .last_child()
                            .map(|c| self.dom_node_to_js(&c))
                            .unwrap_or(JsValue::Null));
                    }
                    "firstElementChild" => {
                        return Ok(node
                            .first_element_child()
                            .map(|c| self.dom_node_to_js(&c))
                            .unwrap_or(JsValue::Null));
                    }
                    "lastElementChild" => {
                        return Ok(node
                            .last_element_child()
                            .map(|c| self.dom_node_to_js(&c))
                            .unwrap_or(JsValue::Null));
                    }
                    "previousSibling" => {
                        return Ok(node
                            .previous_sibling()
                            .map(|s| self.dom_node_to_js(&s))
                            .unwrap_or(JsValue::Null));
                    }
                    "nextSibling" => {
                        return Ok(node
                            .next_sibling()
                            .map(|s| self.dom_node_to_js(&s))
                            .unwrap_or(JsValue::Null));
                    }
                    "previousElementSibling" => {
                        return Ok(node
                            .previous_element_sibling()
                            .map(|s| self.dom_node_to_js(&s))
                            .unwrap_or(JsValue::Null));
                    }
                    "nextElementSibling" => {
                        return Ok(node
                            .next_element_sibling()
                            .map(|s| self.dom_node_to_js(&s))
                            .unwrap_or(JsValue::Null));
                    }
                    "childNodes" => {
                        let children_js = node
                            .children()
                            .iter()
                            .map(|c| self.dom_node_to_js(c))
                            .collect();
                        return Ok(JsValue::new_array(children_js));
                    }
                    "children" => {
                        let children_js = node
                            .children()
                            .iter()
                            .filter(|c| {
                                matches!(c.inner.borrow().node_type, DomNodeType::Element { .. })
                            })
                            .map(|c| self.dom_node_to_js(c))
                            .collect();
                        return Ok(JsValue::new_array(children_js));
                    }
                    "childElementCount" => {
                        return Ok(JsValue::Number(node.child_element_count() as f64));
                    }
                    "innerHTML" => {
                        return Ok(JsValue::String(node.inner_html()));
                    }
                    "outerHTML" => {
                        return Ok(JsValue::String(node.outer_html()));
                    }
                    "textContent" => {
                        return Ok(JsValue::String(node.text_content()));
                    }
                    "className" => {
                        return Ok(JsValue::String(node.class_name()));
                    }
                    "tagName" => {
                        return Ok(JsValue::String(node.tag_name().unwrap_or_default()));
                    }
                    "id" => {
                        return Ok(JsValue::String(node.id().unwrap_or_default()));
                    }
                    "value" => {
                        return Ok(JsValue::String(
                            node.get_attribute("value").unwrap_or_default(),
                        ));
                    }
                    _ => {}
                }
            }

            if let Some(v) = b.get(property) {
                return Ok(v.clone());
            }

            // Prototype chain traversal (__proto__)
            let mut cur_proto = b.get("__proto__").cloned();
            let mut depth = 0;
            while let Some(JsValue::Object(p_map)) = cur_proto {
                depth += 1;
                if depth > 64 {
                    break;
                }
                let pb = p_map.borrow();
                if let Some(v) = pb.get(property) {
                    return Ok(v.clone());
                }
                cur_proto = pb.get("__proto__").cloned();
            }

            // Built-in Object.prototype fallback
            if let Some(v) = self.object_prototype.get(property) {
                return Ok(v.clone());
            }

            return Ok(JsValue::Undefined);
        }
        Ok(JsValue::Undefined)
    }

    fn resolve_dom_node(&self, map: &BTreeMap<String, JsValue>) -> Option<DomNode> {
        if let Some(JsValue::Number(uid)) = map.get("__uid__") {
            let u = *uid as u64;
            if let Some(node) = self.live_nodes.borrow().get(&u).cloned() {
                return Some(node);
            }
            if let Some(found) = self.document.get_element_by_uid(u) {
                self.live_nodes.borrow_mut().insert(u, found.clone());
                return Some(found);
            }
        }
        if let Some(JsValue::String(id)) = map.get("__dom_id__") {
            if let Some(found) = self.document.get_element_by_id(id) {
                self.live_nodes
                    .borrow_mut()
                    .insert(found.uid(), found.clone());
                return Some(found);
            }
        }
        None
    }

    pub fn call_closure(
        &mut self,
        closure: &JsClosure,
        args: Vec<JsValue>,
    ) -> Result<JsValue, String> {
        self.call_closure_with_this(closure, args, None)
    }

    pub fn call_closure_with_this(
        &mut self,
        closure: &JsClosure,
        args: Vec<JsValue>,
        this_val: Option<JsValue>,
    ) -> Result<JsValue, String> {
        if self.call_depth >= MAX_CALL_STACK_DEPTH {
            return Err("RangeError: Maximum call stack size exceeded".to_string());
        }
        self.call_depth += 1;
        let res = self.call_closure_inner(closure, args, this_val);
        self.call_depth -= 1;
        res
    }

    fn call_closure_inner(
        &mut self,
        closure: &JsClosure,
        args: Vec<JsValue>,
        this_val: Option<JsValue>,
    ) -> Result<JsValue, String> {
        let call_scope = Scope::new_child(closure.env.clone());
        if let Some(t) = this_val {
            call_scope.borrow_mut().declare("this", t);
        }
        bind_parameters(&closure.params, args, &call_scope);

        if closure.is_generator {
            let gen_id = self.next_generator_id;
            self.next_generator_id += 1;
            let instance = crate::engine::GeneratorInstance {
                closure: closure.clone(),
                scope: call_scope,
                pc: 0,
                done: false,
            };
            self.generators.borrow_mut().insert(gen_id, instance);

            let mut gen_map = BTreeMap::new();
            gen_map.insert("__is_generator__".to_string(), JsValue::Bool(true));
            gen_map.insert("__gen_id__".to_string(), JsValue::Number(gen_id as f64));
            gen_map.insert(
                "__proto__".to_string(),
                JsValue::new_object(self.object_prototype.clone()),
            );
            return Ok(JsValue::new_object(gen_map));
        }

        let saved_scope = self.current_scope.clone();
        self.current_scope = call_scope;

        let mut ret = JsValue::Undefined;
        let mut thrown = None;
        let mut sys_err = None;
        for stmt in &closure.body {
            match self.execute_stmt_flow(stmt) {
                Ok(StatementFlow::Normal(v)) => ret = v,
                Ok(StatementFlow::Return(v)) => {
                    ret = v;
                    break;
                }
                Ok(StatementFlow::Throw(v)) => {
                    thrown = Some(v);
                    break;
                }
                Ok(StatementFlow::Break(_)) => {
                    sys_err = Some("SyntaxError: Illegal break statement".to_string());
                    break;
                }
                Ok(StatementFlow::Continue(_)) => {
                    sys_err = Some("SyntaxError: Illegal continue statement".to_string());
                    break;
                }
                Ok(StatementFlow::Yield(v)) => {
                    ret = v;
                    break;
                }
                Err(e) => {
                    sys_err = Some(e);
                    break;
                }
            }
        }

        self.current_scope = saved_scope;

        if closure.is_async {
            if let Some(err_val) = thrown {
                let p = JsPromise::reject(err_val);
                return Ok(self.create_promise_value(p));
            }
            if let Some(e) = sys_err {
                let p = JsPromise::reject(JsValue::String(e));
                return Ok(self.create_promise_value(p));
            }
            if let JsValue::Promise(_) = ret {
                return Ok(ret);
            }
            let p = JsPromise::resolve(ret);
            return Ok(self.create_promise_value(p));
        }

        if let Some(v) = thrown {
            self.pending_throw = Some(v);
            return Ok(JsValue::Undefined);
        }

        if let Some(err_res) = sys_err {
            return Err(err_res);
        }

        Ok(ret)
    }

    pub fn step_generator(
        &mut self,
        gen_id: u64,
        _next_arg: Option<JsValue>,
    ) -> Result<JsValue, String> {
        let (closure, scope, mut pc, done) = match self.generators.borrow().get(&gen_id) {
            Some(inst) => (inst.closure.clone(), inst.scope.clone(), inst.pc, inst.done),
            None => return Err("TypeError: Invalid generator".to_string()),
        };

        if done {
            let mut res = BTreeMap::new();
            res.insert("value".to_string(), JsValue::Undefined);
            res.insert("done".to_string(), JsValue::Bool(true));
            return Ok(JsValue::new_object(res));
        }

        let saved_scope = self.current_scope.clone();
        self.current_scope = scope.clone();

        let stmts = closure.body.clone();
        let mut ret = JsValue::Undefined;
        let mut yielded = false;
        let mut thrown = None;
        let mut is_done = false;

        while pc < stmts.len() {
            let stmt = &stmts[pc];
            pc += 1;

            match self.execute_stmt_flow(stmt) {
                Ok(StatementFlow::Normal(v)) => {
                    ret = v;
                    if let Some(y) = self.pending_yield.take() {
                        ret = y;
                        yielded = true;
                        break;
                    }
                }
                Ok(StatementFlow::Return(v)) => {
                    ret = v;
                    is_done = true;
                    break;
                }
                Ok(StatementFlow::Yield(v)) => {
                    ret = v;
                    yielded = true;
                    break;
                }
                Ok(StatementFlow::Throw(v)) => {
                    thrown = Some(v);
                    is_done = true;
                    break;
                }
                Ok(StatementFlow::Break(_)) => {
                    thrown = Some(JsValue::String(
                        "SyntaxError: Illegal break statement".to_string(),
                    ));
                    is_done = true;
                    break;
                }
                Ok(StatementFlow::Continue(_)) => {
                    thrown = Some(JsValue::String(
                        "SyntaxError: Illegal continue statement".to_string(),
                    ));
                    is_done = true;
                    break;
                }
                Err(e) => {
                    thrown = Some(JsValue::String(e));
                    is_done = true;
                    break;
                }
            }
        }

        if pc >= stmts.len() && !yielded {
            is_done = true;
        }

        self.current_scope = saved_scope;

        if let Some(inst) = self.generators.borrow_mut().get_mut(&gen_id) {
            inst.pc = pc;
            inst.done = is_done;
        }

        if let Some(err_val) = thrown {
            return Err(format!("Uncaught exception: {err_val}"));
        }

        let mut res = BTreeMap::new();
        res.insert("value".to_string(), ret);
        res.insert("done".to_string(), JsValue::Bool(is_done));
        Ok(JsValue::new_object(res))
    }

    pub fn to_bigint(&self, val: &JsValue) -> Result<JsValue, String> {
        match val {
            JsValue::BigInt(b) => Ok(JsValue::BigInt(b.clone())),
            JsValue::Number(n) => {
                if n.is_nan() || n.is_infinite() || n.fract() != 0.0 {
                    return Err("RangeError: The number cannot be converted to a BigInt because it is not an integer".to_string());
                }
                let s = format!("{:.0}", n);
                crate::bigint::JsBigInt::parse(&s).map(JsValue::BigInt)
            }
            JsValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    return Ok(JsValue::BigInt(crate::bigint::JsBigInt::zero()));
                }
                crate::bigint::JsBigInt::parse(trimmed).map(JsValue::BigInt)
            }
            JsValue::Bool(b) => {
                if *b {
                    crate::bigint::JsBigInt::parse("1").map(JsValue::BigInt)
                } else {
                    Ok(JsValue::BigInt(crate::bigint::JsBigInt::zero()))
                }
            }
            JsValue::Undefined | JsValue::Null | JsValue::Symbol(_) => {
                Err("TypeError: Cannot convert value to a BigInt".to_string())
            }
            JsValue::Array(_) | JsValue::Object(_) | JsValue::Promise(_) => {
                let s = self.to_display_string(val);
                self.to_bigint(&JsValue::String(s))
            }
            JsValue::Function(_) => {
                Err("TypeError: Cannot convert function to a BigInt".to_string())
            }
        }
    }

    fn eval_date_method(
        &self,
        property: &str,
        ms: f64,
        _args: &[JsValue],
    ) -> Result<JsValue, String> {
        let (y, m, d, h, min, s, milli) = crate::engine::ms_to_date(ms);
        match property {
            "getTime" | "valueOf" => Ok(JsValue::Number(ms)),
            "getFullYear" => Ok(JsValue::Number(y as f64)),
            "getMonth" => Ok(JsValue::Number(m as f64)),
            "getDate" => Ok(JsValue::Number(d as f64)),
            "getHours" => Ok(JsValue::Number(h as f64)),
            "getMinutes" => Ok(JsValue::Number(min as f64)),
            "getSeconds" => Ok(JsValue::Number(s as f64)),
            "getMilliseconds" => Ok(JsValue::Number(milli as f64)),
            "toISOString" => Ok(JsValue::String(format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
                y,
                m + 1,
                d,
                h,
                min,
                s,
                milli
            ))),
            "toUTCString" => Ok(JsValue::String(format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02} GMT",
                y,
                m + 1,
                d,
                h,
                min,
                s
            ))),
            "toString" => Ok(JsValue::String(format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                y,
                m + 1,
                d,
                h,
                min,
                s
            ))),
            _ => Ok(JsValue::Undefined),
        }
    }

    fn eval_weak_map_method(
        &mut self,
        id: u64,
        property: &str,
        args: &[JsValue],
    ) -> Result<JsValue, String> {
        let key_opt = args.first().and_then(|v| match v {
            JsValue::Object(o) => Some(std::rc::Rc::as_ptr(o) as usize),
            JsValue::Array(a) => Some(std::rc::Rc::as_ptr(a) as usize),
            _ => None,
        });

        match property {
            "get" => {
                if let Some(k) = key_opt {
                    let b = self.weak_maps.borrow();
                    if let Some(m) = b.get(&id) {
                        return Ok(m.get(&k).cloned().unwrap_or(JsValue::Undefined));
                    }
                }
                Ok(JsValue::Undefined)
            }
            "set" => {
                if let Some(k) = key_opt {
                    let val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    self.weak_maps
                        .borrow_mut()
                        .entry(id)
                        .or_default()
                        .insert(k, val);
                    if let Some(this_obj) = self.current_scope.borrow().get("this") {
                        return Ok(this_obj);
                    }
                    Ok(JsValue::Undefined)
                } else {
                    Err("TypeError: Invalid value used as weak map key".to_string())
                }
            }
            "has" => {
                if let Some(k) = key_opt {
                    let has = self
                        .weak_maps
                        .borrow()
                        .get(&id)
                        .map(|m| m.contains_key(&k))
                        .unwrap_or(false);
                    Ok(JsValue::Bool(has))
                } else {
                    Ok(JsValue::Bool(false))
                }
            }
            "delete" => {
                if let Some(k) = key_opt {
                    let removed = self
                        .weak_maps
                        .borrow_mut()
                        .get_mut(&id)
                        .map(|m| m.remove(&k).is_some())
                        .unwrap_or(false);
                    Ok(JsValue::Bool(removed))
                } else {
                    Ok(JsValue::Bool(false))
                }
            }
            _ => Ok(JsValue::Undefined),
        }
    }

    fn eval_weak_set_method(
        &mut self,
        id: u64,
        property: &str,
        args: &[JsValue],
    ) -> Result<JsValue, String> {
        let key_opt = args.first().and_then(|v| match v {
            JsValue::Object(o) => Some(std::rc::Rc::as_ptr(o) as usize),
            JsValue::Array(a) => Some(std::rc::Rc::as_ptr(a) as usize),
            _ => None,
        });

        match property {
            "add" => {
                if let Some(k) = key_opt {
                    self.weak_sets.borrow_mut().entry(id).or_default().insert(k);
                    if let Some(this_obj) = self.current_scope.borrow().get("this") {
                        return Ok(this_obj);
                    }
                    Ok(JsValue::Undefined)
                } else {
                    Err("TypeError: Invalid value used in weak set".to_string())
                }
            }
            "has" => {
                if let Some(k) = key_opt {
                    let has = self
                        .weak_sets
                        .borrow()
                        .get(&id)
                        .map(|s| s.contains(&k))
                        .unwrap_or(false);
                    Ok(JsValue::Bool(has))
                } else {
                    Ok(JsValue::Bool(false))
                }
            }
            "delete" => {
                if let Some(k) = key_opt {
                    let removed = self
                        .weak_sets
                        .borrow_mut()
                        .get_mut(&id)
                        .map(|s| s.remove(&k))
                        .unwrap_or(false);
                    Ok(JsValue::Bool(removed))
                } else {
                    Ok(JsValue::Bool(false))
                }
            }
            _ => Ok(JsValue::Undefined),
        }
    }

    pub fn call_function(&mut self, name: &str, args: Vec<JsValue>) -> Result<JsValue, String> {
        if let Some(pid_str) = name.strip_prefix("__promise_res_") {
            if let Ok(pid) = pid_str.parse::<u64>() {
                if let Some((res_fn, _)) = self.promise_controllers.get(&pid).cloned() {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    res_fn(val);
                    drain_microtasks();
                    return Ok(JsValue::Undefined);
                }
            }
        }
        if let Some(pid_str) = name.strip_prefix("__promise_rej_") {
            if let Ok(pid) = pid_str.parse::<u64>() {
                if let Some((_, rej_fn)) = self.promise_controllers.get(&pid).cloned() {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    rej_fn(val);
                    drain_microtasks();
                    return Ok(JsValue::Undefined);
                }
            }
        }

        if name == "BigInt" {
            if let Some(val) = args.first() {
                return self.to_bigint(val);
            }
            return Err("TypeError: Cannot convert undefined to a BigInt".to_string());
        }

        if name == "RegExp" {
            let pat = args
                .first()
                .map(|v| match v {
                    JsValue::Object(m)
                        if m.borrow().get("__is_regex__") == Some(&JsValue::Bool(true)) =>
                    {
                        m.borrow()
                            .get("source")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string()
                    }
                    _ => v.to_string(),
                })
                .unwrap_or_default();
            let flg = if args.len() > 1 {
                args[1].to_string()
            } else {
                String::new()
            };
            return self.create_regex_value(&pat, &flg);
        }

        if name == "setTimeout" {
            let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
            let delay_ms = args
                .get(1)
                .and_then(|v| v.as_f64())
                .map(|n| n.max(0.0) as u64)
                .unwrap_or(0);
            let extra_args = if args.len() > 2 {
                args[2..].to_vec()
            } else {
                Vec::new()
            };
            let timer_id = crate::web::set_timeout(
                move || {
                    crate::engine::ENGINE_TIMER_CALLBACKS.with(|q| {
                        q.borrow_mut().push_back((callback, extra_args));
                    });
                },
                delay_ms,
            );
            return Ok(JsValue::Number(timer_id as f64));
        }

        if name == "clearTimeout" {
            if let Some(id_num) = args.first().and_then(|v| v.as_f64()) {
                crate::web::clear_timeout(id_num as usize);
            }
            return Ok(JsValue::Undefined);
        }

        if name == "setInterval" {
            let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
            let interval_ms = args
                .get(1)
                .and_then(|v| v.as_f64())
                .map(|n| n.max(0.0) as u64)
                .unwrap_or(0);
            let extra_args = if args.len() > 2 {
                args[2..].to_vec()
            } else {
                Vec::new()
            };
            let timer_id = crate::web::set_interval(
                move || {
                    crate::engine::ENGINE_TIMER_CALLBACKS.with(|q| {
                        q.borrow_mut()
                            .push_back((callback.clone(), extra_args.clone()));
                    });
                },
                interval_ms,
            );
            return Ok(JsValue::Number(timer_id as f64));
        }

        if name == "clearInterval" {
            if let Some(id_num) = args.first().and_then(|v| v.as_f64()) {
                crate::web::clear_interval(id_num as usize);
            }
            return Ok(JsValue::Undefined);
        }

        if name == "fetch" {
            let url = args.first().map(|v| v.to_string()).unwrap_or_default();
            let (method, headers_vec, body_bytes) = if args.len() > 1 {
                if let JsValue::Object(ref init_map) = args[1] {
                    let ib = init_map.borrow();
                    let m = ib
                        .get("method")
                        .map(|v| match v {
                            JsValue::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_else(|| "GET".to_string());

                    let mut hdrs = Vec::new();
                    if let Some(JsValue::Object(ref h_map)) = ib.get("headers") {
                        for (k, v) in h_map.borrow().iter() {
                            hdrs.push((
                                k.clone(),
                                match v {
                                    JsValue::String(s) => s.clone(),
                                    other => other.to_string(),
                                },
                            ));
                        }
                    }

                    let b = ib.get("body").map(|v| match v {
                        JsValue::String(s) => s.as_bytes().to_vec(),
                        other => other.to_string().into_bytes(),
                    });

                    (m, hdrs, b)
                } else {
                    ("GET".to_string(), Vec::new(), None)
                }
            } else {
                ("GET".to_string(), Vec::new(), None)
            };

            let (promise, resolve, reject) = JsPromise::<JsValue>::new_pending();

            // Explicit synthetic test mock URL scheme:
            if url.starts_with("mock://") {
                let mut resp_map = BTreeMap::new();
                resp_map.insert("__is_response__".to_string(), JsValue::Bool(true));
                resp_map.insert("status".to_string(), JsValue::Number(200.0));
                resp_map.insert("statusText".to_string(), JsValue::String("OK".to_string()));
                resp_map.insert("ok".to_string(), JsValue::Bool(true));
                resp_map.insert("url".to_string(), JsValue::String(url));
                let body_str = body_bytes
                    .and_then(|b| String::from_utf8(b).ok())
                    .unwrap_or_else(|| "{\"status\":\"ok\"}".to_string());
                resp_map.insert("__body__".to_string(), JsValue::String(body_str));
                let mut headers_map = BTreeMap::new();
                headers_map.insert(
                    "content-type".to_string(),
                    JsValue::String("application/json".to_string()),
                );
                resp_map.insert("headers".to_string(), JsValue::new_object(headers_map));
                resolve(JsValue::new_object(resp_map));
                return Ok(self.create_promise_value(promise));
            }

            // Real HTTP/HTTPS fetch via strata_web_api
            let mut req_init = strata_web_api::fetch::RequestInit {
                method,
                ..Default::default()
            };
            for (k, v) in headers_vec {
                req_init.headers.set(&k, &v);
            }
            if let Some(b) = body_bytes {
                req_init = req_init.body(b);
            }

            match strata_web_api::fetch::fetch(&url, Some(req_init)) {
                Ok(resp) => {
                    let mut resp_map = BTreeMap::new();
                    resp_map.insert("__is_response__".to_string(), JsValue::Bool(true));
                    resp_map.insert("status".to_string(), JsValue::Number(resp.status as f64));
                    resp_map.insert(
                        "statusText".to_string(),
                        JsValue::String(resp.status_text.clone()),
                    );
                    resp_map.insert("ok".to_string(), JsValue::Bool(resp.ok()));
                    resp_map.insert("url".to_string(), JsValue::String(url));
                    let body_str = String::from_utf8_lossy(resp.bytes()).to_string();
                    resp_map.insert("__body__".to_string(), JsValue::String(body_str));

                    let mut headers_map = BTreeMap::new();
                    for (k, v) in resp.headers.all_entries() {
                        headers_map.insert(k.to_ascii_lowercase(), JsValue::String(v.to_string()));
                    }
                    resp_map.insert("headers".to_string(), JsValue::new_object(headers_map));

                    resolve(JsValue::new_object(resp_map));
                }
                Err(err) => {
                    reject(JsValue::String(format!(
                        "TypeError: Failed to fetch: {}",
                        err
                    )));
                }
            }

            return Ok(self.create_promise_value(promise));
        }

        if name == "Symbol" {
            let desc = args.first().and_then(|v| match v {
                JsValue::String(s) => Some(s.clone()),
                JsValue::Undefined => None,
                other => Some(other.to_string()),
            });
            let id = self.next_symbol_id;
            self.next_symbol_id += 1;
            return Ok(JsValue::Symbol(SymbolId::new(id, desc)));
        }

        if matches!(
            name,
            "Error" | "TypeError" | "ReferenceError" | "SyntaxError" | "RangeError"
        ) {
            let msg = args.first().map(|v| v.to_string()).unwrap_or_default();
            let mut map = BTreeMap::new();
            map.insert("__is_error__".to_string(), JsValue::Bool(true));
            map.insert("name".to_string(), JsValue::String(name.to_string()));
            map.insert("message".to_string(), JsValue::String(msg.clone()));
            map.insert(
                "stack".to_string(),
                JsValue::String(format!("{name}: {msg}\n    at strata_js")),
            );
            map.insert(
                "__proto__".to_string(),
                JsValue::new_object(self.error_prototype.clone()),
            );
            return Ok(JsValue::new_object(map));
        }

        if name == "Object" {
            let first = args.first().cloned().unwrap_or_else(JsValue::empty_object);
            return match first {
                JsValue::Undefined | JsValue::Null => Ok(JsValue::empty_object()),
                other => Ok(other),
            };
        }

        if name == "Array" {
            return Ok(JsValue::new_array(args));
        }

        let scope_val = self.current_scope.borrow().get(name);
        if let Some(val) = scope_val {
            return self.call_value(&val, args);
        }

        if let Some(val) = self.globals.get(name).cloned() {
            return self.call_value(&val, args);
        }

        if let Some(closure) = self.user_functions.get(name).cloned() {
            return self.call_closure(&closure, args);
        }

        Err(format!("Undefined function: '{name}'"))
    }

    fn eval_binary(
        &self,
        op: &BinaryOp,
        left: &JsValue,
        right: &JsValue,
    ) -> Result<JsValue, String> {
        match op {
            BinaryOp::Add => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => Ok(JsValue::BigInt(a.add(b))),
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => {
                    if let JsValue::String(_) = left {
                        Ok(JsValue::String(format!(
                            "{}{}",
                            self.to_display_string(left),
                            self.to_display_string(right)
                        )))
                    } else if let JsValue::String(_) = right {
                        Ok(JsValue::String(format!(
                            "{}{}",
                            self.to_display_string(left),
                            self.to_display_string(right)
                        )))
                    } else {
                        Err("TypeError: Cannot mix BigInt and other types, use explicit conversions".to_string())
                    }
                }
                (JsValue::String(a), JsValue::String(b)) => Ok(JsValue::String(format!("{a}{b}"))),
                (JsValue::String(a), b) => {
                    Ok(JsValue::String(format!("{a}{}", self.to_display_string(b))))
                }
                (a, JsValue::String(b)) => {
                    Ok(JsValue::String(format!("{}{b}", self.to_display_string(a))))
                }
                (JsValue::Number(a), JsValue::Number(b)) => Ok(JsValue::Number(a + b)),
                _ => Ok(JsValue::Number(
                    left.as_f64().unwrap_or(0.0) + right.as_f64().unwrap_or(0.0),
                )),
            },
            BinaryOp::Sub => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => Ok(JsValue::BigInt(a.sub(b))),
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => Err(
                    "TypeError: Cannot mix BigInt and other types, use explicit conversions"
                        .to_string(),
                ),
                _ => {
                    let a = left.as_f64().unwrap_or(0.0);
                    let b = right.as_f64().unwrap_or(0.0);
                    Ok(JsValue::Number(a - b))
                }
            },
            BinaryOp::Mul => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => Ok(JsValue::BigInt(a.mul(b))),
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => Err(
                    "TypeError: Cannot mix BigInt and other types, use explicit conversions"
                        .to_string(),
                ),
                _ => {
                    let a = left.as_f64().unwrap_or(0.0);
                    let b = right.as_f64().unwrap_or(0.0);
                    Ok(JsValue::Number(a * b))
                }
            },
            BinaryOp::Div => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => {
                    if b.is_zero() {
                        return Err("RangeError: Division by zero".to_string());
                    }
                    Ok(JsValue::BigInt(a.div(b)?))
                }
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => Err(
                    "TypeError: Cannot mix BigInt and other types, use explicit conversions"
                        .to_string(),
                ),
                _ => {
                    let a = left.as_f64().unwrap_or(0.0);
                    let b = right.as_f64().unwrap_or(0.0);
                    Ok(JsValue::Number(a / b))
                }
            },
            BinaryOp::Mod => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => {
                    if b.is_zero() {
                        return Err("RangeError: Division by zero".to_string());
                    }
                    Ok(JsValue::BigInt(a.rem(b)?))
                }
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => Err(
                    "TypeError: Cannot mix BigInt and other types, use explicit conversions"
                        .to_string(),
                ),
                _ => {
                    let a = left.as_f64().unwrap_or(0.0);
                    let b = right.as_f64().unwrap_or(0.0);
                    Ok(JsValue::Number(a % b))
                }
            },
            BinaryOp::Power => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => Ok(JsValue::BigInt(a.pow(b)?)),
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => Err(
                    "TypeError: Cannot mix BigInt and other types, use explicit conversions"
                        .to_string(),
                ),
                _ => {
                    let a = left.as_f64().unwrap_or(0.0);
                    let b = right.as_f64().unwrap_or(0.0);
                    Ok(JsValue::Number(a.powf(b)))
                }
            },
            BinaryOp::BitAnd => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => Ok(JsValue::BigInt(a.bit_and(b))),
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => Err(
                    "TypeError: Cannot mix BigInt and other types, use explicit conversions"
                        .to_string(),
                ),
                _ => {
                    let a = left.as_f64().unwrap_or(0.0) as i64;
                    let b = right.as_f64().unwrap_or(0.0) as i64;
                    Ok(JsValue::Number((a & b) as f64))
                }
            },
            BinaryOp::BitOr => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => Ok(JsValue::BigInt(a.bit_or(b))),
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => Err(
                    "TypeError: Cannot mix BigInt and other types, use explicit conversions"
                        .to_string(),
                ),
                _ => {
                    let a = left.as_f64().unwrap_or(0.0) as i64;
                    let b = right.as_f64().unwrap_or(0.0) as i64;
                    Ok(JsValue::Number((a | b) as f64))
                }
            },
            BinaryOp::BitXor => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => Ok(JsValue::BigInt(a.bit_xor(b))),
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => Err(
                    "TypeError: Cannot mix BigInt and other types, use explicit conversions"
                        .to_string(),
                ),
                _ => {
                    let a = left.as_f64().unwrap_or(0.0) as i64;
                    let b = right.as_f64().unwrap_or(0.0) as i64;
                    Ok(JsValue::Number((a ^ b) as f64))
                }
            },
            BinaryOp::Shl => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => {
                    let shift = b
                        .to_u64()
                        .ok_or_else(|| "RangeError: Invalid shift amount".to_string())?
                        as usize;
                    Ok(JsValue::BigInt(a.shl(shift)))
                }
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => Err(
                    "TypeError: Cannot mix BigInt and other types, use explicit conversions"
                        .to_string(),
                ),
                _ => {
                    let a = left.as_f64().unwrap_or(0.0) as i32;
                    let b = (right.as_f64().unwrap_or(0.0) as u32) & 31;
                    Ok(JsValue::Number((a << b) as f64))
                }
            },
            BinaryOp::Shr => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => {
                    let shift = b
                        .to_u64()
                        .ok_or_else(|| "RangeError: Invalid shift amount".to_string())?
                        as usize;
                    Ok(JsValue::BigInt(a.shr(shift)))
                }
                (JsValue::BigInt(_), _) | (_, JsValue::BigInt(_)) => Err(
                    "TypeError: Cannot mix BigInt and other types, use explicit conversions"
                        .to_string(),
                ),
                _ => {
                    let a = left.as_f64().unwrap_or(0.0) as i32;
                    let b = (right.as_f64().unwrap_or(0.0) as u32) & 31;
                    Ok(JsValue::Number((a >> b) as f64))
                }
            },
            BinaryOp::StrictEq => Ok(JsValue::Bool(left.strict_eq(right))),
            BinaryOp::StrictNotEq => Ok(JsValue::Bool(!left.strict_eq(right))),
            BinaryOp::Eq => Ok(JsValue::Bool(left.loose_eq(right))),
            BinaryOp::NotEq => Ok(JsValue::Bool(!left.loose_eq(right))),
            BinaryOp::Lt => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => {
                    Ok(JsValue::Bool(a.cmp(b) == std::cmp::Ordering::Less))
                }
                _ => {
                    let a = left.as_f64().unwrap_or(0.0);
                    let b = right.as_f64().unwrap_or(0.0);
                    Ok(JsValue::Bool(a < b))
                }
            },
            BinaryOp::Lte => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => {
                    Ok(JsValue::Bool(a.cmp(b) != std::cmp::Ordering::Greater))
                }
                _ => {
                    let a = left.as_f64().unwrap_or(0.0);
                    let b = right.as_f64().unwrap_or(0.0);
                    Ok(JsValue::Bool(a <= b))
                }
            },
            BinaryOp::Gt => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => {
                    Ok(JsValue::Bool(a.cmp(b) == std::cmp::Ordering::Greater))
                }
                _ => {
                    let a = left.as_f64().unwrap_or(0.0);
                    let b = right.as_f64().unwrap_or(0.0);
                    Ok(JsValue::Bool(a > b))
                }
            },
            BinaryOp::Gte => match (left, right) {
                (JsValue::BigInt(a), JsValue::BigInt(b)) => {
                    Ok(JsValue::Bool(a.cmp(b) != std::cmp::Ordering::Less))
                }
                _ => {
                    let a = left.as_f64().unwrap_or(0.0);
                    let b = right.as_f64().unwrap_or(0.0);
                    Ok(JsValue::Bool(a >= b))
                }
            },
            BinaryOp::And => {
                if !left.is_truthy() {
                    Ok(left.clone())
                } else {
                    Ok(right.clone())
                }
            }
            BinaryOp::Or => {
                if left.is_truthy() {
                    Ok(left.clone())
                } else {
                    Ok(right.clone())
                }
            }
            BinaryOp::NullishCoalescing => {
                if !matches!(left, JsValue::Null | JsValue::Undefined) {
                    Ok(left.clone())
                } else {
                    Ok(right.clone())
                }
            }
            BinaryOp::InstanceOf => {
                let class_name = match right {
                    JsValue::String(s) => s.clone(),
                    JsValue::Object(map) => map
                        .borrow()
                        .get("__class_name__")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    _ => String::new(),
                };
                Ok(JsValue::Bool(self.is_instance_of(left, &class_name)))
            }
        }
    }

    fn to_display_string(&self, val: &JsValue) -> String {
        match val {
            JsValue::String(s) => s.clone(),
            JsValue::Number(n) => n.to_string(),
            JsValue::Bool(b) => b.to_string(),
            JsValue::BigInt(b) => format!("{b}n"),
            JsValue::Null => "null".to_string(),
            JsValue::Undefined => "undefined".to_string(),
            JsValue::Object(map)
                if map.borrow().get("__is_regex__") == Some(&JsValue::Bool(true)) =>
            {
                let b = map.borrow();
                let src = b.get("source").and_then(|v| v.as_str()).unwrap_or("");
                let flg = b.get("flags").and_then(|v| v.as_str()).unwrap_or("");
                format!("/{src}/{flg}")
            }
            other => stringify_json(other).unwrap_or_else(|_| "[object Object]".to_string()),
        }
    }
}
