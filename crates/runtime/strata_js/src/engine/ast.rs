//! Abstract Syntax Tree (AST) definitions for JavaScript parsing and evaluation.

use crate::JsValue;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    StrictEq,
    NotEq,
    StrictNotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
    NullishCoalescing,
    InstanceOf,
    Power,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
    Pos,
    BitNot,
    Typeof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOp {
    Increment,
    Decrement,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    Literal(String),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArrowBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(JsValue),
    Identifier(String),
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Update {
        op: UpdateOp,
        target: Box<Expr>,
        prefix: bool,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Member {
        object: Box<Expr>,
        property: String,
        computed: Option<Box<Expr>>,
    },
    OptionalMember {
        object: Box<Expr>,
        property: String,
        computed: Option<Box<Expr>>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    OptionalCall {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    New {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Spread(Box<Expr>),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    TemplateLiteral(Vec<TemplatePart>),
    ArrowFunction {
        params: Vec<String>,
        body: ArrowBody,
        is_async: bool,
    },
    Await(Box<Expr>),
    RegExpLiteral {
        pattern: String,
        flags: String,
    },
    Yield(Option<Box<Expr>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatementFlow {
    Normal(JsValue),
    Return(JsValue),
    Throw(JsValue),
    Break(Option<String>),
    Continue(Option<String>),
    Yield(JsValue),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDef {
    pub name: String,
    pub super_class: Option<String>,
    pub constructor: Option<(Vec<String>, Vec<Stmt>)>,
    pub methods: HashMap<String, (Vec<String>, Vec<Stmt>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    pub test: Option<Expr>,
    pub consequent: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DestructureObjectProp {
    pub source_key: String,
    pub target_name: String,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DestructureArrayElem {
    pub target_name: String,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DestructurePattern {
    Object {
        properties: Vec<DestructureObjectProp>,
        rest: Option<String>,
    },
    Array {
        elements: Vec<Option<DestructureArrayElem>>,
        rest: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl {
        name: String,
        init: Option<Expr>,
    },
    DestructureDecl {
        pattern: DestructurePattern,
        init: Expr,
    },
    Assign {
        target: Expr,
        value: Expr,
    },
    Expr(Expr),
    Block(Vec<Stmt>),
    If {
        cond: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
    },
    Directive(String),
    FunctionDecl {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
        is_async: bool,
        is_generator: bool,
    },
    ClassDecl {
        name: String,
        super_class: Option<String>,
        constructor: Option<(Vec<String>, Vec<Stmt>)>,
        methods: Vec<(String, Vec<String>, Vec<Stmt>)>,
    },
    Return(Option<Expr>),
    TryCatch {
        try_block: Vec<Stmt>,
        catch_param: Option<String>,
        catch_block: Option<Vec<Stmt>>,
        finally_block: Option<Vec<Stmt>>,
    },
    Throw(Expr),
    Break(Option<String>),
    Continue(Option<String>),
    Switch {
        discriminant: Expr,
        cases: Vec<SwitchCase>,
    },
}
