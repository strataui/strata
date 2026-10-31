use strata::low_level::{LowLevelRenderer, OffscreenBuffer, RawSurfaceHandle};

#[test]
fn test_raw_surface_handle_hidpi_scaling() {
    let surface = RawSurfaceHandle::new(0x12345678, 1280, 800, 2.0);
    assert_eq!(surface.surface_ptr, 0x12345678);
    assert_eq!(surface.width, 1280);
    assert_eq!(surface.height, 800);
    assert_eq!(surface.scale_factor, 2.0);
    assert_eq!(surface.physical_width(), 2560);
    assert_eq!(surface.physical_height(), 1600);
}

#[test]
fn test_offscreen_buffer_operations() {
    let mut buf = OffscreenBuffer::new(10, 10);
    assert_eq!(buf.byte_len(), 400);

    // Initial pixels should be zeroed
    assert_eq!(buf.get_pixel(0, 0), Some([0, 0, 0, 0]));

    // Clear with solid cyan
    buf.clear([0, 255, 255, 255]);
    assert_eq!(buf.get_pixel(0, 0), Some([0, 255, 255, 255]));
    assert_eq!(buf.get_pixel(9, 9), Some([0, 255, 255, 255]));

    // Set individual pixel to amber
    buf.set_pixel(5, 5, [245, 158, 11, 255]);
    assert_eq!(buf.get_pixel(5, 5), Some([245, 158, 11, 255]));

    let raw = buf.to_raw_rgba();
    assert_eq!(raw.len(), 400);
}

#[test]
fn test_low_level_renderer_quad_and_damage_blits() {
    let mut renderer = LowLevelRenderer::new(100, 100);

    // Submit raw quad without RSX DOM
    renderer.submit_quad(10, 10, 20, 20, [16, 185, 129, 255]);
    assert_eq!(renderer.rendered_quad_count(), 1);

    // Verify drawn pixel inside quad
    assert_eq!(
        renderer.buffer().get_pixel(15, 15),
        Some([16, 185, 129, 255])
    );
    // Verify untouched pixel outside quad
    assert_eq!(renderer.buffer().get_pixel(0, 0), Some([0, 0, 0, 0]));

    // Record damaged blit
    renderer.damaged_blit(10, 10, 20, 20);
    assert_eq!(renderer.damaged_rect_count(), 1);
}
