use strata_virtual::prelude::*;

#[test]
fn test_virtual_one_million_items_constant_memory() {
    let handle = use_virtualizer(VirtualOptions {
        count: 1_000_000,
        estimate_size: 40.0,
        overscan: 5,
        viewport_size: 800.0,
    });

    // Total virtual scroll height for 1M items: 40,000,000 px
    assert_eq!(handle.total_size.get(), 40_000_000.0);

    // Initial window: viewport fits 800/40 = 20 items + 5 overscan
    let items = handle.virtual_items.get();
    assert!(items.len() <= 30);
    assert_eq!(items[0].index, 0);

    // Jump to middle: item 500,000 (offset = 20,000,000 px)
    handle.on_scroll(20_000_000.0);
    let mid_items = handle.virtual_items.get();
    assert!(mid_items.len() <= 35);
    assert!(mid_items[0].index >= 499_990);
    assert!(mid_items.last().unwrap().index <= 500_030);
}

#[test]
fn test_2d_virtual_grid_spreadsheet() {
    let grid_opts = VirtualGridOptions {
        rows: 50_000,
        cols: 1_000,
        estimate_row_height: 25.0,
        estimate_col_width: 120.0,
        overscan: 2,
        viewport_width: 1200.0,
        viewport_height: 750.0,
    };

    let grid = VirtualGrid::new(grid_opts);
    assert_eq!(grid.total_height(), 1_250_000.0);
    assert_eq!(grid.total_width(), 120_000.0);

    let cells = grid.get_virtual_cells();
    assert!(!cells.is_empty());
    // Cell count is strictly bounded by (rows_in_view * cols_in_view)
    assert!(cells.len() < 1000);
}
