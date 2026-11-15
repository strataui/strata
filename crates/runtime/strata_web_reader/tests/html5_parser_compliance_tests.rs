//! WHATWG HTML5 Tree Construction Compliance Integration Tests.
//!
//! Validates:
//! 1. Implied `<p>` closing on consecutive `<p>` tags and block-level boundaries
//! 2. Implied `<li>` closing on subsequent list items within list scope
//! 3. Implied `<dt>` and `<dd>` closing in definition lists
//! 4. Table structure auto-generation (`tbody`, `tr`) and cell/row implied closing
//! 5. Foster parenting for misplaced text and non-table elements inside `<table>`
//! 6. Foreign SVG leaf elements treated as self-closing void elements
//! 7. Implied `<option>` closing in `<select>` containers

use strata_web_reader::prelude::*;

#[test]
fn test_implied_p_tag_closing_on_consecutive_paragraphs() {
    let html = "<p>First paragraph<p>Second paragraph<p>Third paragraph";
    let nodes = HtmlParser::parse(html);

    assert_eq!(
        nodes.len(),
        3,
        "Consecutive <p> tags must not nest; must produce 3 sibling paragraphs"
    );
    assert_eq!(nodes[0].tag(), Some("p"));
    assert_eq!(nodes[0].text_content().trim(), "First paragraph");
    assert_eq!(nodes[1].tag(), Some("p"));
    assert_eq!(nodes[1].text_content().trim(), "Second paragraph");
    assert_eq!(nodes[2].tag(), Some("p"));
    assert_eq!(nodes[2].text_content().trim(), "Third paragraph");
}

#[test]
fn test_implied_p_tag_closing_on_block_elements() {
    let html = "<p>Introduction text<div>Inside block container</div><p>Subsequent text";
    let nodes = HtmlParser::parse(html);

    assert_eq!(
        nodes.len(),
        3,
        "Block container <div> must implicitly close the preceding <p>"
    );
    assert_eq!(nodes[0].tag(), Some("p"));
    assert_eq!(nodes[0].text_content().trim(), "Introduction text");
    assert_eq!(nodes[1].tag(), Some("div"));
    assert_eq!(nodes[1].text_content().trim(), "Inside block container");
    assert_eq!(nodes[2].tag(), Some("p"));
    assert_eq!(nodes[2].text_content().trim(), "Subsequent text");
}

#[test]
fn test_implied_p_tag_closing_on_headings_and_lists() {
    let html = "<p>Before heading<h2>Architecture</h2><p>Before list<ul><li>Item 1</li></ul>";
    let nodes = HtmlParser::parse(html);

    assert_eq!(
        nodes.len(),
        4,
        "Headings and lists must close the open paragraph"
    );
    assert_eq!(nodes[0].tag(), Some("p"));
    assert_eq!(nodes[1].tag(), Some("h2"));
    assert_eq!(nodes[2].tag(), Some("p"));
    assert_eq!(nodes[3].tag(), Some("ul"));
}

#[test]
fn test_implied_li_closing_within_list_scope() {
    let html = "<ul><li>Alpha<li>Beta<li>Gamma</ul>";
    let nodes = HtmlParser::parse(html);

    assert_eq!(nodes.len(), 1, "Must parse single <ul> root");
    let ul = &nodes[0];
    assert_eq!(ul.tag(), Some("ul"));

    let items = ul.find_all("li");
    assert_eq!(
        items.len(),
        3,
        "Subsequent <li> must close preceding <li>; must not nest"
    );
    assert_eq!(items[0].text_content().trim(), "Alpha");
    assert_eq!(items[1].text_content().trim(), "Beta");
    assert_eq!(items[2].text_content().trim(), "Gamma");
}

#[test]
fn test_implied_dt_dd_closing_in_dl() {
    let html = "<dl><dt>CPU<dd>Central Processing Unit<dt>GPU<dd>Graphics Processing Unit</dl>";
    let nodes = HtmlParser::parse(html);

    assert_eq!(nodes.len(), 1);
    let dl = &nodes[0];
    assert_eq!(dl.tag(), Some("dl"));

    if let SemanticNode::Element { children, .. } = dl {
        assert_eq!(children.len(), 4, "Must produce 4 sibling children in <dl>");
        assert_eq!(children[0].tag(), Some("dt"));
        assert_eq!(children[0].text_content().trim(), "CPU");
        assert_eq!(children[1].tag(), Some("dd"));
        assert_eq!(children[1].text_content().trim(), "Central Processing Unit");
        assert_eq!(children[2].tag(), Some("dt"));
        assert_eq!(children[2].text_content().trim(), "GPU");
        assert_eq!(children[3].tag(), Some("dd"));
        assert_eq!(
            children[3].text_content().trim(),
            "Graphics Processing Unit"
        );
    } else {
        panic!("Expected dl element");
    }
}

