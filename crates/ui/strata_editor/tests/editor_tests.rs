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

#[test]
fn test_multibyte_utf8_emoji_and_cjk_editing_safety() {
    let mut buffer = LineBuffer::from_str("🦀 Rustacean 🚀\nПривет мир\n東京 Tokyo");

    assert_eq!(buffer.line_count(), 3);
    assert_eq!(buffer.line_len(0), 13);
    assert_eq!(buffer.line_len(1), 10); // 'Привет мир' is 10 chars, 19 bytes

    // 1. Insert character into middle of Cyrillic text at char column 7
    buffer.insert_char(1, 7, '!');
    assert_eq!(buffer.line(1), Some("Привет !мир"));

    // 2. Insert into middle of Japanese text
    buffer.insert_str(2, 1, "・");
    assert_eq!(buffer.line(2), Some("東・京 Tokyo"));

    // 3. Backspace over 4-byte emoji
    buffer.insert_char(0, 1, '⭐');
    assert_eq!(buffer.line(0), Some("🦀⭐ Rustacean 🚀"));
    let pos = buffer.delete_backwards(0, 2);
    assert_eq!(pos, Some((0, 1)));
    assert_eq!(buffer.line(0), Some("🦀 Rustacean 🚀"));

    let pos2 = buffer.delete_backwards(0, 1);
    assert_eq!(pos2, Some((0, 0)));
    assert_eq!(buffer.line(0), Some(" Rustacean 🚀"));

    // 4. Split line in middle of multibyte text
    buffer.split_line(1, 7);
    assert_eq!(buffer.line(1), Some("Привет "));
    assert_eq!(buffer.line(2), Some("!мир"));

    // 5. Delete range across multibyte text
    let deleted = buffer.delete_range(1, 4, 2, 2);
    assert_eq!(deleted, "ет \n!м");
    assert_eq!(buffer.line(1), Some("Привир"));
}

#[test]
fn test_soft_wrapping_and_coordinate_mapping() {
    let long_line = "    let extremely_long_identifier_name = calculate_distributed_workload(param_alpha, param_beta);";
    // Wrap at 40 columns with indent preservation
    let wrapped = wrap_logical_line(long_line, 0, 40, true);

    assert!(wrapped.len() >= 2, "Long line must be split into multiple visual lines");
    assert_eq!(wrapped[0].logical_line, 0);
    assert_eq!(wrapped[0].visual_subline, 0);
    assert!(!wrapped[0].is_continuation);

    // Continuation line must have preserved indentation
    assert!(wrapped[1].is_continuation);
    assert!(wrapped[1].text.starts_with("    "));

    // Coordinate mapping round-trip
    let pos_start = Position::new(0, 4); // right after indent
    let visual_pos = logical_to_visual(pos_start, &wrapped).expect("Must map logical to visual");
    let mapped_back = visual_to_logical(visual_pos.0, visual_pos.1, &wrapped).expect("Must map visual to logical");
    assert_eq!(mapped_back, pos_start);
}

