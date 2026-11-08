//! strata_primitives: Zero-dependency mathematical, geometric, color, memory, and atom primitives for Project Strata.

pub mod geom;
pub mod color;
pub mod arena;
pub mod atom;
pub mod target;
pub mod quantization;

pub use geom::{Point, Size, Rect, EdgeInsets, Transform2D, LayoutUnit, FixedRect, Matrix4x4};
pub use color::{Rgba, Hsla, Oklch};
pub use arena::{BumpArena, FrameArenaManager, ThreadLocalArena, DEFAULT_ARENA_CAPACITY};
pub use atom::{AtomId, AtomTable, intern, resolve};
pub use target::{is_tui_target, set_tui_target_active};
pub use quantization::largest_remainder_quantization;

pub mod prelude {
    pub use crate::geom::{Point, Size, Rect, EdgeInsets, Transform2D, LayoutUnit, FixedRect, Matrix4x4};
    pub use crate::color::{Rgba, Hsla, Oklch};
    pub use crate::arena::{BumpArena, FrameArenaManager, ThreadLocalArena};
    pub use crate::atom::{AtomId, AtomTable, intern, resolve};
    pub use crate::target::{is_tui_target, set_tui_target_active};
    pub use crate::quantization::largest_remainder_quantization;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_and_vector_math() {
        let p1 = Point::new(10.0, 20.0);
        let p2 = Point::new(5.0, 10.0);

        assert_eq!(p1 + p2, Point::new(15.0, 30.0));
        assert_eq!(p1 - p2, Point::new(5.0, 10.0));
        assert_eq!(p2 * 2.0, Point::new(10.0, 20.0));
        assert_eq!(p1 / 2.0, Point::new(5.0, 10.0));
        assert_eq!(-p1, Point::new(-10.0, -20.0));

        let dist = Point::new(0.0, 0.0).distance_to(Point::new(3.0, 4.0));
        assert!((dist - 5.0).abs() < 1e-6);

        let mid = Point::new(0.0, 0.0).lerp(Point::new(10.0, 20.0), 0.5);
        assert_eq!(mid, Point::new(5.0, 10.0));
    }

    #[test]
    fn test_rect_intersection_and_union() {
        let r1 = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        let r2 = Rect::from_xywh(50.0, 50.0, 100.0, 100.0);

        assert!(r1.intersects(r2));
        assert!(r1.contains_point(Point::new(50.0, 50.0)));
        assert!(!r1.contains_point(Point::new(150.0, 150.0)));

        let inter = r1.intersection(r2).unwrap();
        assert_eq!(inter, Rect::from_xywh(50.0, 50.0, 50.0, 50.0));

        let u = r1.union(r2);
        assert_eq!(u, Rect::from_xywh(0.0, 0.0, 150.0, 150.0));

        let deflated = r1.deflate(EdgeInsets::all(10.0));
        assert_eq!(deflated, Rect::from_xywh(10.0, 10.0, 80.0, 80.0));
    }

    #[test]
    fn test_transform2d_affine() {
        let t_ident = Transform2D::IDENTITY;
        let p = Point::new(10.0, 20.0);
        assert_eq!(t_ident.transform_point(p), p);

        let t_trans = Transform2D::translation(50.0, 100.0);
        assert_eq!(t_trans.transform_point(p), Point::new(60.0, 120.0));

        let t_scale = Transform2D::scale(2.0, 3.0);
        assert_eq!(t_scale.transform_point(p), Point::new(20.0, 60.0));

        let inv = t_trans.inverse().unwrap();
        let back = inv.transform_point(t_trans.transform_point(p));
        assert!((back.x - p.x).abs() < 1e-5);
        assert!((back.y - p.y).abs() < 1e-5);
    }

