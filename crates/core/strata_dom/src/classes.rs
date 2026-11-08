//! Zero-allocation compile-time bitmasked and static class lists.

use std::fmt;

/// 256-bit bitmask representation of standard Tailwind utility classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ClassBitmask(pub [u64; 4]);

impl ClassBitmask {
    /// An empty 256-bit class mask.
    pub const fn empty() -> Self {
        Self([0, 0, 0, 0])
    }

    /// Construct mask from 4 64-bit integer words.
    pub const fn from_raw(raw: [u64; 4]) -> Self {
        Self(raw)
    }

    /// Returns the raw 4 64-bit words.
    pub const fn raw(&self) -> [u64; 4] {
        self.0
    }

    /// Returns true if no utility bits are set.
    pub const fn is_empty(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    /// Checks if the utility at the given standard index is active.
    pub const fn contains_index(&self, index: usize) -> bool {
        if index >= 256 {
            return false;
        }
        let word = index / 64;
        let bit = index % 64;
        (self.0[word] & (1u64 << bit)) != 0
    }

    /// Sets the bit at the given index and returns the new mask.
    pub const fn with_bit(mut self, index: usize) -> Self {
        if index < 256 {
            let word = index / 64;
            let bit = index % 64;
            self.0[word] |= 1u64 << bit;
        }
        self
    }

    /// Sets the bit at the given index in place.
    pub fn set_bit(&mut self, index: usize) {
        if index < 256 {
            let word = index / 64;
            let bit = index % 64;
            self.0[word] |= 1u64 << bit;
        }
    }

    /// Returns the total number of utility classes encoded in this bitmask.
    pub fn count_ones(&self) -> u32 {
        self.0[0].count_ones()
            + self.0[1].count_ones()
            + self.0[2].count_ones()
            + self.0[3].count_ones()
    }
}

/// Exactly 256 well-known standard Tailwind utility classes mapped to bit indices 0..255.
pub const STANDARD_CLASSES: [&str; 256] = [
    // 0..11: Display
    "flex", "grid", "inline", "inline-flex", "inline-grid", "block", "inline-block", "hidden",
    "contents", "table", "table-row", "table-cell",
    // 12..26: Flexbox Direction, Wrap, Sizing
    "flex-row", "flex-row-reverse", "flex-col", "flex-col-reverse", "flex-wrap", "flex-wrap-reverse",
    "flex-nowrap", "flex-1", "flex-auto", "flex-initial", "flex-none", "grow", "grow-0", "shrink",
    "shrink-0",
    // 27..31: Align Items
    "items-start", "items-end", "items-center", "items-baseline", "items-stretch",
    // 32..37: Justify Content
    "justify-start", "justify-end", "justify-center", "justify-between", "justify-around", "justify-evenly",
    // 38..41: Justify Items
    "justify-items-start", "justify-items-end", "justify-items-center", "justify-items-stretch",
    // 42..46: Justify Self
    "justify-self-auto", "justify-self-start", "justify-self-end", "justify-self-center", "justify-self-stretch",
    // 47..54: Align Content
    "content-center", "content-start", "content-end", "content-between", "content-around", "content-evenly",
    "content-baseline", "content-stretch",
    // 55..60: Align Self
    "self-auto", "self-start", "self-end", "self-center", "self-stretch", "self-baseline",
    // 61..65: Positioning
    "relative", "absolute", "fixed", "sticky", "static",
    // 66..70: Inset
    "inset-0", "top-0", "bottom-0", "left-0", "right-0",
    // 71..97: Width
    "w-full", "w-auto", "w-screen", "w-min", "w-max", "w-fit", "w-0", "w-1", "w-2", "w-3", "w-4",
    "w-5", "w-6", "w-8", "w-10", "w-12", "w-16", "w-20", "w-24", "w-32", "w-40", "w-48", "w-56",
    "w-64", "w-72", "w-80", "w-96",
    // 98..124: Height
    "h-full", "h-auto", "h-screen", "h-min", "h-max", "h-fit", "h-0", "h-1", "h-2", "h-3", "h-4",
    "h-5", "h-6", "h-8", "h-10", "h-12", "h-16", "h-20", "h-24", "h-32", "h-40", "h-48", "h-56",
    "h-64", "h-72", "h-80", "h-96",
    // 125..133: Min/Max Dimensions
    "min-w-0", "min-w-full", "max-w-none", "max-w-full", "min-h-0", "min-h-full", "min-h-screen",
    "max-h-full", "max-h-screen",
    // 134..144: Overflow
    "overflow-auto", "overflow-hidden", "overflow-clip", "overflow-visible", "overflow-scroll",
    "overflow-x-auto", "overflow-x-hidden", "overflow-x-scroll", "overflow-y-auto", "overflow-y-hidden",
    "overflow-y-scroll",
    // 145..168: Padding
    "p-0", "p-1", "p-2", "p-3", "p-4", "p-5", "p-6", "p-8", "p-10", "p-12", "px-0", "px-1", "px-2",
    "px-3", "px-4", "px-6", "px-8", "py-0", "py-1", "py-2", "py-3", "py-4", "py-6", "py-8",
    // 169..187: Margin
    "m-0", "m-1", "m-2", "m-3", "m-4", "m-5", "m-6", "m-8", "m-auto", "mx-auto", "my-auto",
    "mx-0", "mx-1", "mx-2", "mx-4", "my-0", "my-1", "my-2", "my-4",
    // 188..194: Gap
    "gap-0", "gap-1", "gap-2", "gap-3", "gap-4", "gap-6", "gap-8",
    // 195..202: Text Size
    "text-xs", "text-sm", "text-base", "text-lg", "text-xl", "text-2xl", "text-3xl", "text-4xl",
    // 203..208: Font Weight
    "font-thin", "font-normal", "font-medium", "font-semibold", "font-bold", "font-extrabold",
    // 209..212: Text Align
    "text-left", "text-center", "text-right", "text-justify",
    // 213..216: Text Color & Truncate
    "text-white", "text-black", "text-transparent", "truncate",
    // 217..224: Background Color
    "bg-white", "bg-black", "bg-transparent", "bg-neutral-900", "bg-neutral-800", "bg-neutral-700",
    "bg-neutral-100", "bg-neutral-50",
    // 225..231: Border
    "border", "border-0", "border-2", "border-4", "border-neutral-800", "border-neutral-700",
    "border-transparent",
    // 232..239: Rounded (Border Radius)
    "rounded-none", "rounded-sm", "rounded", "rounded-md", "rounded-lg", "rounded-xl", "rounded-2xl",
    "rounded-full",
    // 240..246: Box Shadow
    "shadow-none", "shadow-sm", "shadow", "shadow-md", "shadow-lg", "shadow-xl", "shadow-2xl",
    // 247..253: Cursor
    "cursor-auto", "cursor-default", "cursor-pointer", "cursor-wait", "cursor-text", "cursor-move",
    "cursor-not-allowed",
    // 254..255: CSS Containment
    "contain-strict", "contain-paint",
];

/// Returns the index of a well-known standard Tailwind class, if recognized.
pub fn standard_class_index(name: &str) -> Option<usize> {
    // Fast lookup using linear scan over static slice (or binary if sorted, but 256 strings is trivial)
    STANDARD_CLASSES.iter().position(|&s| s == name)
}

/// Zero-allocation class list representing compile-time bitmasks, static slices, or dynamic vectors.
#[derive(Clone, PartialEq, Eq, Default)]
pub enum ClassList {
    /// Completely empty class list.
    #[default]
    Empty,
    /// Single static class token. Zero allocations.
    Single(&'static str),
    /// Static slice of string literals in read-only program memory. Zero allocations.
    Static(&'static [&'static str]),
    /// Compile-time 256-bit bitmask of standard Tailwind utilities plus optional static tail. Zero allocations.
    Masked {
        mask: ClassBitmask,
        tail: &'static [&'static str],
    },
    /// Dynamic owned list for runtime-concatenated strings.
    Dynamic(Vec<String>),
}