#[test]
fn test_multicursor_movement_navigation() {
    let code = "alpha\nbeta\ngamma";
    let mut editor = CodeEditor::new(code, Language::Rust);

    // Add cursor to line 1 and line 2
    editor.cursors_mut().add_cursor(Position::new(1, 0));
    editor.cursors_mut().add_cursor(Position::new(2, 0));
    assert_eq!(editor.cursors().count(), 3);

    // Move right 2 times across all 3 cursors
    editor.move_right(false);
    editor.move_right(false);
    assert_eq!(editor.cursors().all()[0].focus, Position::new(0, 2));
    assert_eq!(editor.cursors().all()[1].focus, Position::new(1, 2));
    assert_eq!(editor.cursors().all()[2].focus, Position::new(2, 2));

    // Move left 1 time
    editor.move_left(false);
    assert_eq!(editor.cursors().all()[0].focus, Position::new(0, 1));
    assert_eq!(editor.cursors().all()[1].focus, Position::new(1, 1));
    assert_eq!(editor.cursors().all()[2].focus, Position::new(2, 1));

    // Move to line start
    editor.move_to_line_start(false);
    assert_eq!(editor.cursors().all()[0].focus, Position::new(0, 0));
    assert_eq!(editor.cursors().all()[1].focus, Position::new(1, 0));
    assert_eq!(editor.cursors().all()[2].focus, Position::new(2, 0));

    // Move to line end
    editor.move_to_line_end(false);
    assert_eq!(editor.cursors().all()[0].focus, Position::new(0, 5)); // 'alpha'.len()
    assert_eq!(editor.cursors().all()[1].focus, Position::new(1, 4)); // 'beta'.len()
    assert_eq!(editor.cursors().all()[2].focus, Position::new(2, 5)); // 'gamma'.len()

    // Move up and down clamping
    editor.cursors_mut().set_single(Position::new(0, 2));
    editor.move_down(false);
    assert_eq!(editor.cursors().primary().focus, Position::new(1, 2));
    editor.move_up(false);
    assert_eq!(editor.cursors().primary().focus, Position::new(0, 2));

    // Select all
    editor.select_all();
    assert_eq!(editor.cursors().primary().anchor, Position::new(0, 0));
    assert_eq!(editor.cursors().primary().focus, Position::new(2, 5));
}

#[test]
fn test_virtualized_viewport_rendering_on_massive_buffer() {
    // Generate a massive 10,000-line document
    let mut large_doc = String::new();
    for i in 0..10_000 {
        large_doc.push_str(&format!("let var_{} = calculate_value({});\n", i, i));
    }
    let editor = CodeEditor::new(&large_doc, Language::Rust);
    assert_eq!(editor.line_count(), 10_001);

    let line_height = 24.0;
    let viewport_height = 480.0; // 20 lines visible
    let scroll_top = 2400.0; // scrolled to line 100

    let blueprint = editor.render_virtualized(scroll_top, viewport_height, line_height, "h-[480px]");

    if let strata_dom::NodeBlueprint::Element { children, .. } = blueprint {
        // Child nodes should be: Top spacer + (20 visible + 4 overscan lines = ~24-25 lines) + Bottom spacer
        // Total DOM nodes MUST be under 35, NEVER 10,000!
        assert!(children.len() < 35, "Virtualized blueprint must not instantiate 10,000 nodes! Actual: {}", children.len());
        assert!(children.len() >= 20, "Must contain all visible lines and spacers");
    } else {
        panic!("Expected Element blueprint");
    }
}

#[test]
fn test_rope_100k_lines_scaling_and_editing() {
    use std::time::Instant;

    // 1. Build a massive 100,000-line source document
    let start_build = Instant::now();
    let mut large_code = String::with_capacity(4_000_000);
    for i in 0..100_000 {
        large_code.push_str(&format!("fn logic_gate_{i:06}(clk: bool, in_a: bool, in_b: bool) -> bool {{ (in_a & in_b) ^ clk }}\n"));
    }
    let mut buffer = LineBuffer::from_str(&large_code);
    let build_duration = start_build.elapsed();
    println!("Built 100,000-line rope in {:?}", build_duration);

    assert_eq!(buffer.line_count(), 100_001);

    // 2. High-speed logarithmic random line indexing
    let start_access = Instant::now();
    for &idx in &[0, 25_000, 50_000, 75_000, 99_999] {
        let line = buffer.line(idx).expect("Line must exist");
        assert!(line.contains(&format!("logic_gate_{idx:06}")));
    }
    let access_duration = start_access.elapsed();
    println!("Random line access in {:?}", access_duration);
    assert!(access_duration.as_micros() < 500, "Logarithmic indexing must take < 500 us");

    // 3. Fine-grained editing in the middle of 100,000 lines (O(log N))
    let start_edit = Instant::now();
    buffer.insert_str(50_000, 3, "/* mutated */ ");
    assert!(buffer.line(50_000).unwrap().contains("fn /* mutated */ logic_gate_050000"));

    buffer.split_line(50_000, 16);
    assert_eq!(buffer.line_count(), 100_002);

    let deleted_pos = buffer.delete_backwards(50_001, 0);
    assert_eq!(deleted_pos, Some((50_000, 16)));
    assert_eq!(buffer.line_count(), 100_001);

    let edit_duration = start_edit.elapsed();
    println!("100k-line editing operations in {:?}", edit_duration);
    assert!(edit_duration.as_millis() < 10, "Editing in 100k-line rope must be < 10 ms");

    // 4. Virtualized slice extraction
    let start_slice = Instant::now();
    let slice = buffer.slice_lines(50_000, 30);
    let slice_duration = start_slice.elapsed();
    assert_eq!(slice.len(), 30);
    println!("Slicing 30 lines at offset 50,000 in {:?}", slice_duration);
    assert!(slice_duration.as_micros() < 100, "Slicing viewport lines must be < 100 us");
}

