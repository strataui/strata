//! Zero-dependency string interning atom table with pre-baked HTML/CSS static atoms.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::RwLock;
use std::collections::HashMap;

/// Compact 32-bit String Atom Identifier.
/// Static pre-baked atoms occupy indices 0 ..= 1023.
/// Dynamic interned atoms occupy indices 1024+.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AtomId(pub u32);

impl AtomId {
    pub const NULL: Self = Self(0);

    // Pre-baked HTML Tags (1 ..= 100)
    pub const DIV: Self = Self(1);
    pub const SPAN: Self = Self(2);
    pub const TEXT: Self = Self(3);
    pub const P: Self = Self(4);
    pub const A: Self = Self(5);
    pub const BUTTON: Self = Self(6);
    pub const INPUT: Self = Self(7);
    pub const IMG: Self = Self(8);
    pub const H1: Self = Self(9);
    pub const H2: Self = Self(10);
    pub const H3: Self = Self(11);
    pub const H4: Self = Self(12);
    pub const H5: Self = Self(13);
    pub const H6: Self = Self(14);
    pub const UL: Self = Self(15);
    pub const OL: Self = Self(16);
    pub const LI: Self = Self(17);
    pub const TABLE: Self = Self(18);
    pub const TR: Self = Self(19);
    pub const TD: Self = Self(20);
    pub const TH: Self = Self(21);
    pub const FORM: Self = Self(22);
    pub const HEADER: Self = Self(23);
    pub const FOOTER: Self = Self(24);
    pub const NAV: Self = Self(25);
    pub const SECTION: Self = Self(26);
    pub const ARTICLE: Self = Self(27);
    pub const ASIDE: Self = Self(28);
    pub const MAIN: Self = Self(29);
    pub const CANVAS: Self = Self(30);
    pub const CANVAS_3D: Self = Self(31);
    pub const SVG: Self = Self(32);
    pub const PATH: Self = Self(33);

    // Pre-baked CSS Properties (101 ..= 200)
    pub const DISPLAY: Self = Self(101);
    pub const POSITION: Self = Self(102);
    pub const WIDTH: Self = Self(103);
    pub const HEIGHT: Self = Self(104);
    pub const MIN_WIDTH: Self = Self(105);
    pub const MAX_WIDTH: Self = Self(106);
    pub const MIN_HEIGHT: Self = Self(107);
    pub const MAX_HEIGHT: Self = Self(108);
    pub const MARGIN: Self = Self(109);
    pub const PADDING: Self = Self(110);
    pub const BORDER: Self = Self(111);
    pub const BORDER_RADIUS: Self = Self(112);
    pub const COLOR: Self = Self(113);
    pub const BACKGROUND_COLOR: Self = Self(114);
    pub const OPACITY: Self = Self(115);
    pub const Z_INDEX: Self = Self(116);
    pub const TRANSFORM: Self = Self(117);
    pub const OVERFLOW: Self = Self(118);
    pub const FLEX_DIRECTION: Self = Self(119);
    pub const JUSTIFY_CONTENT: Self = Self(120);
    pub const ALIGN_ITEMS: Self = Self(121);
    pub const GAP: Self = Self(122);
    pub const GRID_TEMPLATE_COLUMNS: Self = Self(123);
    pub const GRID_TEMPLATE_ROWS: Self = Self(124);

    #[inline(always)]
    pub const fn is_static(self) -> bool {
        self.0 > 0 && self.0 <= 1023
    }
}

