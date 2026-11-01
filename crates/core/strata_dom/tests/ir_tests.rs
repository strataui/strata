use strata_dom::ir::*;

#[test]
fn test_ui_ir_node_memory_layout_64_bytes_cache_aligned() {
    assert_eq!(
        std::mem::size_of::<UiIrNode>(),
        64,
        "UiIrNode MUST be exactly 64 bytes (1 CPU cache line)"
    );
    assert_eq!(
        std::mem::align_of::<UiIrNode>(),
        64,
        "UiIrNode MUST be 64-byte cache aligned"
    );
}

#[test]
fn test_fixed_rect_subpixel_conversions() {
    let rect = FixedRect::from_f32(10.25, 20.5, 100.125, 50.875);
    let (x, y, w, h) = rect.to_f32();

    assert!((x - 10.25).abs() < 1e-4);
    assert!((y - 20.5).abs() < 1e-4);
    assert!((w - 100.125).abs() < 1e-4);
    assert!((h - 50.875).abs() < 1e-4);
}

#[test]
fn test_ir_graph_hierarchy_and_subtree_spans() {
    let mut graph = UiIrGraph::new();

    // Tree structure:
    // Root (span = 6)
    // ├── Child1 (span = 1, leaf)
    // └── Child2 (span = 4)
    //     ├── Grandchild1 (span = 1)
    //     ├── Grandchild2 (span = 1)
    //     └── Grandchild3 (span = 1)

    let root = graph.alloc_node(IrTag::ComponentRoot);
    let child1 = graph.alloc_node(IrTag::Div);
    let child2 = graph.alloc_node(IrTag::Div);
    let gchild1 = graph.alloc_node(IrTag::Button);
    let gchild2 = graph.alloc_node(IrTag::Text);
    let gchild3 = graph.alloc_node(IrTag::Span);

    graph.append_child(root, child1);
    graph.append_child(root, child2);
    graph.append_child(child2, gchild1);
    graph.append_child(child2, gchild2);
    graph.append_child(child2, gchild3);

    graph.recompute_subtree_spans();

    assert_eq!(graph.get(root).unwrap().subtree_span, 6);
    assert_eq!(graph.get(child1).unwrap().subtree_span, 1);
    assert_eq!(graph.get(child2).unwrap().subtree_span, 4);
    assert_eq!(graph.get(gchild1).unwrap().subtree_span, 1);
    assert_eq!(graph.get(gchild2).unwrap().subtree_span, 1);
    assert_eq!(graph.get(gchild3).unwrap().subtree_span, 1);

    // Verify O(1) fast-forwarding past child2
    let child2_span = graph.get(child2).unwrap().subtree_span;
    let next_after_child2 = child2.0 + child2_span;
    assert_eq!(next_after_child2, 6); // skipped past gchild1, gchild2, gchild3
}

#[test]
fn test_generational_memo_cache() {
    let mut cache = MemoCache::new();

    let prop_hash = 0x1234567890ABCDEF;
    let signals = vec![(1u32, 5u64), (2u32, 10u64)];

    let hash_v1 = MemoCache::compute_compound_hash(prop_hash, &signals);
    let signals_mutated = vec![(1u32, 6u64), (2u32, 10u64)]; // signal 1 bumped version
    let hash_v2 = MemoCache::compute_compound_hash(prop_hash, &signals_mutated);

    assert_ne!(hash_v1, hash_v2);

    let entry = MemoEntry {
        memo_hash: hash_v1,
        display_chunk: DisplayChunkRange { start: 0, count: 12 },
        layout_rect: FixedRect::from_f32(0.0, 0.0, 300.0, 200.0),
    };

    cache.insert(42, entry.clone());

    // Hit
    assert_eq!(cache.try_reuse(42, hash_v1), Some(&entry));
    // Miss (invalidated signal)
    assert_eq!(cache.try_reuse(42, hash_v2), None);
}

#[test]
fn test_stack_guard_recursion_protection() {
    let mut guard = StackGuard::new(64, 512);

    // Frame entries below trampoline
    for _ in 0..50 {
        assert_eq!(guard.enter(), Ok(false));
    }
    assert_eq!(guard.depth(), 50);

    // Cross into trampoline territory (>= 64)
    for _ in 50..64 {
        guard.enter().unwrap();
    }
    assert_eq!(guard.enter(), Ok(true)); // depth 65 triggers trampoline

    // Continue up to max depth limit 512
    for _ in 65..511 {
        guard.enter().unwrap();
    }
    // Hard cap failure
    let err = guard.enter();
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("recursion depth exceeded"));
}