impl ClassList {
    /// Returns an empty class list.
    pub const fn empty() -> Self {
        Self::Empty
    }

    /// Construct a zero-allocation `ClassList` from a 256-bit bitmask of standard utilities.
    pub const fn from_mask(raw: [u64; 4]) -> Self {
        Self::Masked {
            mask: ClassBitmask::from_raw(raw),
            tail: &[],
        }
    }

    /// Construct a zero-allocation `ClassList` from a single static string.
    pub const fn from_single(s: &'static str) -> Self {
        Self::Single(s)
    }

    /// Construct a zero-allocation `ClassList` from a static slice of string literals.
    pub const fn from_static(slice: &'static [&'static str]) -> Self {
        Self::Static(slice)
    }

    /// Construct a zero-allocation `ClassList` with both a bitmask and custom static tail.
    pub const fn from_masked(mask: ClassBitmask, tail: &'static [&'static str]) -> Self {
        Self::Masked { mask, tail }
    }

    /// Returns true if the class list has zero classes.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Single(s) => s.is_empty(),
            Self::Static(s) => s.is_empty(),
            Self::Masked { mask, tail } => mask.is_empty() && tail.is_empty(),
            Self::Dynamic(v) => v.is_empty(),
        }
    }

    /// Returns the number of distinct classes in the list.
    pub fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Single(_) => 1,
            Self::Static(s) => s.len(),
            Self::Masked { mask, tail } => mask.count_ones() as usize + tail.len(),
            Self::Dynamic(v) => v.len(),
        }
    }

    /// Checks if the specified class name exists in the list.
    /// Standard utility classes in masked lists are checked in O(1) bitwise operations.
    pub fn contains(&self, class: &str) -> bool {
        match self {
            Self::Empty => false,
            Self::Single(s) => *s == class,
            Self::Static(s) => s.contains(&class),
            Self::Masked { mask, tail } => {
                if let Some(idx) = standard_class_index(class) {
                    if mask.contains_index(idx) {
                        return true;
                    }
                }
                tail.contains(&class)
            }
            Self::Dynamic(v) => v.iter().any(|c| c == class),
        }
    }

    /// Appends a class to the list, promoting it to dynamic allocation if needed.
    pub fn push(&mut self, class: impl Into<String>) {
        self.as_mut_vec().push(class.into());
    }

    /// Extends the class list, promoting it to dynamic allocation if needed.
    pub fn extend(&mut self, iter: impl IntoIterator<Item = impl Into<String>>) {
        let vec = self.as_mut_vec();
        for item in iter {
            vec.push(item.into());
        }
    }

    fn as_mut_vec(&mut self) -> &mut Vec<String> {
        if let Self::Dynamic(ref mut v) = self {
            return v;
        }
        let owned: Vec<String> = self.iter().map(|s| s.to_string()).collect();
        *self = Self::Dynamic(owned);
        if let Self::Dynamic(ref mut v) = self {
            v
        } else {
            unreachable!()
        }
    }

    /// Converts this class list to an owned `Vec<String>`.
    pub fn to_vec(&self) -> Vec<String> {
        self.iter().map(|s| s.to_string()).collect()
    }

    /// Concatenates all classes in this list with the given separator string.
    pub fn join(&self, sep: &str) -> String {
        let mut out = String::new();
        for (i, c) in self.iter().enumerate() {
            if i > 0 {
                out.push_str(sep);
            }
            out.push_str(c);
        }
        out
    }

    /// Returns an iterator yielding `&str` references over all classes in the list.
    pub fn iter(&self) -> ClassIter<'_> {
        match self {
            Self::Empty => ClassIter::Empty,
            Self::Single(item) => ClassIter::Single { item: Some(item) },
            Self::Static(slice) => ClassIter::Static { slice, index: 0 },
            Self::Masked { mask, tail } => ClassIter::Masked {
                mask: *mask,
                current_bit: 0,
                tail,
                tail_index: 0,
            },
            Self::Dynamic(slice) => ClassIter::Dynamic {
                slice: slice.as_slice(),
                index: 0,
            },
        }
    }
}

