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
    pub is_group_hovered: bool,
    pub is_group_focused: bool,
    pub is_group_active: bool,
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

/// Splits a comma-separated selector list respecting parentheses, brackets, and quotes.
pub fn split_selector_list(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut cur = String::new();
    let mut paren_depth = 0;
    let mut bracket_depth = 0;
    let mut in_quote = None;

    for c in input.chars() {
        if let Some(q) = in_quote {
            cur.push(c);
            if c == q {
                in_quote = None;
            }
        } else if c == '"' || c == '\'' {
            in_quote = Some(c);
            cur.push(c);
        } else if c == '(' {
            paren_depth += 1;
            cur.push(c);
        } else if c == ')' {
            if paren_depth > 0 {
                paren_depth -= 1;
            }
            cur.push(c);
        } else if c == '[' {
            bracket_depth += 1;
            cur.push(c);
        } else if c == ']' {
            if bracket_depth > 0 {
                bracket_depth -= 1;
            }
            cur.push(c);
        } else if c == ',' && paren_depth == 0 && bracket_depth == 0 {
            let s = cur.trim();
            if !s.is_empty() {
                result.push(s.to_string());
            }
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    let s = cur.trim();
    if !s.is_empty() {
        result.push(s.to_string());
    }
    result
}

/// Computes specificity for a subselector, including pseudo-classes (:is, :where, :not, :has).
pub fn compute_subselector_specificity(input: &str) -> Specificity {
    let trimmed = input.trim();
    let clean = if let Some(s) = trimmed.strip_prefix('>') {
        s.trim()
    } else if let Some(s) = trimmed.strip_prefix('+') {
        s.trim()
    } else if let Some(s) = trimmed.strip_prefix('~') {
        s.trim()
    } else {
        trimmed
    };

    let mut spec = Specificity::ZERO;
    let mut in_bracket = false;
    let mut chars = clean.chars().peekable();
    let mut last_was_boundary = true;

    while let Some(c) = chars.next() {
        if in_bracket {
            if c == ']' {
                in_bracket = false;
            }
            continue;
        }
        match c {
            '#' => {
                spec += Specificity::ID;
                last_was_boundary = false;
            }
            '.' => {
                spec += Specificity::CLASS;
                last_was_boundary = false;
            }
            '[' => {
                in_bracket = true;
                spec += Specificity::CLASS;
                last_was_boundary = false;
            }
            ':' => {
                if chars.peek() == Some(&':') {
                    chars.next();
                    spec += Specificity::TYPE;
                } else {
                    let mut ident = String::new();
                    while let Some(&nc) = chars.peek() {
                        if nc.is_alphanumeric() || nc == '-' || nc == '_' {
                            ident.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    if chars.peek() == Some(&'(') {
                        chars.next();
                        let mut depth = 1;
                        let mut inner = String::new();
                        for nc in chars.by_ref() {
                            if nc == '(' {
                                depth += 1;
                                inner.push(nc);
                            } else if nc == ')' {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                inner.push(nc);
                            } else {
                                inner.push(nc);
                            }
                        }
                        if ident == "where" {
                            // :where() has 0 specificity
                        } else if ident == "is" || ident == "not" || ident == "has" {
                            let mut inner_max = Specificity::ZERO;
                            for sub in split_selector_list(&inner) {
                                let sub_spec = compute_subselector_specificity(&sub);
                                if sub_spec > inner_max {
                                    inner_max = sub_spec;
                                }
                            }
                            spec += inner_max;
                        } else {
                            spec += Specificity::CLASS;
                        }
                    } else {
                        spec += Specificity::CLASS;
                    }
                }
                last_was_boundary = false;
            }
            c if c.is_alphabetic() && last_was_boundary => {
                spec += Specificity::TYPE;
                last_was_boundary = false;
            }
            c if c.is_whitespace() || c == '>' || c == '+' || c == '~' => {
                last_was_boundary = true;
            }
            _ => {
                last_was_boundary = false;
            }
        }
    }
    spec
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
                    let is_pseudo_element = if chars.peek() == Some(&':') {
                        chars.next();
                        true
                    } else {
                        false
                    };
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

                    if is_pseudo_element {
                        spec += Specificity::TYPE;
                    } else if ident.starts_with("where(") && ident.ends_with(')') {
                        // W3C Selectors Level 4: :where() always has zero specificity
                    } else if (ident.starts_with("is(")
                        || ident.starts_with("not(")
                        || ident.starts_with("has("))
                        && ident.ends_with(')')
                    {
                        // W3C Selectors Level 4: :is(), :not(), :has() take max specificity of their selector list
                        let start = if ident.starts_with("is(") { 3 } else { 4 };
                        let inner = &ident[start..ident.len() - 1];
                        let mut max_arg_spec = Specificity::ZERO;
                        for sub in split_selector_list(inner) {
                            let sub_spec = compute_subselector_specificity(&sub);
                            if sub_spec > max_arg_spec {
                                max_arg_spec = sub_spec;
                            }
                        }
                        spec += max_arg_spec;
                    } else {
                        spec += Specificity::CLASS;
                    }

                    parts.push(SelectorPart::PseudoClass(ident));
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
        self.matches_with_context(tag, classes, id, attributes, state, &[], None, None, &[])
    }

    /// Evaluates whether this compound selector matches an element considering sibling, positional, and children context.
    #[allow(clippy::too_many_arguments)]
    pub fn matches_with_context<C: AsRef<str>>(
        &self,
        tag: &str,
        classes: &[C],
        id: Option<&str>,
        attributes: &[(String, String)],
        state: &PseudoState,
        preceding_siblings: &[ElementRef],
        is_last_child: Option<bool>,
        is_last_of_type: Option<bool>,
        children: &[ElementRef],
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
                SelectorPart::PseudoClass(pseudo) => {
                    let p = pseudo.trim();
                    if p.eq_ignore_ascii_case("hover") {
                        if !state.is_hovered {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("focus") {
                        if !state.is_focused {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("active") {
                        if !state.is_active {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("disabled") {
                        if !state.is_disabled {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("dark") {
                        if !state.is_dark {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("root") {
                        if !tag.eq_ignore_ascii_case("html") && !tag.eq_ignore_ascii_case(":root") {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("first-child") {
                        let elem_siblings = preceding_siblings
                            .iter()
                            .filter(|s| !s.tag.starts_with('#'))
                            .count();
                        if elem_siblings != 0 {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("last-child") {
                        if is_last_child != Some(true) {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("only-child") {
                        let elem_siblings = preceding_siblings
                            .iter()
                            .filter(|s| !s.tag.starts_with('#'))
                            .count();
                        if elem_siblings != 0 || is_last_child != Some(true) {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("first-of-type") {
                        let type_siblings = preceding_siblings
                            .iter()
                            .filter(|s| s.tag.eq_ignore_ascii_case(tag))
                            .count();
                        if type_siblings != 0 {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("last-of-type") {
                        if is_last_of_type != Some(true) {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("only-of-type") {
                        let type_siblings = preceding_siblings
                            .iter()
                            .filter(|s| s.tag.eq_ignore_ascii_case(tag))
                            .count();
                        if type_siblings != 0 || is_last_of_type != Some(true) {
                            return false;
                        }
                    } else if p.starts_with("nth-child(") && p.ends_with(')') {
                        let formula = &p[10..p.len() - 1];
                        let child_idx = preceding_siblings
                            .iter()
                            .filter(|s| !s.tag.starts_with('#'))
                            .count()
                            + 1;
                        if !matches_an_plus_b(formula, child_idx) {
                            return false;
                        }
                    } else if p.starts_with("nth-of-type(") && p.ends_with(')') {
                        let formula = &p[12..p.len() - 1];
                        let type_idx = preceding_siblings
                            .iter()
                            .filter(|s| s.tag.eq_ignore_ascii_case(tag))
                            .count()
                            + 1;
                        if !matches_an_plus_b(formula, type_idx) {
                            return false;
                        }
                    } else if p.starts_with("not(") && p.ends_with(')') {
                        let inner = &p[4..p.len() - 1];
                        let el_classes: Vec<String> =
                            classes.iter().map(|c| c.as_ref().to_string()).collect();
                        let el = ElementRef {
                            tag,
                            classes: &el_classes,
                            id,
                            attributes,
                            children,
                        };
                        for sub in split_selector_list(inner) {
                            let sub_trimmed = sub.trim();
                            let matches_sub = if let Ok(sub_sel) = Selector::parse(sub_trimmed) {
                                sub_sel.matches_hierarchy_full(
                                    &el,
                                    &[],
                                    preceding_siblings,
                                    is_last_child,
                                    is_last_of_type,
                                    state,
                                )
                            } else if let Ok(inner_sel) = CompoundSelector::parse(sub_trimmed) {
                                inner_sel.matches_with_context(
                                    tag,
                                    classes,
                                    id,
                                    attributes,
                                    state,
                                    preceding_siblings,
                                    is_last_child,
                                    is_last_of_type,
                                    children,
                                )
                            } else {
                                false
                            };
                            if matches_sub {
                                return false;
                            }
                        }
                    } else if (p.starts_with("is(") || p.starts_with("where(")) && p.ends_with(')')
                    {
                        let start = if p.starts_with("is(") { 3 } else { 6 };
                        let inner = &p[start..p.len() - 1];
                        let el_classes: Vec<String> =
                            classes.iter().map(|c| c.as_ref().to_string()).collect();
                        let el = ElementRef {
                            tag,
                            classes: &el_classes,
                            id,
                            attributes,
                            children,
                        };
                        let mut any_matched = false;
                        for sub in split_selector_list(inner) {
                            let sub_trimmed = sub.trim();
                            let matches_sub = if let Ok(sub_sel) = Selector::parse(sub_trimmed) {
                                sub_sel.matches_hierarchy_full(
                                    &el,
                                    &[],
                                    preceding_siblings,
                                    is_last_child,
                                    is_last_of_type,
                                    state,
                                )
                            } else if let Ok(sub_sel) = CompoundSelector::parse(sub_trimmed) {
                                sub_sel.matches_with_context(
                                    tag,
                                    classes,
                                    id,
                                    attributes,
                                    state,
                                    preceding_siblings,
                                    is_last_child,
                                    is_last_of_type,
                                    children,
                                )
                            } else {
                                false
                            };
                            if matches_sub {
                                any_matched = true;
                                break;
                            }
                        }
                        if !any_matched {
                            return false;
                        }
                    } else if p.starts_with("has(") && p.ends_with(')') {
                        let inner = &p[4..p.len() - 1];
                        let mut any_matched = false;
                        for sub in split_selector_list(inner) {
                            let sub_trimmed = sub.trim();
                            if let Some(child_sel_str) = sub_trimmed.strip_prefix('>') {
                                let child_sel_str = child_sel_str.trim();
                                if let Ok(child_sel) = Selector::parse(child_sel_str) {
                                    for child in children {
                                        if child_sel.matches_hierarchy_full(
                                            child,
                                            &[],
                                            &[],
                                            None,
                                            None,
                                            state,
                                        ) {
                                            any_matched = true;
                                            break;
                                        }
                                    }
                                } else if let Ok(comp) = CompoundSelector::parse(child_sel_str) {
                                    for child in children {
                                        if comp.matches_with_context(
                                            child.tag,
                                            child.classes,
                                            child.id,
                                            child.attributes,
                                            state,
                                            &[],
                                            None,
                                            None,
                                            child.children,
                                        ) {
                                            any_matched = true;
                                            break;
                                        }
                                    }
                                }
                            } else {
                                // Descendant match across entire subtree
                                if let Ok(desc_sel) = Selector::parse(sub_trimmed) {
                                    fn check_desc(
                                        node: &ElementRef,
                                        sel: &Selector,
                                        st: &PseudoState,
                                    ) -> bool {
                                        if sel.matches_hierarchy_full(
                                            node,
                                            &[],
                                            &[],
                                            None,
                                            None,
                                            st,
                                        ) {
                                            return true;
                                        }
                                        for ch in node.children {
                                            if check_desc(ch, sel, st) {
                                                return true;
                                            }
                                        }
                                        false
                                    }
                                    for child in children {
                                        if check_desc(child, &desc_sel, state) {
                                            any_matched = true;
                                            break;
                                        }
                                    }
                                }
                            }
                            if any_matched {
                                break;
                            }
                        }
                        if !any_matched {
                            return false;
                        }
                    } else if p.eq_ignore_ascii_case("empty") {
                        let is_empty = children.is_empty()
                            && attributes
                                .iter()
                                .any(|(k, v)| k == "data-empty" && v == "true");
                        if !is_empty && !children.is_empty() {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Returns true if this compound selector references `:hover`, `:active`, or `:focus`.
    pub fn has_interactive_pseudo(&self) -> bool {
        self.parts.iter().any(|p| match p {
            SelectorPart::PseudoClass(name) => {
                let n = name.trim().to_ascii_lowercase();
                n == "hover"
                    || n == "active"
                    || n == "focus"
                    || n.contains(":hover")
                    || n.contains(":active")
                    || n.contains(":focus")
            }
            _ => false,
        })
    }
}

/// Light-weight reference to a DOM element for hierarchical selector matching.
#[derive(Debug, Clone, Copy)]
pub struct ElementRef<'a> {
    pub tag: &'a str,
    pub classes: &'a [String],
    pub id: Option<&'a str>,
    pub attributes: &'a [(String, String)],
    pub children: &'a [ElementRef<'a>],
}

impl<'a> ElementRef<'a> {
    /// Constructs a basic ElementRef with no children.
    pub const fn new(
        tag: &'a str,
        classes: &'a [String],
        id: Option<&'a str>,
        attributes: &'a [(String, String)],
    ) -> Self {
        Self {
            tag,
            classes,
            id,
            attributes,
            children: &[],
        }
    }

    /// Attaches child ElementRefs for :has() and structural evaluation.
    pub const fn with_children(mut self, children: &'a [ElementRef<'a>]) -> Self {
        self.children = children;
        self
    }
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

/// Evaluates whether a 1-based index matches an `An+B` CSS formula per W3C Selectors Level 4.
pub fn matches_an_plus_b(formula: &str, index: usize) -> bool {
    let trimmed = formula.trim().to_ascii_lowercase();
    if trimmed == "odd" {
        return index % 2 == 1;
    }
    if trimmed == "even" {
        return index.is_multiple_of(2);
    }
    if let Ok(exact) = trimmed.parse::<usize>() {
        return index == exact;
    }
    let n_idx = match trimmed.find('n') {
        Some(i) => i,
        None => return false,
    };
    let a_str = trimmed[..n_idx].trim();
    let a: i64 = match a_str {
        "" | "+" => 1,
        "-" => -1,
        s => match s.parse::<i64>() {
            Ok(v) => v,
            Err(_) => return false,
        },
    };
    let b: i64 = if n_idx + 1 < trimmed.len() {
        let b_str = trimmed[n_idx + 1..].replace(' ', "");
        b_str.parse::<i64>().unwrap_or(0)
    } else {
        0
    };

    let idx = index as i64;
    if a == 0 {
        idx == b
    } else {
        let diff = idx - b;
        if a > 0 {
            diff >= 0 && diff % a == 0
        } else {
            diff <= 0 && diff % a == 0
        }
    }
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
        let mut paren_depth = 0;

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

            if let Some(q) = in_quote {
                cur_buf.push(c);
                if c == q {
                    in_quote = None;
                }
                i += 1;
                continue;
            } else if c == '"' || c == '\'' {
                in_quote = Some(c);
                cur_buf.push(c);
                i += 1;
                continue;
            }

            if c == '(' {
                paren_depth += 1;
                cur_buf.push(c);
                i += 1;
                continue;
            }

            if c == ')' {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                cur_buf.push(c);
                i += 1;
                continue;
            }

            if paren_depth > 0 {
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

    /// Returns true if this compound selector references `:hover`, `:active`, or `:focus` (including inside `:is`/`:where`).
    pub fn has_interactive_pseudo(&self) -> bool {
        self.parts.iter().any(|p| match p {
            SelectorPart::PseudoClass(name) => {
                let n = name.trim().to_ascii_lowercase();
                n == "hover"
                    || n == "active"
                    || n == "focus"
                    || n.contains(":hover")
                    || n.contains(":active")
                    || n.contains(":focus")
            }
            _ => false,
        })
    }

    /// Returns true if this selector (either on the target or any ancestor/sibling combinator step)
    /// depends on `:hover`, `:active`, or `:focus`.
    pub fn has_any_interactive_pseudo(&self) -> bool {
        self.target.has_interactive_pseudo()
            || self
                .steps
                .iter()
                .any(|(_, comp)| comp.has_interactive_pseudo())
    }

    /// Returns true if this selector (either on target or any ancestor/sibling step) uses `:hover`.
    pub fn has_any_hover_pseudo(&self) -> bool {
        let compound_has_hover = |comp: &CompoundSelector| {
            comp.parts.iter().any(|p| match p {
                SelectorPart::PseudoClass(name) => {
                    let n = name.trim().to_ascii_lowercase();
                    n == "hover" || n.contains(":hover")
                }
                _ => false,
            })
        };
        compound_has_hover(&self.target) || self.steps.iter().any(|(_, c)| compound_has_hover(c))
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
        self.matches_hierarchy_full(target, ancestors, preceding_siblings, None, None, state)
    }

    /// Evaluates whether this selector matches an element given full ancestor, sibling, and position context.
    pub fn matches_hierarchy_full(
        &self,
        target: &ElementRef,
        ancestors: &[ElementRef],
        preceding_siblings: &[ElementRef],
        is_last_child: Option<bool>,
        is_last_of_type: Option<bool>,
        state: &PseudoState,
    ) -> bool {
        self.matches_hierarchy_with_ancestor_states(
            target,
            ancestors,
            None,
            preceding_siblings,
            None,
            is_last_child,
            is_last_of_type,
            state,
        )
    }

    /// Evaluates whether this selector matches an element using per-ancestor and per-sibling `PseudoState`s
    /// when provided (enabling descendant rules like `.resizer:hover .resizer-line` when the ancestor is hovered).
    #[allow(clippy::too_many_arguments)]
    pub fn matches_hierarchy_with_ancestor_states(
        &self,
        target: &ElementRef,
        ancestors: &[ElementRef],
        ancestor_states: Option<&[PseudoState]>,
        preceding_siblings: &[ElementRef],
        sibling_states: Option<&[PseudoState]>,
        is_last_child: Option<bool>,
        is_last_of_type: Option<bool>,
        state: &PseudoState,
    ) -> bool {
        // Step 1: Target element must match target compound selector
        if !self.target.matches_with_context(
            target.tag,
            target.classes,
            target.id,
            target.attributes,
            state,
            preceding_siblings,
            is_last_child,
            is_last_of_type,
            target.children,
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
                    let parent_state = ancestor_states
                        .and_then(|s| s.get(cur_ancestor_idx))
                        .unwrap_or(state);
                    let _parent_preceding = if cur_ancestor_idx > 0 {
                        &ancestors[..cur_ancestor_idx]
                    } else {
                        &[]
                    };
                    if !compound.matches_with_context(
                        parent.tag,
                        parent.classes,
                        parent.id,
                        parent.attributes,
                        parent_state,
                        _parent_preceding,
                        None,
                        None,
                        parent.children,
                    ) {
                        return false;
                    }
                }
                Combinator::Descendant => {
                    let mut matched = false;
                    while cur_ancestor_idx > 0 {
                        cur_ancestor_idx -= 1;
                        let anc = &ancestors[cur_ancestor_idx];
                        let anc_state = ancestor_states
                            .and_then(|s| s.get(cur_ancestor_idx))
                            .unwrap_or(state);
                        if compound.matches_with_context(
                            anc.tag,
                            anc.classes,
                            anc.id,
                            anc.attributes,
                            anc_state,
                            &[],
                            None,
                            None,
                            anc.children,
                        ) {
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
                    let sib_state = sibling_states
                        .and_then(|s| s.get(cur_sibling_idx))
                        .unwrap_or(state);
                    let sib_preceding = &preceding_siblings[..cur_sibling_idx];
                    if !compound.matches_with_context(
                        sib.tag,
                        sib.classes,
                        sib.id,
                        sib.attributes,
                        sib_state,
                        sib_preceding,
                        None,
                        None,
                        sib.children,
                    ) {
                        return false;
                    }
                }
                Combinator::SubsequentSibling => {
                    let mut matched = false;
                    while cur_sibling_idx > 0 {
                        cur_sibling_idx -= 1;
                        let sib = &preceding_siblings[cur_sibling_idx];
                        let sib_state = sibling_states
                            .and_then(|s| s.get(cur_sibling_idx))
                            .unwrap_or(state);
                        let sib_preceding = &preceding_siblings[..cur_sibling_idx];
                        if compound.matches_with_context(
                            sib.tag,
                            sib.classes,
                            sib.id,
                            sib.attributes,
                            sib_state,
                            sib_preceding,
                            None,
                            None,
                            sib.children,
                        ) {
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
