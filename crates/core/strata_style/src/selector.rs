//! CSS Selector parser, specificity calculator, and hierarchical element matcher.
//!
//! Implements W3C Selectors Level 3 & Level 4:
//! - Compound selectors (Universal, Type, ID, Class, Attribute, Pseudo-class)
//! - Combinators: Descendant (` `), Child (`>`), Adjacent Sibling (`+`), Subsequent Sibling (`~`)
//! - Attribute matching operators: `[attr]`, `[attr=val]`, `[attr^=val]`, `[attr$=val]`, `[attr*=val]`, `[attr~=val]`, `[attr|=val]`
//! - W3C 4-tuple specificity calculation `(inline, id, class/attr/pseudo, type)`

use crate::specificity::Specificity;

/// Interactive element pseudo-classes and environmental state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PseudoState {
    pub is_hovered: bool,
    pub is_focused: bool,
    pub is_active: bool,
    pub is_disabled: bool,
    pub is_dark: bool,
}

/// Attribute matching operator per W3C CSS Selectors Level 4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeMatchOp {
    /// `[attr]` - Element has attribute regardless of value.
    Exists,
    /// `[attr=val]` - Element attribute equals exact value.
    Exact(String),
    /// `[attr^=val]` - Element attribute value begins with prefix.
    Prefix(String),
    /// `[attr$=val]` - Element attribute value ends with suffix.
    Suffix(String),
    /// `[attr*=val]` - Element attribute value contains substring.
    Substring(String),
    /// `[attr~=val]` - Element attribute value is a whitespace-separated list containing word.
    Includes(String),
    /// `[attr|=val]` - Element attribute value is exactly val or starts with val followed by '-'.
    DashMatch(String),
}

/// Parsed attribute selector declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeSelector {
    pub name: String,
    pub op: AttributeMatchOp,
}

impl AttributeSelector {
    /// Evaluates whether this attribute selector matches a list of key-value attributes.
    pub fn matches(&self, attributes: &[(String, String)]) -> bool {
        for (k, v) in attributes {
            if k.eq_ignore_ascii_case(&self.name) {
                return match &self.op {
                    AttributeMatchOp::Exists => true,
                    AttributeMatchOp::Exact(expected) => v == expected,
                    AttributeMatchOp::Prefix(prefix) => v.starts_with(prefix),
                    AttributeMatchOp::Suffix(suffix) => v.ends_with(suffix),
                    AttributeMatchOp::Substring(sub) => v.contains(sub),
                    AttributeMatchOp::Includes(word) => v.split_whitespace().any(|w| w == word),
                    AttributeMatchOp::DashMatch(prefix) => {
                        v == prefix || v.starts_with(&format!("{}-", prefix))
                    }
                };
            }
        }
        false
    }
}

/// Atomic part of a CSS compound selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorPart {
    Universal,
    Tag(String),
    Id(String),
    Class(String),
    Attribute(AttributeSelector),
    PseudoClass(String),
}

/// Combinator relating two compound selectors in a complex selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    /// `E F` - Element F is an arbitrary descendant of element E.
    Descendant,
    /// `E > F` - Element F is an immediate child of element E.
    Child,
    /// `E + F` - Element F is immediately preceded by sibling element E.
    AdjacentSibling,
    /// `E ~ F` - Element F is preceded by sibling element E.
    SubsequentSibling,
}

/// A compound selector representing a sequence of simple selectors without combinators.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompoundSelector {
    pub parts: Vec<SelectorPart>,
    pub specificity: Specificity,
}

