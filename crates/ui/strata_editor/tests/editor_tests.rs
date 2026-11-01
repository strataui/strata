//! Integration tests for Strata Editor: LineBuffer, MultiCursor, Syntax, History, Clipboard, IME, and CodeEditor.

use strata_editor::prelude::*;

#[test]
fn test_line_buffer_basic_mutations() {
    let mut buffer = LineBuffer::new();
    assert_eq!(buffer.line_count(), 1);
    assert_eq!(buffer.line(0), Some(""));

    buffer.insert_char(0, 0, 'h');
    buffer.insert_str(0, 1, "ello world");
    assert_eq!(buffer.line(0), Some("hello world"));

    buffer.split_line(0, 5);
    assert_eq!(buffer.line_count(), 2);
    assert_eq!(buffer.line(0), Some("hello"));
    assert_eq!(buffer.line(1), Some(" world"));

    buffer.join_line_with_next(0);
    assert_eq!(buffer.line_count(), 1);
    assert_eq!(buffer.line(0), Some("hello world"));

    buffer.delete_range(0, 0, 0, 6);
    assert_eq!(buffer.line(0), Some("world"));
}

#[test]
fn test_line_buffer_multiline_operations() {
    let text = "fn main() {\n    let val = 42;\n    println!(\"{}\", val);\n}";
    let mut buffer = LineBuffer::from_str(text);
    assert_eq!(buffer.line_count(), 4);
    assert_eq!(buffer.line(1), Some("    let val = 42;"));

    buffer.delete_range(1, 4, 2, 4);
    assert_eq!(buffer.line(1), Some("    println!(\"{}\", val);"));
    assert_eq!(buffer.line_count(), 3);

    buffer.insert_str(1, 4, "let x = 10;\n    ");
    assert_eq!(buffer.line_count(), 4);
    assert_eq!(buffer.line(1), Some("    let x = 10;"));
    assert_eq!(buffer.line(2), Some("    println!(\"{}\", val);"));
}

#[test]
fn test_multi_cursor_selection_and_normalization() {
    let mut cursors = MultiCursor::new();
    assert_eq!(cursors.count(), 1);
    assert_eq!(cursors.primary().focus, Position::new(0, 0));

    cursors.add_cursor(Position::new(1, 4));
    cursors.add_cursor(Position::new(2, 4));
    assert_eq!(cursors.count(), 3);

    // Overlapping or duplicate cursor should normalize/deduplicate
    cursors.add_cursor(Position::new(1, 4));
    assert_eq!(cursors.count(), 3);

    let sel = Selection::range(Position::new(0, 0), Position::new(0, 5));
    assert!(!sel.is_collapsed());
    assert_eq!(sel.ordered_range(), (Position::new(0, 0), Position::new(0, 5)));

    let reverse_sel = Selection::range(Position::new(0, 5), Position::new(0, 0));
    assert_eq!(reverse_sel.ordered_range(), (Position::new(0, 0), Position::new(0, 5)));
}

#[test]
fn test_syntax_highlighting_rust_and_python() {
    let highlighter = SyntaxHighlighter::new();

    let rust_code = "fn compute_sum(a: i32, b: i32) -> i32 {\n    let result = a + b;\n    // return value\n    \"done\"\n}";
    let rust_tokens = highlighter.highlight_line("fn compute_sum(a: i32) -> i32 {", Language::Rust);
    
    assert!(rust_tokens.iter().any(|t| t.token_type == TokenType::Keyword && t.text == "fn"));
    assert!(rust_tokens.iter().any(|t| t.token_type == TokenType::Function && t.text == "compute_sum"));
    assert!(rust_tokens.iter().any(|t| t.token_type == TokenType::Type && t.text == "i32"));

    let comment_tokens = highlighter.highlight_line("// this is a comment", Language::Rust);
    assert_eq!(comment_tokens.len(), 1);
    assert_eq!(comment_tokens[0].token_type, TokenType::Comment);

    let py_tokens = highlighter.highlight_line("def process_data(records):", Language::Python);
    assert!(py_tokens.iter().any(|t| t.token_type == TokenType::Keyword && t.text == "def"));
    assert!(py_tokens.iter().any(|t| t.token_type == TokenType::Function && t.text == "process_data"));

    let buffer = LineBuffer::from_str(rust_code);
    let folds = highlighter.compute_fold_ranges(&buffer);
    assert!(!folds.is_empty());
    assert_eq!(folds[0].start_line, 0);
    assert_eq!(folds[0].end_line, 4);
}

