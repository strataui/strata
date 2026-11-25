//! Phase 7 Vector PDF 1.7 Exporter verification suite.

use strata_layout::{FillRule, ImageResource, Rect, StrokeLineCap, StrokeLineJoin};
use strata_primitives::geom::EdgeInsets;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::pdf::{export_display_list_to_pdf, PdfExportOptions, PdfPageSize};
use strata_style::types::{BorderRadius, Color, FontWeight};
use strata_style::TextAlign;

fn build_rich_display_list() -> DisplayList {
    let mut dl = DisplayList::new();
    dl.commands.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 800.0, 600.0),
        color: Color::rgba(15, 23, 42, 255),
    });
    dl.commands.push(DrawCommand::DrawRoundedRect {
        bounds: Rect::new(40.0, 40.0, 320.0, 140.0),
        color: Color::rgba(30, 41, 59, 230),
        border_radius: BorderRadius::all(12.0),
        border_width: EdgeInsets::all(2.0),
        border_color: Some(Color::rgba(56, 189, 248, 255)),
    });
    dl.commands.push(DrawCommand::DrawText {
        origin: (64.0, 72.0),
        bounds_width: Some(260.0),
        bounds_height: Some(32.0),
        text_align: TextAlign::Left,
        text: "Strata Mission Telemetry (PDF 1.7)".into(),
        font_size: 18.0,
        line_height: 24.0,
        font_weight: FontWeight::Bold,
        color: Color::rgba(248, 250, 252, 255),
        is_monospace: false,
        font_family: strata_primitives::AtomId(0),
    });
    dl.commands.push(DrawCommand::DrawSvgPath {
        bounds: Rect::new(400.0, 40.0, 120.0, 120.0),
        path_data: "M 10 10 L 90 10 L 50 90 Z".into(),
        stroke_color: Some(Color::rgba(34, 197, 94, 255)),
        fill_color: Some(Color::rgba(34, 197, 94, 90)),
        stroke_width: 2.0,
        stroke_linecap: StrokeLineCap::Round,
        stroke_linejoin: StrokeLineJoin::Round,
        stroke_dasharray: Vec::new(),
        fill_rule: FillRule::NonZero,
        view_box: (100.0, 100.0),
    });
    dl.commands.push(DrawCommand::DrawImage {
        bounds: Rect::new(40.0, 220.0, 64.0, 64.0),
        image: ImageResource {
            width: 2,
            height: 2,
            data: std::sync::Arc::new(vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ]),
        },
    });
    dl
}

#[test]
fn test_vector_pdf_17_structure_exact_xref_offsets_and_operators() {
    let dl = build_rich_display_list();
    let opts = PdfExportOptions {
        page_size: PdfPageSize::A4,
        compress_streams: false,
        title: "Mission Report".into(),
        author: "Strata".into(),
        ..Default::default()
    };

    let pdf = export_display_list_to_pdf(&dl, 800.0, 600.0, &opts);
    assert!(pdf.starts_with(b"%PDF-1.7\n"));
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.ends_with("%%EOF\n"));

    // Verify vector operators for rounded rect cubic Beziers, text, SVG path, alpha ExtGState, & image XObject
    assert!(text.contains(" c\n"), "Expected cubic Bezier corner operators");
    assert!(text.contains("(Strata Mission Telemetry \\(PDF 1.7\\)) Tj"));
    assert!(text.contains("/Im1 Do"));
    assert!(text.contains("/ExtGState"));
    assert!(text.contains("/Subtype /Image"));

    // Parse startxref from raw bytes and mathematically verify EVERY xref byte offset points to `N 0 obj`!
    let marker = b"startxref\n";
    let startxref_pos = pdf
        .windows(marker.len())
        .rposition(|w| w == marker)
        .expect("missing startxref");
    let after_marker = std::str::from_utf8(&pdf[startxref_pos + marker.len()..]).unwrap();
    let xref_offset: usize = after_marker
        .lines()
        .next()
        .unwrap()
        .trim()
        .parse()
        .expect("invalid startxref offset");
    assert_eq!(&pdf[xref_offset..xref_offset + 5], b"xref\n");

    let xref_ascii = std::str::from_utf8(&pdf[xref_offset..startxref_pos]).unwrap();
    let xref_lines: Vec<&str> = xref_ascii.lines().collect();
    // Skip `xref`, `0 N`, and entry 0 (`0000000000 65535 f`)
    for (idx, line) in xref_lines.iter().skip(3).take_while(|l| !l.starts_with("trailer")).enumerate() {
        let obj_id = idx + 1;
        let off: usize = line[0..10].parse().expect("invalid xref offset");
        let expected_prefix = format!("{obj_id} 0 obj\n");
        assert_eq!(
            &pdf[off..off + expected_prefix.len()],
            expected_prefix.as_bytes(),
            "xref offset mismatch for object {obj_id}"
        );
    }
}

#[test]
fn test_vector_pdf_flate_compressed_streams_smaller_and_valid() {
    let dl = build_rich_display_list();
    let raw_pdf = export_display_list_to_pdf(
        &dl,
        800.0,
        600.0,
        &PdfExportOptions {
            compress_streams: false,
            ..Default::default()
        },
    );
    let zip_pdf = export_display_list_to_pdf(
        &dl,
        800.0,
        600.0,
        &PdfExportOptions {
            compress_streams: true,
            ..Default::default()
        },
    );
    assert!(zip_pdf.starts_with(b"%PDF-1.7\n"));
    let text = String::from_utf8_lossy(&zip_pdf);
    assert!(text.contains("/Filter /FlateDecode"));
    assert!(zip_pdf.len() < raw_pdf.len());
}
