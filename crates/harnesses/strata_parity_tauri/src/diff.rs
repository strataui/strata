//! Image comparison and differential analysis engine for the parity suite.
//!
//! The comparison uses a straight RGBA Euclidean colour distance. It is called
//! delta E in report output for continuity with the existing harness. Exact
//! matches use a distance below 0.5, perceptual matches use a distance at or
//! below 15.0, and significant differences use a distance above 15.0.

use std::path::Path;

use image::{ImageBuffer, Rgba, RgbaImage};
use strata_mobile_test::visual::{SsimEngine, VisualDiffResult};

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

    let ssim_result = SsimEngine::compute_ssim(
        &raw_reference,
        &raw_candidate,
        width as usize,
        height as usize,
        4,
    )
    .unwrap_or(VisualDiffResult {
        ssim_score: 0.0,
        is_match: false,
        mismatched_pixels: total_pixels,
        damage_rects: vec![],
    });

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
        ssim_score: ssim_result.ssim_score,
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
