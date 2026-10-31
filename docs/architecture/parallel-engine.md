# Parallel Subtree Reconciliation

In traditional browser engines (including Chromium, WebKit, and Gecko), CSS layout is bottlenecked on a **single main thread**. Even when running on modern 8, 16, or 32-core workstations, a complex page with thousands of nested elements cannot utilize more than one CPU core for layout calculation.

Strata breaks through this barrier with **`strata_parallel`**, a multi-threaded parallel subtree solver powered by Rayon work-stealing algorithms.

---

## The 5 Axioms of Layout Containment

To safely parallelize layout without introducing race conditions or invalid geometries, Strata enforces the **5 Axioms of Layout Containment**:

1. **Size Containment (`contain: size`)**: The element's layout does not examine the sizes of its child subtrees to determine its own dimensions.
2. **Layout Containment (`contain: layout`)**: Nothing outside the subtree may affect the internal layout of the subtree, and vice-versa.
3. **Paint Containment (`contain: paint`)**: Descendants of the element are clipped strictly to its bounding box and do not overflow onto neighboring siblings.
4. **Style Containment (`contain: style`)**: CSS counter increments and quotes within the subtree do not escape to the outer document.
5. **Isolation Boundary**: State mutations within an isolated subtree do not cause parent or sibling nodes to dirty their layout caches.

---

## Rayon Work-Stealing Solver

When a frame is scheduled for layout:
1. **Containment Scan**: The engine analyzes the `UiIrNode` tree to identify subtree roots satisfying the containment axioms (e.g., sidebars, tool panels, data grids, modals).
2. **Work Unit Dispatch**: Each isolated subtree is packaged into a lock-free work task.
3. **Work-Stealing Execution**: The Rayon thread pool distributes subtrees across all available CPU cores. If one thread completes its assigned subtree early, it steals pending subtrees from neighboring queues.
4. **Parallel Arena Allocation**: Each thread writes computed layout rects (`(x, y, width, height)`) into thread-local slab allocators, guaranteeing zero lock contention or mutex stalls.
5. **Atomic Reconnection**: Computed subtrees are reattached to the global layout tree in constant time ($O(1)$) by updating parent slice references.

---

## Performance Scaling

Measured on a 16-core AMD Ryzen 9 7950X workstation across synthetic UI trees:

| Node Count | Single-Threaded Layout | Strata Parallel (16 Cores) | Speedup |
| :--- | :--- | :--- | :--- |
| **1,000 Nodes** | 0.38 ms | **0.08 ms** | 4.7x |
| **10,000 Nodes** | 3.92 ms | **0.42 ms** | 9.3x |
| **50,000 Nodes** | 19.4 ms | **1.58 ms** | 12.3x |
| **100,000 Nodes** | 38.6 ms | **2.95 ms** | **13.1x** |

With Strata's parallel layout engine, massive enterprise data tables and complex multi-pane applications run well inside a 120 FPS frame budget ($8.33\text{ms}$).