impl<'a> IntoIterator for &'a ClassList {
    type Item = &'a str;
    type IntoIter = ClassIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl From<Vec<String>> for ClassList {
    fn from(v: Vec<String>) -> Self {
        Self::Dynamic(v)
    }
}

impl From<&'static [&'static str]> for ClassList {
    fn from(slice: &'static [&'static str]) -> Self {
        Self::Static(slice)
    }
}

impl From<&'static str> for ClassList {
    fn from(s: &'static str) -> Self {
        Self::Single(s)
    }
}

impl From<[u64; 4]> for ClassList {
    fn from(raw: [u64; 4]) -> Self {
        Self::from_mask(raw)
    }
}

impl PartialEq<Vec<String>> for ClassList {
    fn eq(&self, other: &Vec<String>) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a == b.as_str())
    }
}

impl PartialEq<ClassList> for Vec<String> {
    fn eq(&self, other: &ClassList) -> bool {
        other == self
    }
}

impl PartialEq<[&str]> for ClassList {
    fn eq(&self, other: &[&str]) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a == *b)
    }
}

impl fmt::Debug for ClassList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// Iterator over classes in a `ClassList`.
#[derive(Clone)]
pub enum ClassIter<'a> {
    Empty,
    Single {
        item: Option<&'a str>,
    },
    Static {
        slice: &'a [&'static str],
        index: usize,
    },
    Masked {
        mask: ClassBitmask,
        current_bit: usize,
        tail: &'a [&'static str],
        tail_index: usize,
    },
    Dynamic {
        slice: &'a [String],
        index: usize,
    },
}

impl<'a> Iterator for ClassIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::Single { item } => item.take(),
            Self::Static { slice, index } => {
                if *index < slice.len() {
                    let item = slice[*index];
                    *index += 1;
                    Some(item)
                } else {
                    None
                }
            }
            Self::Dynamic { slice, index } => {
                if *index < slice.len() {
                    let item = slice[*index].as_str();
                    *index += 1;
                    Some(item)
                } else {
                    None
                }
            }
            Self::Masked {
                mask,
                current_bit,
                tail,
                tail_index,
            } => {
                while *current_bit < 256 {
                    let bit = *current_bit;
                    *current_bit += 1;
                    if mask.contains_index(bit) {
                        return Some(STANDARD_CLASSES[bit]);
                    }
                }
                if *tail_index < tail.len() {
                    let item = tail[*tail_index];
                    *tail_index += 1;
                    Some(item)
                } else {
                    None
                }
            }
        }
    }
}