#[test]
fn test_table_structure_and_cell_auto_closing() {
    let html = "<table><tr><td>Alpha<td>Beta<tr><td>Gamma<td>Delta</table>";
    let nodes = HtmlParser::parse(html);

    assert_eq!(nodes.len(), 1);
    let table = &nodes[0];
    assert_eq!(table.tag(), Some("table"));

    let rows = table.find_all("tr");
    assert_eq!(rows.len(), 2, "Must auto-close row 1 on row 2 start");

    let row1_cells = rows[0].find_all("td");
    assert_eq!(row1_cells.len(), 2, "Row 1 must contain 2 cells");
    assert_eq!(row1_cells[0].text_content().trim(), "Alpha");
    assert_eq!(row1_cells[1].text_content().trim(), "Beta");

    let row2_cells = rows[1].find_all("td");
    assert_eq!(row2_cells.len(), 2, "Row 2 must contain 2 cells");
    assert_eq!(row2_cells[0].text_content().trim(), "Gamma");
    assert_eq!(row2_cells[1].text_content().trim(), "Delta");
}

#[test]
fn test_table_foster_parenting_misplaced_content() {
    let html = "<div><table>Misplaced Leading Text<tr><td>Cell Content</td></tr></table></div>";
    let nodes = HtmlParser::parse(html);

    assert_eq!(nodes.len(), 1);
    let div = &nodes[0];
    assert_eq!(div.tag(), Some("div"));

    if let SemanticNode::Element { children, .. } = div {
        // According to WHATWG §13.2.6.1 Foster Parenting:
        // Misplaced text inside <table> is inserted into table's parent before the table
        assert_eq!(
            children.len(),
            2,
            "Foster parenting must move misplaced text into parent container before table"
        );
        assert_eq!(children[0].text_content().trim(), "Misplaced Leading Text");
        assert_eq!(children[1].tag(), Some("table"));
    } else {
        panic!("Expected div element");
    }
}

#[test]
fn test_foreign_svg_leaf_self_closing() {
    let html = r#"
        <svg viewBox="0 0 100 100">
            <path d="M10 10 L90 90" />
            <circle cx="50" cy="50" r="20" />
            <rect x="0" y="0" width="10" height="10" />
        </svg>
    "#;
    let nodes = HtmlParser::parse(html);

    assert_eq!(nodes.len(), 1);
    let svg = &nodes[0];
    assert_eq!(svg.tag(), Some("svg"));

    if let SemanticNode::Element { children, .. } = svg {
        assert_eq!(
            children.len(),
            3,
            "SVG self-closing leaf shapes must be siblings, not nested"
        );
        assert_eq!(children[0].tag(), Some("path"));
        assert_eq!(children[1].tag(), Some("circle"));
        assert_eq!(children[2].tag(), Some("rect"));
    } else {
        panic!("Expected svg element");
    }
}

#[test]
fn test_select_option_auto_closing() {
    let html = "<select><option value='1'>One<option value='2'>Two<option value='3'>Three</select>";
    let nodes = HtmlParser::parse(html);

    assert_eq!(nodes.len(), 1);
    let select = &nodes[0];
    assert_eq!(select.tag(), Some("select"));

    let options = select.find_all("option");
    assert_eq!(
        options.len(),
        3,
        "Subsequent <option> must close preceding <option>"
    );
    assert_eq!(options[0].text_content().trim(), "One");
    assert_eq!(options[1].text_content().trim(), "Two");
    assert_eq!(options[2].text_content().trim(), "Three");
}

#[test]
fn test_deeply_nested_html_tree_depth_bound() {
    let mut html = String::new();
    for _ in 0..1000 {
        html.push_str("<div>");
    }
    html.push_str("Deep Payload");

    // Must safely parse without thread stack exhaustion or uncontrolled heap tree growth
    let nodes = HtmlParser::parse(&html);
    assert!(!nodes.is_empty(), "Parser must complete and produce nodes");

    fn compute_depth(node: &SemanticNode) -> usize {
        match node {
            SemanticNode::Element { children, .. } => {
                1 + children.iter().map(compute_depth).max().unwrap_or(0)
            }
            _ => 1,
        }
    }
    let max_depth = nodes.iter().map(compute_depth).max().unwrap_or(0);
    assert!(
        max_depth <= MAX_TREE_DEPTH + 2,
        "Tree depth must be bounded by MAX_TREE_DEPTH ({MAX_TREE_DEPTH}), got {max_depth}"
    );
}