impl CompoundSelector {
    /// Parses a single compound selector (e.g. `button.btn-primary:hover[disabled]`).
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("Empty compound selector".to_string());
        }

        let mut parts = Vec::new();
        let mut spec = Specificity::ZERO;
        let mut chars = trimmed.chars().peekable();

        while let Some(&c) = chars.peek() {
            match c {
                '*' => {
                    chars.next();
                    parts.push(SelectorPart::Universal);
                }
                '#' => {
                    chars.next();
                    let ident = parse_ident(&mut chars);
                    if ident.is_empty() {
                        return Err("Empty ID selector after '#' in CSS".to_string());
                    }
                    parts.push(SelectorPart::Id(ident));
                    spec += Specificity::ID;
                }
                '.' => {
                    chars.next();
                    let ident = parse_ident(&mut chars);
                    if ident.is_empty() {
                        return Err("Empty class selector after '.' in CSS".to_string());
                    }
                    parts.push(SelectorPart::Class(ident));
                    spec += Specificity::CLASS;
                }
                ':' => {
                    chars.next();
                    // Handle double colons for pseudo-elements as pseudo-classes
                    if chars.peek() == Some(&':') {
                        chars.next();
                    }
                    let mut ident = parse_ident(&mut chars);
                    if ident.is_empty() {
                        return Err("Empty pseudo-class after ':' in CSS".to_string());
                    }
                    if chars.peek() == Some(&'(') {
                        chars.next();
                        let mut depth = 1;
                        ident.push('(');
                        for ch in chars.by_ref() {
                            ident.push(ch);
                            if ch == '(' {
                                depth += 1;
                            } else if ch == ')' {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                        }
                    }
                    parts.push(SelectorPart::PseudoClass(ident));
                    spec += Specificity::CLASS;
                }
                '[' => {
                    chars.next();
                    let attr_sel = parse_attribute_selector(&mut chars)?;
                    parts.push(SelectorPart::Attribute(attr_sel));
                    spec += Specificity::CLASS;
                }
                _ if c.is_alphabetic() || c == '_' || c == '-' => {
                    let ident = parse_ident(&mut chars);
                    parts.push(SelectorPart::Tag(ident));
                    spec += Specificity::TYPE;
                }
                _ => {
                    return Err(format!(
                        "Unexpected character '{}' in compound selector '{}'",
                        c, trimmed
                    ));
                }
            }
        }

        Ok(Self {
            parts,
            specificity: spec,
        })
    }

    /// Evaluates whether this compound selector matches an element's attributes and pseudo-state.
    pub fn matches<C: AsRef<str>>(
        &self,
        tag: &str,
        classes: &[C],
        id: Option<&str>,
        attributes: &[(String, String)],
        state: &PseudoState,
    ) -> bool {
        for part in &self.parts {
            match part {
                SelectorPart::Universal => {}
                SelectorPart::Tag(expected_tag) => {
                    if !expected_tag.eq_ignore_ascii_case(tag) {
                        return false;
                    }
                }
                SelectorPart::Id(expected_id) => match id {
                    Some(actual_id) if actual_id == expected_id => {}
                    _ => return false,
                },
                SelectorPart::Class(expected_class) => {
                    if !classes
                        .iter()
                        .any(|c| c.as_ref() == expected_class.as_str())
                    {
                        return false;
                    }
                }
                SelectorPart::Attribute(attr_sel) => {
                    if !attr_sel.matches(attributes) {
                        return false;
                    }
                }
                SelectorPart::PseudoClass(pseudo) => match pseudo.as_str() {
                    "hover" => {
                        if !state.is_hovered {
                            return false;
                        }
                    }
                    "focus" => {
                        if !state.is_focused {
                            return false;
                        }
                    }
                    "active" => {
                        if !state.is_active {
                            return false;
                        }
                    }
                    "disabled" => {
                        if !state.is_disabled {
                            return false;
                        }
                    }
                    "dark" => {
                        if !state.is_dark {
                            return false;
                        }
                    }
                    "root" => {
                        if !tag.eq_ignore_ascii_case("html") && !tag.eq_ignore_ascii_case(":root") {
                            return false;
                        }
                    }
                    _ => return false,
                },
            }
        }
        true
    }
}

/// Light-weight reference to a DOM element for hierarchical selector matching.
#[derive(Debug, Clone, Copy)]
pub struct ElementRef<'a> {
    pub tag: &'a str,
    pub classes: &'a [String],
    pub id: Option<&'a str>,
    pub attributes: &'a [(String, String)],
}

