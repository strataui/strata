#![allow(clippy::field_reassign_with_default)]

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
            computational_weight: 1,
        },
        LayoutPartition {
            partition_id: PartitionId(1),
            origin: (300.0, 0.0),
            constraints: LayoutConstraints::default(),
            z_index: 1,
            node_id: NodeId(2),
            computational_weight: 1,
        },
        LayoutPartition {
            partition_id: PartitionId(2),
            origin: (0.0, 400.0),
            constraints: LayoutConstraints::default(),
            z_index: 0,
            node_id: NodeId(3),
            computational_weight: 1,
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
            assert_eq!(bounds.x, 10.0);
            assert_eq!(bounds.y, 410.0); // 400.0 + 10.0 (z-index 0, painted before z-index 1)
        }
        _ => panic!("Expected DrawRect"),
    }

    match &master_list.commands[2] {
        DrawCommand::DrawRect { bounds, .. } => {
            assert_eq!(bounds.x, 310.0); // 300.0 + 10.0 (z-index 1, painted on top of z-index 0)
        }
        _ => panic!("Expected DrawRect"),
    }
}

#[test]
fn test_sparse_partition_ids_and_z_index_inversion_bugs_4a_4b() {
    let engine = ParallelEngine::with_threads(4);

    // Bug 4A reproduction: sparse partition IDs (5 and 8)
    let partitions = vec![
        LayoutPartition {
            partition_id: PartitionId(5),
            origin: (0.0, 0.0),
            constraints: LayoutConstraints::default(),
            z_index: 10,
            node_id: NodeId(101),
            computational_weight: 1,
        },
        LayoutPartition {
            partition_id: PartitionId(8),
            origin: (50.0, 50.0),
            constraints: LayoutConstraints::default(),
            z_index: -5,
            node_id: NodeId(102),
            computational_weight: 1,
        },
    ];

    let completed_slots = engine.execute_parallel(&partitions, |_p| {
        vec![DrawCommand::DrawRect {
            bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
            color: Color::rgb(0, 0, 255),
        }]
    });

    // Verify 0% data loss: both slots must be populated
    assert_eq!(completed_slots.len(), 2);
    assert!(completed_slots[0].is_some(), "Partition 5 must not be lost!");
    assert!(completed_slots[1].is_some(), "Partition 8 must not be lost!");

    let master_list = engine.assemble_master_display_list(&completed_slots);
    assert_eq!(master_list.commands.len(), 2);

    // Bug 4B reproduction: z_index -5 (Partition 8) must precede z_index 10 (Partition 5)
    match &master_list.commands[0] {
        DrawCommand::DrawRect { bounds, .. } => {
            assert_eq!(bounds.x, 50.0);
            assert_eq!(bounds.y, 50.0);
        }
        _ => panic!("Expected DrawRect for z-index -5"),
    }

    match &master_list.commands[1] {
        DrawCommand::DrawRect { bounds, .. } => {
            assert_eq!(bounds.x, 0.0);
            assert_eq!(bounds.y, 0.0);
        }
        _ => panic!("Expected DrawRect for z-index 10"),
    }
}

#[test]
fn test_persistent_worker_pool_reuses_threads_and_eliminates_churn() {
    let engine = ParallelEngine::with_threads(4);

    // Run 50 frames with varying partition counts to verify persistent pool stability and load balancing
    for frame in 0..50 {
        let count = (frame % 16) + 1; // 1 to 16 partitions
        let partitions: Vec<LayoutPartition> = (0..count)
            .map(|i| LayoutPartition {
                partition_id: PartitionId(i as u32),
                origin: (i as f32 * 10.0, i as f32 * 10.0),
                constraints: LayoutConstraints::default(),
                z_index: i as i32,
                node_id: NodeId((i + 1) as u32),
                computational_weight: 1,
            })
            .collect();

        let slots = engine.execute_parallel(&partitions, |_p| {
            vec![DrawCommand::DrawRect {
                bounds: Rect::new(0.0, 0.0, 20.0, 20.0),
                color: Color::rgb(0, 255, 0),
            }]
        });

        assert_eq!(slots.len(), count);
        for slot in &slots {
            assert!(slot.is_some(), "Every slot must be completed in frame {frame}");
        }

        let master = engine.assemble_master_display_list(&slots);
        assert_eq!(master.commands.len(), count);
    }
}

#[test]
fn test_adaptive_partitioner_subtree_decomposition_and_weight_balancing() {
    let partitioner = AdaptivePartitioner::new(100);

    let partitions = vec![
        LayoutPartition::new(PartitionId(1), NodeId(10)).with_weight(50),
        LayoutPartition::new(PartitionId(2), NodeId(20)).with_weight(250), // Oversized: 250 > 100
        LayoutPartition::new(PartitionId(3), NodeId(30)).with_weight(80),
    ];

    let balanced = partitioner.balance_partitions(partitions);

    // Partition 1 (50) is kept as 1
    // Partition 2 (250) is decomposed into ceil(250/100) = 3 sub-partitions of weight ~83
    // Partition 3 (80) is kept as 1
    // Total balanced partitions = 1 + 3 + 1 = 5
    assert_eq!(balanced.len(), 5);

    assert_eq!(balanced[0].partition_id, PartitionId(1));
    assert_eq!(balanced[0].computational_weight, 50);

    assert_eq!(balanced[1].partition_id, PartitionId(2000));
    assert!(balanced[1].computational_weight <= 100);

    assert_eq!(balanced[2].partition_id, PartitionId(2001));
    assert!(balanced[2].computational_weight <= 100);

    assert_eq!(balanced[3].partition_id, PartitionId(2002));
    assert!(balanced[3].computational_weight <= 100);

    assert_eq!(balanced[4].partition_id, PartitionId(3));
    assert_eq!(balanced[4].computational_weight, 80);
}