    #[test]
    fn test_chromium_layout_unit_subpixel_snapping() {
        let lu = LayoutUnit::from_f32(33.333);
        // Device Pixel Ratio 2.0 (Retina)
        let snapped_2x = lu.snap_to_device_grid(2.0);
        assert_eq!(snapped_2x, 33.5); // 33.328125 * 2 = 66.65625 -> 67 / 2 = 33.5

        // Border min-thickness snapping
        let hairline = LayoutUnit::from_f32(0.2);
        assert_eq!(hairline.snap_border(1.0), 1.0);
        assert_eq!(hairline.snap_border(2.0), 0.5);

        let fixed = FixedRect::from_f32(10.5, 20.25, 100.125, 50.875);
        let back = fixed.to_rect_f32();
        assert!((back.origin.x - 10.5).abs() < 0.02);
        assert!((back.origin.y - 20.25).abs() < 0.02);
    }

    #[test]
    fn test_color_hex_parsing_and_wcag_contrast() {
        assert_eq!(Rgba::parse_hex("#fff"), Some(Rgba::WHITE));
        assert_eq!(Rgba::parse_hex("#000"), Some(Rgba::BLACK));
        assert_eq!(Rgba::parse_hex("#ff0000"), Some(Rgba::RED));
        assert_eq!(Rgba::parse_hex("#00ff00"), Some(Rgba::GREEN));
        assert_eq!(Rgba::parse_hex("#0000ff"), Some(Rgba::BLUE));
        assert_eq!(Rgba::parse_hex("#12345678"), Some(Rgba::new(0x12, 0x34, 0x56, 0x78)));

        // WCAG Black vs White contrast is maximum (21:1)
        let contrast = Rgba::BLACK.contrast_ratio(Rgba::WHITE);
        assert!((contrast - 21.0).abs() < 0.1);

        // Alpha blending
        let semi_red = Rgba::from_f32(1.0, 0.0, 0.0, 0.5);
        let blended = semi_red.blend_over(Rgba::BLACK);
        assert_eq!(blended, Rgba::new(128, 0, 0, 255));
    }

    #[test]
    fn test_hsla_and_oklch_conversions() {
        let red_hsla = Hsla::new(0.0, 1.0, 0.5, 1.0);
        let red_rgba = red_hsla.to_rgba();
        assert_eq!(red_rgba, Rgba::RED);

        let roundtrip_hsla = Hsla::from_rgba(Rgba::GREEN);
        assert!((roundtrip_hsla.h - 120.0).abs() < 1.0);
        assert!((roundtrip_hsla.s - 1.0).abs() < 0.05);

        let oklch = Oklch::new(0.627955, 0.257683, 29.233885, 1.0);
        let oklch_rgba = oklch.to_rgba();
        // Oklch coordinates for sRGB red (approx 255, 0, 0)
        assert!(oklch_rgba.r > 240);
        assert!(oklch_rgba.g < 25);
    }

    #[test]
    fn test_bump_arena_and_frame_manager() {
        let mut arena = BumpArena::new(1024 * 64);
        let p1 = arena.alloc(Point::new(10.0f32, 20.0f32));
        assert_eq!(*p1, Point::new(10.0, 20.0));

        let slice = arena.alloc_slice(&[1u32, 2, 3, 4, 5]);
        assert_eq!(slice, &[1, 2, 3, 4, 5]);

        let str_ref = arena.alloc_str("Strata Zero-Chromium Engine");
        assert_eq!(str_ref, "Strata Zero-Chromium Engine");

        assert!(arena.allocated_bytes() > 0);
        arena.reset();
        assert_eq!(arena.allocated_bytes(), 0);

        // Double-buffered frame manager
        let mut frame_mgr = FrameArenaManager::new(1024 * 32);
        assert_eq!(frame_mgr.frame_epoch(), 0);
        let _ = frame_mgr.active_arena().alloc(12345u32);
        frame_mgr.swap_and_reset();
        assert_eq!(frame_mgr.frame_epoch(), 1);
        assert_eq!(frame_mgr.active_arena().allocated_bytes(), 0);
    }

