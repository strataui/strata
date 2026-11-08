//! CSS calc() mathematical expression tree, tokenizer, parser, and evaluator.
//!
//! Evaluates complex mixed-unit expressions (e.g. `calc(100% - 24px + 2rem)`)
//! with strict arithmetic operator precedence and contextual dimension resolution.

use super::types::Dimension;
use std::sync::{Arc, RwLock};

/// Identifier for an interned calc expression AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CalcId(pub u32);

const MAX_CALC_ENTRIES: usize = 8192;

struct CalcInternerState {
    entries: Vec<Arc<CalcExpr>>,
    index: std::collections::HashMap<u64, Vec<u32>>,
}

impl CalcInternerState {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: std::collections::HashMap::new(),
        }
    }
}

static CALC_INTERNER: std::sync::LazyLock<RwLock<CalcInternerState>> =
    std::sync::LazyLock::new(|| RwLock::new(CalcInternerState::new()));

#[inline]
fn feed_hash(h: &mut u64, v: u64) {
    *h ^= v;
    *h = h.wrapping_mul(0x100000001b3);
}

fn hash_dimension_bits(dim: &Dimension, h: &mut u64) {
    match *dim {
        Dimension::Auto => feed_hash(h, 0),
        Dimension::Px(v) => {
            feed_hash(h, 1);
            feed_hash(h, v.to_bits() as u64);
        }
        Dimension::Percent(v) => {
            feed_hash(h, 2);
            feed_hash(h, v.to_bits() as u64);
        }
        Dimension::Vw(v) => {
            feed_hash(h, 3);
            feed_hash(h, v.to_bits() as u64);
        }
        Dimension::Vh(v) => {
            feed_hash(h, 4);
            feed_hash(h, v.to_bits() as u64);
        }
        Dimension::Rem(v) => {
            feed_hash(h, 5);
            feed_hash(h, v.to_bits() as u64);
        }
        Dimension::Em(v) => {
            feed_hash(h, 6);
            feed_hash(h, v.to_bits() as u64);
        }
        Dimension::Calc(id) => {
            feed_hash(h, 7);
            feed_hash(h, id.0 as u64);
        }
        Dimension::MinContent => feed_hash(h, 8),
        Dimension::MaxContent => feed_hash(h, 9),
        Dimension::FitContent(v) => {
            feed_hash(h, 10);
            feed_hash(h, v.to_bits() as u64);
        }
    }
}

fn hash_calc_expr_bits(expr: &CalcExpr, h: &mut u64) {
    match expr {
        CalcExpr::Value(dim) => {
            feed_hash(h, 100);
            hash_dimension_bits(dim, h);
        }
        CalcExpr::Add(l, r) => {
            feed_hash(h, 101);
            hash_calc_expr_bits(l, h);
            hash_calc_expr_bits(r, h);
        }
        CalcExpr::Sub(l, r) => {
            feed_hash(h, 102);
            hash_calc_expr_bits(l, h);
            hash_calc_expr_bits(r, h);
        }
        CalcExpr::Mul(l, s) => {
            feed_hash(h, 103);
            hash_calc_expr_bits(l, h);
            feed_hash(h, s.to_bits() as u64);
        }
        CalcExpr::Div(l, s) => {
            feed_hash(h, 104);
            hash_calc_expr_bits(l, h);
            feed_hash(h, s.to_bits() as u64);
        }
    }
}

fn compute_expr_hash(expr: &CalcExpr) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    hash_calc_expr_bits(expr, &mut h);
    h
}

/// Interns a calc expression AST into the global cache and returns its lightweight CalcId.
/// Automatically deduplicates identical expressions and bounds capacity to prevent memory leaks.
pub fn intern_calc(expr: CalcExpr) -> CalcId {
    let hash = compute_expr_hash(&expr);

    // Fast path: check for existing interned expression under read lock
    {
        let state = CALC_INTERNER.read().unwrap();
        if let Some(candidates) = state.index.get(&hash) {
            for &id in candidates {
                if let Some(existing) = state.entries.get(id as usize) {
                    if **existing == expr {
                        return CalcId(id);
                    }
                }
            }
        }
    }

    // Slow path: acquire write lock to insert
    let mut state = CALC_INTERNER.write().unwrap();
    if let Some(candidates) = state.index.get(&hash) {
        for &id in candidates {
            if let Some(existing) = state.entries.get(id as usize) {
                if **existing == expr {
                    return CalcId(id);
                }
            }
        }
    }

    if state.entries.len() >= MAX_CALC_ENTRIES {
        // Enforce hard bound to avoid unbounded memory leak under dynamic DoS
        return CalcId(0);
    }

    let id = state.entries.len() as u32;
    state.entries.push(Arc::new(expr));
    state.index.entry(hash).or_default().push(id);
    CalcId(id)
}

/// Retrieves an interned calc expression AST by its CalcId.
pub fn get_calc(id: CalcId) -> Option<Arc<CalcExpr>> {
    let state = CALC_INTERNER.read().unwrap();
    state.entries.get(id.0 as usize).cloned()
}