/// Parsed CSS selector with support for combinators and W3C specificity calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub raw: String,
    /// Target compound selector (rightmost subject in CSS rule).
    pub target: CompoundSelector,
    /// Combinator steps from right to left (Combinator preceding the target, CompoundSelector of that element).
    pub steps: Vec<(Combinator, CompoundSelector)>,
    /// Simple parts of the target compound selector for backward compatibility.
    pub parts: Vec<SelectorPart>,
    /// Total W3C specificity for the entire selector chain.
    pub specificity: Specificity,
}

impl Selector {
    /// Parses a complete CSS selector, including combinators (e.g. `div.card > ul + p.text[title]`).
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("Empty selector".to_string());
        }

        // Tokenize selector into alternating compound selector strings and combinators
        let mut segments: Vec<(Option<Combinator>, String)> = Vec::new();
        let mut cur_buf = String::new();
        let mut pending_combinator: Option<Combinator> = None;
        let mut in_bracket = false;
        let mut in_quote = None;

        let chars: Vec<char> = trimmed.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if in_bracket {
                cur_buf.push(c);
                if let Some(q) = in_quote {
                    if c == q {
                        in_quote = None;
                    }
                } else if c == '"' || c == '\'' {
                    in_quote = Some(c);
                } else if c == ']' {
                    in_bracket = false;
                }
                i += 1;
                continue;
            }

            if c == '[' {
                in_bracket = true;
                cur_buf.push(c);
                i += 1;
                continue;
            }

            if c == '>' || c == '+' || c == '~' {
                let trimmed_buf = cur_buf.trim().to_string();
                if !trimmed_buf.is_empty() {
                    segments.push((pending_combinator.take(), trimmed_buf));
                    cur_buf.clear();
                }
                let comb = match c {
                    '>' => Combinator::Child,
                    '+' => Combinator::AdjacentSibling,
                    '~' => Combinator::SubsequentSibling,
                    _ => unreachable!(),
                };
                pending_combinator = Some(comb);
                i += 1;
                continue;
            }

            if c.is_whitespace() {
                let trimmed_buf = cur_buf.trim().to_string();
                if !trimmed_buf.is_empty() {
                    segments.push((pending_combinator.take(), trimmed_buf));
                    cur_buf.clear();
                    // Set default descendant combinator if no explicit combinator follows
                    pending_combinator = Some(Combinator::Descendant);
                }
                i += 1;
                continue;
            }

            cur_buf.push(c);
            i += 1;
        }

        let trimmed_buf = cur_buf.trim().to_string();
        if !trimmed_buf.is_empty() {
            segments.push((pending_combinator.take(), trimmed_buf));
        }

        if segments.is_empty() {
            return Err("Failed to extract selector segments".to_string());
        }

        // Parse each compound segment and accumulate specificity
        let mut parsed_compounds: Vec<(Option<Combinator>, CompoundSelector)> = Vec::new();
        let mut total_spec = Specificity::ZERO;

        for (comb, seg_str) in segments {
            let compound = CompoundSelector::parse(&seg_str)?;
            total_spec += compound.specificity;
            parsed_compounds.push((comb, compound));
        }

        // Rightmost segment is the target selector
        let (mut pending_comb, target) = parsed_compounds.pop().unwrap();
        let parts = target.parts.clone();

        // Remaining segments in reverse order represent the right-to-left traversal steps
        let mut steps = Vec::new();
        while let Some((prev_comb, compound)) = parsed_compounds.pop() {
            let comb = pending_comb.unwrap_or(Combinator::Descendant);
            steps.push((comb, compound));
            pending_comb = prev_comb;
        }

        Ok(Self {
            raw: trimmed.to_string(),
            target,
            steps,
            parts,
            specificity: total_spec,
        })
    }

    /// Evaluates whether this selector matches an element without ancestors (simple matching).
    pub fn matches(
        &self,
        tag: &str,
        classes: &[&str],
        id: Option<&str>,
        state: &PseudoState,
    ) -> bool {
        self.target.matches(tag, classes, id, &[], state)
    }

    /// Evaluates whether this selector matches an element given its full ancestor chain and preceding siblings.
    pub fn matches_hierarchy(
        &self,
        target: &ElementRef,
        ancestors: &[ElementRef],
        preceding_siblings: &[ElementRef],
        state: &PseudoState,
    ) -> bool {
        // Step 1: Target element must match target compound selector
        if !self.target.matches(
            target.tag,
            target.classes,
            target.id,
            target.attributes,
            state,
        ) {
            return false;
        }

        if self.steps.is_empty() {
            return true;
        }

        // Step 2: Traverse right-to-left combinator chain
        let mut cur_ancestor_idx = ancestors.len();
        let mut cur_sibling_idx = preceding_siblings.len();

        for (comb, compound) in &self.steps {
            match comb {
                Combinator::Child => {
                    if cur_ancestor_idx == 0 {
                        return false;
                    }
                    cur_ancestor_idx -= 1;
                    let parent = &ancestors[cur_ancestor_idx];
                    if !compound.matches(
                        parent.tag,
                        parent.classes,
                        parent.id,
                        parent.attributes,
                        state,
                    ) {
                        return false;
                    }
                }
                Combinator::Descendant => {
                    let mut matched = false;
                    while cur_ancestor_idx > 0 {
                        cur_ancestor_idx -= 1;
                        let anc = &ancestors[cur_ancestor_idx];
                        if compound.matches(anc.tag, anc.classes, anc.id, anc.attributes, state) {
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        return false;
                    }
                }
                Combinator::AdjacentSibling => {
                    if cur_sibling_idx == 0 {
                        return false;
                    }
                    cur_sibling_idx -= 1;
                    let sib = &preceding_siblings[cur_sibling_idx];
                    if !compound.matches(sib.tag, sib.classes, sib.id, sib.attributes, state) {
                        return false;
                    }
                }
                Combinator::SubsequentSibling => {
                    let mut matched = false;
                    while cur_sibling_idx > 0 {
                        cur_sibling_idx -= 1;
                        let sib = &preceding_siblings[cur_sibling_idx];
                        if compound.matches(sib.tag, sib.classes, sib.id, sib.attributes, state) {
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        return false;
                    }
                }
            }
        }

        true
    }
}

fn parse_attribute_selector<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<AttributeSelector, String> {
    let mut name = String::new();
    while let Some(&c) = chars.peek() {
        if c == ']' || c == '=' || c == '~' || c == '|' || c == '^' || c == '$' || c == '*' {
            break;
        }
        if !c.is_whitespace() {
            name.push(c);
        }
        chars.next();
    }

    if name.is_empty() {
        return Err("Empty attribute name in '[...]' selector".to_string());
    }

    let mut op_str = String::new();
    while let Some(&c) = chars.peek() {
        if c == '=' || c == '~' || c == '|' || c == '^' || c == '$' || c == '*' {
            op_str.push(c);
            chars.next();
        } else {
            break;
        }
    }

    if op_str.is_empty() {
        // [attr]
        if chars.peek() == Some(&']') {
            chars.next();
        }
        return Ok(AttributeSelector {
            name,
            op: AttributeMatchOp::Exists,
        });
    }

    // Skip whitespace before value
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }

    // Read attribute value (handling optional single/double quotes)
    let mut val = String::new();
    let quote = if chars.peek() == Some(&'"') || chars.peek() == Some(&'\'') {
        Some(chars.next().unwrap())
    } else {
        None
    };

    while let Some(&c) = chars.peek() {
        if let Some(q) = quote {
            if c == q {
                chars.next();
                break;
            }
            val.push(c);
            chars.next();
        } else {
            if c == ']' || c.is_whitespace() {
                break;
            }
            val.push(c);
            chars.next();
        }
    }

    // Advance past closing ']'
    for c in chars.by_ref() {
        if c == ']' {
            break;
        }
    }

    let op = match op_str.as_str() {
        "=" => AttributeMatchOp::Exact(val),
        "^=" => AttributeMatchOp::Prefix(val),
        "$=" => AttributeMatchOp::Suffix(val),
        "*=" => AttributeMatchOp::Substring(val),
        "~=" => AttributeMatchOp::Includes(val),
        "|=" => AttributeMatchOp::DashMatch(val),
        _ => return Err(format!("Unsupported attribute match operator '{}'", op_str)),
    };

    Ok(AttributeSelector { name, op })
}

fn parse_ident<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> String {
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            s.push(chars.next().unwrap());
        } else {
            break;
        }
    }
    s
}
