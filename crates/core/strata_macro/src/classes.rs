//! Compile-time Tailwind class deduplication and 256-bit SIMD bitmask resolution for proc-macro expansion.

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
    STANDARD_CLASSES.iter().position(|&s| s == name)
}

/// Evaluates class string at compile-time: deduplicates tokens and maps to 256-bit bitmask + static tail.
pub fn emit_static_class_list(raw_classes: &str) -> String {
    let mut tokens: Vec<&str> = Vec::new();
    for token in raw_classes.split_whitespace() {
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    }

    if tokens.is_empty() {
        return "strata_dom::ClassList::Empty".to_string();
    }

    if tokens.len() == 1 {
        let single = tokens[0];
        if let Some(idx) = standard_class_index(single) {
            let mut mask = [0u64; 4];
            mask[idx / 64] |= 1u64 << (idx % 64);
            return format!(
                "strata_dom::ClassList::from_mask([{}u64, {}u64, {}u64, {}u64])",
                mask[0], mask[1], mask[2], mask[3]
            );
        } else {
            return format!("strata_dom::ClassList::from_single({:?})", single);
        }
    }

    let mut mask = [0u64; 4];
    let mut tail: Vec<&str> = Vec::new();

    for token in tokens {
        if let Some(idx) = standard_class_index(token) {
            mask[idx / 64] |= 1u64 << (idx % 64);
        } else {
            tail.push(token);
        }
    }

    let has_mask = mask != [0, 0, 0, 0];

    if has_mask && tail.is_empty() {
        format!(
            "strata_dom::ClassList::from_mask([{}u64, {}u64, {}u64, {}u64])",
            mask[0], mask[1], mask[2], mask[3]
        )
    } else if !has_mask && !tail.is_empty() {
        let tail_entries: Vec<String> = tail.iter().map(|s| format!("{:?}", s)).collect();
        format!(
            "strata_dom::ClassList::Static(&[{}])",
            tail_entries.join(", ")
        )
    } else {
        let tail_entries: Vec<String> = tail.iter().map(|s| format!("{:?}", s)).collect();
        format!(
            "strata_dom::ClassList::Masked {{ mask: strata_dom::ClassBitmask([{}u64, {}u64, {}u64, {}u64]), tail: &[{}] }}",
            mask[0], mask[1], mask[2], mask[3],
            tail_entries.join(", ")
        )
    }
}
