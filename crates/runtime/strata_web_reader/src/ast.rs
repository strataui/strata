//! Semantic DOM AST node definitions and traversal helpers.

/// A node in the semantic DOM tree.
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticNode {
    /// HTML Element containing attributes and child nodes.
    Element {
        tag: String,
        attributes: Vec<(String, String)>,
        children: Vec<SemanticNode>,
    },
    /// Text leaf node.
    Text(String),
    /// HTML comment.
    Comment(String),
}

impl SemanticNode {
    /// Constructs an element node.
    pub fn element(tag: impl Into<String>, attributes: Vec<(String, String)>, children: Vec<SemanticNode>) -> Self {
        Self::Element {
            tag: tag.into(),
            attributes,
            children,
        }
    }

    /// Constructs a text node.
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(content.into())
    }

    /// Returns the element tag name if this is an element.
    pub fn tag(&self) -> Option<&str> {
        match self {
            Self::Element { tag, .. } => Some(tag.as_str()),
            _ => None,
        }
    }

    /// Looks up an attribute value by case-insensitive name.
    pub fn attr(&self, name: &str) -> Option<&str> {
        match self {
            Self::Element { attributes, .. } => {
                let name_lower = name.to_lowercase();
                for (k, v) in attributes {
                    if k.eq_ignore_ascii_case(&name_lower) {
                        return Some(v.as_str());
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Returns the element ID if present.
    pub fn id(&self) -> Option<&str> {
        self.attr("id")
    }

    /// Returns whether this element has a given CSS class.
    pub fn has_class(&self, class_name: &str) -> bool {
        if let Some(classes) = self.attr("class") {
            classes.split_whitespace().any(|c| c == class_name)
        } else {
            false
        }
    }

    /// Returns a list of all CSS classes on this element.
    pub fn classes(&self) -> Vec<&str> {
        if let Some(classes) = self.attr("class") {
            classes.split_whitespace().collect()
        } else {
            Vec::new()
        }
    }

    /// Extracts plain text content recursively from this node and its descendants.
    pub fn text_content(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        match self {
            Self::Text(t) => {
                if !out.is_empty()
                    && !out.ends_with(char::is_whitespace)
                    && !t.starts_with(char::is_whitespace)
                    && !t.starts_with(',')
                    && !t.starts_with('.')
                    && !t.starts_with(';')
                    && !t.starts_with(':')
                    && !t.starts_with(')')
                    && !t.starts_with(']')
                    && !out.ends_with('(')
                    && !out.ends_with('[')
                {
                    out.push(' ');
                }
                out.push_str(t);
            }
            Self::Element { tag, children, .. } => {
                if tag == "style" || tag == "script" {
                    return;
                }
                for child in children {
                    child.collect_text(out);
                }
            }
            Self::Comment(_) => {}
        }
    }

    /// Recursively finds the first descendant element matching the specified tag.
    pub fn find_first(&self, target_tag: &str) -> Option<&SemanticNode> {
        match self {
            Self::Element { tag, children, .. } => {
                if tag == target_tag {
                    return Some(self);
                }
                for child in children {
                    if let Some(found) = child.find_first(target_tag) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Recursively finds all descendant elements matching the specified tag.
    pub fn find_all(&self, target_tag: &str) -> Vec<&SemanticNode> {
        let mut results = Vec::new();
        self.collect_all(target_tag, &mut results);
        results
    }

    fn collect_all<'a>(&'a self, target_tag: &str, results: &mut Vec<&'a SemanticNode>) {
        if let Self::Element { tag, children, .. } = self {
            if tag == target_tag {
                results.push(self);
            }
            for child in children {
                child.collect_all(target_tag, results);
            }
        }
    }

    /// Recursively finds all descendant elements containing the specified CSS class.
    pub fn find_by_class(&self, target_class: &str) -> Vec<&SemanticNode> {
        let mut results = Vec::new();
        self.collect_by_class(target_class, &mut results);
        results
    }

    fn collect_by_class<'a>(&'a self, target_class: &str, results: &mut Vec<&'a SemanticNode>) {
        if let Self::Element { children, .. } = self {
            if self.has_class(target_class) {
                results.push(self);
            }
            for child in children {
                child.collect_by_class(target_class, results);
            }
        }
    }

    /// Recursively finds the first descendant element with the specified ID.
    pub fn find_by_id(&self, target_id: &str) -> Option<&SemanticNode> {
        if let Self::Element { children, .. } = self {
            if self.id() == Some(target_id) {
                return Some(self);
            }
            for child in children {
                if let Some(found) = child.find_by_id(target_id) {
                    return Some(found);
                }
            }
        }
        None
    }
}
