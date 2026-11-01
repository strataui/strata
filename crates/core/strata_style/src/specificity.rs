//! Specificity tuple and ordering per W3C Selectors Level 4.

use std::ops::{Add, AddAssign};

/// Specificity vector $\mathbf{S} = (I, A, B, C)$:
/// - $I$: Inline style (1 for style attribute, 0 otherwise)
/// - $A$: Number of ID selectors
/// - $B$: Number of class selectors, attribute selectors, and pseudo-classes
/// - $C$: Number of type (tag) selectors and pseudo-elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Specificity {
    pub inline: u8,
    pub id_count: u16,
    pub class_count: u16,
    pub type_count: u16,
}

impl Specificity {
    pub const ZERO: Self = Self { inline: 0, id_count: 0, class_count: 0, type_count: 0 };
    pub const INLINE: Self = Self { inline: 1, id_count: 0, class_count: 0, type_count: 0 };
    pub const ID: Self = Self { inline: 0, id_count: 1, class_count: 0, type_count: 0 };
    pub const CLASS: Self = Self { inline: 0, id_count: 0, class_count: 1, type_count: 0 };
    pub const TYPE: Self = Self { inline: 0, id_count: 0, class_count: 0, type_count: 1 };

    pub const fn new(inline: u8, id_count: u16, class_count: u16, type_count: u16) -> Self {
        Self { inline, id_count, class_count, type_count }
    }
}

impl Add for Specificity {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            inline: self.inline.saturating_add(rhs.inline),
            id_count: self.id_count.saturating_add(rhs.id_count),
            class_count: self.class_count.saturating_add(rhs.class_count),
            type_count: self.type_count.saturating_add(rhs.type_count),
        }
    }
}

impl AddAssign for Specificity {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}