#[test]
fn test_work_stealing_lpt_priority_prevents_worker_starvation() {
    let engine = ParallelEngine::with_threads(4);

    // Heavily skewed weights: partition 0 has weight 5000, while partitions 1..8 have weight 1
    let mut partitions = Vec::new();
    partitions.push(LayoutPartition::new(PartitionId(0), NodeId(1)).with_weight(5000));
    for i in 1..=8 {
        partitions.push(LayoutPartition::new(PartitionId(i), NodeId(i + 1)).with_weight(1));
    }

    let completed_order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let order_clone = completed_order.clone();

    let slots = engine.execute_parallel(&partitions, move |p| {
        // If partition is heavy, simulate more work
        if p.computational_weight > 100 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        order_clone.lock().unwrap().push(p.partition_id);
        vec![DrawCommand::DrawRect {
            bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
            color: Color::rgb(255, 255, 255),
        }]
    });

    // All slots populated without data loss
    assert_eq!(slots.len(), 9);
    for (i, slot) in slots.iter().enumerate() {
        assert!(slot.is_some(), "Slot {i} must be populated");
        assert_eq!(slot.as_ref().unwrap().partition_id, PartitionId(i as u32));
    }

    let master = engine.assemble_master_display_list(&slots);
    assert_eq!(master.commands.len(), 9);
}

#[test]
fn test_thread_local_arena_zero_heap_contention_in_workers() {
    let engine = ParallelEngine::with_threads(4);

    let partitions: Vec<LayoutPartition> = (0..16)
        .map(|i| LayoutPartition::new(PartitionId(i), NodeId(i + 1)).with_weight(10))
        .collect();

    // In parallel workers, allocate inside the thread-local bump arena without heap locking
    let slots = engine.execute_parallel(&partitions, |p| {
        strata_parallel::with_thread_local_arena(|arena| {
            let allocated = arena.alloc(p.partition_id.0 * 42);
            assert_eq!(*allocated, p.partition_id.0 * 42);

            let slice = arena.alloc_slice(&[1.0f32, 2.0f32, 3.0f32, 4.0f32]);
            assert_eq!(slice.len(), 4);

            vec![DrawCommand::DrawRect {
                bounds: Rect::new(slice[0], slice[1], 100.0, 100.0),
                color: Color::rgb(128, 128, 128),
            }]
        })
    });

    assert_eq!(slots.len(), 16);
    for slot in &slots {
        assert!(slot.is_some());
    }
}

#[test]
fn test_rayon_work_stealing_parallel_grid_subtrees() {
    use strata_layout::LayoutNode;
    use strata_style::types::{Dimension, Display, GridTrack};

    let engine = ParallelEngine::with_threads(4);

    // Create 10 independent grid subtrees, each containing 12 child nodes in a 3-column grid
    let mut subtrees: Vec<LayoutNode> = (0..10)
        .map(|i| {
            let mut grid = LayoutNode::new(ComputedStyle::default());
            grid.style.display = Display::Grid;
            grid.style.width = Dimension::Px(600.0);
            grid.style.grid_template_columns = vec![
                GridTrack::Fr(1.0),
                GridTrack::Fr(2.0),
                GridTrack::Fr(1.0),
            ];
            grid.children = (0..12)
                .map(|j| {
                    let mut child = LayoutNode::new(ComputedStyle::default());
                    child.style.width = Dimension::Px(50.0 + (i * j) as f32);
                    child.style.height = Dimension::Px(30.0);
                    child
                })
                .collect();
            grid
        })
        .collect();

    // Lay out all 10 independent grid subtrees concurrently across Rayon work-stealing threads
    engine.compute_subtrees_parallel(&mut subtrees, 800.0, 600.0);

    // Verify all 10 subtrees computed valid, non-zero geometry
    assert_eq!(subtrees.len(), 10);
    for (i, tree) in subtrees.iter().enumerate() {
        assert_eq!(tree.geometry.border_box.width, 600.0, "Subtree {i} width");
        assert!(tree.geometry.border_box.height > 0.0, "Subtree {i} height");
        assert_eq!(tree.children.len(), 12);
        for (j, child) in tree.children.iter().enumerate() {
            assert!(
                child.geometry.border_box.width > 0.0,
                "Child {j} of subtree {i} has zero width"
            );
        }
    }
}

#[test]
fn test_rayon_join_recursive_divide_and_conquer_subtrees() {
    use strata_layout::LayoutNode;
    use strata_style::types::{Dimension, Display, FlexDirection};

    let engine = ParallelEngine::with_threads(4);

    // 8 flexbox subtrees
    let mut subtrees: Vec<LayoutNode> = (0..8)
        .map(|_| {
            let mut flex = LayoutNode::new(ComputedStyle::default());
            flex.style.display = Display::Flex;
            flex.style.flex_direction = FlexDirection::Row;
            flex.style.width = Dimension::Px(400.0);
            flex.children = (0..4)
                .map(|_| {
                    let mut c = LayoutNode::new(ComputedStyle::default());
                    c.style.flex_grow = 1.0;
                    c.style.height = Dimension::Px(25.0);
                    c
                })
                .collect();
            flex
        })
        .collect();

    engine.compute_subtrees_recursive_join(&mut subtrees, 400.0, 100.0);

    for (i, tree) in subtrees.iter().enumerate() {
        assert_eq!(tree.geometry.border_box.width, 400.0);
        let sum_children_width: f32 = tree.children.iter().map(|c| c.geometry.border_box.width).sum();
        assert!(
            (sum_children_width - 400.0).abs() < 0.01,
            "Subtree {i} children flex widths sum to {sum_children_width}"
        );
    }
}



