//! Recursive-descent JavaScript Parser.

use super::ast::*;
use crate::JsValue;

/// Maximum AST parsing recursion depth before returning RangeError (CWE-674).
pub const MAX_PARSER_DEPTH: usize = 256;

pub struct JsParser<'a> {
    src: &'a str,
    pos: usize,
    pub depth: usize,
}

impl<'a> JsParser<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            depth: 0,
        }
    }

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        self.skip_whitespace_and_comments();
        while !self.is_eof() {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            self.skip_whitespace_and_comments();
        }
        Ok(stmts)
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn starts_with(&self, s: &str) -> bool {
        self.src[self.pos..].starts_with(s)
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn skip_whitespace_and_comments(&mut self) {
        while !self.is_eof() {
            let ch = self.peek().unwrap();
            if ch.is_whitespace() {
                self.advance();
            } else if self.starts_with("//") {
                // Line comment
                self.pos += 2;
                while !self.is_eof() && self.peek() != Some('\n') {
                    self.advance();
                }
            } else if self.starts_with("/*") {
                // Block comment
                self.pos += 2;
                while !self.is_eof() {
                    if self.starts_with("*/") {
                        self.pos += 2;
                        break;
                    }
                    self.advance();
                }
            } else {
                break;
            }
        }
    }

    fn consume_semicolon(&mut self) {
        self.skip_whitespace_and_comments();
        if self.peek() == Some(';') {
            self.advance();
        }
    }

    fn parse_statement(&mut self) -> Result<Stmt, String> {
        if self.depth >= MAX_PARSER_DEPTH {
            return Err("RangeError: Maximum call stack size exceeded (parser recursion limit)".to_string());
        }
        self.depth += 1;
        let res = self.parse_statement_inner();
        self.depth -= 1;
        res
    }

    fn parse_statement_inner(&mut self) -> Result<Stmt, String> {
        self.skip_whitespace_and_comments();
        if self.is_eof() {
            return Err("Unexpected EOF while parsing statement".to_string());
        }

        if self.peek() == Some('{') {
            return self.parse_block();
        }

        let ident = self.peek_identifier();
        if let Some(ref kw) = ident {
            match kw.as_str() {
                "var" | "let" | "const" => return self.parse_var_decl(),
                "if" => return self.parse_if(),
                "while" => return self.parse_while(),
                "for" => return self.parse_for(),
                "function" => return self.parse_function_decl(false),
                "async" => {
                    let saved_pos = self.pos;
                    let _ = self.read_identifier();
                    self.skip_whitespace_and_comments();
                    if self.peek_identifier().as_deref() == Some("function") {
                        return self.parse_function_decl(true);
                    }
                    self.pos = saved_pos;
                }
                "class" => return self.parse_class_decl(),
                "return" => return self.parse_return(),
                "try" => return self.parse_try_catch(),
                "throw" => return self.parse_throw(),
                "break" => return self.parse_break(),
                "continue" => return self.parse_continue(),
                "switch" => return self.parse_switch(),
                _ => {}
            }
        }

        // Expression statement (including assignment expressions or directives)
        let expr = self.parse_expression()?;
        self.consume_semicolon();
        if let Expr::Literal(JsValue::String(ref s)) = expr {
            if s == "use strict" {
                return Ok(Stmt::Directive(s.clone()));
            }
        }
        Ok(Stmt::Expr(expr))
    }

    fn parse_break(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // "break"
        self.skip_whitespace_and_comments();
        let mut label = None;
        if self.peek() != Some(';') && self.peek() != Some('}') && !self.is_eof() {
            if let Some(ident) = self.peek_identifier() {
                if ident != "case" && ident != "default" {
                    label = Some(self.read_identifier()?);
                }
            }
        }
        self.consume_semicolon();
        Ok(Stmt::Break(label))
    }

    fn parse_continue(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // "continue"
        self.skip_whitespace_and_comments();
        let mut label = None;
        if self.peek() != Some(';') && self.peek() != Some('}') && !self.is_eof() {
            if let Some(ident) = self.peek_identifier() {
                if ident != "case" && ident != "default" {
                    label = Some(self.read_identifier()?);
                }
            }
        }
        self.consume_semicolon();
        Ok(Stmt::Continue(label))
    }

    fn parse_switch(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // "switch"
        self.skip_whitespace_and_comments();
        self.expect_char('(')?;
        let discriminant = self.parse_expression()?;
        self.expect_char(')')?;
        self.skip_whitespace_and_comments();
        self.expect_char('{')?;

        let mut cases = Vec::new();
        self.skip_whitespace_and_comments();

        while !self.is_eof() && self.peek() != Some('}') {
            let kw = self.peek_identifier();
            match kw.as_deref() {
                Some("case") => {
                    self.read_identifier()?; // "case"
                    self.skip_whitespace_and_comments();
                    let test = self.parse_expression()?;
                    self.skip_whitespace_and_comments();
                    self.expect_char(':')?;
                    let mut consequent = Vec::new();
                    self.skip_whitespace_and_comments();
                    while !self.is_eof() && self.peek() != Some('}') {
                        if let Some(next_kw) = self.peek_identifier() {
                            if next_kw == "case" || next_kw == "default" {
                                break;
                            }
                        }
                        let stmt = self.parse_statement()?;
                        consequent.push(stmt);
                        self.skip_whitespace_and_comments();
                    }
                    cases.push(SwitchCase {
                        test: Some(test),
                        consequent,
                    });
                }
                Some("default") => {
                    self.read_identifier()?; // "default"
                    self.skip_whitespace_and_comments();
                    self.expect_char(':')?;
                    let mut consequent = Vec::new();
                    self.skip_whitespace_and_comments();
                    while !self.is_eof() && self.peek() != Some('}') {
                        if let Some(next_kw) = self.peek_identifier() {
                            if next_kw == "case" || next_kw == "default" {
                                break;
                            }
                        }
                        let stmt = self.parse_statement()?;
                        consequent.push(stmt);
                        self.skip_whitespace_and_comments();
                    }
                    cases.push(SwitchCase {
                        test: None,
                        consequent,
                    });
                }
                _ => {
                    return Err(format!("SyntaxError: Unexpected token in switch block at pos {}", self.pos));
                }
            }
            self.skip_whitespace_and_comments();
        }

        self.expect_char('}')?;
        Ok(Stmt::Switch { discriminant, cases })
    }

    fn parse_block_stmts(&mut self) -> Result<Vec<Stmt>, String> {
        self.skip_whitespace_and_comments();
        self.expect_char('{')?;
        let mut stmts = Vec::new();
        self.skip_whitespace_and_comments();
        while !self.is_eof() && self.peek() != Some('}') {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            self.skip_whitespace_and_comments();
        }
        self.expect_char('}')?;
        Ok(stmts)
    }

    fn parse_block(&mut self) -> Result<Stmt, String> {
        let stmts = self.parse_block_stmts()?;
        Ok(Stmt::Block(stmts))
    }

    fn parse_throw(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // "throw"
        self.skip_whitespace_and_comments();
        let expr = self.parse_expression()?;
        self.consume_semicolon();
        Ok(Stmt::Throw(expr))
    }

    fn parse_try_catch(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // "try"
        let try_block = self.parse_block_stmts()?;
        self.skip_whitespace_and_comments();

        let mut catch_param = None;
        let mut catch_block = None;
        if self.peek_identifier().as_deref() == Some("catch") {
            self.read_identifier()?; // "catch"
            self.skip_whitespace_and_comments();
            if self.peek() == Some('(') {
                self.advance();
                self.skip_whitespace_and_comments();
                let param = self.read_identifier()?;
                self.skip_whitespace_and_comments();
                self.expect_char(')')?;
                catch_param = Some(param);
            }
            catch_block = Some(self.parse_block_stmts()?);
        }

        self.skip_whitespace_and_comments();
        let mut finally_block = None;
        if self.peek_identifier().as_deref() == Some("finally") {
            self.read_identifier()?; // "finally"
            finally_block = Some(self.parse_block_stmts()?);
        }

        if catch_block.is_none() && finally_block.is_none() {
            return Err("SyntaxError: Missing catch or finally clause after try".to_string());
        }

        Ok(Stmt::TryCatch {
            try_block,
            catch_param,
            catch_block,
            finally_block,
        })
    }

    fn parse_var_decl(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // var/let/const
        self.skip_whitespace_and_comments();
        if self.peek() == Some('{') {
            return self.parse_object_destructure_decl();
        }
        if self.peek() == Some('[') {
            return self.parse_array_destructure_decl();
        }
        let name = self.read_identifier()?;
        self.skip_whitespace_and_comments();

        let mut init = None;
        if self.peek() == Some('=') {
            self.advance();
            init = Some(self.parse_expression()?);
        }
        self.consume_semicolon();
        Ok(Stmt::VarDecl { name, init })
    }

    fn parse_object_destructure_decl(&mut self) -> Result<Stmt, String> {
        self.advance(); // consume '{'
        let mut properties = Vec::new();
        let mut rest = None;
        self.skip_whitespace_and_comments();

        while !self.is_eof() && self.peek() != Some('}') {
            if self.starts_with("...") {
                self.pos += 3;
                self.skip_whitespace_and_comments();
                let rest_name = self.read_identifier()?;
                rest = Some(rest_name);
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
                break;
            }

            let key = if self.peek() == Some('"') || self.peek() == Some('\'') {
                self.read_string()?
            } else {
                self.read_identifier()?
            };
            self.skip_whitespace_and_comments();

            let target_name = if self.peek() == Some(':') {
                self.advance(); // consume ':'
                self.skip_whitespace_and_comments();
                self.read_identifier()?
            } else {
                key.clone()
            };
            self.skip_whitespace_and_comments();

            let default_value = if self.peek() == Some('=') {
                self.advance(); // consume '='
                Some(self.parse_assignment()?)
            } else {
                None
            };

            properties.push(DestructureObjectProp {
                source_key: key,
                target_name,
                default_value,
            });

            self.skip_whitespace_and_comments();
            if self.peek() == Some(',') {
                self.advance();
                self.skip_whitespace_and_comments();
            }
        }

        self.expect_char('}')?;
        self.skip_whitespace_and_comments();
        self.expect_char('=')?;
        let init = self.parse_expression()?;
        self.consume_semicolon();

        Ok(Stmt::DestructureDecl {
            pattern: DestructurePattern::Object { properties, rest },
            init,
        })
    }

    fn parse_array_destructure_decl(&mut self) -> Result<Stmt, String> {
        self.advance(); // consume '['
        let mut elements = Vec::new();
        let mut rest = None;
        self.skip_whitespace_and_comments();

        while !self.is_eof() && self.peek() != Some(']') {
            if self.peek() == Some(',') {
                self.advance();
                elements.push(None);
                self.skip_whitespace_and_comments();
                continue;
            }

            if self.starts_with("...") {
                self.pos += 3;
                self.skip_whitespace_and_comments();
                let rest_name = self.read_identifier()?;
                rest = Some(rest_name);
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
                break;
            }

            let target_name = self.read_identifier()?;
            self.skip_whitespace_and_comments();

            let default_value = if self.peek() == Some('=') {
                self.advance(); // consume '='
                Some(self.parse_assignment()?)
            } else {
                None
            };

            elements.push(Some(DestructureArrayElem {
                target_name,
                default_value,
            }));

            self.skip_whitespace_and_comments();
            if self.peek() == Some(',') {
                self.advance();
                self.skip_whitespace_and_comments();
            }
        }

        self.expect_char(']')?;
        self.skip_whitespace_and_comments();
        self.expect_char('=')?;
        let init = self.parse_expression()?;
        self.consume_semicolon();

        Ok(Stmt::DestructureDecl {
            pattern: DestructurePattern::Array { elements, rest },
            init,
        })
    }

    fn parse_if(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // if
        self.skip_whitespace_and_comments();
        self.expect_char('(')?;
        let cond = self.parse_expression()?;
        self.expect_char(')')?;
        let then_branch = Box::new(self.parse_statement()?);

        self.skip_whitespace_and_comments();
        let mut else_branch = None;
        if self.peek_identifier().as_deref() == Some("else") {
            self.read_identifier()?; // else
            else_branch = Some(Box::new(self.parse_statement()?));
        }

        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // while
        self.skip_whitespace_and_comments();
        self.expect_char('(')?;
        let cond = self.parse_expression()?;
        self.expect_char(')')?;
        let body = Box::new(self.parse_statement()?);
        Ok(Stmt::While { cond, body })
    }

    fn parse_for(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // for
        self.skip_whitespace_and_comments();
        self.expect_char('(')?;

        let init = if self.peek() == Some(';') {
            self.advance();
            None
        } else {
            Some(Box::new(self.parse_statement()?))
        };

        self.skip_whitespace_and_comments();
        let cond = if self.peek() == Some(';') {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.consume_semicolon();

        self.skip_whitespace_and_comments();
        let step = if self.peek() == Some(')') {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect_char(')')?;

        let body = Box::new(self.parse_statement()?);
        Ok(Stmt::For {
            init,
            cond,
            step,
            body,
        })
    }

    fn parse_function_decl(&mut self, is_async: bool) -> Result<Stmt, String> {
        self.read_identifier()?; // function
        self.skip_whitespace_and_comments();
        let is_generator = if self.peek() == Some('*') {
            self.advance();
            self.skip_whitespace_and_comments();
            true
        } else {
            false
        };
        let name = self.read_identifier()?;
        self.skip_whitespace_and_comments();
        self.expect_char('(')?;

        let mut params = Vec::new();
        self.skip_whitespace_and_comments();
        while !self.is_eof() && self.peek() != Some(')') {
            if self.starts_with("...") {
                self.pos += 3;
                self.skip_whitespace_and_comments();
                let param = self.read_identifier()?;
                params.push(format!("...{param}"));
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
                break;
            }
            let param = self.read_identifier()?;
            params.push(param);
            self.skip_whitespace_and_comments();
            if self.peek() == Some(',') {
                self.advance();
                self.skip_whitespace_and_comments();
            }
        }
        self.expect_char(')')?;

        let block = self.parse_block()?;
        let body = match block {
            Stmt::Block(stmts) => stmts,
            other => vec![other],
        };

        Ok(Stmt::FunctionDecl {
            name,
            params,
            body,
            is_async,
            is_generator,
        })
    }

    fn parse_class_decl(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // class
        self.skip_whitespace_and_comments();
        let name = self.read_identifier()?;
        self.skip_whitespace_and_comments();
        let mut super_class = None;
        if self.peek_identifier().as_deref() == Some("extends") {
            self.read_identifier()?; // extends
            self.skip_whitespace_and_comments();
            super_class = Some(self.read_identifier()?);
            self.skip_whitespace_and_comments();
        }
        self.expect_char('{')?;
        self.skip_whitespace_and_comments();

        let mut constructor = None;
        let mut methods = Vec::new();

        while !self.is_eof() && self.peek() != Some('}') {
            self.skip_whitespace_and_comments();
            if self.peek() == Some(';') {
                self.advance();
                continue;
            }
            if self.peek() == Some('}') {
                break;
            }

            let method_name = self.read_identifier()?;
            self.skip_whitespace_and_comments();
            self.expect_char('(')?;
            let mut params = Vec::new();
            self.skip_whitespace_and_comments();
            while !self.is_eof() && self.peek() != Some(')') {
                if self.starts_with("...") {
                    self.pos += 3;
                    self.skip_whitespace_and_comments();
                    let p = self.read_identifier()?;
                    params.push(format!("...{p}"));
                    self.skip_whitespace_and_comments();
                    if self.peek() == Some(',') {
                        self.advance();
                        self.skip_whitespace_and_comments();
                    }
                    break;
                }
                let p = self.read_identifier()?;
                params.push(p);
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
            }
            self.expect_char(')')?;

            let body_block = self.parse_block()?;
            let body = match body_block {
                Stmt::Block(stmts) => stmts,
                other => vec![other],
            };

            if method_name == "constructor" {
                constructor = Some((params, body));
            } else {
                methods.push((method_name, params, body));
            }

            self.skip_whitespace_and_comments();
            if self.peek() == Some(';') {
                self.advance();
            }
            self.skip_whitespace_and_comments();
        }

        self.expect_char('}')?;
        self.consume_semicolon();

        Ok(Stmt::ClassDecl {
            name,
            super_class,
            constructor,
            methods,
        })
    }

    fn parse_return(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // return
        self.skip_whitespace_and_comments();
        let mut expr = None;
        if self.peek() != Some(';') && self.peek() != Some('}') {
            expr = Some(self.parse_expression()?);
        }
        self.consume_semicolon();
        Ok(Stmt::Return(expr))
    }

    // Expressions
    pub fn parse_expression(&mut self) -> Result<Expr, String> {
        if self.depth >= MAX_PARSER_DEPTH {
            return Err("RangeError: Maximum call stack size exceeded (parser recursion limit)".to_string());
        }
        self.depth += 1;
        let res = self.parse_assignment();
        self.depth -= 1;
        res
    }

    fn parse_assignment(&mut self) -> Result<Expr, String> {
        self.skip_whitespace_and_comments();
        if self.peek_identifier().as_deref() == Some("yield") {
            self.read_identifier()?; // "yield"
            self.skip_whitespace_and_comments();
            if self.peek() == Some(';') || self.peek() == Some('}') || self.peek() == Some(')') || self.peek() == Some(']') || self.peek() == Some(',') || self.is_eof() {
                return Ok(Expr::Yield(None));
            }
            let val = self.parse_assignment()?;
            return Ok(Expr::Yield(Some(Box::new(val))));
        }
        let left = self.parse_logical_or()?;
        self.skip_whitespace_and_comments();
        if self.peek() == Some('=') && !self.starts_with("==") {
            self.advance(); // consume '='
            let right = self.parse_assignment()?;
            return Ok(Expr::Assign {
                target: Box::new(left),
                value: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_logical_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_nullish_coalescing()?;
        self.skip_whitespace_and_comments();
        while self.starts_with("||") {
            self.pos += 2;
            let right = self.parse_nullish_coalescing()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_nullish_coalescing(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_logical_and()?;
        self.skip_whitespace_and_comments();
        while self.starts_with("??") {
            self.pos += 2;
            let right = self.parse_logical_and()?;
            left = Expr::Binary {
                op: BinaryOp::NullishCoalescing,
                left: Box::new(left),
                right: Box::new(right),
            };
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_bitwise_or()?;
        self.skip_whitespace_and_comments();
        while self.starts_with("&&") {
            self.pos += 2;
            let right = self.parse_bitwise_or()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_bitwise_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_bitwise_xor()?;
        self.skip_whitespace_and_comments();
        while self.peek() == Some('|') && !self.starts_with("||") && !self.starts_with("|=") {
            self.advance();
            let right = self.parse_bitwise_xor()?;
            left = Expr::Binary {
                op: BinaryOp::BitOr,
                left: Box::new(left),
                right: Box::new(right),
            };
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_bitwise_and()?;
        self.skip_whitespace_and_comments();
        while self.peek() == Some('^') && !self.starts_with("^=") {
            self.advance();
            let right = self.parse_bitwise_and()?;
            left = Expr::Binary {
                op: BinaryOp::BitXor,
                left: Box::new(left),
                right: Box::new(right),
            };
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_equality()?;
        self.skip_whitespace_and_comments();
        while self.peek() == Some('&') && !self.starts_with("&&") && !self.starts_with("&=") {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::Binary {
                op: BinaryOp::BitAnd,
                left: Box::new(left),
                right: Box::new(right),
            };
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_relational()?;
        self.skip_whitespace_and_comments();

        while !self.is_eof() {
            if self.starts_with("===") {
                self.pos += 3;
                let right = self.parse_relational()?;
                left = Expr::Binary {
                    op: BinaryOp::StrictEq,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.starts_with("!==") {
                self.pos += 3;
                let right = self.parse_relational()?;
                left = Expr::Binary {
                    op: BinaryOp::StrictNotEq,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.starts_with("==") {
                self.pos += 2;
                let right = self.parse_relational()?;
                left = Expr::Binary {
                    op: BinaryOp::Eq,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.starts_with("!=") {
                self.pos += 2;
                let right = self.parse_relational()?;
                left = Expr::Binary {
                    op: BinaryOp::NotEq,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_shift()?;
        self.skip_whitespace_and_comments();

        while !self.is_eof() {
            if self.starts_with("<=") {
                self.pos += 2;
                let right = self.parse_shift()?;
                left = Expr::Binary {
                    op: BinaryOp::Lte,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.starts_with(">=") {
                self.pos += 2;
                let right = self.parse_shift()?;
                left = Expr::Binary {
                    op: BinaryOp::Gte,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek() == Some('<') {
                self.advance();
                let right = self.parse_shift()?;
                left = Expr::Binary {
                    op: BinaryOp::Lt,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek() == Some('>') {
                self.advance();
                let right = self.parse_shift()?;
                left = Expr::Binary {
                    op: BinaryOp::Gt,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek_identifier().as_deref() == Some("instanceof") {
                self.read_identifier()?;
                let right = self.parse_shift()?;
                left = Expr::Binary {
                    op: BinaryOp::InstanceOf,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_additive()?;
        self.skip_whitespace_and_comments();

        while !self.is_eof() {
            if self.starts_with("<<") && !self.starts_with("<<=") {
                self.pos += 2;
                let right = self.parse_additive()?;
                left = Expr::Binary {
                    op: BinaryOp::Shl,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.starts_with(">>") && !self.starts_with(">>=") && !self.starts_with(">>>") {
                self.pos += 2;
                let right = self.parse_additive()?;
                left = Expr::Binary {
                    op: BinaryOp::Shr,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplicative()?;
        self.skip_whitespace_and_comments();

        while !self.is_eof() {
            if self.peek() == Some('+') {
                self.advance();
                let right = self.parse_multiplicative()?;
                left = Expr::Binary {
                    op: BinaryOp::Add,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek() == Some('-') {
                self.advance();
                let right = self.parse_multiplicative()?;
                left = Expr::Binary {
                    op: BinaryOp::Sub,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_exponentiation()?;
        self.skip_whitespace_and_comments();

        while !self.is_eof() {
            if self.peek() == Some('*') && !self.starts_with("**") && !self.starts_with("*=") {
                self.advance();
                let right = self.parse_exponentiation()?;
                left = Expr::Binary {
                    op: BinaryOp::Mul,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek() == Some('/') && !self.starts_with("/=") && !self.starts_with("//") && !self.starts_with("/*") {
                self.advance();
                let right = self.parse_exponentiation()?;
                left = Expr::Binary {
                    op: BinaryOp::Div,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek() == Some('%') && !self.starts_with("%=") {
                self.advance();
                let right = self.parse_exponentiation()?;
                left = Expr::Binary {
                    op: BinaryOp::Mod,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_exponentiation(&mut self) -> Result<Expr, String> {
        let left = self.parse_unary()?;
        self.skip_whitespace_and_comments();
        if self.starts_with("**") && !self.starts_with("**=") {
            self.pos += 2;
            let right = self.parse_exponentiation()?; // right-associative
            return Ok(Expr::Binary {
                op: BinaryOp::Power,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        self.skip_whitespace_and_comments();
        if self.starts_with("++") {
            self.pos += 2;
            let expr = self.parse_unary()?;
            return Ok(Expr::Update {
                op: UpdateOp::Increment,
                target: Box::new(expr),
                prefix: true,
            });
        }
        if self.starts_with("--") {
            self.pos += 2;
            let expr = self.parse_unary()?;
            return Ok(Expr::Update {
                op: UpdateOp::Decrement,
                target: Box::new(expr),
                prefix: true,
            });
        }
        if self.peek() == Some('!') {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            });
        }
        if self.peek() == Some('~') {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::BitNot,
                expr: Box::new(expr),
            });
        }
        if self.peek() == Some('+') && !self.starts_with("++") {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Pos,
                expr: Box::new(expr),
            });
        }
        if self.peek() == Some('-') {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            });
        }
        if self.peek_identifier().as_deref() == Some("typeof") {
            self.read_identifier()?;
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Typeof,
                expr: Box::new(expr),
            });
        }
        if self.peek_identifier().as_deref() == Some("await") {
            self.read_identifier()?;
            let expr = self.parse_unary()?;
            return Ok(Expr::Await(Box::new(expr)));
        }

        self.parse_call_and_member()
    }

    fn parse_call_and_member(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;

        loop {
            self.skip_whitespace_and_comments();
            if self.starts_with("?.")
                && !self.src[self.pos + 2..].chars().next().is_some_and(|c| c.is_ascii_digit())
            {
                self.pos += 2; // consume '?.'
                self.skip_whitespace_and_comments();
                if self.peek() == Some('[') {
                    self.advance(); // consume '['
                    let prop_expr = self.parse_expression()?;
                    self.expect_char(']')?;
                    if let Expr::Literal(JsValue::String(s)) = prop_expr {
                        expr = Expr::OptionalMember {
                            object: Box::new(expr),
                            property: s,
                            computed: None,
                        };
                    } else {
                        expr = Expr::OptionalMember {
                            object: Box::new(expr),
                            property: String::new(),
                            computed: Some(Box::new(prop_expr)),
                        };
                    }
                } else if self.peek() == Some('(') {
                    self.advance(); // consume '('
                    let mut args = Vec::new();
                    self.skip_whitespace_and_comments();
                    while !self.is_eof() && self.peek() != Some(')') {
                        let arg = if self.starts_with("...") {
                            self.pos += 3;
                            Expr::Spread(Box::new(self.parse_assignment()?))
                        } else {
                            self.parse_expression()?
                        };
                        args.push(arg);
                        self.skip_whitespace_and_comments();
                        if self.peek() == Some(',') {
                            self.advance();
                            self.skip_whitespace_and_comments();
                        }
                    }
                    self.expect_char(')')?;
                    expr = Expr::OptionalCall {
                        callee: Box::new(expr),
                        args,
                    };
                } else {
                    let prop = self.read_identifier()?;
                    expr = Expr::OptionalMember {
                        object: Box::new(expr),
                        property: prop,
                        computed: None,
                    };
                }
            } else if self.peek() == Some('.') {
                self.advance(); // '.'
                self.skip_whitespace_and_comments();
                let prop = self.read_identifier()?;
                expr = Expr::Member {
                    object: Box::new(expr),
                    property: prop,
                    computed: None,
                };
            } else if self.peek() == Some('[') {
                self.advance(); // '['
                let prop_expr = self.parse_expression()?;
                self.expect_char(']')?;
                if let Expr::Literal(JsValue::String(s)) = prop_expr {
                    expr = Expr::Member {
                        object: Box::new(expr),
                        property: s,
                        computed: None,
                    };
                } else {
                    expr = Expr::Member {
                        object: Box::new(expr),
                        property: String::new(),
                        computed: Some(Box::new(prop_expr)),
                    };
                }
            } else if self.peek() == Some('(') {
                self.advance(); // '('
                let mut args = Vec::new();
                self.skip_whitespace_and_comments();
                while !self.is_eof() && self.peek() != Some(')') {
                    let arg = if self.starts_with("...") {
                        self.pos += 3;
                        Expr::Spread(Box::new(self.parse_assignment()?))
                    } else {
                        self.parse_expression()?
                    };
                    args.push(arg);
                    self.skip_whitespace_and_comments();
                    if self.peek() == Some(',') {
                        self.advance();
                        self.skip_whitespace_and_comments();
                    }
                }
                self.expect_char(')')?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                };
            } else if self.starts_with("++") {
                self.pos += 2;
                expr = Expr::Update {
                    op: UpdateOp::Increment,
                    target: Box::new(expr),
                    prefix: false,
                };
            } else if self.starts_with("--") {
                self.pos += 2;
                expr = Expr::Update {
                    op: UpdateOp::Decrement,
                    target: Box::new(expr),
                    prefix: false,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_arrow_body(&mut self, params: Vec<String>, is_async: bool) -> Result<Expr, String> {
        self.skip_whitespace_and_comments();
        if self.peek() == Some('{') {
            let body = self.parse_block_stmts()?;
            Ok(Expr::ArrowFunction {
                params,
                body: ArrowBody::Block(body),
                is_async,
            })
        } else {
            let expr = self.parse_assignment()?;
            Ok(Expr::ArrowFunction {
                params,
                body: ArrowBody::Expr(Box::new(expr)),
                is_async,
            })
        }
    }

    fn parse_template_literal(&mut self) -> Result<Expr, String> {
        self.advance(); // consume opening '`'
        let mut parts = Vec::new();
        let mut current_lit = String::new();
        while !self.is_eof() {
            let c = self.advance().unwrap();
            if c == '`' {
                if !current_lit.is_empty() {
                    parts.push(TemplatePart::Literal(current_lit));
                }
                return Ok(Expr::TemplateLiteral(parts));
            }
            if c == '$' && self.peek() == Some('{') {
                self.advance(); // consume '{'
                if !current_lit.is_empty() {
                    parts.push(TemplatePart::Literal(current_lit));
                    current_lit = String::new();
                }
                self.skip_whitespace_and_comments();
                let expr = self.parse_expression()?;
                self.skip_whitespace_and_comments();
                self.expect_char('}')?;
                parts.push(TemplatePart::Expr(expr));
            } else if c == '\\' && !self.is_eof() {
                let escaped = self.advance().unwrap();
                match escaped {
                    'n' => current_lit.push('\n'),
                    't' => current_lit.push('\t'),
                    'r' => current_lit.push('\r'),
                    '`' => current_lit.push('`'),
                    '$' => current_lit.push('$'),
                    '\\' => current_lit.push('\\'),
                    other => {
                        current_lit.push('\\');
                        current_lit.push(other);
                    }
                }
            } else {
                current_lit.push(c);
            }
        }
        Err("SyntaxError: Unterminated template literal".to_string())
    }

    fn parse_regex_literal(&mut self) -> Result<Expr, String> {
        self.advance(); // consume opening '/'
        let mut pattern = String::new();
        let mut in_char_class = false;
        let mut escaped = false;

        while !self.is_eof() {
            let c = self.advance().unwrap();
            if escaped {
                pattern.push('\\');
                pattern.push(c);
                escaped = false;
                continue;
            }

            if c == '\\' {
                escaped = true;
                continue;
            }

            if c == '[' {
                in_char_class = true;
                pattern.push(c);
            } else if c == ']' {
                in_char_class = false;
                pattern.push(c);
            } else if c == '/' && !in_char_class {
                // End of regex pattern
                let mut flags = String::new();
                while !self.is_eof() {
                    if let Some(ch) = self.peek() {
                        if ch.is_ascii_alphabetic() {
                            flags.push(ch);
                            self.advance();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                return Ok(Expr::RegExpLiteral { pattern, flags });
            } else if c == '\n' || c == '\r' {
                return Err("SyntaxError: Unterminated regular expression literal".to_string());
            } else {
                pattern.push(c);
            }
        }

        Err("SyntaxError: Unterminated regular expression literal".to_string())
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        self.skip_whitespace_and_comments();
        if self.is_eof() {
            return Err("Unexpected EOF in primary expression".to_string());
        }

        let ch = self.peek().unwrap();

        // Template literal
        if ch == '`' {
            return self.parse_template_literal();
        }

        // Regular expression literal `/pattern/flags`
        if ch == '/' {
            return self.parse_regex_literal();
        }

        // Grouping `(expr)` or Arrow function `(a, b) => ...` / `() => ...`
        if ch == '(' {
            let saved_pos = self.pos;
            self.advance(); // '('
            self.skip_whitespace_and_comments();
            if self.peek() == Some(')') {
                self.advance(); // ')'
                self.skip_whitespace_and_comments();
                if self.starts_with("=>") {
                    self.pos += 2; // consume '=>'
                    return self.parse_arrow_body(Vec::new(), false);
                }
            } else {
                let mut params = Vec::new();
                let mut is_params = true;
                while !self.is_eof() && self.peek() != Some(')') {
                    if self.starts_with("...") {
                        self.pos += 3;
                        self.skip_whitespace_and_comments();
                        if let Ok(param_name) = self.read_identifier() {
                            params.push(format!("...{param_name}"));
                            self.skip_whitespace_and_comments();
                            if self.peek() == Some(',') {
                                self.advance();
                                self.skip_whitespace_and_comments();
                            }
                            break;
                        } else {
                            is_params = false;
                            break;
                        }
                    }
                    if let Ok(param_name) = self.read_identifier() {
                        params.push(param_name);
                        self.skip_whitespace_and_comments();
                        if self.peek() == Some(',') {
                            self.advance();
                            self.skip_whitespace_and_comments();
                        } else if self.peek() != Some(')') {
                            is_params = false;
                            break;
                        }
                    } else {
                        is_params = false;
                        break;
                    }
                }
                if is_params && self.peek() == Some(')') {
                    self.advance(); // ')'
                    self.skip_whitespace_and_comments();
                    if self.starts_with("=>") {
                        self.pos += 2; // consume '=>'
                        return self.parse_arrow_body(params, false);
                    }
                }
            }
            // Not an arrow function, rewind and parse standard grouped expression
            self.pos = saved_pos;
            self.advance(); // '('
            let expr = self.parse_expression()?;
            self.expect_char(')')?;
            return Ok(expr);
        }

        // Array literal `[...]`
        if ch == '[' {
            self.advance();
            let mut items = Vec::new();
            self.skip_whitespace_and_comments();
            while !self.is_eof() && self.peek() != Some(']') {
                if self.peek() == Some(',') {
                    self.advance();
                    items.push(Expr::Literal(JsValue::Undefined));
                    self.skip_whitespace_and_comments();
                    continue;
                }
                let item = if self.starts_with("...") {
                    self.pos += 3;
                    let expr = self.parse_assignment()?;
                    Expr::Spread(Box::new(expr))
                } else {
                    self.parse_assignment()?
                };
                items.push(item);
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
            }
            self.expect_char(']')?;
            return Ok(Expr::Array(items));
        }

        // Object literal `{...}`
        if ch == '{' {
            self.advance();
            let mut pairs = Vec::new();
            self.skip_whitespace_and_comments();
            while !self.is_eof() && self.peek() != Some('}') {
                if self.starts_with("...") {
                    self.pos += 3;
                    let expr = self.parse_assignment()?;
                    pairs.push((String::new(), Expr::Spread(Box::new(expr))));
                } else {
                    let key = if self.peek() == Some('"') || self.peek() == Some('\'') {
                        self.read_string()?
                    } else {
                        self.read_identifier()?
                    };
                    self.skip_whitespace_and_comments();
                    let val = if self.peek() == Some(':') {
                        self.advance();
                        self.parse_assignment()?
                    } else {
                        Expr::Identifier(key.clone())
                    };
                    pairs.push((key, val));
                }
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
            }
            self.expect_char('}')?;
            return Ok(Expr::Object(pairs));
        }

        // String literals
        if ch == '"' || ch == '\'' {
            let s = self.read_string()?;
            return Ok(Expr::Literal(JsValue::String(s)));
        }

        // Number / BigInt literals
        if ch.is_ascii_digit() {
            return self.parse_number_literal();
        }

        // Identifiers and Keywords
        let ident = self.read_identifier()?;
        if ident == "async" {
            self.skip_whitespace_and_comments();
            if self.peek_identifier().as_deref() == Some("function") {
                self.read_identifier()?; // function
                self.skip_whitespace_and_comments();
                let _name = if self.peek() != Some('(') {
                    self.read_identifier().ok()
                } else {
                    None
                };
                self.skip_whitespace_and_comments();
                self.expect_char('(')?;
                let mut params = Vec::new();
                self.skip_whitespace_and_comments();
                while !self.is_eof() && self.peek() != Some(')') {
                    if self.starts_with("...") {
                        self.pos += 3;
                        self.skip_whitespace_and_comments();
                        let p = self.read_identifier()?;
                        params.push(format!("...{p}"));
                        self.skip_whitespace_and_comments();
                        if self.peek() == Some(',') {
                            self.advance();
                            self.skip_whitespace_and_comments();
                        }
                        break;
                    }
                    let p = self.read_identifier()?;
                    params.push(p);
                    self.skip_whitespace_and_comments();
                    if self.peek() == Some(',') {
                        self.advance();
                        self.skip_whitespace_and_comments();
                    }
                }
                self.expect_char(')')?;
                let body = self.parse_block_stmts()?;
                return Ok(Expr::ArrowFunction {
                    params,
                    body: ArrowBody::Block(body),
                    is_async: true,
                });
            } else if self.peek() == Some('(') {
                let saved_pos = self.pos;
                self.advance(); // '('
                self.skip_whitespace_and_comments();
                if self.peek() == Some(')') {
                    self.advance(); // ')'
                    self.skip_whitespace_and_comments();
                    if self.starts_with("=>") {
                        self.pos += 2; // consume '=>'
                        return self.parse_arrow_body(Vec::new(), true);
                    }
                } else {
                    let mut params = Vec::new();
                    let mut is_params = true;
                    while !self.is_eof() && self.peek() != Some(')') {
                        if self.starts_with("...") {
                            self.pos += 3;
                            self.skip_whitespace_and_comments();
                            if let Ok(param_name) = self.read_identifier() {
                                params.push(format!("...{param_name}"));
                                self.skip_whitespace_and_comments();
                                if self.peek() == Some(',') {
                                    self.advance();
                                    self.skip_whitespace_and_comments();
                                }
                                break;
                            } else {
                                is_params = false;
                                break;
                            }
                        }
                        if let Ok(param_name) = self.read_identifier() {
                            params.push(param_name);
                            self.skip_whitespace_and_comments();
                            if self.peek() == Some(',') {
                                self.advance();
                                self.skip_whitespace_and_comments();
                            } else if self.peek() != Some(')') {
                                is_params = false;
                                break;
                            }
                        } else {
                            is_params = false;
                            break;
                        }
                    }
                    if is_params && self.peek() == Some(')') {
                        self.advance(); // ')'
                        self.skip_whitespace_and_comments();
                        if self.starts_with("=>") {
                            self.pos += 2; // consume '=>'
                            return self.parse_arrow_body(params, true);
                        }
                    }
                }
                self.pos = saved_pos;
            } else if let Some(param) = self.peek_identifier() {
                let saved_pos = self.pos;
                let _ = self.read_identifier();
                self.skip_whitespace_and_comments();
                if self.starts_with("=>") {
                    self.pos += 2; // consume '=>'
                    return self.parse_arrow_body(vec![param], true);
                }
                self.pos = saved_pos;
            }
        }

        if ident == "function" {
            self.skip_whitespace_and_comments();
            let _name = if self.peek() != Some('(') {
                self.read_identifier().ok()
            } else {
                None
            };
            self.skip_whitespace_and_comments();
            self.expect_char('(')?;
            let mut params = Vec::new();
            self.skip_whitespace_and_comments();
            while !self.is_eof() && self.peek() != Some(')') {
                if self.starts_with("...") {
                    self.pos += 3;
                    self.skip_whitespace_and_comments();
                    let p = self.read_identifier()?;
                    params.push(format!("...{p}"));
                    self.skip_whitespace_and_comments();
                    if self.peek() == Some(',') {
                        self.advance();
                        self.skip_whitespace_and_comments();
                    }
                    break;
                }
                let p = self.read_identifier()?;
                params.push(p);
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
            }
            self.expect_char(')')?;
            let body = self.parse_block_stmts()?;
            return Ok(Expr::ArrowFunction {
                params,
                body: ArrowBody::Block(body),
                is_async: false,
            });
        }

        if ident == "new" {
            self.skip_whitespace_and_comments();
            let mut callee = self.parse_primary()?;
            loop {
                self.skip_whitespace_and_comments();
                if self.peek() == Some('.') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                    let prop = self.read_identifier()?;
                    callee = Expr::Member {
                        object: Box::new(callee),
                        property: prop,
                        computed: None,
                    };
                } else if self.peek() == Some('[') {
                    self.advance();
                    let prop_expr = self.parse_expression()?;
                    self.expect_char(']')?;
                    callee = Expr::Member {
                        object: Box::new(callee),
                        property: String::new(),
                        computed: Some(Box::new(prop_expr)),
                    };
                } else {
                    break;
                }
            }
            self.skip_whitespace_and_comments();
            let mut args = Vec::new();
            if self.peek() == Some('(') {
                self.advance();
                self.skip_whitespace_and_comments();
                while !self.is_eof() && self.peek() != Some(')') {
                    let arg = if self.starts_with("...") {
                        self.pos += 3;
                        Expr::Spread(Box::new(self.parse_assignment()?))
                    } else {
                        self.parse_expression()?
                    };
                    args.push(arg);
                    self.skip_whitespace_and_comments();
                    if self.peek() == Some(',') {
                        self.advance();
                        self.skip_whitespace_and_comments();
                    }
                }
                self.expect_char(')')?;
            }
            return Ok(Expr::New {
                callee: Box::new(callee),
                args,
            });
        }

        self.skip_whitespace_and_comments();
        if self.starts_with("=>") {
            self.pos += 2; // consume '=>'
            return self.parse_arrow_body(vec![ident], false);
        }

        if ident == "yield" {
            self.skip_whitespace_and_comments();
            if self.peek() == Some(';') || self.peek() == Some('}') || self.peek() == Some(')') || self.peek() == Some(']') || self.peek() == Some(',') || self.is_eof() {
                return Ok(Expr::Yield(None));
            }
            let val = self.parse_assignment()?;
            return Ok(Expr::Yield(Some(Box::new(val))));
        }

        match ident.as_str() {
            "true" => Ok(Expr::Literal(JsValue::Bool(true))),
            "false" => Ok(Expr::Literal(JsValue::Bool(false))),
            "null" => Ok(Expr::Literal(JsValue::Null)),
            "undefined" => Ok(Expr::Literal(JsValue::Undefined)),
            _ => Ok(Expr::Identifier(ident)),
        }
    }

    fn parse_number_literal(&mut self) -> Result<Expr, String> {
        let start = self.pos;
        if self.starts_with("0x") || self.starts_with("0X") {
            self.pos += 2;
            let digit_start = self.pos;
            while !self.is_eof() && self.peek().map(|c| c.is_ascii_hexdigit()).unwrap_or(false) {
                self.advance();
            }
            let digits = &self.src[digit_start..self.pos];
            if digits.is_empty() {
                return Err("SyntaxError: Expected hexadecimal digits after 0x".to_string());
            }
            if self.peek() == Some('n') {
                self.advance();
                let literal_str = &self.src[start..self.pos];
                let bi = crate::bigint::JsBigInt::parse(literal_str)?;
                return Ok(Expr::Literal(JsValue::BigInt(bi)));
            }
            let val = u64::from_str_radix(digits, 16)
                .map_err(|e| format!("Invalid hex literal: {e}"))?;
            return Ok(Expr::Literal(JsValue::Number(val as f64)));
        } else if self.starts_with("0b") || self.starts_with("0B") {
            self.pos += 2;
            let digit_start = self.pos;
            while !self.is_eof() && (self.peek() == Some('0') || self.peek() == Some('1')) {
                self.advance();
            }
            let digits = &self.src[digit_start..self.pos];
            if digits.is_empty() {
                return Err("SyntaxError: Expected binary digits after 0b".to_string());
            }
            if self.peek() == Some('n') {
                self.advance();
                let literal_str = &self.src[start..self.pos];
                let bi = crate::bigint::JsBigInt::parse(literal_str)?;
                return Ok(Expr::Literal(JsValue::BigInt(bi)));
            }
            let val = u64::from_str_radix(digits, 2)
                .map_err(|e| format!("Invalid binary literal: {e}"))?;
            return Ok(Expr::Literal(JsValue::Number(val as f64)));
        } else if self.starts_with("0o") || self.starts_with("0O") {
            self.pos += 2;
            let digit_start = self.pos;
            while !self.is_eof() && self.peek().map(|c| ('0'..='7').contains(&c)).unwrap_or(false) {
                self.advance();
            }
            let digits = &self.src[digit_start..self.pos];
            if digits.is_empty() {
                return Err("SyntaxError: Expected octal digits after 0o".to_string());
            }
            if self.peek() == Some('n') {
                self.advance();
                let literal_str = &self.src[start..self.pos];
                let bi = crate::bigint::JsBigInt::parse(literal_str)?;
                return Ok(Expr::Literal(JsValue::BigInt(bi)));
            }
            let val = u64::from_str_radix(digits, 8)
                .map_err(|e| format!("Invalid octal literal: {e}"))?;
            return Ok(Expr::Literal(JsValue::Number(val as f64)));
        }

        // Decimal digits
        let mut is_float = false;
        while !self.is_eof() {
            let ch = self.peek().unwrap();
            if ch.is_ascii_digit() {
                self.advance();
            } else if ch == '.' && !is_float {
                let next_char = self.src[self.pos + 1..].chars().next();
                if next_char.map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    is_float = true;
                    self.advance();
                } else {
                    break;
                }
            } else if ch == 'e' || ch == 'E' {
                is_float = true;
                self.advance();
                if self.peek() == Some('+') || self.peek() == Some('-') {
                    self.advance();
                }
            } else {
                break;
            }
        }

        if !is_float && self.peek() == Some('n') {
            let digits = &self.src[start..self.pos];
            self.advance(); // consume 'n'
            let bi = crate::bigint::JsBigInt::parse(digits)?;
            return Ok(Expr::Literal(JsValue::BigInt(bi)));
        }

        let num_str = &self.src[start..self.pos];
        let n = num_str.parse::<f64>().map_err(|e| format!("Invalid number: {e}"))?;
        Ok(Expr::Literal(JsValue::Number(n)))
    }

    fn read_string(&mut self) -> Result<String, String> {
        let quote = self.advance().unwrap();
        let mut out = String::new();
        while !self.is_eof() {
            let ch = self.advance().unwrap();
            if ch == quote {
                return Ok(out);
            }
            if ch == '\\' && !self.is_eof() {
                let escaped = self.advance().unwrap();
                match escaped {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '\\' => out.push('\\'),
                    '\'' => out.push('\''),
                    '"' => out.push('"'),
                    _ => out.push(escaped),
                }
            } else {
                out.push(ch);
            }
        }
        Err("Unterminated string literal".to_string())
    }

    #[allow(dead_code)]
    fn read_number(&mut self) -> Result<f64, String> {
        let start = self.pos;
        while !self.is_eof() {
            let ch = self.peek().unwrap();
            if ch.is_ascii_digit() || ch == '.' {
                self.advance();
            } else {
                break;
            }
        }
        let num_str = &self.src[start..self.pos];
        num_str
            .parse::<f64>()
            .map_err(|e| format!("Invalid number: {e}"))
    }

    fn read_identifier(&mut self) -> Result<String, String> {
        self.skip_whitespace_and_comments();
        let start = self.pos;
        if self.is_eof() {
            return Err("Unexpected EOF while reading identifier".to_string());
        }
        let first = self.peek().unwrap();
        if !first.is_alphabetic() && first != '_' && first != '$' {
            return Err(format!("Expected identifier, found '{first}'"));
        }
        self.advance();

        while !self.is_eof() {
            let ch = self.peek().unwrap();
            if ch.is_alphanumeric() || ch == '_' || ch == '$' {
                self.advance();
            } else {
                break;
            }
        }
        Ok(self.src[start..self.pos].to_string())
    }

    fn peek_identifier(&self) -> Option<String> {
        let mut i = self.pos;
        while i < self.src.len() {
            let ch = self.src[i..].chars().next().unwrap();
            if ch.is_whitespace() {
                i += ch.len_utf8();
            } else {
                break;
            }
        }
        if i >= self.src.len() {
            return None;
        }
        let first = self.src[i..].chars().next().unwrap();
        if !first.is_alphabetic() && first != '_' && first != '$' {
            return None;
        }
        let start = i;
        i += first.len_utf8();
        while i < self.src.len() {
            let ch = self.src[i..].chars().next().unwrap();
            if ch.is_alphanumeric() || ch == '_' || ch == '$' {
                i += ch.len_utf8();
            } else {
                break;
            }
        }
        Some(self.src[start..i].to_string())
    }

    fn expect_char(&mut self, expected: char) -> Result<(), String> {
        self.skip_whitespace_and_comments();
        if self.peek() == Some(expected) {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "Expected '{}', found '{:?}'",
                expected,
                self.peek()
            ))
        }
    }
}
