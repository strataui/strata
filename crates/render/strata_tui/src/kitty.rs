//! Authentic Kitty Graphics Protocol Implementation (RFC-grade terminal graphics).
//! Reference: https://sw.kovidgoyal.net/kitty/graphics-protocol/

use strata_primitives::color::Rgba;

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes raw binary data into a standard Base64 string.
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut i = 0;

    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(B64_CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((n >> 6) & 0x3F) as usize] as char);
        out.push(B64_CHARS[(n & 0x3F) as usize] as char);
        i += 3;
    }

    if data.len() - i == 1 {
        let n = (data[i] as u32) << 16;
        out.push(B64_CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if data.len() - i == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(B64_CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }

    out
}

/// Options for rendering an image using the Kitty Graphics Protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KittyImageOptions {
    /// Target column placement in the terminal (0-based).
    pub col: u16,
    /// Target row placement in the terminal (0-based).
    pub row: u16,
    /// Destination width in character cells (optional).
    pub cell_width: Option<u16>,
    /// Destination height in character cells (optional).
    pub cell_height: Option<u16>,
    /// Optional unique image ID (0 for ephemeral).
    pub image_id: u32,
}

/// Serializes an RGBA pixel buffer into Kitty graphics protocol escape sequences.
/// Automatically splits large payloads into standard 4096-byte Base64 chunks.
pub fn encode_kitty_rgba(
    pixels: &[Rgba],
    width: u32,
    height: u32,
    opts: &KittyImageOptions,
) -> String {
    if width == 0 || height == 0 || pixels.len() < (width * height) as usize {
        return String::new();
    }

    // Convert Rgba slice into raw 32-bit RGBA byte buffer
    let mut raw_bytes = Vec::with_capacity((width * height * 4) as usize);
    for p in &pixels[..(width * height) as usize] {
        raw_bytes.push(p.r);
        raw_bytes.push(p.g);
        raw_bytes.push(p.b);
        raw_bytes.push(p.a);
    }

    encode_kitty_raw(&raw_bytes, 32, width, height, opts)
}

/// Serializes a pre-encoded PNG byte buffer into Kitty graphics protocol escape sequences.
pub fn encode_kitty_png(png_bytes: &[u8], opts: &KittyImageOptions) -> String {
    encode_kitty_raw(png_bytes, 100, 0, 0, opts)
}

/// Core chunked serializer for Kitty Graphics Protocol.
fn encode_kitty_raw(
    payload: &[u8],
    format_code: u8, // 32 for RGBA, 100 for PNG
    width: u32,
    height: u32,
    opts: &KittyImageOptions,
) -> String {
    let b64 = base64_encode(payload);
    let mut out = String::with_capacity(b64.len() + 128);

    // Position cursor at destination character cell (1-based)
    out.push_str(&format!("\x1b[{};{}H", opts.row + 1, opts.col + 1));

    const CHUNK_SIZE: usize = 4096;
    let total_chunks = b64.len().div_ceil(CHUNK_SIZE);

    if total_chunks == 0 {
        return out;
    }

    for (idx, chunk) in b64.as_bytes().chunks(CHUNK_SIZE).enumerate() {
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        let is_last = idx + 1 == total_chunks;
        let m = if is_last { 0 } else { 1 };

        if idx == 0 {
            // First chunk contains control parameters:
            // a=T : Transmit and display immediately
            // f=32 (RGBA) or f=100 (PNG)
            // q=2 : Suppress response (quiet mode)
            out.push_str("\x1b_Ga=T,q=2");
            out.push_str(&format!(",f={format_code}"));
            if width > 0 && height > 0 {
                out.push_str(&format!(",s={width},v={height}"));
            }
            if let Some(cw) = opts.cell_width {
                out.push_str(&format!(",c={cw}"));
            }
            if let Some(ch) = opts.cell_height {
                out.push_str(&format!(",r={ch}"));
            }
            if opts.image_id > 0 {
                out.push_str(&format!(",i={}", opts.image_id));
            }
            out.push_str(&format!(",m={m};{chunk_str}\x1b\\"));
        } else {
            // Subsequent continuation chunks
            out.push_str(&format!("\x1b_Gq=2,m={m};{chunk_str}\x1b\\"));
        }
    }

    out
}

/// Emits the Kitty escape command to delete all active images or an image by ID.
pub fn delete_kitty_images(image_id: Option<u32>) -> String {
    if let Some(id) = image_id {
        format!("\x1b_Ga=d,d=i,i={id}\x1b\\")
    } else {
        "\x1b_Ga=d,d=A\x1b\\".to_string()
    }
}
