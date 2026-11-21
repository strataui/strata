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
    pub leading_colon: bool,
    pub path: Vec<String>,
    pub generics: Option<String>,
    pub props: Vec<RsxAttribute>,
    pub children: Vec<RsxNode>,
    pub self_closing: bool,
}

impl RsxComponent {
    pub fn full_path(&self) -> String {
        format_tag_path(self.leading_colon, &self.path)
    }
}

pub fn format_tag_path(leading_colon: bool, path: &[String]) -> String {
    if leading_colon {
        format!("::{}", path.join("::"))
    } else {
        path.join("::")
    }
}

pub fn segment_starts_uppercase(seg: &str) -> bool {
    seg.strip_prefix("r#")
        .unwrap_or(seg)
        .chars()
        .next()
        .is_some_and(|c| c.is_uppercase())
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
    Spread(TokenStream),
}
