//! Image comparison and differential analysis engine for the parity suite.
//!
//! The comparison uses a straight RGBA Euclidean colour distance. It is called
//! delta E in report output for continuity with the existing harness. Exact
//! matches use a distance below 0.5, perceptual matches use a distance at or
//! below 15.0, and significant differences use a distance above 15.0.

use std::path::Path;

use image::{ImageBuffer, Rgba, RgbaImage};

/// Block-based Mean SSIM (Wang et al. 2004 IEEE TIP, 8-bit dynamic range L = 255).
fn compute_rgba_ssim(buf_a: &[u8], buf_b: &[u8], width: usize, height: usize) -> f32 {
    const C1: f32 = 6.5025; // (0.01 * 255)^2
    const C2: f32 = 58.5225; // (0.03 * 255)^2
    let expected = width.saturating_mul(height).saturating_mul(4);
    if expected == 0 || buf_a.len() < expected || buf_b.len() < expected {
        return 0.0;
    }
    let lum = |buf: &[u8], idx: usize| -> f32 {
        0.2126 * buf[idx] as f32 + 0.7152 * buf[idx + 1] as f32 + 0.0722 * buf[idx + 2] as f32
    };
    let block_size = if width >= 16 && height >= 16 {
        8
    } else {
        width.min(height).max(1)
    };
    let mut total_block_ssim = 0.0f32;
    let mut block_count = 0usize;
    for by in (0..height).step_by(block_size) {
        let bh = (height - by).min(block_size);
        for bx in (0..width).step_by(block_size) {
            let bw = (width - bx).min(block_size);
            let n = (bw * bh) as f32;
            let (mut sum_a, mut sum_b) = (0.0f32, 0.0f32);
            for y in by..by + bh {
                for x in bx..bx + bw {
                    let idx = (y * width + x) * 4;
                    sum_a += lum(buf_a, idx);
                    sum_b += lum(buf_b, idx);
                }
            }
            let mean_a = sum_a / n;
            let mean_b = sum_b / n;
            let (mut var_a, mut var_b, mut cov_ab) = (0.0f32, 0.0f32, 0.0f32);
            for y in by..by + bh {
                for x in bx..bx + bw {
                    let idx = (y * width + x) * 4;
                    let da = lum(buf_a, idx) - mean_a;
                    let db = lum(buf_b, idx) - mean_b;
                    var_a += da * da;
                    var_b += db * db;
                    cov_ab += da * db;
                }
            }
            var_a /= n;
            var_b /= n;
            cov_ab /= n;
            let num = (2.0 * mean_a * mean_b + C1) * (2.0 * cov_ab + C2);
            let den = (mean_a * mean_a + mean_b * mean_b + C1) * (var_a + var_b + C2);
            total_block_ssim += if den > 0.0 {
                (num / den).clamp(0.0, 1.0)
            } else {
                1.0
            };
            block_count += 1;
        }
    }
    if block_count == 0 {
        0.0
    } else {
        total_block_ssim / block_count as f32
    }
}

/// Detailed statistical and perceptual metrics from comparing two rendered windows.
#[derive(Debug, Clone)]
pub struct ParityMetrics {
    pub width: u32,
    pub height: u32,
    pub total_pixels: usize,
    pub exact_match_pixels: usize,
    pub exact_match_percentage: f64,
    pub perceptual_match_pixels: usize,
    pub perceptual_match_percentage: f64,
    pub ssim_score: f32,
    pub mean_delta_e: f64,
    pub max_delta_e: f64,
    pub heatmap_path: String,
}

/// Integer bounding box for significant-difference pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DifferenceBounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Extended metrics used by the three-engine report.
#[derive(Debug, Clone)]
pub struct DetailedParityMetrics {
    pub width: u32,
    pub height: u32,
    pub total_pixels: usize,
    pub exact_match_pixels: usize,
    pub exact_match_percentage: f64,
    pub perceptual_match_pixels: usize,
    pub perceptual_match_percentage: f64,
    pub ssim_score: f32,
    pub mean_delta_e: f64,
    pub max_delta_e: f64,
    pub significant_diff_pixels: usize,
    pub significant_diff_bounds: Option<DifferenceBounds>,
    pub diff_path: String,
}