#[test]
fn test_virtualized_rendering_with_soft_wrapping_continuation() {
    let mut doc = String::new();
    for i in 0..100 {
        doc.push_str(&format!(
            "let component_pipeline_stage_{i} = execute_extremely_long_mathematical_synthesis_pass(input_alpha, input_beta, input_gamma);\n"
        ));
    }

    let mut editor = CodeEditor::new(&doc, Language::Rust);
    editor.set_wrap_column(Some(45)); // wrap long line into ~3 visual lines

    let blueprint = editor.render_virtualized(0.0, 480.0, 24.0, "h-[480px]");

    if let strata_dom::NodeBlueprint::Element { children, .. } = blueprint {
        // Find continuation gutter elements with arrow indicator '↳'
        let mut found_continuation_gutter = false;
        for row in &children {
            if let strata_dom::NodeBlueprint::Element { children: row_children, .. } = row {
                for col in row_children {
                    if let strata_dom::NodeBlueprint::Element { children: gutter_children, classes, .. } = col {
                        if classes.iter().any(|c| c.contains("text-slate-600")) {
                            for g in gutter_children {
                                if let strata_dom::NodeBlueprint::Text(t) = g {
                                    if t == "↳" {
                                        found_continuation_gutter = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        assert!(found_continuation_gutter, "Virtualized rendering with soft-wrapping must render continuation markers '↳'");
    } else {
        panic!("Expected Element blueprint");
    }
}

#[test]
fn test_code_editor_auto_virtualization_on_large_document() {
    let mut large_doc = String::new();
    for i in 0..5_000 {
        large_doc.push_str(&format!("fn item_{i}() -> i32 {{ {i} }}\n"));
    }
    let editor = CodeEditor::new(&large_doc, Language::Rust);

    // render() on 5,000 lines should NOT instantiate 5,000 DOM rows
    let blueprint = editor.render();
    if let strata_dom::NodeBlueprint::Element { children, .. } = blueprint {
        assert!(children.len() < 50, "Auto-virtualization must bound DOM tree nodes. Actual: {}", children.len());
    } else {
        panic!("Expected Element blueprint");
    }
}

#[test]
fn test_tree_sitter_ast_parsing_and_token_classification() {
    let mut highlighter = SyntaxHighlighter::with_language(Language::Rust);
    let rust_src = "pub fn add_values(left: u32, right: u32) -> u32 {\n    left + right\n}";
    highlighter.parse(rust_src);

    assert!(highlighter.tree().is_some());
    let root = highlighter.root_node().expect("Root node must exist");
    assert_eq!(root.kind(), "source_file");

    let tokens = highlighter.highlight_line_ast(0, "pub fn add_values(left: u32, right: u32) -> u32 {");
    assert!(!tokens.is_empty());
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword && t.text == "pub"));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword && t.text == "fn"));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Function && t.text == "add_values"));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Type && t.text == "u32"));

    // JavaScript grammar
    let mut js_highlighter = SyntaxHighlighter::with_language(Language::JavaScript);
    let js_src = "function calculate(x) {\n    return x * 2;\n}";
    js_highlighter.parse(js_src);
    assert!(js_highlighter.tree().is_some());
    let js_tokens = js_highlighter.highlight_line_ast(0, "function calculate(x) {");
    assert!(js_tokens.iter().any(|t| t.token_type == TokenType::Keyword && t.text == "function"));
    assert!(js_tokens.iter().any(|t| t.token_type == TokenType::Function && t.text == "calculate"));
}