/// Static lookup table mapping pre-baked AtomId to static string slice.
const STATIC_ATOMS: &[(AtomId, &str)] = &[
    (AtomId::DIV, "div"),
    (AtomId::SPAN, "span"),
    (AtomId::TEXT, "text"),
    (AtomId::P, "p"),
    (AtomId::A, "a"),
    (AtomId::BUTTON, "button"),
    (AtomId::INPUT, "input"),
    (AtomId::IMG, "img"),
    (AtomId::H1, "h1"),
    (AtomId::H2, "h2"),
    (AtomId::H3, "h3"),
    (AtomId::H4, "h4"),
    (AtomId::H5, "h5"),
    (AtomId::H6, "h6"),
    (AtomId::UL, "ul"),
    (AtomId::OL, "ol"),
    (AtomId::LI, "li"),
    (AtomId::TABLE, "table"),
    (AtomId::TR, "tr"),
    (AtomId::TD, "td"),
    (AtomId::TH, "th"),
    (AtomId::FORM, "form"),
    (AtomId::HEADER, "header"),
    (AtomId::FOOTER, "footer"),
    (AtomId::NAV, "nav"),
    (AtomId::SECTION, "section"),
    (AtomId::ARTICLE, "article"),
    (AtomId::ASIDE, "aside"),
    (AtomId::MAIN, "main"),
    (AtomId::CANVAS, "canvas"),
    (AtomId::CANVAS_3D, "canvas3d"),
    (AtomId::SVG, "svg"),
    (AtomId::PATH, "path"),
    // CSS Properties
    (AtomId::DISPLAY, "display"),
    (AtomId::POSITION, "position"),
    (AtomId::WIDTH, "width"),
    (AtomId::HEIGHT, "height"),
    (AtomId::MIN_WIDTH, "min-width"),
    (AtomId::MAX_WIDTH, "max-width"),
    (AtomId::MIN_HEIGHT, "min-height"),
    (AtomId::MAX_HEIGHT, "max-height"),
    (AtomId::MARGIN, "margin"),
    (AtomId::PADDING, "padding"),
    (AtomId::BORDER, "border"),
    (AtomId::BORDER_RADIUS, "border-radius"),
    (AtomId::COLOR, "color"),
    (AtomId::BACKGROUND_COLOR, "background-color"),
    (AtomId::OPACITY, "opacity"),
    (AtomId::Z_INDEX, "z-index"),
    (AtomId::TRANSFORM, "transform"),
    (AtomId::OVERFLOW, "overflow"),
    (AtomId::FLEX_DIRECTION, "flex-direction"),
    (AtomId::JUSTIFY_CONTENT, "justify-content"),
    (AtomId::ALIGN_ITEMS, "align-items"),
    (AtomId::GAP, "gap"),
    (AtomId::GRID_TEMPLATE_COLUMNS, "grid-template-columns"),
    (AtomId::GRID_TEMPLATE_ROWS, "grid-template-rows"),
];

/// Thread-safe string interning table with pre-baked static atoms.
pub struct AtomTable {
    string_to_id: RwLock<HashMap<String, AtomId>>,
    id_to_string: RwLock<Vec<String>>,
    next_id: AtomicU32,
}

impl Default for AtomTable {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomTable {
    pub fn new() -> Self {
        let mut string_to_id = HashMap::with_capacity(STATIC_ATOMS.len() + 64);
        for &(id, name) in STATIC_ATOMS {
            string_to_id.insert(name.to_string(), id);
        }

        Self {
            string_to_id: RwLock::new(string_to_id),
            id_to_string: RwLock::new(Vec::new()),
            next_id: AtomicU32::new(1024), // Dynamic atoms start at 1024
        }
    }

    /// Interns a string slice, returning its unique copyable AtomId.
    pub fn intern(&self, text: &str) -> AtomId {
        // Fast read lock check
        {
            let map = self.string_to_id.read().unwrap();
            if let Some(&id) = map.get(text) {
                return id;
            }
        }

        // Upgrade to write lock
        let mut map = self.string_to_id.write().unwrap();
        if let Some(&id) = map.get(text) {
            return id;
        }

        let id = AtomId(self.next_id.fetch_add(1, Ordering::Relaxed));
        map.insert(text.to_string(), id);

        let mut list = self.id_to_string.write().unwrap();
        list.push(text.to_string());

        id
    }

    /// Resolves an AtomId back to its string representation.
    pub fn resolve(&self, id: AtomId) -> Option<String> {
        if id.is_static() {
            for &(static_id, name) in STATIC_ATOMS {
                if static_id == id {
                    return Some(name.to_string());
                }
            }
        }

        let idx = (id.0.checked_sub(1024)?) as usize;
        let list = self.id_to_string.read().unwrap();
        list.get(idx).cloned()
    }
}
