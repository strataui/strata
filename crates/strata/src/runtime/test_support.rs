//! Headless test helpers: locate elements by visible text or `title`/`alt`, and click them.
//!
//! These replace hand-rolled depth-first searches over `NodeBlueprint` / `LayoutNode` that match
//! on `Debug` output. Matching walks the blueprint and layout trees in lockstep (so it honours
//! fragment flattening and skips `display: none` subtrees) and returns hierarchical index paths
//! compatible with [`AppRuntime::find_node_by_path`].
//!
//! Matches are returned in document (pre-order) order. Later elements, such as modals and
//! `<Portal>` content, are painted above earlier ones, so use the `*_last` variants to target the
//! top-most match.

use super::*;

/// How a [`NodeQuery`] compares its needle against an element's text or title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchMode {
    /// Whitespace-normalized equality (default).
    #[default]
    Exact,
    /// Whitespace-normalized substring containment.
    Contains,
}

/// Declarative element query for [`AppRuntime::find_all`], [`AppRuntime::click`] and friends.
///
/// When both `text` and `title` are set, an element must satisfy both. Only the *deepest*
/// matching elements are reported: an ancestor whose text merely aggregates a matching
/// descendant is not itself a match.
#[derive(Debug, Clone, Default)]
pub struct NodeQuery {
    text: Option<String>,
    title: Option<String>,
    mode: MatchMode,
}

impl NodeQuery {
    /// Matches elements whose rendered text content equals `text` (see [`NodeQuery::contains`]).
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }

    /// Matches elements whose `title` attribute (or `alt` on `<button>` / `<img>`) equals `title`.
    pub fn title(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            ..Self::default()
        }
    }

    /// Additionally requires the element's rendered text content to match.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Additionally requires the element's `title` / `alt` to match.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Switches from whitespace-normalized equality to substring containment.
    pub fn contains(mut self) -> Self {
        self.mode = MatchMode::Contains;
        self
    }

    fn matches_str(&self, needle: &str, haystack: &str) -> bool {
        let needle = normalize_ws(needle);
        let haystack = normalize_ws(haystack);
        match self.mode {
            MatchMode::Exact => haystack == needle,
            MatchMode::Contains => haystack.contains(&needle),
        }
    }
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collect_text(bp: &NodeBlueprint, out: &mut String) {
    match bp {
        NodeBlueprint::Text(t) => {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(t);
        }
        NodeBlueprint::DynamicText(f) => {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(&f());
        }
        NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
            for c in children {
                collect_text(c, out);
            }
        }
        NodeBlueprint::Empty => {}
    }
}

fn own_tooltip_attrs(bp: &NodeBlueprint) -> Vec<&str> {
    let mut found = Vec::new();
    if let NodeBlueprint::Element {
        tag, attributes, ..
    } = bp
    {
        let supports_alt = match tag {
            strata_dom::Tag::Img | strata_dom::Tag::Button => true,
            strata_dom::Tag::Custom(name) => {
                name.eq_ignore_ascii_case("img") || name.eq_ignore_ascii_case("button")
            }
            _ => false,
        };
        for attr in attributes {
            if let strata_dom::Attribute::Property { key, value } = attr {
                if key.eq_ignore_ascii_case("title")
                    || (supports_alt && key.eq_ignore_ascii_case("alt"))
                {
                    found.push(value.as_str());
                }
            }
        }
    }
    found
}

impl AppRuntime {
    fn query_matches_node(query: &NodeQuery, bp: &NodeBlueprint) -> bool {
        if !matches!(bp, NodeBlueprint::Element { .. }) {
            return false;
        }
        if query.text.is_none() && query.title.is_none() {
            return false;
        }
        if let Some(ref needle) = query.text {
            let mut content = String::new();
            collect_text(bp, &mut content);
            if !query.matches_str(needle, &content) {
                return false;
            }
        }
        if let Some(ref needle) = query.title {
            if !own_tooltip_attrs(bp)
                .into_iter()
                .any(|v| query.matches_str(needle, v))
            {
                return false;
            }
        }
        true
    }