#[test]
fn test_tree_sitter_incremental_reparsing_on_editor_mutations() {
    let initial_code = "fn alpha() -> i32 {\n    1\n}";
    let mut editor = CodeEditor::new(initial_code, Language::Rust);

    let initial_tree = editor.highlighter().tree().cloned();
    assert!(initial_tree.is_some());

    // Insert text at end of document
    editor.move_to_line_end(false);
    editor.insert_text("\n\nfn beta() -> i32 {\n    2\n}");

    let updated_root = editor.highlighter().root_node().expect("AST must be updated");
    let has_beta = (0..updated_root.child_count())
        .any(|i| updated_root.child(i).map(|c| c.kind() == "function_item").unwrap_or(false));
    assert!(has_beta, "Updated AST must contain newly parsed function nodes");
}

#[test]
fn test_tree_sitter_ast_code_folding_and_navigation() {
    let src = "struct Engine {\n    cores: usize,\n}\n\nimpl Engine {\n    fn run(&self) {\n        let x = 10;\n    }\n}";
    let mut highlighter = SyntaxHighlighter::with_language(Language::Rust);
    highlighter.parse(src);

    let buffer = LineBuffer::from_str(src);
    let folds = highlighter.compute_fold_ranges(&buffer);
    assert!(!folds.is_empty(), "AST folding should extract multiple foldable blocks");
    
    // Check that struct, impl, and fn blocks are folded
    assert!(folds.iter().any(|f| f.start_line == 0 && f.end_line == 2));
    assert!(folds.iter().any(|f| f.start_line == 4 && f.end_line == 8));

    // Structural AST navigation: find enclosing scope for position inside fn run
    let scope = highlighter.enclosing_scope_range(6, 12);
    assert!(scope.is_some());
    let (s_start, s_end) = scope.unwrap();
    assert_eq!(s_start, 5);
    assert_eq!(s_end, 7);

    // Node at position (col 16 is '1' in '10')
    let node = highlighter.node_at_position(6, 16);
    assert!(node.is_some());
    assert_eq!(node.unwrap().kind(), "integer_literal");
}

