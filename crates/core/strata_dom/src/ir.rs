//! Strata UI IR: Pure Component-Centric Intermediate Representation, 64-byte Cache-Aligned Nodes, and Generational Memoization.

use std::collections::HashMap;

/// Opaque node handle referencing an entry in a flat `UiIrGraph` array.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const NULL: Self = Self(u32::MAX);

    #[inline(always)]
    pub fn is_null(self) -> bool {
        self.0 == u32::MAX
    }
}

/// Abstract component/element tag in the Strata UI IR.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Default)]
pub enum IrTag {
    #[default]
    Div = 0,
    Span = 1,
    Text = 2,
    Button = 3,
    Input = 4,
    Image = 5,
    Svg = 6,
    Canvas3D = 7,
    ScrollView = 8,
    FlexContainer = 9,
    GridContainer = 10,
    ComponentRoot = 11,
}


/// Bit flags representing state, containment, and dirty status.
pub struct IrNodeFlags;

impl IrNodeFlags {
    pub const NONE: u16 = 0;
    pub const HAS_DIRTY_LAYOUT: u16 = 1 << 0;
    pub const HAS_DIRTY_STYLE: u16 = 1 << 1;
    pub const IS_CONTAINMENT_BOUNDARY: u16 = 1 << 2;
    pub const IS_TEXT_LEAF: u16 = 1 << 3;
    pub const IS_VISIBLE: u16 = 1 << 4;
    pub const HAS_EVENT_LISTENERS: u16 = 1 << 5;
    pub const HAS_MEMO_CACHE: u16 = 1 << 6;
}

/// Fixed-point 1/64th px subpixel coordinates (Chromium Blink parity).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FixedRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl FixedRect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    /// Converts floating point pixels to 1/64th fixed-point subpixels.
    pub fn from_f32(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x: (x * 64.0).round() as i32,
            y: (y * 64.0).round() as i32,
            width: (width * 64.0).round() as i32,
            height: (height * 64.0).round() as i32,
        }
    }

    /// Converts back to floating point pixels.
    pub fn to_f32(self) -> (f32, f32, f32, f32) {
        (
            self.x as f32 / 64.0,
            self.y as f32 / 64.0,
            self.width as f32 / 64.0,
            self.height as f32 / 64.0,
        )
    }
}

/// Range of draw commands in the global or partition display list.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DisplayChunkRange {
    pub start: u32,
    pub count: u32,
}

/// The atomic Strata UI IR Node: Exactly 64 bytes (1 CPU cache line).
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct UiIrNode {
    // 8-byte aligned fields (16 bytes)
    pub memo_hash: u64,                   // 8 bytes (offset 0..8)
    pub display_chunk: DisplayChunkRange, // 8 bytes (offset 8..16)

    // 4-byte aligned fields (44 bytes)
    pub layout_rect: FixedRect,           // 16 bytes (offset 16..32)
    pub id: NodeId,                       // 4 bytes (offset 32..36)
    pub parent: NodeId,                   // 4 bytes (offset 36..40)
    pub first_child: NodeId,              // 4 bytes (offset 40..44)
    pub next_sibling: NodeId,             // 4 bytes (offset 44..48)
    pub child_count: u32,                 // 4 bytes (offset 48..52)
    pub style_id: u32,                    // 4 bytes (offset 52..56)
    pub subtree_span: u32,                // 4 bytes (offset 56..60)

    // 2-byte aligned fields (2 bytes)
    pub flags: u16,                       // 2 bytes (offset 60..62)

    // 1-byte aligned fields (2 bytes)
    pub tag: IrTag,                       // 1 byte (offset 62..63)
    pub _reserved0: u8,                   // 1 byte (offset 63..64)
}

impl Default for UiIrNode {
    fn default() -> Self {
        Self {
            memo_hash: 0,
            display_chunk: DisplayChunkRange::default(),
            layout_rect: FixedRect::default(),
            id: NodeId::NULL,
            parent: NodeId::NULL,
            first_child: NodeId::NULL,
            next_sibling: NodeId::NULL,
            child_count: 0,
            style_id: 0,
            subtree_span: 1,
            flags: IrNodeFlags::IS_VISIBLE,
            tag: IrTag::Div,
            _reserved0: 0,
        }
    }
}

/// Contiguous, cache-friendly array of `UiIrNode` items forming a UI tree.
#[derive(Debug, Clone, Default)]
pub struct UiIrGraph {
    pub nodes: Vec<UiIrNode>,
    pub last_children: Vec<NodeId>,
    pub root_id: Option<NodeId>,
}

