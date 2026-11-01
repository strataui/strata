//! 2D Virtual Grid Engine for High-Scale Spreadsheets and Data Grids.

use crate::virtualizer::{VirtualOptions, Virtualizer};

/// A single virtual cell within a 2D virtual grid.
#[derive(Debug, Clone, PartialEq)]
pub struct VirtualGridCell {
    pub row: usize,
    pub col: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Configuration for 2D Grid Virtualization.
#[derive(Debug, Clone)]
pub struct VirtualGridOptions {
    pub rows: usize,
    pub cols: usize,
    pub estimate_row_height: f32,
    pub estimate_col_width: f32,
    pub overscan: usize,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// 2D Grid Virtualizer coordinating row and column slicing.
#[derive(Debug, Clone)]
pub struct VirtualGrid {
    row_virtualizer: Virtualizer,
    col_virtualizer: Virtualizer,
}

impl VirtualGrid {
    pub fn new(options: VirtualGridOptions) -> Self {
        let row_opts = VirtualOptions {
            count: options.rows,
            estimate_size: options.estimate_row_height,
            overscan: options.overscan,
            viewport_size: options.viewport_height,
        };

        let col_opts = VirtualOptions {
            count: options.cols,
            estimate_size: options.estimate_col_width,
            overscan: options.overscan,
            viewport_size: options.viewport_width,
        };

        Self {
            row_virtualizer: Virtualizer::new(row_opts),
            col_virtualizer: Virtualizer::new(col_opts),
        }
    }

    /// Update 2D scroll coordinates.
    pub fn set_scroll(&mut self, scroll_x: f32, scroll_y: f32) {
        self.col_virtualizer.set_scroll_offset(scroll_x);
        self.row_virtualizer.set_scroll_offset(scroll_y);
    }

    /// Total virtual width of all columns.
    pub fn total_width(&self) -> f32 {
        self.col_virtualizer.total_size()
    }

    /// Total virtual height of all rows.
    pub fn total_height(&self) -> f32 {
        self.row_virtualizer.total_size()
    }

    /// Visible virtual cells in the current viewport window.
    pub fn get_virtual_cells(&self) -> Vec<VirtualGridCell> {
        let row_items = self.row_virtualizer.get_virtual_items();
        let col_items = self.col_virtualizer.get_virtual_items();

        let mut cells = Vec::with_capacity(row_items.len() * col_items.len());

        for r in &row_items {
            for c in &col_items {
                cells.push(VirtualGridCell {
                    row: r.index,
                    col: c.index,
                    x: c.start,
                    y: r.start,
                    width: c.size,
                    height: r.size,
                });
            }
        }

        cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_grid_matrix_slicing() {
        let opts = VirtualGridOptions {
            rows: 10_000,
            cols: 500,
            estimate_row_height: 30.0,
            estimate_col_width: 100.0,
            overscan: 1,
            viewport_width: 500.0,
            viewport_height: 300.0,
        };

        let mut grid = VirtualGrid::new(opts);
        assert_eq!(grid.total_height(), 300_000.0);
        assert_eq!(grid.total_width(), 50_000.0);

        let cells = grid.get_virtual_cells();
        assert!(!cells.is_empty());
        assert_eq!(cells[0].row, 0);
        assert_eq!(cells[0].col, 0);

        // Scroll diagonally
        grid.set_scroll(1000.0, 3000.0);
        let scrolled_cells = grid.get_virtual_cells();
        assert!(!scrolled_cells.is_empty());
        assert!(scrolled_cells[0].row >= 90);
        assert!(scrolled_cells[0].col >= 9);
    }
}