/// Contextual environment for resolving relative CSS units inside a calc() expression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalcContext {
    /// Resolved parent container dimension in CSS pixels (for resolving percentages).
    pub parent_size: f32,
    /// Root font size in CSS pixels (for resolving `rem`, default 16.0).
    pub root_font_size: f32,
    /// Current element font size in CSS pixels (for resolving `em`, default 16.0).
    pub current_font_size: f32,
    /// Viewport width in CSS pixels (for resolving `vw`).
    pub viewport_width: f32,
    /// Viewport height in CSS pixels (for resolving `vh`).
    pub viewport_height: f32,
}

impl Default for CalcContext {
    fn default() -> Self {
        Self {
            parent_size: 1000.0,
            root_font_size: 16.0,
            current_font_size: 16.0,
            viewport_width: 1920.0,
            viewport_height: 1080.0,
        }
    }
}

/// Recursive AST representation of a CSS calc() expression.
#[derive(Debug, Clone, PartialEq)]
pub enum CalcExpr {
    /// Concrete or relative dimension value (e.g. `24px`, `50%`, `1.5rem`).
    Value(Dimension),
    /// Addition: `Left + Right`.
    Add(Box<CalcExpr>, Box<CalcExpr>),
    /// Subtraction: `Left - Right`.
    Sub(Box<CalcExpr>, Box<CalcExpr>),
    /// Multiplication by a scalar: `Expr * Scalar`.
    Mul(Box<CalcExpr>, f32),
    /// Division by a scalar: `Expr / Scalar`.
    Div(Box<CalcExpr>, f32),
}

impl CalcExpr {
    /// Evaluates the expression AST to concrete CSS pixels given a context.
    pub fn eval(&self, ctx: &CalcContext) -> f32 {
        let raw = match self {
            CalcExpr::Value(dim) => match *dim {
                Dimension::Auto => 0.0,
                Dimension::Px(px) => px,
                Dimension::Percent(pct) => ctx.parent_size * (pct / 100.0),
                Dimension::Rem(rem) => rem * ctx.root_font_size,
                Dimension::Em(em) => em * ctx.current_font_size,
                Dimension::Vw(vw) => ctx.viewport_width * (vw / 100.0),
                Dimension::Vh(vh) => ctx.viewport_height * (vh / 100.0),
                Dimension::Calc(id) => get_calc(id).map(|inner| inner.eval(ctx)).unwrap_or(0.0),
                Dimension::MinContent | Dimension::MaxContent | Dimension::FitContent(_) => 0.0,
            },
            CalcExpr::Add(left, right) => left.eval(ctx) + right.eval(ctx),
            CalcExpr::Sub(left, right) => left.eval(ctx) - right.eval(ctx),
            CalcExpr::Mul(left, scalar) => {
                if scalar.is_finite() {
                    left.eval(ctx) * scalar
                } else {
                    0.0
                }
            }
            CalcExpr::Div(left, scalar) => {
                if scalar.is_finite() && *scalar != 0.0 {
                    left.eval(ctx) / scalar
                } else {
                    0.0
                }
            }
        };

        if raw.is_finite() {
            raw
        } else {
            0.0
        }
    }

    /// Convenience constructor for px value.
    pub fn px(val: f32) -> Self {
        CalcExpr::Value(Dimension::Px(val))
    }

    /// Convenience constructor for percentage value.
    pub fn percent(val: f32) -> Self {
        CalcExpr::Value(Dimension::Percent(val))
    }

    /// Convenience constructor for rem value.
    pub fn rem(val: f32) -> Self {
        CalcExpr::Value(Dimension::Rem(val))
    }
}

/// Tokenizer token for calc() expressions.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Dim(Dimension),
    Number(f32),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

/// Maximum recursion depth allowed during calc() parsing to prevent stack overflow crashes.
pub const MAX_CALC_DEPTH: usize = 32;

/// Parses a CSS calc(...) expression string into a `CalcExpr` AST.
pub fn parse_calc(input: &str) -> Result<CalcExpr, String> {
    let clean = input.trim();
    let inner = if clean.starts_with("calc(") && clean.ends_with(')') {
        &clean[5..clean.len() - 1]
    } else {
        clean
    };

    let tokens = tokenize_calc(inner)?;
    let mut cursor = TokenCursor::new(tokens);
    let expr = parse_add_sub(&mut cursor, 0)?;

    if !cursor.is_eof() {
        return Err(format!(
            "Unexpected tokens remaining in calc(): {:?}",
            cursor.peek()
        ));
    }

    Ok(expr)
}

struct TokenCursor {
    tokens: Vec<Token>,
    pos: usize,
}

impl TokenCursor {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let t = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }
}

