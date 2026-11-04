//! First-Principles Structural Similarity Index Measure (SSIM) and Perceptual Visual Regression Engine.

/// Constants for 8-bit image dynamic range (L = 255)
const C1: f32 = 6.5025;   // (0.01 * 255)^2
const C2: f32 = 58.5225;  // (0.03 * 255)^2

/// Result of a perceptual visual comparison between rendered and baseline framebuffers.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualDiffResult {
    pub ssim_score: f32,
    pub is_match: bool,
    pub mismatched_pixels: usize,
    pub damage_rects: Vec<(u32, u32, u32, u32)>, // (x, y, width, height)
}

/// Structural Similarity (SSIM) analysis engine.
pub struct SsimEngine;

impl SsimEngine {
    /// Computes the SSIM score between two grayscale or RGBA buffers of dimensions (width x height).
    /// Assumes 4 bytes per pixel (RGBA) or 1 byte per pixel (Grayscale).
    pub fn compute_ssim(
        buf_a: &[u8],
        buf_b: &[u8],
        width: usize,
        height: usize,
        bytes_per_pixel: usize,
    ) -> Result<VisualDiffResult, &'static str> {
        let total_pixels = width * height;
        let expected_len = total_pixels * bytes_per_pixel;

        if buf_a.len() < expected_len || buf_b.len() < expected_len {
            return Err("Buffer length smaller than specified dimensions");
        }

        let mut mismatched = 0;
        let mut min_x = width as u32;
        let mut max_x = 0u32;
        let mut min_y = height as u32;
        let mut max_y = 0u32;

        // Compute damage bounding box
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * bytes_per_pixel;
                // Luminance approximation if RGBA
                let lum_a = if bytes_per_pixel >= 3 {
                    0.2126 * buf_a[idx] as f32 + 0.7152 * buf_a[idx + 1] as f32 + 0.0722 * buf_a[idx + 2] as f32
                } else {
                    buf_a[idx] as f32
                };

                let lum_b = if bytes_per_pixel >= 3 {
                    0.2126 * buf_b[idx] as f32 + 0.7152 * buf_b[idx + 1] as f32 + 0.0722 * buf_b[idx + 2] as f32
                } else {
                    buf_b[idx] as f32
                };

                if (lum_a - lum_b).abs() > 1.0 {
                    mismatched += 1;
                    min_x = min_x.min(x as u32);
                    max_x = max_x.max(x as u32);
                    min_y = min_y.min(y as u32);
                    max_y = max_y.max(y as u32);
                }
            }
        }

        // Block-based Mean SSIM (Wang et al. 2004 IEEE TIP standard)
        let block_size = if width >= 16 && height >= 16 { 8 } else { width.min(height).max(1) };
        let mut total_block_ssim = 0.0f32;
        let mut block_count = 0usize;

        for by in (0..height).step_by(block_size) {
            let bh = (height - by).min(block_size);
            for bx in (0..width).step_by(block_size) {
                let bw = (width - bx).min(block_size);
                let n_b = (bw * bh) as f32;

                let mut b_sum_a = 0.0f32;
                let mut b_sum_b = 0.0f32;

                for y in by..by + bh {
                    for x in bx..bx + bw {
                        let idx = (y * width + x) * bytes_per_pixel;
                        let lum_a = if bytes_per_pixel >= 3 {
                            0.2126 * buf_a[idx] as f32 + 0.7152 * buf_a[idx + 1] as f32 + 0.0722 * buf_a[idx + 2] as f32
                        } else {
                            buf_a[idx] as f32
                        };
                        let lum_b = if bytes_per_pixel >= 3 {
                            0.2126 * buf_b[idx] as f32 + 0.7152 * buf_b[idx + 1] as f32 + 0.0722 * buf_b[idx + 2] as f32
                        } else {
                            buf_b[idx] as f32
                        };
                        b_sum_a += lum_a;
                        b_sum_b += lum_b;
                    }
                }

                let b_mean_a = b_sum_a / n_b;
                let b_mean_b = b_sum_b / n_b;

                let mut b_var_a = 0.0f32;
                let mut b_var_b = 0.0f32;
                let mut b_cov_ab = 0.0f32;

                for y in by..by + bh {
                    for x in bx..bx + bw {
                        let idx = (y * width + x) * bytes_per_pixel;
                        let lum_a = if bytes_per_pixel >= 3 {
                            0.2126 * buf_a[idx] as f32 + 0.7152 * buf_a[idx + 1] as f32 + 0.0722 * buf_a[idx + 2] as f32
                        } else {
                            buf_a[idx] as f32
                        };
                        let lum_b = if bytes_per_pixel >= 3 {
                            0.2126 * buf_b[idx] as f32 + 0.7152 * buf_b[idx + 1] as f32 + 0.0722 * buf_b[idx + 2] as f32
                        } else {
                            buf_b[idx] as f32
                        };
                        let da = lum_a - b_mean_a;
                        let db = lum_b - b_mean_b;
                        b_var_a += da * da;
                        b_var_b += db * db;
                        b_cov_ab += da * db;
                    }
                }

                b_var_a /= n_b;
                b_var_b /= n_b;
                b_cov_ab /= n_b;

                let num = (2.0 * b_mean_a * b_mean_b + C1) * (2.0 * b_cov_ab + C2);
                let den = (b_mean_a * b_mean_a + b_mean_b * b_mean_b + C1) * (b_var_a + b_var_b + C2);
                let b_ssim = if den == 0.0 { 1.0 } else { (num / den).clamp(0.0, 1.0) };

                total_block_ssim += b_ssim;
                block_count += 1;
            }
        }

        let ssim_score = if block_count == 0 {
            1.0
        } else {
            total_block_ssim / (block_count as f32)
        };

        let mut damage_rects = Vec::new();
        if mismatched > 0 && max_x >= min_x && max_y >= min_y {
            damage_rects.push((min_x, min_y, max_x - min_x + 1, max_y - min_y + 1));
        }

        Ok(VisualDiffResult {
            ssim_score,
            is_match: ssim_score >= 0.9995,
            mismatched_pixels: mismatched,
            damage_rects,
        })
    }
}
