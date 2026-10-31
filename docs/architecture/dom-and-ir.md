# DOM & UiIrNode Intermediate Representation

Traditional web DOM implementations like Chromium Blink represent HTML elements as complex, multi-kilobyte heap objects riddled with pointer indirections, virtual method tables, and C++ inheritance hierarchies.

Strata discards the classic object-oriented DOM model in favor of an **Intermediate Representation (IR)** designed around modern hardware cache lines: the **`UiIrNode`**.

---

## The 64-Byte Cache-Aligned Node

Modern x86-64 and ARM64 CPUs fetch memory from RAM into L1 caches in **64-byte cache lines**. If a data structure spans across multiple cache lines, accessing it incurs cache misses and stalls CPU execution pipelines.

`UiIrNode` in `strata_dom` is strictly padded and aligned to **exactly 64 bytes**:

```rust
#[repr(C, align(64))]
pub struct UiIrNode {
    pub node_id: NodeId,              // 4 bytes: Arena index
    pub parent_id: Option<NodeId>,     // 4 bytes: Parent index
    pub first_child: Option<NodeId>,   // 4 bytes: First child index
    pub next_sibling: Option<NodeId>,  // 4 bytes: Next sibling index
    pub style_mask: StyleMask256,      // 32 bytes: 256-bit SIMD style mask
    pub flags: NodeFlags,              // 4 bytes: Bitflags (visible, layout dirty, focused)
    pub text_or_layout_handle: u32,    // 4 bytes: Index into text arena or layout cache
    pub signal_subscriber_id: u32,     // 4 bytes: Handle into reactive signal subscriber table
    pub _reserved: [u8; 4],            // 4 bytes: Padding to 64 bytes
}
```

### Memory Efficiency Implications
- **1,000 Nodes**: Occupies only **64 KB** of memory, fitting entirely inside the L2 cache of modern processors.
- **100,000 Nodes**: Occupies **6.4 MB**, allowing entire application trees to be iterated and reconciled within milliseconds.

---

## Index Arenas vs. Raw Pointers

In traditional C++ or Rust pointer-based trees (`Rc<RefCell<Node>>` or `Box<Node>`):
1. Every child insertion or removal triggers heap allocator synchronization.
2. Nodes are scattered arbitrarily across virtual memory, causing memory fragmentation.
3. Garbage collectors must traverse heap references, inducing latency spikes.

Strata allocates `UiIrNode` instances inside **generational index arenas**:
- Adding a child is a contiguous vector push ($O(1)$).
- Node references are 32-bit integer indexes (`NodeId(u32)`), cutting pointer size in half compared to 64-bit machine pointers.
- Deleting nodes uses generational tags, preventing use-after-free and dangling handle bugs with zero runtime safety checks.

---

## The Reactive Binding Layer

Each `UiIrNode` contains a `signal_subscriber_id`. When a component executes:
1. Signal reads register the node's `NodeId` into a thread-local subscriber list.
2. When the signal's value changes, the signal directly marks the specific node's `flags` bitfield (`NodeFlags::DIRTY_LAYOUT` or `NodeFlags::DIRTY_PAINT`).
3. No tree traversal or virtual diffing is required; the scheduler immediately enqueues only the invalidated node for the next frame pass.
