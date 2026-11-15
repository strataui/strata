//! End-to-end RSX + AppRuntime regression tests for:
//! 1. Nested percentage-sized flex/grid/table split panes (no p% * p% squaring)
//! 2. Column flex-1 items propagating definite grown height without h-full
//! 3. Auto-width .truncate inline/text elements across subpixel x offsets (no false ellipsis)
//! 4. Display::Flex preservation on justify-between rows without w-full

use strata::prelude::*;
use strata_layout::LayoutNode;
use strata_style::types::Display;

fn collect_rendered_text(node: &LayoutNode, out: &mut Vec<String>) {
    for line in &node.line_boxes {
        for frag in &line.text_fragments {
            out.push(frag.text.clone());
        }
    }
    for child in &node.children {
        collect_rendered_text(child, out);
    }
}

#[test]
fn test_e2e_nested_percentage_splits_and_column_flex1_without_h_full() {
    let view = rsx! {
        <div class="flex flex-col w-full h-full">
            <div class="h-[30px] w-full shrink-0">"Top Bar"</div>
            <div class="flex-1 w-full flex flex-row">
                <div style="width: 24%; height: 100%;" class="flex flex-col">
                    <div class="flex items-center justify-between h-[36px] px-2">
                        <span class="truncate">"Workspace Explorer"</span>
                        <span>"+"</span>
                    </div>
                    <div class="flex-1 w-full">"Sidebar Tree"</div>
                </div>
                <div class="w-[4px] h-full shrink-0"></div>
                <div style="width: 76%; height: 100%;" class="flex flex-row">
                    <div style="width: 70%; height: 100%;" class="flex flex-col">
                        <div class="flex-1 w-full">"Main Viewport"</div>
                    </div>
                    <div class="w-[4px] h-full shrink-0"></div>
                    <div style="width: 30%; height: 100%;" class="flex flex-col">
                        <div class="flex-1 w-full">"Inspector"</div>
                    </div>
                </div>
            </div>
        </div>
    };

    let runtime = AppRuntime::new(view, 1440.0, 900.0, 1.0);

    let main_row = &runtime.layout_root.children[1];
    assert!(
        (main_row.geometry.border_box.height - 870.0).abs() <= 1.0,
        "flex-1 main row height {} != 870",
        main_row.geometry.border_box.height
    );

    // Bug 1: 24% left pane (~345px) lays out its children across the full ~345px (not 82.7px)
    let left_pane = &main_row.children[0];
    let sidebar_header = &left_pane.children[0];
    let sidebar_tree = &left_pane.children[1];
    assert!((left_pane.geometry.border_box.width - 345.0).abs() <= 2.0);
    assert!(
        (sidebar_header.geometry.border_box.width - left_pane.geometry.border_box.width).abs() <= 1.0,
        "sidebar header width {} != left pane width {}",
        sidebar_header.geometry.border_box.width,
        left_pane.geometry.border_box.width
    );
    assert_eq!(sidebar_header.style.display, Display::Flex);
    assert!(
        (sidebar_tree.geometry.border_box.width - left_pane.geometry.border_box.width).abs() <= 1.0
    );
    assert!(
        (sidebar_tree.geometry.border_box.height - 834.0).abs() <= 2.0,
        "sidebar flex-1 tree height {} != 834",
        sidebar_tree.geometry.border_box.height
    );

    // Bug 4: justify-between header row without w-full places "+" near the right edge of the 345px pane
    let plus_badge = &sidebar_header.children[1];
    assert!(
        plus_badge.geometry.border_box.x > 300.0,
        "justify-between right child x {} collapsed instead of reaching right edge",
        plus_badge.geometry.border_box.x
    );

    // Bug 1 (3 levels): 76% -> 70% (~764px) and 76% -> 30% (~327px) fill their respective panes
    let right_split = &main_row.children[2];
    let center_pane = &right_split.children[0];
    let inspector_pane = &right_split.children[2];
    assert!((center_pane.geometry.border_box.width - 764.0).abs() <= 3.0);
    assert!(
        (center_pane.children[0].geometry.border_box.width - center_pane.geometry.border_box.width)
            .abs()
            <= 1.0
    );
    assert!((center_pane.children[0].geometry.border_box.height - 870.0).abs() <= 1.0);

    assert!((inspector_pane.geometry.border_box.width - 327.0).abs() <= 3.0);
    assert!(
        (inspector_pane.children[0].geometry.border_box.width
            - inspector_pane.geometry.border_box.width)
            .abs()
            <= 1.0
    );
    assert!((inspector_pane.children[0].geometry.border_box.height - 870.0).abs() <= 1.0);
}

#[test]
fn test_e2e_auto_width_truncate_labels_never_false_truncate_across_subpixel_offsets() {
    // Test across 10 fractional left-spacer widths (10.0, 10.1, ..., 10.9) so snap_to_pixel()
    // exercises every possible (x + width).round() - x.round() rounding boundary.
    for step in 0..10 {
        let spacer_px = 10.0 + (step as f32) * 0.1;
        let spacer_style = format!("width: {:.1}px; height: 16px;", spacer_px);

        let view = rsx! {
            <div class="flex flex-row items-center w-[800px] h-[40px]">
                <div style={spacer_style}></div>
                <span class="truncate">"Sentinel-4"</span>
                <span class="truncate ml-2">"Workspace Explorer"</span>
                <span class="truncate ml-2">"Select..."</span>
            </div>
        };

        let runtime = AppRuntime::new(view, 800.0, 40.0, 1.0);
        let mut fragments = Vec::new();
        collect_rendered_text(&runtime.layout_root, &mut fragments);

        for frag in &fragments {
            assert!(
                !frag.contains('\u{2026}'),
                "False-positive ellipsis truncation at spacer {:.1}px: {:?}",
                spacer_px,
                fragments
            );
        }
        assert!(fragments.iter().any(|s| s == "Sentinel-4"));
        assert!(fragments.iter().any(|s| s == "Workspace Explorer"));
        assert!(fragments.iter().any(|s| s == "Select..."));
    }
}
