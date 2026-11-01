//! Abstract Syntax Tree (AST) definitions for RSX templates.

use proc_macro::TokenStream;

#[derive(Debug, Clone)]
pub enum RsxNode {
    Element(RsxElement),
    Component(RsxComponent),
    Fragment(Vec<RsxNode>),
    Expression(TokenStream),
    Text(String),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RsxElement {
    pub tag: String,
    pub attributes: Vec<RsxAttribute>,
    pub children: Vec<RsxNode>,
    pub self_closing: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RsxComponent {
    pub path: Vec<String>,
    pub props: Vec<RsxAttribute>,
    pub children: Vec<RsxNode>,
    pub self_closing: bool,
}

#[derive(Debug, Clone)]
pub struct RsxAttribute {
    pub name: String,
    pub value: RsxAttrValue,
}

#[derive(Debug, Clone)]
pub enum RsxAttrValue {
    Literal(String),
    Expression(TokenStream),
    Flag,
}