impl DetailedParityMetrics {
    /// Returns true when the metrics meet the supplied thresholds.
    pub fn passes(&self, ssim_threshold: f64, perceptual_threshold: f64) -> bool {
        (self.ssim_score as f64) >= ssim_threshold
            && self.perceptual_match_percentage >= perceptual_threshold
    }
}

fn crop_rgba(image: &RgbaImage, width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            out.extend_from_slice(&image.get_pixel(x, y).0);
        }
    }
    out
}

/// Compares two screenshots and writes a false-colour differential image.
///
/// The reference image is normally Chromium. Unchanged context is dimmed,
/// differences between 0.5 and 15.0 are amber, and differences above 15.0 are
/// red.
pub fn compare_images_detailed(
    reference_path: &Path,
    candidate_path: &Path,
    output_diff: &Path,
) -> Result<DetailedParityMetrics, String> {
    let reference = image::open(reference_path)
        .map_err(|e| {
            format!(
                "Failed to open reference screenshot {}: {}",
                reference_path.display(),
                e
            )
        })?
        .to_rgba8();

    let candidate = image::open(candidate_path)
        .map_err(|e| {
            format!(
                "Failed to open candidate screenshot {}: {}",
                candidate_path.display(),
                e
            )
        })?
        .to_rgba8();

    let (w1, h1) = reference.dimensions();
    let (w2, h2) = candidate.dimensions();
    let width = w1.min(w2);
    let height = h1.min(h2);

    if width == 0 || height == 0 {
        return Err("Screenshots have no overlapping pixels".to_string());
    }

    let total_pixels = width as usize * height as usize;
    let mut exact_matches = 0usize;
    let mut perceptual_matches = 0usize;
    let mut sum_delta_e = 0.0f64;
    let mut max_delta_e = 0.0f64;
    let mut significant_diff_pixels = 0usize;
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    let mut heatmap: RgbaImage = ImageBuffer::new(width, height);
    let raw_reference = crop_rgba(&reference, width, height);
    let raw_candidate = crop_rgba(&candidate, width, height);

    let ssim_score = compute_rgba_ssim(&raw_reference, &raw_candidate, width as usize, height as usize);

    for y in 0..height {
        for x in 0..width {
            let reference_pixel = reference.get_pixel(x, y);
            let candidate_pixel = candidate.get_pixel(x, y);

            let dr = reference_pixel[0] as f64 - candidate_pixel[0] as f64;
            let dg = reference_pixel[1] as f64 - candidate_pixel[1] as f64;
            let db = reference_pixel[2] as f64 - candidate_pixel[2] as f64;
            let da = reference_pixel[3] as f64 - candidate_pixel[3] as f64;
            let delta_e = (dr * dr + dg * dg + db * db + da * da).sqrt();

            sum_delta_e += delta_e;
            if delta_e > max_delta_e {
                max_delta_e = delta_e;
            }

            if delta_e < 0.5 {
                exact_matches += 1;
                perceptual_matches += 1;
                heatmap.put_pixel(
                    x,
                    y,
                    Rgba([
                        (reference_pixel[0] as f32 * 0.25) as u8,
                        (reference_pixel[1] as f32 * 0.25) as u8,
                        (reference_pixel[2] as f32 * 0.25) as u8,
                        255,
                    ]),
                );
            } else if delta_e <= 15.0 {
                perceptual_matches += 1;
                heatmap.put_pixel(x, y, Rgba([245, 158, 11, 255]));
            } else {
                significant_diff_pixels += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                heatmap.put_pixel(x, y, Rgba([255, 24, 24, 255]));
            }
        }
    }

    if let Some(parent) = output_diff.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create diff directory {}: {}",
                parent.display(),
                e
            )
        })?;
    }
    heatmap.save(output_diff).map_err(|e| {
        format!(
            "Failed to save diff image to {}: {}",
            output_diff.display(),
            e
        )
    })?;

    let significant_diff_bounds = if significant_diff_pixels > 0 {
        Some(DifferenceBounds {
            x: min_x,
            y: min_y,
            width: max_x - min_x + 1,
            height: max_y - min_y + 1,
        })
    } else {
        None
    };

    Ok(DetailedParityMetrics {
        width,
        height,
        total_pixels,
        exact_match_pixels: exact_matches,
        exact_match_percentage: (exact_matches as f64 / total_pixels as f64) * 100.0,
        perceptual_match_pixels: perceptual_matches,
        perceptual_match_percentage: (perceptual_matches as f64 / total_pixels as f64) * 100.0,
        ssim_score,
        mean_delta_e: sum_delta_e / total_pixels as f64,
        max_delta_e,
        significant_diff_pixels,
        significant_diff_bounds,
        diff_path: output_diff.display().to_string(),
    })
}

