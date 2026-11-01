//! Image comparison and differential analysis engine for Strata vs Tauri.

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

/// Computes pixel-by-pixel color distance and perceptual SSIM between two screenshots.
pub fn compare_screenshots(
    path_tauri: &Path,
    path_strata: &Path,
    output_heatmap: &Path,
) -> Result<ParityMetrics, String> {
    let img_tauri = image::open(path_tauri)
        .map_err(|e| format!("Failed to open Tauri screenshot {}: {}", path_tauri.display(), e))?
        .to_rgba8();

    let img_strata = image::open(path_strata)
        .map_err(|e| format!("Failed to open Strata screenshot {}: {}", path_strata.display(), e))?
        .to_rgba8();

    let (w1, h1) = img_tauri.dimensions();
    let (w2, h2) = img_strata.dimensions();

    let w = w1.min(w2);
    let h = h1.min(h2);

    let total_pixels = (w as usize) * (h as usize);
    let mut exact_matches = 0usize;
    let mut perceptual_matches = 0usize;
    let mut sum_delta_e = 0.0f64;
    let mut max_delta_e = 0.0f64;

    let mut heatmap: RgbaImage = ImageBuffer::new(w, h);

    let raw_tauri = img_tauri.as_raw();
    let raw_strata = img_strata.as_raw();

    // Compute SSIM on the overlapping region
    let ssim_res: VisualDiffResult = SsimEngine::compute_ssim(
        &raw_tauri[..(w as usize * h as usize * 4)],
        &raw_strata[..(w as usize * h as usize * 4)],
        w as usize,
        h as usize,
        4,
    ).unwrap_or(VisualDiffResult {
        ssim_score: 0.0,
        is_match: false,
        mismatched_pixels: total_pixels,
        damage_rects: vec![],
    });

    for y in 0..h {
        for x in 0..w {
            let p_t = img_tauri.get_pixel(x, y);
            let p_s = img_strata.get_pixel(x, y);

            let dr = (p_t[0] as f64) - (p_s[0] as f64);
            let dg = (p_t[1] as f64) - (p_s[1] as f64);
            let db = (p_t[2] as f64) - (p_s[2] as f64);
            let da = (p_t[3] as f64) - (p_s[3] as f64);

            let delta_e = (dr * dr + dg * dg + db * db + da * da).sqrt();
            sum_delta_e += delta_e;
            if delta_e > max_delta_e {
                max_delta_e = delta_e;
            }

            if delta_e < 0.5 {
                exact_matches += 1;
                perceptual_matches += 1;
                // Muted context pixel in heatmap
                let dim_r = (p_t[0] as f32 * 0.25) as u8;
                let dim_g = (p_t[1] as f32 * 0.25) as u8;
                let dim_b = (p_t[2] as f32 * 0.25) as u8;
                heatmap.put_pixel(x, y, Rgba([dim_r, dim_g, dim_b, 255]));
            } else if delta_e <= 15.0 {
                // Minor font subpixel antialiasing divergence
                perceptual_matches += 1;
                // Amber/Orange highlight
                heatmap.put_pixel(x, y, Rgba([245, 158, 11, 255]));
            } else {
                // Significant divergence: Hot Neon Magenta / Red highlight
                let intensity = ((delta_e / 255.0) * 200.0).min(200.0) as u8;
                heatmap.put_pixel(x, y, Rgba([255, 0, 80 + (intensity / 4), 255]));
            }
        }
    }

    heatmap.save(output_heatmap)
        .map_err(|e| format!("Failed to save diff heatmap to {}: {}", output_heatmap.display(), e))?;

    let exact_pct = (exact_matches as f64 / total_pixels as f64) * 100.0;
    let perceptual_pct = (perceptual_matches as f64 / total_pixels as f64) * 100.0;
    let mean_delta_e = sum_delta_e / (total_pixels as f64);

    Ok(ParityMetrics {
        width: w,
        height: h,
        total_pixels,
        exact_match_pixels: exact_matches,
        exact_match_percentage: exact_pct,
        perceptual_match_pixels: perceptual_matches,
        perceptual_match_percentage: perceptual_pct,
        ssim_score: ssim_res.ssim_score,
        mean_delta_e,
        max_delta_e,
        heatmap_path: output_heatmap.display().to_string(),
    })
}