#[test]
fn test_syntax_highlighting_bracket_matching() {
    let highlighter = SyntaxHighlighter::new();
    let buffer = LineBuffer::from_str("function test() {\n    const arr = [1, 2, 3];\n}");

    let match_brace = highlighter.find_matching_bracket(&buffer, Position::new(0, 16));
    assert_eq!(match_brace, Some(Position::new(2, 0)));

    let match_bracket = highlighter.find_matching_bracket(&buffer, Position::new(1, 16));
    assert_eq!(match_bracket, Some(Position::new(1, 24)));
}

#[test]
fn test_history_undo_redo_and_coalescing() {
    let mut history = HistoryStack::default();
    assert!(!history.can_undo());
    assert!(!history.can_redo());

    let c0 = vec![Selection::point(Position::new(0, 0))];
    let c1 = vec![Selection::point(Position::new(0, 1))];
    let c2 = vec![Selection::point(Position::new(0, 2))];

    history.record_insert(Position::new(0, 0), "a", c0.clone(), c1.clone());
    assert!(history.can_undo());

    // Rapid second keystroke within coalescing window (coalesces into single transaction)
    history.record_insert(Position::new(0, 1), "b", c1.clone(), c2.clone());

    let mut buffer = LineBuffer::from_str("ab");
    let restored_before = history.undo(&mut buffer).expect("Undo should succeed");
    assert_eq!(restored_before, c0);
    assert_eq!(buffer.line(0), Some(""));

    let restored_after = history.redo(&mut buffer).expect("Redo should succeed");
    assert_eq!(restored_after, c2);
    assert_eq!(buffer.line(0), Some("ab"));
}

#[test]
fn test_clipboard_bridge_multiformat_payload() {
    let buffer = LineBuffer::from_str("line one\nline two\nline three");
    let sel = Selection::range(Position::new(0, 5), Position::new(1, 4));

    let payload = ClipboardBridge::from_selection(&buffer, sel);
    assert_eq!(payload.plain_text, "one\nline");
    assert!(payload.html.as_ref().unwrap().contains("<pre><code>one\nline</code></pre>"));
    assert!(payload.rtf.as_ref().unwrap().contains(r"{\rtf1"));

    let mut doc = Document::new();
    doc.push(BlockNode::heading("h1", 1, "Document Title"));
    doc.push(BlockNode::paragraph("p1", "Rich text paragraph with formatting"));

    let doc_payload = ClipboardBridge::from_document(&doc);
    assert!(doc_payload.plain_text.contains("Document Title"));
    assert!(doc_payload.markdown.as_ref().unwrap().contains("# Document Title"));
    assert!(doc_payload.html.as_ref().unwrap().contains("<h1>Document Title</h1>"));
    assert!(doc_payload.html.as_ref().unwrap().contains("<p>Rich text paragraph with formatting</p>"));
}

#[test]
fn test_ime_composition_lifecycle() {
    let mut ime = ImeComposition::new();
    assert!(!ime.is_composing());
    assert_eq!(*ime.state(), ImeState::Inactive);

    ime.start();
    assert!(ime.is_composing());

    ime.update("nihao", 5);
    if let ImeState::Composing { ref preedit_text, cursor_offset, .. } = *ime.state() {
        assert_eq!(preedit_text, "nihao");
        assert_eq!(cursor_offset, 5);
    } else {
        panic!("Expected Composing state");
    }

    let committed = ime.commit();
    assert_eq!(committed, Some("nihao".to_string()));
    assert!(!ime.is_composing());
    assert_eq!(*ime.state(), ImeState::Inactive);
}

#[test]
fn test_code_editor_controller_and_rendering() {
    let source = "fn hello_world() {\n    let msg = \"strata\";\n}";
    let mut editor = CodeEditor::new(source, Language::Rust);

    assert_eq!(editor.line_count(), 3);
    assert_eq!(editor.cursors().count(), 1);

    // Multi-cursor addition and typing
    editor.cursors_mut().add_cursor(Position::new(1, 4));
    editor.insert_char('/');
    editor.insert_char('/');
    assert!(editor.text().contains("//fn hello_world()"));
    assert!(editor.text().contains("    //let msg"));

    // Code folding toggles
    assert!(!editor.is_folded(0));
    editor.toggle_fold(0);
    assert!(editor.is_folded(0));
    editor.toggle_fold(0);
    assert!(!editor.is_folded(0));

    // Render blueprint
    let blueprint = editor.render();
    if let strata_dom::NodeBlueprint::Element { tag, children, .. } = blueprint {
        assert_eq!(tag, strata_dom::Tag::Div);
        // Contains rows for the lines
        assert_eq!(children.len(), 3);
    } else {
        panic!("Expected Element blueprint");
    }
}