    /// Walks the paired trees; returns whether `bp` or any descendant matched. A node is reported
    /// only if none of its descendants matched, so results are the deepest matches in
    /// document order.
    fn collect_query_matches(
        query: &NodeQuery,
        bp: &NodeBlueprint,
        layout: &LayoutNode,
        path: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) -> bool {
        if path.len() >= 256 {
            return false;
        }
        let mut any_child = false;
        for (idx, child_bp, child_layout) in Self::paired_blueprint_children(bp, layout) {
            if matches!(
                child_layout.style.display,
                strata_style::types::Display::None
            ) {
                continue;
            }
            path.push(idx);
            if Self::collect_query_matches(query, child_bp, child_layout, path, out) {
                any_child = true;
            }
            path.pop();
        }
        if !any_child && Self::query_matches_node(query, bp) {
            out.push(path.clone());
            return true;
        }
        any_child
    }

    /// Returns the tree paths of every element matching `query`, in document order.
    pub fn find_all(&self, query: &NodeQuery) -> Vec<Vec<usize>> {
        let mut out = Vec::new();
        let mut path = Vec::new();
        Self::collect_query_matches(
            query,
            &self.root_blueprint,
            &self.layout_root,
            &mut path,
            &mut out,
        );
        out
    }

    /// First (bottom-most in paint order) element matching `query`.
    pub fn find(&self, query: &NodeQuery) -> Option<Vec<usize>> {
        self.find_all(query).into_iter().next()
    }

    /// Last (top-most in paint order) element matching `query`, e.g. a modal's button when the
    /// same label also exists on the page behind it.
    pub fn find_last(&self, query: &NodeQuery) -> Option<Vec<usize>> {
        self.find_all(query).into_iter().next_back()
    }

    /// Shorthand for `find(&NodeQuery::text(text))`.
    pub fn find_by_text(&self, text: &str) -> Option<Vec<usize>> {
        self.find(&NodeQuery::text(text))
    }

    /// Shorthand for `find_last(&NodeQuery::text(text))`.
    pub fn find_last_by_text(&self, text: &str) -> Option<Vec<usize>> {
        self.find_last(&NodeQuery::text(text))
    }

    /// Shorthand for `find(&NodeQuery::title(title))`.
    pub fn find_by_title(&self, title: &str) -> Option<Vec<usize>> {
        self.find(&NodeQuery::title(title))
    }

    /// Shorthand for `find_last(&NodeQuery::title(title))`.
    pub fn find_last_by_title(&self, title: &str) -> Option<Vec<usize>> {
        self.find_last(&NodeQuery::title(title))
    }

    /// Rendered text content of the element at `path`, whitespace-normalized.
    pub fn node_text(&self, path: &[usize]) -> Option<String> {
        let (bp, _) = Self::find_node_by_path(&self.root_blueprint, &self.layout_root, path)?;
        let mut content = String::new();
        collect_text(bp, &mut content);
        Some(normalize_ws(&content))
    }

    /// Border-box center of the element at `path` in window coordinates.
    pub fn node_center(&self, path: &[usize]) -> Option<(f32, f32)> {
        let (_, layout) = Self::find_node_by_path(&self.root_blueprint, &self.layout_root, path)?;
        let b = layout.geometry.border_box;
        Some((b.x + b.width * 0.5, b.y + b.height * 0.5))
    }

    /// Clicks the center of the element at `path` through the normal hit-test path (so an
    /// occluding element receives the click, as for a real pointer). Returns `true` if a
    /// click listener ran.
    pub fn click_node(&self, path: &[usize]) -> bool {
        match self.node_center(path) {
            Some((x, y)) => self.dispatch_click(x, y),
            None => false,
        }
    }

    /// Clicks the first element matching `query`. Returns `false` if none matched or no listener ran.
    pub fn click(&self, query: &NodeQuery) -> bool {
        self.find(query).is_some_and(|p| self.click_node(&p))
    }

    /// Clicks the last (top-most) element matching `query`.
    pub fn click_last(&self, query: &NodeQuery) -> bool {
        self.find_last(query).is_some_and(|p| self.click_node(&p))
    }

    /// Clicks the first element whose text equals `text`.
    pub fn click_text(&self, text: &str) -> bool {
        self.click(&NodeQuery::text(text))
    }

    /// Clicks the last (top-most) element whose text equals `text`.
    pub fn click_last_text(&self, text: &str) -> bool {
        self.click_last(&NodeQuery::text(text))
    }

    /// Clicks the first element whose `title` (or button/img `alt`) equals `title`.
    pub fn click_title(&self, title: &str) -> bool {
        self.click(&NodeQuery::title(title))
    }

    /// Clicks the last (top-most) element whose `title` (or button/img `alt`) equals `title`.
    pub fn click_last_title(&self, title: &str) -> bool {
        self.click_last(&NodeQuery::title(title))
    }
}
