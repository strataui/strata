//! W3C CSS Grid Level 2 Named Grid Areas & Line Synthesis Tests.

use strata_layout::tree::LayoutNode;
use strata_style::cascade::CascadeSolver;
use strata_style::parser::StyleSheet;
use strata_style::selector::PseudoState;
use strata_style::types::{ComputedStyle, Dimension, Display, GridTrack};

#[test]
fn test_classic_holy_grail_named_grid_areas() {
    let container_style = ComputedStyle {
        display: Display::Grid,
        width: Dimension::Px(600.0),
        height: Dimension::Px(400.0),
        grid_template_columns: vec![GridTrack::Px(200.0), GridTrack::Px(400.0)],
        grid_template_rows: vec![
            GridTrack::Px(80.0),
            GridTrack::Px(260.0),
            GridTrack::Px(60.0),
        ],
        grid_template_areas: vec![
            vec!["header".to_string(), "header".to_string()],
            vec!["sidebar".to_string(), "content".to_string()],
            vec!["footer".to_string(), "footer".to_string()],
        ],
        ..Default::default()
    };

    let item_header = LayoutNode::new(ComputedStyle {
        grid_area: Some("header".to_string()),
        ..Default::default()
    });
    let item_sidebar = LayoutNode::new(ComputedStyle {
        grid_area: Some("sidebar".to_string()),
        ..Default::default()
    });
    let item_content = LayoutNode::new(ComputedStyle {
        grid_area: Some("content".to_string()),
        ..Default::default()
    });
    let item_footer = LayoutNode::new(ComputedStyle {
        grid_area: Some("footer".to_string()),
        ..Default::default()
    });

    let mut container = LayoutNode::with_children(
        container_style,
        vec![item_header, item_sidebar, item_content, item_footer],
    );

    container.compute_layout(600.0, 400.0);

    let header_geom = container.children[0].geometry.border_box;
    let sidebar_geom = container.children[1].geometry.border_box;
    let content_geom = container.children[2].geometry.border_box;
    let footer_geom = container.children[3].geometry.border_box;

    // Header spans both columns on row 1
    assert_eq!(header_geom.x, 0.0);
    assert_eq!(header_geom.y, 0.0);
    assert_eq!(header_geom.width, 600.0); // 200 + 400
    assert_eq!(header_geom.height, 80.0);

    // Sidebar on row 2, col 1
    assert_eq!(sidebar_geom.x, 0.0);
    assert_eq!(sidebar_geom.y, 80.0);
    assert_eq!(sidebar_geom.width, 200.0);
    assert_eq!(sidebar_geom.height, 260.0);

    // Content on row 2, col 2
    assert_eq!(content_geom.x, 200.0);
    assert_eq!(content_geom.y, 80.0);
    assert_eq!(content_geom.width, 400.0);
    assert_eq!(content_geom.height, 260.0);

    // Footer spans both columns on row 3
    assert_eq!(footer_geom.x, 0.0);
    assert_eq!(footer_geom.y, 340.0); // 80 + 260
    assert_eq!(footer_geom.width, 600.0);
    assert_eq!(footer_geom.height, 60.0);
}

#[test]
fn test_multi_row_and_multi_column_named_area_spanning() {
    let container_style = ComputedStyle {
        display: Display::Grid,
        width: Dimension::Px(900.0),
        height: Dimension::Px(500.0),
        grid_template_columns: vec![
            GridTrack::Px(300.0),
            GridTrack::Px(300.0),
            GridTrack::Px(300.0),
        ],
        grid_template_rows: vec![GridTrack::Px(250.0), GridTrack::Px(250.0)],
        grid_template_areas: vec![
            vec!["nav".to_string(), "main".to_string(), "main".to_string()],
            vec!["nav".to_string(), "main".to_string(), "main".to_string()],
        ],
        ..Default::default()
    };

    let item_nav = LayoutNode::new(ComputedStyle {
        grid_area: Some("nav".to_string()),
        ..Default::default()
    });
    let item_main = LayoutNode::new(ComputedStyle {
        grid_area: Some("main".to_string()),
        ..Default::default()
    });

    let mut container = LayoutNode::with_children(container_style, vec![item_nav, item_main]);

    container.compute_layout(900.0, 500.0);

    let nav_geom = container.children[0].geometry.border_box;
    let main_geom = container.children[1].geometry.border_box;

    // Nav spans row 1 to 2, column 1
    assert_eq!(nav_geom.x, 0.0);
    assert_eq!(nav_geom.y, 0.0);
    assert_eq!(nav_geom.width, 300.0);
    assert_eq!(nav_geom.height, 500.0); // spans 250 + 250

    // Main spans row 1 to 2, columns 2 and 3
    assert_eq!(main_geom.x, 300.0);
    assert_eq!(main_geom.y, 0.0);
    assert_eq!(main_geom.width, 600.0); // 300 + 300
    assert_eq!(main_geom.height, 500.0); // 250 + 250
}

#[test]
fn test_css_cascade_grid_template_areas_and_grid_area_parsing() {
    let css = r#"
        .layout-grid {
            display: grid;
            grid-template-areas: "head head" "side body" "foot foot";
            grid-template-columns: 200px 1fr;
        }
        .header-item {
            grid-area: head;
        }
        .shorthand-box {
            grid-area: 1 / 2 / 3 / 4;
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("valid CSS");
    let state = PseudoState::default();

    let style_grid =
        CascadeSolver::resolve("div", &["layout-grid"], None, None, &[&sheet], None, &state);

    assert_eq!(
        style_grid.grid_template_areas,
        vec![
            vec!["head".to_string(), "head".to_string()],
            vec!["side".to_string(), "body".to_string()],
            vec!["foot".to_string(), "foot".to_string()],
        ]
    );

    let style_item =
        CascadeSolver::resolve("div", &["header-item"], None, None, &[&sheet], None, &state);
    assert_eq!(style_item.grid_area, Some("head".to_string()));

    let style_shorthand = CascadeSolver::resolve(
        "div",
        &["shorthand-box"],
        None,
        None,
        &[&sheet],
        None,
        &state,
    );
    assert_eq!(style_shorthand.grid_row_start, Some(1));
    assert_eq!(style_shorthand.grid_column_start, Some(2));
    assert_eq!(style_shorthand.grid_row_end, Some(3));
    assert_eq!(style_shorthand.grid_column_end, Some(4));
}