/// Computes pixel-by-pixel colour distance and perceptual SSIM between two screenshots.
pub fn compare_screenshots(
    path_tauri: &Path,
    path_strata: &Path,
    output_heatmap: &Path,
) -> Result<ParityMetrics, String> {
    let detailed = compare_images_detailed(path_tauri, path_strata, output_heatmap)?;
    Ok(ParityMetrics {
        width: detailed.width,
        height: detailed.height,
        total_pixels: detailed.total_pixels,
        exact_match_pixels: detailed.exact_match_pixels,
        exact_match_percentage: detailed.exact_match_percentage,
        perceptual_match_pixels: detailed.perceptual_match_pixels,
        perceptual_match_percentage: detailed.perceptual_match_percentage,
        ssim_score: detailed.ssim_score,
        mean_delta_e: detailed.mean_delta_e,
        max_delta_e: detailed.max_delta_e,
        heatmap_path: detailed.diff_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(path: &Path, width: u32, height: u32, rgba: [u8; 4]) {
        let mut image: RgbaImage = ImageBuffer::new(width, height);
        for pixel in image.pixels_mut() {
            *pixel = Rgba(rgba);
        }
        image.save(path).expect("save image");
    }

    #[test]
    fn identical_images_report_a_perfect_match() {
        let dir = std::env::temp_dir().join("strata_diff_identical");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let a = dir.join("a.png");
        let b = dir.join("b.png");
        let diff = dir.join("diff.png");
        solid(&a, 32, 24, [10, 20, 30, 255]);
        solid(&b, 32, 24, [10, 20, 30, 255]);

        let metrics = compare_images_detailed(&a, &b, &diff).expect("compare");
        assert_eq!(metrics.exact_match_pixels, 32 * 24);
        assert_eq!(metrics.perceptual_match_percentage, 100.0);
        assert_eq!(metrics.significant_diff_pixels, 0);
        assert!(metrics.significant_diff_bounds.is_none());
        assert!(metrics.ssim_score > 0.99);
        assert!(diff.exists());
    }

    #[test]
    fn distant_colours_are_significant_and_bounded() {
        let dir = std::env::temp_dir().join("strata_diff_distant");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let a = dir.join("a.png");
        let b = dir.join("b.png");
        let diff = dir.join("diff.png");
        solid(&a, 16, 16, [255, 255, 255, 255]);
        solid(&b, 16, 16, [0, 0, 0, 255]);

        let metrics = compare_images_detailed(&a, &b, &diff).expect("compare");
        assert_eq!(metrics.significant_diff_pixels, 16 * 16);
        assert_eq!(
            metrics.significant_diff_bounds,
            Some(DifferenceBounds {
                x: 0,
                y: 0,
                width: 16,
                height: 16
            })
        );
        assert!(!metrics.passes(0.95, 95.0));
    }
}
