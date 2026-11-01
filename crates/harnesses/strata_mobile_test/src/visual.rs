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

        let mut sum_a = 0.0f32;
        let mut sum_b = 0.0f32;
        let mut mismatched = 0;
        let mut min_x = width as u32;
        let mut max_x = 0u32;
        let mut min_y = height as u32;
        let mut max_y = 0u32;

        // Compute means and locate damage bounding box
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

                sum_a += lum_a;
                sum_b += lum_b;

                if (lum_a - lum_b).abs() > 1.0 {
                    mismatched += 1;
                    min_x = min_x.min(x as u32);
                    max_x = max_x.max(x as u32);
                    min_y = min_y.min(y as u32);
                    max_y = max_y.max(y as u32);
                }
            }
        }

        let n = total_pixels as f32;
        let mean_a = sum_a / n;
        let mean_b = sum_b / n;

        // Compute variance and covariance
        let mut var_a = 0.0f32;
        let mut var_b = 0.0f32;
        let mut cov_ab = 0.0f32;

        for y in 0..height {
            for x in 0..width {
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

                let diff_a = lum_a - mean_a;
                let diff_b = lum_b - mean_b;

                var_a += diff_a * diff_a;
                var_b += diff_b * diff_b;
                cov_ab += diff_a * diff_b;
            }
        }

        var_a /= n;
        var_b /= n;
        cov_ab /= n;

        // Formula: SSIM = ((2 * mu_x * mu_y + C1) * (2 * sigma_xy + C2)) / ((mu_x^2 + mu_y^2 + C1) * (sigma_x^2 + sigma_y^2 + C2))
        let numerator = (2.0 * mean_a * mean_b + C1) * (2.0 * cov_ab + C2);
        let denominator = (mean_a * mean_a + mean_b * mean_b + C1) * (var_a + var_b + C2);

        let ssim_score = if denominator == 0.0 {
            1.0
        } else {
            (numerator / denominator).clamp(0.0, 1.0)
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