impl UiIrGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::with_capacity(64),
            last_children: Vec::with_capacity(64),
            root_id: None,
        }
    }

    /// Allocates a new node in the IR graph, returning its `NodeId`.
    pub fn alloc_node(&mut self, tag: IrTag) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        let node = UiIrNode {
            id,
            tag,
            ..Default::default()
        };
        self.nodes.push(node);
        self.last_children.push(NodeId::NULL);
        if self.root_id.is_none() {
            self.root_id = Some(id);
        }
        id
    }

    /// Appends a child to a parent node in O(1) time, maintaining sibling pointers and child counts.
    pub fn append_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        if parent_id.is_null() || child_id.is_null() {
            return;
        }

        self.nodes[child_id.0 as usize].parent = parent_id;

        let last_child = self.last_children[parent_id.0 as usize];
        if last_child.is_null() {
            self.nodes[parent_id.0 as usize].first_child = child_id;
        } else {
            self.nodes[last_child.0 as usize].next_sibling = child_id;
        }
        self.last_children[parent_id.0 as usize] = child_id;

        self.nodes[parent_id.0 as usize].child_count += 1;
    }

    /// Retrieves an immutable reference to a node.
    #[inline(always)]
    pub fn get(&self, id: NodeId) -> Option<&UiIrNode> {
        self.nodes.get(id.0 as usize)
    }

    /// Retrieves a mutable reference to a node.
    #[inline(always)]
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut UiIrNode> {
        self.nodes.get_mut(id.0 as usize)
    }

    /// Total number of nodes in the graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Recomputes `subtree_span` for all nodes in post-order.
    /// Enables O(1) subtree skipping during generational memoization.
    pub fn recompute_subtree_spans(&mut self) {
        if let Some(root) = self.root_id {
            self.compute_span_recursive(root);
        }
    }

    fn compute_span_recursive(&mut self, node_id: NodeId) -> u32 {
        if node_id.is_null() {
            return 0;
        }

        let mut span = 1u32;
        let mut child = self.nodes[node_id.0 as usize].first_child;
        while !child.is_null() {
            span += self.compute_span_recursive(child);
            child = self.nodes[child.0 as usize].next_sibling;
        }

        self.nodes[node_id.0 as usize].subtree_span = span;
        span
    }

    /// Iterates through the direct children of a given parent node.
    pub fn children(&self, parent: NodeId) -> ChildIter<'_> {
        let first = self.get(parent).map(|n| n.first_child).unwrap_or(NodeId::NULL);
        ChildIter {
            graph: self,
            current: first,
        }
    }
}

/// Iterator over direct children of a `UiIrNode`.
pub struct ChildIter<'a> {
    graph: &'a UiIrGraph,
    current: NodeId,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = &'a UiIrNode;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            None
        } else {
            let node = self.graph.get(self.current)?;
            self.current = node.next_sibling;
            Some(node)
        }
    }
}

/// Memoized display entry for an unchanged component subtree.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoEntry {
    pub memo_hash: u64,
    pub display_chunk: DisplayChunkRange,
    pub layout_rect: FixedRect,
}

/// Generational Subtree Memoization Cache.
#[derive(Debug, Clone, Default)]
pub struct MemoCache {
    entries: HashMap<u32, MemoEntry>,
}

impl MemoCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Computes the compound memo hash:
    /// PropHash XOR [RotateLeft(SignalId * 0x9E3779B9, version % 32)]
    pub fn compute_compound_hash(prop_hash: u64, signals: &[(u32, u64)]) -> u64 {
        let mut hash = prop_hash;
        for (sig_id, version) in signals {
            let mixed = (*sig_id as u64).wrapping_mul(0x9E3779B97F4A7C15);
            let shift = (*version & 0x1F) as u32;
            let rotated = mixed.rotate_left(shift);
            hash ^= rotated;
        }
        hash
    }

    /// Looks up a cached subtree entry if the memo hash matches.
    pub fn try_reuse(&self, component_id: u32, current_hash: u64) -> Option<&MemoEntry> {
        let entry = self.entries.get(&component_id)?;
        if entry.memo_hash == current_hash {
            Some(entry)
        } else {
            None
        }
    }

    /// Records or updates a cached subtree entry.
    pub fn insert(&mut self, component_id: u32, entry: MemoEntry) {
        self.entries.insert(component_id, entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Recursive Call Stack Guard preventing stack exhaustion attacks.
#[derive(Debug, Clone)]
pub struct StackGuard {
    current_depth: usize,
    trampoline_depth: usize,
    max_depth: usize,
}

impl Default for StackGuard {
    fn default() -> Self {
        Self::new(64, 512)
    }
}

impl StackGuard {
    pub fn new(trampoline_depth: usize, max_depth: usize) -> Self {
        Self {
            current_depth: 0,
            trampoline_depth,
            max_depth,
        }
    }

    /// Enters a recursive frame. Returns Err if max recursion depth is violated.
    pub fn enter(&mut self) -> Result<bool, String> {
        self.current_depth += 1;
        if self.current_depth >= self.max_depth {
            return Err(format!(
                "StackGuard: recursion depth exceeded safety threshold ({})",
                self.max_depth
            ));
        }
        // Returns true if caller should pivot to heap trampoline
        Ok(self.current_depth >= self.trampoline_depth)
    }

    /// Exits a recursive frame.
    pub fn leave(&mut self) {
        if self.current_depth > 0 {
            self.current_depth -= 1;
        }
    }

    pub fn depth(&self) -> usize {
        self.current_depth
    }
}
