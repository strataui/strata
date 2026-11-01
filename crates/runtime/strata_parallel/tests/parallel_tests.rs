use strata_dom::NodeId;
use strata_layout::Rect;
use strata_parallel::prelude::*;
use strata_parallel::ThreadLocalArena;
use strata_render::DrawCommand;
use strata_style::types::{Color, Display, Overflow, Position};
use strata_style::ComputedStyle;

#[test]
fn test_containment_classifier_5_axioms() {
    let mut style = ComputedStyle::default();
    style.display = Display::Flex;
    style.position = Position::Relative;
    style.overflow_x = Overflow::Hidden;
    style.overflow_y = Overflow::Hidden;
    style.padding.top = 16.0;

    // Both dimensions definite -> certified boundary
    assert!(ContainmentClassifier::is_layout_boundary(&style, true, true));

    // Violate Axiom 1 (Indefinite sizing)
    assert!(!ContainmentClassifier::is_layout_boundary(&style, false, true));
    assert!(!ContainmentClassifier::is_layout_boundary(&style, true, false));

    // Violate Axiom 4 (Inline element)
    style.display = Display::Inline;
    assert!(!ContainmentClassifier::is_layout_boundary(&style, true, true));
}

#[test]
fn test_thread_local_arena_allocation_and_single_cycle_reset() {
    let mut arena = ThreadLocalArena::new(1024 * 64);
    assert_eq!(arena.bytes_allocated(), 0);

    // Allocate 100 structs
    for i in 0..100 {
        let val = arena.alloc(i as u64 * 2);
        assert_eq!(*val, i as u64 * 2);
    }
    assert!(arena.bytes_allocated() > 0);

    // Reset back to 0
    arena.reset();
    assert_eq!(arena.bytes_allocated(), 0);

    // Re-allocate after reset
    let slice = arena.alloc_slice(&[10u32, 20u32, 30u32]);
    assert_eq!(slice, &[10, 20, 30]);
}

#[test]
fn test_dirty_signal_queue_concurrency_and_bitset_drain() {
    use std::sync::Arc;

    let queue = Arc::new(DirtySignalQueue::new(256));
    let mut handles = Vec::new();

    // 4 concurrent threads pushing signal IDs
    for t in 0..4 {
        let q = Arc::clone(&queue);
        handles.push(std::thread::spawn(move || {
            for i in 0..25 {
                let sig_id = (t * 25 + i) as u32;
                assert!(q.push(sig_id));
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Drain into 64-bit bitset
    let mut bitset = vec![0u64; 4]; // 256 bits total
    let count = queue.drain_into_bitset(&mut bitset);
    assert_eq!(count, 100);

    // Verify bitset has 100 bits set (signals 0..99)
    for sig in 0..100 {
        let word_idx = sig / 64;
        let bit_idx = sig % 64;
        assert_ne!(
            bitset[word_idx] & (1u64 << bit_idx),
            0,
            "Signal {sig} bit was not set"
        );
    }
}

#[test]
fn test_parallel_engine_dispatch_and_master_display_list_merge() {
    let engine = ParallelEngine::with_threads(4);

    let partitions = vec![
        LayoutPartition {
            partition_id: PartitionId(0),
            origin: (0.0, 0.0),
            constraints: LayoutConstraints::default(),
            z_index: 0,
            node_id: NodeId(1),
        },
        LayoutPartition {
            partition_id: PartitionId(1),
            origin: (300.0, 0.0),
            constraints: LayoutConstraints::default(),
            z_index: 1,
            node_id: NodeId(2),
        },
        LayoutPartition {
            partition_id: PartitionId(2),
            origin: (0.0, 400.0),
            constraints: LayoutConstraints::default(),
            z_index: 0,
            node_id: NodeId(3),
        },
    ];

    // Worker generates a single DrawRect relative to partition origin (10, 10, 50, 50)
    let completed_slots = engine.execute_parallel(&partitions, |_p| {
        vec![DrawCommand::DrawRect {
            bounds: Rect::new(10.0, 10.0, 50.0, 50.0),
            color: Color::rgb(255, 0, 0),
        }]
    });

    assert_eq!(completed_slots.len(), 3);
    for slot in &completed_slots {
        assert!(slot.is_some());
    }

    let master_list = engine.assemble_master_display_list(&completed_slots);
    assert_eq!(master_list.commands.len(), 3);

    // Verify translated origins
    match &master_list.commands[0] {
        DrawCommand::DrawRect { bounds, .. } => {
            assert_eq!(bounds.x, 10.0);
            assert_eq!(bounds.y, 10.0);
        }
        _ => panic!("Expected DrawRect"),
    }

    match &master_list.commands[1] {
        DrawCommand::DrawRect { bounds, .. } => {
            assert_eq!(bounds.x, 310.0); // 300.0 + 10.0
            assert_eq!(bounds.y, 10.0);
        }
        _ => panic!("Expected DrawRect"),
    }

    match &master_list.commands[2] {
        DrawCommand::DrawRect { bounds, .. } => {
            assert_eq!(bounds.x, 10.0);
            assert_eq!(bounds.y, 410.0); // 400.0 + 10.0
        }
        _ => panic!("Expected DrawRect"),
    }
}
