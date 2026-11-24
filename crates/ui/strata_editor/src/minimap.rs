//! High-FPS Canvas2D Minimap renderer emitting `Vec<CanvasCommand>` micro-bars,
//! active cursor markers, search match markers, diagnostic ticks, and viewport slider.

use serde::{Deserialize, Serialize};
use strata_layout::Rect;
use strata_render::canvas2d::CanvasCommand;

use crate::code_editor::CodeEditor;
use crate::theme::EditorTheme;

/// Visual & sampling configuration for the Canvas2D code minimap.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MinimapConfig {
    /// Width of the minimap canvas in CSS pixels.
    pub width: f32,
    /// Vertical pitch per rendered minimap row in CSS pixels.
    pub line_pitch: f32,
    /// Height of each syntax micro-bar in CSS pixels.
    pub bar_height: f32,
    /// Horizontal pixel scale per character column.
    pub char_width: f32,
    /// Maximum character columns rendered per row.
    pub max_cols: usize,
    /// Left horizontal padding inside the minimap canvas.
    pub padding_x: f32,
    /// Whether to draw the translucent viewport slider box.
    pub show_slider: bool,
    /// Whether to draw diagnostic error/warning markers along the right edge.
    pub show_diagnostics: bool,
    /// Whether to draw amber search match markers.
    pub show_search_matches: bool,
}

impl Default for MinimapConfig {
    fn default() -> Self {
        Self {
            width: 76.0,
            line_pitch: 3.0,
            bar_height: 2.0,
            char_width: 0.9,
            max_cols: 72,
            padding_x: 4.0,
            show_slider: true,
            show_diagnostics: true,
            show_search_matches: true,
        }
    }
}

/// Computed geometric projection metrics between document lines and minimap canvas space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapMetrics {
    pub total_lines: usize,
    pub max_rows: usize,
    /// Logical lines covered per single minimap row (`>= 1.0`).
    pub lines_per_row: f32,
    /// Top Y coordinate of the viewport slider inside the minimap.
    pub slider_y: f32,
    /// Height of the viewport slider inside the minimap.
    pub slider_height: f32,
    /// Total rendered content height of the minimap.
    pub rendered_height: f32,
}

