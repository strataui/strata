//! Automated tests for offscreen FrameBuffer PNG export and perceptual hash (dHash) regression testing.

use strata_render::rasterizer::FrameBuffer;
use strata_style::types::Color;

#[test]
fn test_framebuffer_png_binary_structure_and_chunks() {
    let mut fb = FrameBuffer::new(64, 48);
    // Draw a red rectangle
    for y in 10..30 {
        for x in 10..40 {
            fb.set_pixel(x, y, Color::rgba(255, 0, 0, 255));
        }
    }

    let png_bytes = fb.to_png_bytes().expect("PNG encoding must succeed");
    assert!(png_bytes.len() > 60);

    // 1. Verify PNG 8-byte signature
    let expected_sig = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    assert_eq!(&png_bytes[0..8], &expected_sig);

    // 2. Verify IHDR chunk
    let ihdr_len = u32::from_be_bytes(png_bytes[8..12].try_into().unwrap());
    assert_eq!(ihdr_len, 13);
    assert_eq!(&png_bytes[12..16], b"IHDR");

    let width = u32::from_be_bytes(png_bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png_bytes[20..24].try_into().unwrap());
    assert_eq!(width, 64);
    assert_eq!(height, 48);
    assert_eq!(png_bytes[24], 8); // 8-bit depth
    assert_eq!(png_bytes[25], 6); // RGBA color type

    // 3. Verify IEND chunk at the end
    let total_len = png_bytes.len();
    assert_eq!(&png_bytes[total_len - 12..total_len - 8], &[0, 0, 0, 0]);
    assert_eq!(&png_bytes[total_len - 8..total_len - 4], b"IEND");
    assert_eq!(
        &png_bytes[total_len - 4..total_len],
        &[0xAE, 0x42, 0x60, 0x82]
    );
}

#[test]
fn test_framebuffer_export_png_to_disk() {
    let mut fb = FrameBuffer::new(32, 32);
    fb.clear(Color::rgba(30, 40, 50, 255));

    let tmp_path = std::env::temp_dir().join("strata_test_screenshot.png");
    let export_res = fb.export_png(&tmp_path);
    assert!(
        export_res.is_ok(),
        "export_png must succeed: {:?}",
        export_res
    );

    assert!(tmp_path.exists());
    let file_metadata = std::fs::metadata(&tmp_path).expect("File metadata must exist");
    assert!(file_metadata.len() > 50);

    let _ = std::fs::remove_file(&tmp_path);
}

#[test]
fn test_framebuffer_perceptual_hash_and_hamming_distance() {
    let mut fb_a = FrameBuffer::new(128, 128);
    // Draw horizontal split: top half white, bottom half black
    for y in 0..128 {
        for x in 0..128 {
            if y < 64 {
                fb_a.set_pixel(x, y, Color::rgba(255, 255, 255, 255));
            } else {
                fb_a.set_pixel(x, y, Color::rgba(0, 0, 0, 255));
            }
        }
    }

    let mut fb_b = fb_a.clone();

    let hash_a = fb_a.compute_phash();
    let hash_b = fb_b.compute_phash();
    assert_eq!(FrameBuffer::phash_distance(hash_a, hash_b), 0);

    // Minor antialiasing-like change (single pixel modified)
    fb_b.set_pixel(10, 10, Color::rgba(250, 250, 250, 255));
    let hash_b_minor = fb_b.compute_phash();
    assert!(FrameBuffer::phash_distance(hash_a, hash_b_minor) <= 1);

    // Significant structural change: vertical split (left half white, right half black)
    let mut fb_c = FrameBuffer::new(128, 128);
    for y in 0..128 {
        for x in 0..128 {
            if x < 64 {
                fb_c.set_pixel(x, y, Color::rgba(255, 255, 255, 255));
            } else {
                fb_c.set_pixel(x, y, Color::rgba(0, 0, 0, 255));
            }
        }
    }

    let hash_c = fb_c.compute_phash();
    let dist_ac = FrameBuffer::phash_distance(hash_a, hash_c);
    assert!(
        dist_ac > 5,
        "Structural change must produce significant perceptual distance: got {}",
        dist_ac
    );
}
