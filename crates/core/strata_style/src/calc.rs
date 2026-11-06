//! CSS calc() mathematical expression tree, tokenizer, parser, and evaluator.
//!
//! Evaluates complex mixed-unit expressions (e.g. `calc(100% - 24px + 2rem)`)
//! with strict arithmetic operator precedence and contextual dimension resolution.

use super::types::Dimension;
use std::sync::{Arc, RwLock};

/// Identifier for an interned calc expression AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CalcId(pub u32);

static CALC_INTERNER: std::sync::LazyLock<RwLock<Vec<Arc<CalcExpr>>>> =
    std::sync::LazyLock::new(|| RwLock::new(Vec::new()));

/// Interns a calc expression AST into the global cache and returns its lightweight CalcId.
pub fn intern_calc(expr: CalcExpr) -> CalcId {
    let mut table = CALC_INTERNER.write().unwrap();
    let id = table.len() as u32;
    table.push(Arc::new(expr));
    CalcId(id)
}

/// Retrieves an interned calc expression AST by its CalcId.
pub fn get_calc(id: CalcId) -> Option<Arc<CalcExpr>> {
    let table = CALC_INTERNER.read().unwrap();
    table.get(id.0 as usize).cloned()
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
        match self {
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
            CalcExpr::Mul(left, scalar) => left.eval(ctx) * scalar,
            CalcExpr::Div(left, scalar) => {
                if *scalar != 0.0 {
                    left.eval(ctx) / scalar
                } else {
                    0.0
                }
            }
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
    let expr = parse_add_sub(&mut cursor)?;

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
    if raw.ends_with("px") {
        raw[..raw.len() - 2].parse::<f32>().ok().map(Dimension::Px)
    } else if raw.ends_with('%') {
        raw[..raw.len() - 1]
            .parse::<f32>()
            .ok()
            .map(Dimension::Percent)
    } else if raw.ends_with("rem") {
        raw[..raw.len() - 3].parse::<f32>().ok().map(Dimension::Rem)
    } else if raw.ends_with("em") {
        raw[..raw.len() - 2].parse::<f32>().ok().map(Dimension::Em)
    } else if raw.ends_with("vw") {
        raw[..raw.len() - 2].parse::<f32>().ok().map(Dimension::Vw)
    } else if raw.ends_with("vh") {
        raw[..raw.len() - 2].parse::<f32>().ok().map(Dimension::Vh)
    } else {
        None
    }
}

// Precedence Level 1: Add (+) and Sub (-)
fn parse_add_sub(cursor: &mut TokenCursor) -> Result<CalcExpr, String> {
    let mut left = parse_mul_div(cursor)?;

    while let Some(tok) = cursor.peek() {
        match tok {
            Token::Plus => {
                cursor.next();
                let right = parse_mul_div(cursor)?;
                left = CalcExpr::Add(Box::new(left), Box::new(right));
            }
            Token::Minus => {
                cursor.next();
                let right = parse_mul_div(cursor)?;
                left = CalcExpr::Sub(Box::new(left), Box::new(right));
            }
            _ => break,
        }
    }

    Ok(left)
}

// Precedence Level 2: Mul (*) and Div (/)
fn parse_mul_div(cursor: &mut TokenCursor) -> Result<CalcExpr, String> {
    let mut left = parse_factor(cursor)?;

    while let Some(tok) = cursor.peek() {
        match tok {
            Token::Star => {
                cursor.next();
                match cursor.next() {
                    Some(Token::Number(scalar)) => {
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
fn parse_factor(cursor: &mut TokenCursor) -> Result<CalcExpr, String> {
    match cursor.next() {
        Some(Token::Dim(dim)) => Ok(CalcExpr::Value(dim)),
        Some(Token::Number(num)) => Ok(CalcExpr::Value(Dimension::Px(num))),
        Some(Token::LParen) => {
            let inner = parse_add_sub(cursor)?;
            match cursor.next() {
                Some(Token::RParen) => Ok(inner),
                other => Err(format!("Expected ')', found {:?}", other)),
            }
        }
        other => Err(format!("Unexpected token in calc() factor: {:?}", other)),
    }
}
