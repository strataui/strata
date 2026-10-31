//! Integration and Unit Tests for Strata Language Server (strata-lsp).

use strata_lsp::*;

#[test]
fn test_jsonrpc_framing_and_parsing() {
    let header = "Content-Length: 42\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n";
    assert_eq!(parse_content_length(header), Some(42));

    let payload = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}";
    let framed = encode_framed_message(payload);
    assert!(framed.starts_with("Content-Length: "));
    assert!(framed.contains("\r\n\r\n"));

    let msg = JsonRpcMessage::parse(payload).expect("Failed to parse JSON-RPC request");
    match msg {
        JsonRpcMessage::Request { id, method, .. } => {
            assert_eq!(id, 1);
            assert_eq!(method, "initialize");
        }
        _ => panic!("Expected Request variant"),
    }
}

#[test]
fn test_rsx_ast_parser_and_source_map() {
    let source = "<div class=\"flex p-4\" id=\"main\">\n    <span class=\"text-white\">{\"Hello Strata\"}</span>\n</div>";
    let doc = RsxDocument::parse(source);

    assert_eq!(doc.errors.len(), 0, "Balanced RSX must have 0 parse errors");
    assert_eq!(doc.nodes.len(), 1);

    if let RsxNode::Element { name, attributes, children, .. } = &doc.nodes[0] {
        assert_eq!(name, "div");
        assert_eq!(attributes.len(), 2);
        assert_eq!(attributes[0].name, "class");
        assert_eq!(attributes[0].value.as_deref(), Some("flex p-4"));
        assert_eq!(attributes[1].name, "id");
        assert_eq!(attributes[1].value.as_deref(), Some("main"));

        assert_eq!(children.len(), 1);
        if let RsxNode::Element { name: child_name, .. } = &children[0] {
            assert_eq!(child_name, "span");
        } else {
            panic!("Expected child Element span");
        }
    } else {
        panic!("Expected root Element node");
    }

    // Verify SourceMap line/character coordinate translation
    let pos_line0 = doc.source_map.offset_to_position(5);
    assert_eq!(pos_line0.line, 0);
    assert_eq!(pos_line0.character, 5);

    let offset_line1 = source.find("span").unwrap();
    let pos_line1 = doc.source_map.offset_to_position(offset_line1);
    assert_eq!(pos_line1.line, 1);
}

#[test]
fn test_diagnostics_unclosed_and_mismatched_tags() {
    // 1. Mismatched tag test
    let mismatched = "<div class=\"box\"></span>";
    let doc_mismatch = RsxDocument::parse(mismatched);
    let diags_mismatch = validate_document(&doc_mismatch);
    assert!(!diags_mismatch.is_empty(), "Must emit diagnostic for mismatched closing tag");
    assert!(diags_mismatch.iter().any(|d| d.message.contains("Mismatched closing tag")));

    // 2. Unclosed tag test
    let unclosed = "<div><button>Click";
    let doc_unclosed = RsxDocument::parse(unclosed);
    let diags_unclosed = validate_document(&doc_unclosed);
    assert!(!diags_unclosed.is_empty(), "Must emit diagnostic for unclosed tag");

    // 3. Duplicate attribute test
    let duplicate_attr = "<div class=\"p-4\" class=\"p-8\"></div>";
    let doc_dup = RsxDocument::parse(duplicate_attr);
    let diags_dup = validate_document(&doc_dup);
    assert!(diags_dup.iter().any(|d| d.message.contains("Duplicate attribute 'class'")));
}

#[test]
fn test_tailwind_autocomplete_and_hover() {
    // Completion test inside class attribute
    let doc = RsxDocument::parse("<div class=\"p-");
    let pos = Position::new(0, 14); // after "p-"
    let completions = compute_completions(&doc, pos);

    assert!(!completions.is_empty(), "Must suggest Tailwind utilities for p-");
    assert!(completions.iter().any(|c| c.label == "p-4"));
    assert!(completions.iter().any(|c| c.label == "px-4"));

    // Tag completion test
    let doc_tag = RsxDocument::parse("<Canv");
    let pos_tag = Position::new(0, 5);
    let tag_completions = compute_completions(&doc_tag, pos_tag);
    assert!(tag_completions.iter().any(|c| c.label == "Canvas3D"));

    // Hover test on Tailwind class
    let doc_hover = RsxDocument::parse("<div class=\"p-4\"></div>");
    let hover_pos = Position::new(0, 13); // on "p-4"
    let hover = resolve_hover(&doc_hover, hover_pos).expect("Expected hover result");
    assert!(hover.contents.contains("padding: 1rem;"));
}

#[test]
fn test_document_color_provider() {
    let source = "<div class=\"bg-[#08090a] text-white\">\n    <button class=\"bg-emerald-500\"></button>\n</div>";
    let doc = RsxDocument::parse(source);
    let colors = find_document_colors(&doc);

    assert!(!colors.is_empty(), "Must discover colors in document");

    // Check hex color #08090a
    let hex_color = colors.iter().find(|c| {
        let r = (c.color.red * 255.0).round() as u8;
        let g = (c.color.green * 255.0).round() as u8;
        let b = (c.color.blue * 255.0).round() as u8;
        r == 8 && g == 9 && b == 10
    });
    assert!(hex_color.is_some(), "Must detect #08090a hex color");

    // Check Tailwind color bg-emerald-500
    let emerald = colors.iter().find(|c| {
        let g = (c.color.green * 255.0).round() as u8;
        g == 185
    });
    assert!(emerald.is_some(), "Must detect bg-emerald-500 class");

    // Presentation format
    let presentations = format_color_presentation(
        ColorRgba { red: 0.031, green: 0.035, blue: 0.039, alpha: 1.0 },
        Range::default(),
    );
    assert!(presentations.iter().any(|p| p.label.starts_with('#')));
}

#[test]
fn test_lsp_server_lifecycle_and_stdio() {
    let mut server = LspServer::new();

    // 1. Initialize Request
    let init_req = JsonRpcMessage::Request {
        id: 1,
        method: "initialize".to_string(),
        params_json: "{}".to_string(),
    };
    let init_res = server.handle_message(init_req).expect("Expected initialize response");
    assert!(init_res.contains("\"capabilities\""));
    assert!(init_res.contains("\"completionProvider\""));
    assert!(server.initialized);

    // 2. textDocument/didOpen Notification
    let did_open = JsonRpcMessage::Notification {
        method: "textDocument/didOpen".to_string(),
        params_json: "{\"uri\":\"file:///test.rsx\",\"text\":\"<div class=\\\"flex\\\"><span></span></div>\"}".to_string(),
    };
    let diag_notif = server.handle_message(did_open).expect("Expected diagnostics notification");
    assert!(diag_notif.contains("textDocument/publishDiagnostics"));

    // 3. textDocument/completion Request
    let comp_req = JsonRpcMessage::Request {
        id: 2,
        method: "textDocument/completion".to_string(),
        params_json: "{\"uri\":\"file:///test.rsx\",\"line\":0,\"character\":5}".to_string(),
    };
    let comp_res = server.handle_message(comp_req).expect("Expected completion response");
    assert!(comp_res.contains("\"result\":["));

    // 4. Shutdown Request
    let shutdown_req = JsonRpcMessage::Request {
        id: 3,
        method: "shutdown".to_string(),
        params_json: "{}".to_string(),
    };
    let shutdown_res = server.handle_message(shutdown_req).expect("Expected shutdown response");
    assert!(shutdown_res.contains("\"result\":null"));
    assert!(server.shutdown_requested);
}