    #[test]
    fn test_bump_arena_destructor_cleanup() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        struct DropCounter(Arc<AtomicUsize>);
        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let counter = Arc::new(AtomicUsize::new(0));
        {
            let mut arena = BumpArena::new(1024 * 64);
            for _ in 0..10 {
                let _ = arena.alloc(DropCounter(Arc::clone(&counter)));
            }
            assert_eq!(counter.load(Ordering::SeqCst), 0);
            arena.reset();
            // Destructors must have executed upon reset
            assert_eq!(counter.load(Ordering::SeqCst), 10);

            // Allocate more and drop arena
            for _ in 0..5 {
                let _ = arena.alloc(DropCounter(Arc::clone(&counter)));
            }
        }
        // Destructors must have executed upon arena drop
        assert_eq!(counter.load(Ordering::SeqCst), 15);
    }

    #[test]
    fn test_10000_rects_allocation_and_reset_speed() {
        let mut arena = BumpArena::new(1024 * 1024); // 1MB
        let start = std::time::Instant::now();
        for i in 0..10_000 {
            let _ = arena.alloc(Rect::from_xywh(i as f32, i as f32, 100.0, 50.0));
        }
        let alloc_duration = start.elapsed();
        let reset_start = std::time::Instant::now();
        arena.reset();
        let reset_duration = reset_start.elapsed();

        println!(
            "[*] 10,000 Rect allocations: {:?}, O(1) bulk reset: {:?}",
            alloc_duration, reset_duration
        );
        assert_eq!(arena.allocated_bytes(), 0);
        assert!(reset_duration.as_micros() < 10, "Reset must complete in <10 microseconds!");
    }

    #[test]
    fn test_atom_table_interning() {
        let table = AtomTable::new();

        // Static atoms are pre-baked
        assert_eq!(table.intern("div"), AtomId::DIV);
        assert_eq!(table.intern("span"), AtomId::SPAN);
        assert_eq!(table.intern("button"), AtomId::BUTTON);
        assert_eq!(table.intern("width"), AtomId::WIDTH);
        assert_eq!(table.intern("display"), AtomId::DISPLAY);

        assert_eq!(table.resolve(AtomId::DIV).unwrap(), "div");
        assert_eq!(table.resolve(AtomId::WIDTH).unwrap(), "width");

        // Dynamic atoms
        let id1 = table.intern("custom-user-component-key");
        let id2 = table.intern("custom-user-component-key");
        assert_eq!(id1, id2);
        assert!(id1.0 >= 1024);
        assert_eq!(table.resolve(id1).unwrap(), "custom-user-component-key");
    }

    #[test]
    fn test_matrix4x4_math_and_inversion() {
        let t = Matrix4x4::translation(50.0, 100.0, 0.0);
        let s = Matrix4x4::scaling(2.0, 3.0, 1.0);
        let r = Matrix4x4::rotation_z(std::f32::consts::FRAC_PI_2); // 90 deg

        // Composition: T * R * S
        let m = t.multiply(&r).multiply(&s);

        // Transform point (10, 0)
        // Scaled to (20, 0), rotated 90 deg to (0, 20), translated by (50, 100) -> (50, 120)
        let p = m.transform_point2d(Point::new(10.0, 0.0));
        assert!((p.x - 50.0).abs() < 1e-4);
        assert!((p.y - 120.0).abs() < 1e-4);

        // Analytical Inverse
        let inv = m.inverse().expect("Matrix must be invertible");
        let identity = m.multiply(&inv);
        assert!(identity.is_identity());

        // Unproject (50, 120) -> should yield back (10, 0)
        let unprojected = inv.transform_point2d(p);
        assert!((unprojected.x - 10.0).abs() < 1e-4);
        assert!((unprojected.y - 0.0).abs() < 1e-4);
    }
}