/// Computes downsampling stride and viewport slider bounds for any document size (1 to 100k+ lines).
pub fn compute_minimap_metrics(
    total_lines: usize,
    scroll_top: f32,
    viewport_height: f32,
    editor_line_height: f32,
    minimap_height: f32,
    config: &MinimapConfig,
) -> MinimapMetrics {
    let total = total_lines.max(1);
    let pitch = config.line_pitch.max(1.0);
    let mh = minimap_height.max(pitch);
    let max_rows = (mh / pitch).floor().max(1.0) as usize;
    let lines_per_row = if total > max_rows {
        total as f32 / max_rows as f32
    } else {
        1.0
    };
    let rendered_rows = (total as f32 / lines_per_row).ceil().min(max_rows as f32);
    let rendered_height = (rendered_rows * pitch).min(mh);

    let eff_lh = editor_line_height.max(1.0);
    let doc_height = (total as f32 * eff_lh).max(eff_lh);
    let visible_frac = (viewport_height / doc_height).clamp(0.0, 1.0);
    let max_scroll = (doc_height - viewport_height).max(0.0);
    let scroll_frac = if max_scroll > 0.0 {
        (scroll_top / max_scroll).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let slider_height = (visible_frac * rendered_height).clamp(16.0_f32.min(mh), mh);
    let slider_travel = (rendered_height - slider_height).max(0.0);
    let slider_y = scroll_frac * slider_travel;

    MinimapMetrics {
        total_lines: total,
        max_rows,
        lines_per_row,
        slider_y,
        slider_height,
        rendered_height,
    }
}

/// Converts a pointer Y coordinate inside the minimap track into the corresponding
/// `scroll_top` offset in editor pixels (centered on the viewport slider).
pub fn minimap_y_to_scroll_top(
    pointer_y: f32,
    minimap_height: f32,
    total_lines: usize,
    viewport_height: f32,
    editor_line_height: f32,
    config: &MinimapConfig,
) -> f32 {
    let eff_lh = editor_line_height.max(1.0);
    let total = total_lines.max(1);
    let doc_height = total as f32 * eff_lh;
    let max_scroll = (doc_height - viewport_height).max(0.0);
    if max_scroll <= 0.0 {
        return 0.0;
    }

    let metrics = compute_minimap_metrics(
        total,
        0.0,
        viewport_height,
        eff_lh,
        minimap_height,
        config,
    );
    let travel = (metrics.rendered_height - metrics.slider_height).max(1.0);
    let target_slider_y = (pointer_y - metrics.slider_height * 0.5).clamp(0.0, travel);
    (target_slider_y / travel) * max_scroll
}

/// Emits a bounded, high-FPS `Vec<CanvasCommand>` display list for the Canvas2D minimap.
///
/// Even on a 100,000-line buffer this samples at most `ceil(minimap_height / line_pitch)` rows
/// (~200–300 rows) with zero DOM node overhead.
pub fn render_minimap_canvas_commands(
    editor: &CodeEditor,
    scroll_top: f32,
    viewport_height: f32,
    editor_line_height: f32,
    minimap_height: f32,
    config: &MinimapConfig,
    theme: &EditorTheme,
) -> Vec<CanvasCommand> {
    let w = config.width.max(24.0);
    let h = minimap_height.max(24.0);
    let total_lines = editor.line_count().max(1);
    let metrics = compute_minimap_metrics(
        total_lines,
        scroll_top,
        viewport_height,
        editor_line_height,
        h,
        config,
    );

    let mut commands = Vec::with_capacity(metrics.max_rows * 3 + 32);

    // 1. Background fill
    commands.push(CanvasCommand::FillRect {
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: w,
            height: h,
        },
        color: theme.minimap_bg_rgba(),
    });

    // 2. Downsampled color-coded syntax micro-bars
    let usable_w = (w - config.padding_x * 2.0 - 6.0).max(8.0);
    let bar_h = config.bar_height.clamp(1.0, config.line_pitch);
    let row_count = ((total_lines as f32) / metrics.lines_per_row)
        .ceil()
        .min(metrics.max_rows as f32) as usize;

    for row in 0..row_count {
        let logical_line = ((row as f32 * metrics.lines_per_row).floor() as usize)
            .min(total_lines.saturating_sub(1));
        let Some(line_text) = editor.buffer().line(logical_line) else {
            continue;
        };
        if line_text.trim().is_empty() {
            continue;
        }
        let y = row as f32 * config.line_pitch;
        let tokens = editor.highlighter().highlight_line_ast(logical_line, line_text);

        if tokens.is_empty() {
            let leading = line_text.chars().take_while(|c| c.is_whitespace()).count();
            let content_len = line_text.trim().chars().count().min(config.max_cols);
            let x = config.padding_x + (leading.min(config.max_cols) as f32) * config.char_width;
            let bar_w = ((content_len as f32) * config.char_width)
                .clamp(2.0, (usable_w - (x - config.padding_x)).max(2.0));
            commands.push(CanvasCommand::FillRect {
                bounds: Rect {
                    x,
                    y,
                    width: bar_w,
                    height: bar_h,
                },
                color: theme.minimap_token_rgba(crate::syntax::TokenType::Identifier),
            });
            continue;
        }

        // Render compact syntax-colored bars (cap at 6 spans per row to keep command count tiny).
        for tok in tokens.iter().take(6) {
            let start_b = tok.start_col.min(line_text.len());
            if !line_text.is_char_boundary(start_b) {
                continue;
            }
            let char_col = line_text[..start_b].chars().count();
            if char_col >= config.max_cols {
                break;
            }
            let tok_chars = tok.text.chars().count().min(config.max_cols - char_col);
            if tok_chars == 0 {
                continue;
            }
            let x = config.padding_x + (char_col as f32) * config.char_width;
            let max_w = (config.padding_x + usable_w - x).max(0.0);
            if max_w < 1.0 {
                break;
            }
            let bar_w = ((tok_chars as f32) * config.char_width).clamp(1.5, max_w);
            commands.push(CanvasCommand::FillRect {
                bounds: Rect {
                    x,
                    y,
                    width: bar_w,
                    height: bar_h,
                },
                color: theme.minimap_token_rgba(tok.token_type),
            });
        }
    }

    // 3. Search match markers (amber ticks)
    if config.show_search_matches && !editor.search().matches.is_empty() {
        let search_color = theme.minimap_search_rgba();
        let mut last_y = -10.0f32;
        for m in &editor.search().matches {
            let y = ((m.start.line as f32) / metrics.lines_per_row) * config.line_pitch;
            if (y - last_y).abs() < 2.0 {
                continue;
            }
            last_y = y;
            commands.push(CanvasCommand::FillRect {
                bounds: Rect {
                    x: w - 10.0,
                    y: y.min(h - 2.0),
                    width: 5.0,
                    height: 2.0,
                },
                color: search_color,
            });
        }
    }

    // 4. Diagnostic severity ticks along the right edge
    if config.show_diagnostics {
        let mut last_y = -10.0f32;
        for diag in editor.diagnostics().all_sorted() {
            let y = ((diag.start.line as f32) / metrics.lines_per_row) * config.line_pitch;
            if (y - last_y).abs() < 2.0 {
                continue;
            }
            last_y = y;
            commands.push(CanvasCommand::FillRect {
                bounds: Rect {
                    x: w - 5.0,
                    y: y.min(h - 3.0),
                    width: 4.0,
                    height: 3.0,
                },
                color: theme.diagnostic_rgba(diag.severity),
            });
        }
    }

    // 5. Active primary cursor line indicator
    let cursor_line = editor.cursors().primary().focus.line;
    let cursor_y = (((cursor_line as f32) / metrics.lines_per_row) * config.line_pitch)
        .min((h - 2.0).max(0.0));
    commands.push(CanvasCommand::FillRect {
        bounds: Rect {
            x: 0.0,
            y: cursor_y,
            width: w,
            height: 2.0,
        },
        color: theme.minimap_cursor_rgba(),
    });

    // 6. Translucent viewport slider window
    if config.show_slider {
        let slider_bounds = Rect {
            x: 0.0,
            y: metrics.slider_y,
            width: w,
            height: metrics.slider_height,
        };
        commands.push(CanvasCommand::FillRect {
            bounds: slider_bounds,
            color: theme.minimap_slider_rgba(),
        });
        commands.push(CanvasCommand::StrokeRect {
            bounds: slider_bounds,
            color: theme.minimap_slider_border_rgba(),
            line_width: 1.0,
        });
    }

    commands
}