fn tokenize_calc(s: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        match b {
            b'+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            b'-' => {
                // Distinguish between binary minus and negative number
                tokens.push(Token::Minus);
                i += 1;
            }
            b'*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            b'/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            b'(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            b')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            _ if b.is_ascii_digit() || b == b'.' => {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_digit()
                        || bytes[i] == b'.'
                        || bytes[i].is_ascii_alphabetic()
                        || bytes[i] == b'%')
                {
                    i += 1;
                }
                let raw = &s[start..i];
                if let Some(dim) = parse_dimension_token(raw) {
                    tokens.push(Token::Dim(dim));
                } else if let Ok(num) = raw.parse::<f32>() {
                    tokens.push(Token::Number(num));
                } else {
                    return Err(format!("Unrecognized calc token: '{}'", raw));
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    Ok(tokens)
}

fn parse_dimension_token(raw: &str) -> Option<Dimension> {
    let parse_num = |s: &str| -> Option<f32> {
        let n = s.parse::<f32>().ok()?;
        if n.is_finite() {
            Some(n)
        } else {
            None
        }
    };

    if let Some(stripped) = raw.strip_suffix("px") {
        parse_num(stripped).map(Dimension::Px)
    } else if let Some(stripped) = raw.strip_suffix('%') {
        parse_num(stripped).map(Dimension::Percent)
    } else if let Some(stripped) = raw.strip_suffix("rem") {
        parse_num(stripped).map(Dimension::Rem)
    } else if let Some(stripped) = raw.strip_suffix("em") {
        parse_num(stripped).map(Dimension::Em)
    } else if let Some(stripped) = raw.strip_suffix("vw") {
        parse_num(stripped).map(Dimension::Vw)
    } else if let Some(stripped) = raw.strip_suffix("vh") {
        parse_num(stripped).map(Dimension::Vh)
    } else {
        None
    }
}

// Precedence Level 1: Add (+) and Sub (-)
fn parse_add_sub(cursor: &mut TokenCursor, depth: usize) -> Result<CalcExpr, String> {
    if depth > MAX_CALC_DEPTH {
        return Err(format!("Maximum calc() expression nesting depth ({MAX_CALC_DEPTH}) exceeded"));
    }
    let mut left = parse_mul_div(cursor, depth)?;

    while let Some(tok) = cursor.peek() {
        match tok {
            Token::Plus => {
                cursor.next();
                let right = parse_mul_div(cursor, depth)?;
                left = CalcExpr::Add(Box::new(left), Box::new(right));
            }
            Token::Minus => {
                cursor.next();
                let right = parse_mul_div(cursor, depth)?;
                left = CalcExpr::Sub(Box::new(left), Box::new(right));
            }
            _ => break,
        }
    }

    Ok(left)
}

// Precedence Level 2: Mul (*) and Div (/)
fn parse_mul_div(cursor: &mut TokenCursor, depth: usize) -> Result<CalcExpr, String> {
    if depth > MAX_CALC_DEPTH {
        return Err(format!("Maximum calc() expression nesting depth ({MAX_CALC_DEPTH}) exceeded"));
    }
    let mut left = parse_factor(cursor, depth)?;

    while let Some(tok) = cursor.peek() {
        match tok {
            Token::Star => {
                cursor.next();
                match cursor.next() {
                    Some(Token::Number(scalar)) => {
                        if !scalar.is_finite() {
                            return Err(format!("Non-finite scalar multiplier in calc(): {scalar}"));
                        }
                        left = CalcExpr::Mul(Box::new(left), scalar);
                    }
                    other => {
                        return Err(format!(
                            "Expected scalar number after '*', found {:?}",
                            other
                        ))
                    }
                }
            }
            Token::Slash => {
                cursor.next();
                match cursor.next() {
                    Some(Token::Number(scalar)) => {
                        if !scalar.is_finite() || scalar == 0.0 {
                            return Err(format!("Division by zero or non-finite scalar in calc(): {scalar}"));
                        }
                        left = CalcExpr::Div(Box::new(left), scalar);
                    }
                    other => {
                        return Err(format!(
                            "Expected scalar number after '/', found {:?}",
                            other
                        ))
                    }
                }
            }
            _ => break,
        }
    }

    Ok(left)
}

// Precedence Level 3: Atoms, Parentheses, Values
fn parse_factor(cursor: &mut TokenCursor, depth: usize) -> Result<CalcExpr, String> {
    if depth > MAX_CALC_DEPTH {
        return Err(format!("Maximum calc() expression nesting depth ({MAX_CALC_DEPTH}) exceeded"));
    }
    match cursor.next() {
        Some(Token::Dim(dim)) => Ok(CalcExpr::Value(dim)),
        Some(Token::Number(num)) => {
            if !num.is_finite() {
                return Err(format!("Non-finite number in calc(): {num}"));
            }
            Ok(CalcExpr::Value(Dimension::Px(num)))
        }
        Some(Token::LParen) => {
            let inner = parse_add_sub(cursor, depth + 1)?;
            match cursor.next() {
                Some(Token::RParen) => Ok(inner),
                other => Err(format!("Expected ')', found {:?}", other)),
            }
        }
        other => Err(format!("Unexpected token in calc() factor: {:?}", other)),
    }
}
