//! Comprehensive Tests for Kitty and Sixel Terminal Graphics Protocols.

use strata_primitives::color::Rgba;
use strata_tui::kitty::{base64_encode, delete_kitty_images, encode_kitty_png, encode_kitty_rgba, KittyImageOptions};
use strata_tui::sixel::{encode_sixel, SixelOptions};

#[test]
fn test_base64_encoding() {
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"f"), "Zg==");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert_eq!(base64_encode(b"foo"), "Zm9v");
    assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
    assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
}

#[test]
fn test_kitty_graphics_small_rgba_serialization() {
    // 2x2 image
    let pixels = vec![
        Rgba::new(255, 0, 0, 255),   // Red
        Rgba::new(0, 255, 0, 255),   // Green
        Rgba::new(0, 0, 255, 255),   // Blue
        Rgba::new(255, 255, 0, 255), // Yellow
    ];

    let opts = KittyImageOptions {
        col: 5,
        row: 10,
        cell_width: Some(4),
        cell_height: Some(2),
        image_id: 101,
    };

    let escape_str = encode_kitty_rgba(&pixels, 2, 2, &opts);

    // Verify cursor positioning (\x1b[11;6H)
    assert!(escape_str.starts_with("\x1b[11;6H"));

    // Verify Kitty control payload
    assert!(escape_str.contains("\x1b_Ga=T,q=2,f=32,s=2,v=2,c=4,r=2,i=101,m=0;"));
    assert!(escape_str.ends_with("\x1b\\"));
}

#[test]
fn test_kitty_graphics_4096_byte_chunking() {
    // Create an image large enough to exceed 4096 Base64 bytes (e.g. 64x64 pixels = 16384 bytes raw -> ~21848 base64 chars)
    let width = 64;
    let height = 64;
    let pixels = vec![Rgba::new(128, 64, 32, 255); (width * height) as usize];

    let opts = KittyImageOptions {
        col: 0,
        row: 0,
        ..Default::default()
    };

    let escape_str = encode_kitty_rgba(&pixels, width, height, &opts);

    // Count chunks (each ends with \x1b\)
    let chunk_count = escape_str.matches("\x1b\\").count();
    assert!(chunk_count >= 5, "Expected at least 5 chunks for 16KB payload, got {chunk_count}");

    // First chunk must have m=1
    assert!(escape_str.contains("m=1;"));
    // Final chunk must have m=0
    assert!(escape_str.contains("m=0;"));
}

#[test]
fn test_kitty_png_and_deletion() {
    let dummy_png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
    let opts = KittyImageOptions::default();
    let escape_str = encode_kitty_png(dummy_png, &opts);

    assert!(escape_str.contains("f=100"));
    assert!(escape_str.ends_with("\x1b\\"));

    let del_all = delete_kitty_images(None);
    assert_eq!(del_all, "\x1b_Ga=d,d=A\x1b\\");

    let del_one = delete_kitty_images(Some(77));
    assert_eq!(del_one, "\x1b_Ga=d,d=i,i=77\x1b\\");
}

#[test]
fn test_sixel_raster_encoding_and_rle() {
    // 8x6 red rectangle
    let width = 8;
    let height = 6;
    let pixels = vec![Rgba::new(255, 0, 0, 255); (width * height) as usize];

    let opts = SixelOptions {
        col: 2,
        row: 4,
        aspect_ratio: (1, 1),
    };

    let sixel_str = encode_sixel(&pixels, width, height, &opts);

    // Verify cursor positioning (\x1b[5;3H)
    assert!(sixel_str.starts_with("\x1b[5;3H"));

    // Verify Sixel DCS header and image dimensions
    assert!(sixel_str.contains("\x1bPq\"1;1;8;6"));

    // Verify palette definition (#0;2;100;0;0 for pure red)
    assert!(sixel_str.contains("#0;2;100;0;0"));

    // Verify run-length encoding (!8~ for 8 consecutive vertical 6-pixel columns)
    // 6 bits all set (0x3F) + 0x3F = 0x7E ('~')
    assert!(sixel_str.contains("!8~"), "Expected RLE '!8~' in Sixel output: {sixel_str}");

    // Verify DCS termination (\x1b\)
    assert!(sixel_str.ends_with("\x1b\\"));
}
