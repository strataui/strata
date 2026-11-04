//! Integration tests for the Zero-Copy Shared Memory Compositor & Ring Buffer.

use strata_layout::Rect;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::shm_compositor::{ShmRingBuffer, ZeroCopyCompositor};
use strata_style::types::Color;

#[test]
fn test_shm_ring_buffer_double_buffering() {
    let ring = ShmRingBuffer::new(2);
    assert_eq!(ring.capacity(), 2);
    assert_eq!(ring.dropped_frames(), 0);

    // Initial check: no frames available
    assert!(!ring.has_new_frame(0));
    assert!(ring.acquire_latest(0).is_none());

    // Producer writes Frame 1
    let slot_idx = ring.begin_write();
    let mut dl = DisplayList::default();
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 100.0, 50.0),
        color: Color::rgb(255, 0, 0),
    });
    ring.commit_write(
        slot_idx,
        800,
        600,
        vec![Rect::new(0.0, 0.0, 100.0, 50.0)],
        dl,
        None,
    );

    // Consumer reads Frame 1
    assert!(ring.has_new_frame(0));
    let (seq, slot) = ring.acquire_latest(0).expect("Frame 1 should be available");
    assert_eq!(seq, 1);
    assert_eq!(slot.width, 800);
    assert_eq!(slot.height, 600);
    assert_eq!(slot.damage_rects.len(), 1);
    assert_eq!(slot.display_list.len(), 1);

    // Consumer checks again with sequence 1: no newer frame
    assert!(!ring.has_new_frame(seq));
    assert!(ring.acquire_latest(seq).is_none());

    // Producer writes Frame 2
    let slot_idx2 = ring.begin_write();
    let mut dl2 = DisplayList::default();
    dl2.push(DrawCommand::DrawRect {
        bounds: Rect::new(10.0, 10.0, 200.0, 100.0),
        color: Color::rgb(0, 255, 0),
    });
    ring.commit_write(
        slot_idx2,
        1024,
        768,
        vec![Rect::new(10.0, 10.0, 200.0, 100.0)],
        dl2,
        None,
    );

    // Consumer reads Frame 2
    let (seq2, slot2) = ring.acquire_latest(seq).expect("Frame 2 should be available");
    assert_eq!(seq2, 2);
    assert_eq!(slot2.width, 1024);
    assert_eq!(slot2.height, 768);
    assert_eq!(slot2.display_list.len(), 1);
    assert_eq!(ring.dropped_frames(), 0);
}

#[test]
fn test_shm_ring_buffer_dropped_frame_telemetry() {
    let ring = ShmRingBuffer::new(3);

    // Producer commits 10 frames rapidly without consumer draining
    for i in 1..=10 {
        let slot_idx = ring.begin_write();
        let mut dl = DisplayList::default();
        dl.push(DrawCommand::DrawRect {
            bounds: Rect::new(i as f32, 0.0, 50.0, 50.0),
            color: Color::rgb(0, 0, 255),
        });
        ring.commit_write(slot_idx, 640, 480, vec![], dl, None);
    }

    // Consumer acquires latest: must receive frame 10
    let (seq, slot) = ring.acquire_latest(0).expect("Latest frame should be available");
    assert_eq!(seq, 10);
    assert_eq!(slot.display_list.len(), 1);
    assert!(ring.dropped_frames() > 0, "Dropped frames counter should be non-zero");
}

#[test]
fn test_zero_copy_compositor_multi_surface() {
    let mut compositor = ZeroCopyCompositor::new();

    let ring1 = ShmRingBuffer::new(2);
    let ring2 = ShmRingBuffer::new(2);

    compositor.attach_surface(1, ring1.clone());
    compositor.attach_surface(2, ring2.clone());

    // Surface 1: Tab at top (0, 0)
    compositor.set_surface_layout(1, (0.0, 0.0), true);
    // Surface 2: Tab at offset (0, 400)
    compositor.set_surface_layout(2, (0.0, 400.0), true);

    // Commit frame on Surface 1
    let s1 = ring1.begin_write();
    let mut dl1 = DisplayList::default();
    dl1.push(DrawCommand::DrawRect {
        bounds: Rect::new(10.0, 10.0, 100.0, 50.0),
        color: Color::rgb(255, 0, 0),
    });
    ring1.commit_write(s1, 800, 400, vec![], dl1, None);

    // Commit frame on Surface 2
    let s2 = ring2.begin_write();
    let mut dl2 = DisplayList::default();
    dl2.push(DrawCommand::DrawRect {
        bounds: Rect::new(20.0, 20.0, 150.0, 60.0),
        color: Color::rgb(0, 255, 0),
    });
    ring2.commit_write(s2, 800, 400, vec![], dl2, None);

    // Composite into master display list
    let mut master_dl = DisplayList::default();
    let count = compositor.composite_into(&mut master_dl);
    assert_eq!(count, 2);
    assert_eq!(master_dl.len(), 2);

    // Verify Surface 1 untranslated
    match &master_dl.commands()[0] {
        DrawCommand::DrawRect { bounds, .. } => {
            assert_eq!(bounds.x, 10.0);
            assert_eq!(bounds.y, 10.0);
        }
        _ => panic!("Expected DrawRect for surface 1"),
    }

    // Verify Surface 2 translated by (0, 400) -> y: 20 + 400 = 420
    match &master_dl.commands()[1] {
        DrawCommand::DrawRect { bounds, .. } => {
            assert_eq!(bounds.x, 20.0);
            assert_eq!(bounds.y, 420.0);
        }
        _ => panic!("Expected DrawRect for surface 2"),
    }

    // Verify telemetry
    assert_eq!(compositor.telemetry().total_frames_composited, 1);

    // Subsequent composite with no new frames
    let mut empty_dl = DisplayList::default();
    let empty_count = compositor.composite_into(&mut empty_dl);
    assert_eq!(empty_count, 0);
    assert!(empty_dl.is_empty());
}