#[test]
fn test_editor_rendering_preserves_token_whitespace_gaps() {
    let src = "let count = 42;";
    let editor = CodeEditor::new(src, Language::Rust);
    let blueprint = editor.render();

    // Verify whitespace nodes exist between syntax tokens
    if let strata_dom::NodeBlueprint::Element { children, .. } = blueprint {
        let mut found_whitespace_node = false;
        for row in &children {
            if let strata_dom::NodeBlueprint::Element { children: row_cols, .. } = row {
                for col in row_cols {
                    if let strata_dom::NodeBlueprint::Element { children: code_spans, .. } = col {
                        for span in code_spans {
                            if let strata_dom::NodeBlueprint::Text(t) = span {
                                if t.contains(' ') {
                                    found_whitespace_node = true;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(found_whitespace_node, "Rendering must preserve whitespace gaps between tokens");
    } else {
        panic!("Expected Element blueprint");
    }
}

#[test]
fn test_btree_rope_node_metrics_and_logarithmic_height() {
    let mut doc = String::new();
    for i in 0..10_000 {
        doc.push_str(&format!("line number {i:05} content\n"));
    }
    let buffer = LineBuffer::from_str(&doc);

    // Height of balanced B-Tree for 10,000 lines should be <= 4
    let height = buffer.height();
    assert!((2..=5).contains(&height), "B-Tree height must be logarithmic (2..=5). Actual: {height}");

    // Cached metrics verification
    assert_eq!(buffer.line_count(), 10_001);
    assert_eq!(buffer.rope().total_char_count(), buffer.to_text().chars().count() - 10_000); // without newline chars
    assert_eq!(buffer.rope().total_byte_count(), buffer.to_text().len() - 10_000);

    // Verify fine-grained mutations maintain exact line counts and height
    let mut rope = buffer.rope().clone();
    rope.insert_line(500, "new inserted line".to_string());
    assert_eq!(rope.line_count(), 10_002);
    assert_eq!(rope.line(500), Some("new inserted line"));

    let deleted = rope.delete_line(500);
    assert_eq!(deleted, Some("new inserted line".to_string()));
    assert_eq!(rope.line_count(), 10_001);
}

#[test]
fn test_multicursor_transactional_descending_coalescing() {
    let mut editor = CodeEditor::new("apple, banana, cherry", Language::PlainText);
    
    // Add cursors on the same line at col 0, col 7, col 15
    editor.cursors_mut().set_single(Position::new(0, 0));
    editor.cursors_mut().add_cursor(Position::new(0, 7));
    editor.cursors_mut().add_cursor(Position::new(0, 15));
    assert_eq!(editor.cursors().count(), 3);

    // Insert prefix at all 3 cursors simultaneously
    editor.insert_text("fruit: ");

    assert_eq!(editor.buffer().line(0), Some("fruit: apple, fruit: banana, fruit: cherry"));
    
    // Verify carets shifted correctly and did not invalidate each other's offsets
    let carets: Vec<Position> = editor.cursors().all().iter().map(|s| s.focus).collect();
    assert_eq!(carets, vec![
        Position::new(0, 7),
        Position::new(0, 21),
        Position::new(0, 36),
    ]);

    // Single atomic undo should restore initial state and all 3 original cursors
    editor.undo();
    assert_eq!(editor.buffer().line(0), Some("apple, banana, cherry"));
    let restored_carets: Vec<Position> = editor.cursors().all().iter().map(|s| s.focus).collect();
    assert_eq!(restored_carets, vec![
        Position::new(0, 0),
        Position::new(0, 7),
        Position::new(0, 15),
    ]);

    // Redo restores modified text
    editor.redo();
    assert_eq!(editor.buffer().line(0), Some("fruit: apple, fruit: banana, fruit: cherry"));
}

#[test]
fn test_multicursor_overlapping_selection_merge_and_undo() {
    let mut editor = CodeEditor::new("abcdefghijk", Language::PlainText);

    // Add two overlapping ranges: 2..5 ("cde") and 4..8 ("efgh")
    editor.cursors_mut().clear();
    editor.cursors_mut().add_selection(Selection::range(Position::new(0, 2), Position::new(0, 5)));
    editor.cursors_mut().add_selection(Selection::range(Position::new(0, 4), Position::new(0, 8)));

    // MultiCursor should merge them into single range 2..8 ("cdefgh")
    assert_eq!(editor.cursors().count(), 1);
    assert_eq!(
        editor.cursors().primary().ordered_range(),
        (Position::new(0, 2), Position::new(0, 8))
    );

    // Replacing selection with "XYZ"
    editor.insert_text("XYZ");
    assert_eq!(editor.buffer().line(0), Some("abXYZijk"));
    assert_eq!(editor.cursors().primary().focus, Position::new(0, 5));

    // Undo restores the original merged range
    editor.undo();
    assert_eq!(editor.buffer().line(0), Some("abcdefghijk"));
}

#[test]
fn test_multicursor_multiline_newline_insertion_safety() {
    let mut editor = CodeEditor::new("alpha\nbeta\ngamma", Language::PlainText);
    editor.cursors_mut().set_single(Position::new(0, 2));
    editor.cursors_mut().add_cursor(Position::new(1, 2));
    editor.cursors_mut().add_cursor(Position::new(2, 2));

    editor.insert_newline();
    assert_eq!(editor.line_count(), 6);
    assert_eq!(editor.buffer().line(0), Some("al"));
    assert_eq!(editor.buffer().line(1), Some("pha"));
    assert_eq!(editor.buffer().line(2), Some("be"));
    assert_eq!(editor.buffer().line(3), Some("ta"));
    assert_eq!(editor.buffer().line(4), Some("ga"));
    assert_eq!(editor.buffer().line(5), Some("mma"));

    // Undo should cleanly revert all 6 lines back to the original 3 lines in 1 transaction
    editor.undo();
    assert_eq!(editor.line_count(), 3);
    assert_eq!(editor.buffer().line(0), Some("alpha"));
    assert_eq!(editor.buffer().line(1), Some("beta"));
    assert_eq!(editor.buffer().line(2), Some("gamma"));
}




