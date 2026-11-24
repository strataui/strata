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
    assert_eq!(
        sel.ordered_range(),
        (Position::new(0, 0), Position::new(0, 5))
    );

    let reverse_sel = Selection::range(Position::new(0, 5), Position::new(0, 0));
    assert_eq!(
        reverse_sel.ordered_range(),
        (Position::new(0, 0), Position::new(0, 5))
    );
}

#[test]
fn test_syntax_highlighting_rust_and_python() {
    let highlighter = SyntaxHighlighter::new();

    let rust_code = "fn compute_sum(a: i32, b: i32) -> i32 {\n    let result = a + b;\n    // return value\n    \"done\"\n}";
    let rust_tokens = highlighter.highlight_line("fn compute_sum(a: i32) -> i32 {", Language::Rust);

    assert!(rust_tokens
        .iter()
        .any(|t| t.token_type == TokenType::Keyword && t.text == "fn"));
    assert!(rust_tokens
        .iter()
        .any(|t| t.token_type == TokenType::Function && t.text == "compute_sum"));
    assert!(rust_tokens
        .iter()
        .any(|t| t.token_type == TokenType::Type && t.text == "i32"));

    let comment_tokens = highlighter.highlight_line("// this is a comment", Language::Rust);
    assert_eq!(comment_tokens.len(), 1);
    assert_eq!(comment_tokens[0].token_type, TokenType::Comment);

    let py_tokens = highlighter.highlight_line("def process_data(records):", Language::Python);
    assert!(py_tokens
        .iter()
        .any(|t| t.token_type == TokenType::Keyword && t.text == "def"));
    assert!(py_tokens
        .iter()
        .any(|t| t.token_type == TokenType::Function && t.text == "process_data"));

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
    assert!(payload
        .html
        .as_ref()
        .unwrap()
        .contains("<pre><code>one\nline</code></pre>"));
    assert!(payload.rtf.as_ref().unwrap().contains(r"{\rtf1"));

    let mut doc = Document::new();
    doc.push(BlockNode::heading("h1", 1, "Document Title"));
    doc.push(BlockNode::paragraph(
        "p1",
        "Rich text paragraph with formatting",
    ));

    let doc_payload = ClipboardBridge::from_document(&doc);
    assert!(doc_payload.plain_text.contains("Document Title"));
    assert!(doc_payload
        .markdown
        .as_ref()
        .unwrap()
        .contains("# Document Title"));
    assert!(doc_payload
        .html
        .as_ref()
        .unwrap()
        .contains("<h1>Document Title</h1>"));
    assert!(doc_payload
        .html
        .as_ref()
        .unwrap()
        .contains("<p>Rich text paragraph with formatting</p>"));
}

#[test]
fn test_ime_composition_lifecycle() {
    let mut ime = ImeComposition::new();
    assert!(!ime.is_composing());
    assert_eq!(*ime.state(), ImeState::Inactive);

    ime.start();
    assert!(ime.is_composing());

    ime.update("nihao", 5);
    if let ImeState::Composing {
        ref preedit_text,
        cursor_offset,
        ..
    } = *ime.state()
    {
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

    assert!(
        wrapped.len() >= 2,
        "Long line must be split into multiple visual lines"
    );
    assert_eq!(wrapped[0].logical_line, 0);
    assert_eq!(wrapped[0].visual_subline, 0);
    assert!(!wrapped[0].is_continuation);

    // Continuation line must have preserved indentation
    assert!(wrapped[1].is_continuation);
    assert!(wrapped[1].text.starts_with("    "));

    // Coordinate mapping round-trip
    let pos_start = Position::new(0, 4); // right after indent
    let visual_pos = logical_to_visual(pos_start, &wrapped).expect("Must map logical to visual");
    let mapped_back = visual_to_logical(visual_pos.0, visual_pos.1, &wrapped)
        .expect("Must map visual to logical");
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

    let blueprint =
        editor.render_virtualized(scroll_top, viewport_height, line_height, "h-[480px]");

    if let strata_dom::NodeBlueprint::Element { children, .. } = blueprint {
        // Child nodes should be: Top spacer + (20 visible + 4 overscan lines = ~24-25 lines) + Bottom spacer
        // Total DOM nodes MUST be under 35, NEVER 10,000!
        assert!(
            children.len() < 35,
            "Virtualized blueprint must not instantiate 10,000 nodes! Actual: {}",
            children.len()
        );
        assert!(
            children.len() >= 20,
            "Must contain all visible lines and spacers"
        );
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
    assert!(
        access_duration.as_micros() < 500,
        "Logarithmic indexing must take < 500 us"
    );

    // 3. Fine-grained editing in the middle of 100,000 lines (O(log N))
    let start_edit = Instant::now();
    buffer.insert_str(50_000, 3, "/* mutated */ ");
    assert!(buffer
        .line(50_000)
        .unwrap()
        .contains("fn /* mutated */ logic_gate_050000"));

    buffer.split_line(50_000, 16);
    assert_eq!(buffer.line_count(), 100_002);

    let deleted_pos = buffer.delete_backwards(50_001, 0);
    assert_eq!(deleted_pos, Some((50_000, 16)));
    assert_eq!(buffer.line_count(), 100_001);

    let edit_duration = start_edit.elapsed();
    println!("100k-line editing operations in {:?}", edit_duration);
    assert!(
        edit_duration.as_millis() < 10,
        "Editing in 100k-line rope must be < 10 ms"
    );

    // 4. Virtualized slice extraction
    let start_slice = Instant::now();
    let slice = buffer.slice_lines(50_000, 30);
    let slice_duration = start_slice.elapsed();
    assert_eq!(slice.len(), 30);
    println!("Slicing 30 lines at offset 50,000 in {:?}", slice_duration);
    assert!(
        slice_duration.as_micros() < 100,
        "Slicing viewport lines must be < 100 us"
    );
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
            if let strata_dom::NodeBlueprint::Element {
                children: row_children,
                ..
            } = row
            {
                for col in row_children {
                    if let strata_dom::NodeBlueprint::Element {
                        children: gutter_children,
                        classes,
                        ..
                    } = col
                    {
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

        assert!(
            found_continuation_gutter,
            "Virtualized rendering with soft-wrapping must render continuation markers '↳'"
        );
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
        assert!(
            children.len() < 50,
            "Auto-virtualization must bound DOM tree nodes. Actual: {}",
            children.len()
        );
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

    let tokens =
        highlighter.highlight_line_ast(0, "pub fn add_values(left: u32, right: u32) -> u32 {");
    assert!(!tokens.is_empty());
    assert!(tokens
        .iter()
        .any(|t| t.token_type == TokenType::Keyword && t.text == "pub"));
    assert!(tokens
        .iter()
        .any(|t| t.token_type == TokenType::Keyword && t.text == "fn"));
    assert!(tokens
        .iter()
        .any(|t| t.token_type == TokenType::Function && t.text == "add_values"));
    assert!(tokens
        .iter()
        .any(|t| t.token_type == TokenType::Type && t.text == "u32"));

    // JavaScript grammar
    let mut js_highlighter = SyntaxHighlighter::with_language(Language::JavaScript);
    let js_src = "function calculate(x) {\n    return x * 2;\n}";
    js_highlighter.parse(js_src);
    assert!(js_highlighter.tree().is_some());
    let js_tokens = js_highlighter.highlight_line_ast(0, "function calculate(x) {");
    assert!(js_tokens
        .iter()
        .any(|t| t.token_type == TokenType::Keyword && t.text == "function"));
    assert!(js_tokens
        .iter()
        .any(|t| t.token_type == TokenType::Function && t.text == "calculate"));
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

    let updated_root = editor
        .highlighter()
        .root_node()
        .expect("AST must be updated");
    let has_beta = (0..updated_root.child_count()).any(|i| {
        updated_root
            .child(i)
            .map(|c| c.kind() == "function_item")
            .unwrap_or(false)
    });
    assert!(
        has_beta,
        "Updated AST must contain newly parsed function nodes"
    );
}

#[test]
fn test_tree_sitter_ast_code_folding_and_navigation() {
    let src = "struct Engine {\n    cores: usize,\n}\n\nimpl Engine {\n    fn run(&self) {\n        let x = 10;\n    }\n}";
    let mut highlighter = SyntaxHighlighter::with_language(Language::Rust);
    highlighter.parse(src);

    let buffer = LineBuffer::from_str(src);
    let folds = highlighter.compute_fold_ranges(&buffer);
    assert!(
        !folds.is_empty(),
        "AST folding should extract multiple foldable blocks"
    );

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
            if let strata_dom::NodeBlueprint::Element {
                children: row_cols, ..
            } = row
            {
                for col in row_cols {
                    if let strata_dom::NodeBlueprint::Element {
                        children: code_spans,
                        ..
                    } = col
                    {
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
        assert!(
            found_whitespace_node,
            "Rendering must preserve whitespace gaps between tokens"
        );
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
    assert!(
        (2..=5).contains(&height),
        "B-Tree height must be logarithmic (2..=5). Actual: {height}"
    );

    // Cached metrics verification
    assert_eq!(buffer.line_count(), 10_001);
    assert_eq!(
        buffer.rope().total_char_count(),
        buffer.to_text().chars().count() - 10_000
    ); // without newline chars
    assert_eq!(
        buffer.rope().total_byte_count(),
        buffer.to_text().len() - 10_000
    );

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

    assert_eq!(
        editor.buffer().line(0),
        Some("fruit: apple, fruit: banana, fruit: cherry")
    );

    // Verify carets shifted correctly and did not invalidate each other's offsets
    let carets: Vec<Position> = editor.cursors().all().iter().map(|s| s.focus).collect();
    assert_eq!(
        carets,
        vec![
            Position::new(0, 7),
            Position::new(0, 21),
            Position::new(0, 36),
        ]
    );

    // Single atomic undo should restore initial state and all 3 original cursors
    editor.undo();
    assert_eq!(editor.buffer().line(0), Some("apple, banana, cherry"));
    let restored_carets: Vec<Position> = editor.cursors().all().iter().map(|s| s.focus).collect();
    assert_eq!(
        restored_carets,
        vec![
            Position::new(0, 0),
            Position::new(0, 7),
            Position::new(0, 15),
        ]
    );

    // Redo restores modified text
    editor.redo();
    assert_eq!(
        editor.buffer().line(0),
        Some("fruit: apple, fruit: banana, fruit: cherry")
    );
}

#[test]
fn test_multicursor_overlapping_selection_merge_and_undo() {
    let mut editor = CodeEditor::new("abcdefghijk", Language::PlainText);

    // Add two overlapping ranges: 2..5 ("cde") and 4..8 ("efgh")
    editor.cursors_mut().clear();
    editor
        .cursors_mut()
        .add_selection(Selection::range(Position::new(0, 2), Position::new(0, 5)));
    editor
        .cursors_mut()
        .add_selection(Selection::range(Position::new(0, 4), Position::new(0, 8)));

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

#[test]
fn test_rope_byte_position_roundtrip_and_metrics_after_multiline_edits() {
    let mut doc = String::from("🦀 alpha\nПривет мир\n東京 Tokyo\n");
    for i in 0..1_500 {
        doc.push_str(&format!("let row_{i:04} = {i};\n"));
    }
    let mut buf = LineBuffer::from_str(&doc);

    let assert_metrics = |b: &LineBuffer| {
        let full = b.to_string();
        assert_eq!(b.total_text_byte_len(), full.len());
        assert_eq!(
            b.rope().total_byte_count() + b.line_count().saturating_sub(1),
            full.len(),
            "SubtreeMetrics byte sum must stay invariant across mutations"
        );
    };
    assert_metrics(&buf);

    // Verify exact O(log N) byte <-> Position bijection across ASCII and multi-byte UTF-8 lines.
    for line in [0, 1, 2, 10, 750, 1_500] {
        let len = buf.line_len(line);
        for col in [0, len / 2, len] {
            let pos = Position::new(line, col);
            let byte = buf.position_to_byte(pos);
            let back = buf.byte_to_position(byte);
            assert_eq!(back, pos, "Round-trip failed for pos={pos:?} byte={byte}");
        }
    }

    // Mutate with multiline insert, multiline delete, split, and join — metrics MUST never drift.
    buf.insert_str(1, 3, "🚀\nmiddle line\n日本語");
    assert_metrics(&buf);
    buf.delete_range(1, 2, 3, 2);
    assert_metrics(&buf);
    buf.split_line(750, 8);
    assert_metrics(&buf);
    buf.join_line_with_next(750);
    assert_metrics(&buf);

    // Re-check bijection after all mutations.
    for line in [0, 1, 2, 750, buf.line_count() - 1] {
        let len = buf.line_len(line);
        let pos = Position::new(line, len);
        assert_eq!(buf.byte_to_position(buf.position_to_byte(pos)), pos);
    }
}

#[test]
fn test_true_incremental_tree_sitter_edits_and_ast_equivalence() {
    let mut src = String::new();
    for i in 0..120 {
        src.push_str(&format!("pub fn func_{i}(x: i32) -> i32 {{ x + {i} }}\n"));
    }

    let assert_ast_matches_fresh = |ed: &CodeEditor, step: &str| {
        let inc_sexp = ed
            .highlighter()
            .root_node()
            .expect("Incremental AST must exist")
            .to_sexp();
        let mut fresh = SyntaxHighlighter::with_language(ed.language());
        fresh.parse(&ed.text());
        let fresh_sexp = fresh
            .root_node()
            .expect("Fresh AST must exist")
            .to_sexp();
        assert_eq!(
            inc_sexp, fresh_sexp,
            "Incremental Tree-Sitter AST diverged from fresh parse at step: {step}"
        );
    };

    let mut editor = CodeEditor::new(&src, Language::Rust);
    assert_eq!(editor.highlighter().incremental_edit_count(), 0);
    assert_ast_matches_fresh(&editor, "initial");

    // 1. Single-line edit in the middle of 120 functions
    editor.cursors_mut().set_single(Position::new(60, 38));
    editor.insert_text(" * 2");
    assert!(editor.highlighter().incremental_edit_count() >= 1);
    assert_ast_matches_fresh(&editor, "single-line insert");
    // Localized invalidation: changed line ranges must NOT cover all 120 lines
    let changed_span: usize = editor
        .highlighter()
        .last_changed_lines()
        .iter()
        .map(|r| r.len())
        .sum();
    assert!(
        changed_span < 20,
        "Single-line edit should localize invalidation; got {changed_span} lines"
    );

    // 2. Auto-indented newline insertion
    editor.cursors_mut().set_single(Position::new(10, 34));
    editor.insert_newline();
    editor.insert_text("let y = x + 1;");
    assert_ast_matches_fresh(&editor, "newline + body insert");

    // 3. Delete forward & backwards
    editor.cursors_mut().set_single(Position::new(11, 4));
    editor.delete_forward();
    editor.delete_backwards();
    assert_ast_matches_fresh(&editor, "delete_forward + delete_backwards");

    // 4. Multi-cursor simultaneous edit
    editor.cursors_mut().set_single(Position::new(2, 0));
    editor.cursors_mut().add_cursor(Position::new(4, 0));
    editor.insert_text("// note\n");
    assert_ast_matches_fresh(&editor, "multi-cursor edit");

    // 5. Undo & Redo driving incremental InputEdit
    let edits_before_undo = editor.highlighter().incremental_edit_count();
    editor.undo();
    assert!(editor.highlighter().incremental_edit_count() > edits_before_undo);
    assert_ast_matches_fresh(&editor, "undo");

    let edits_before_redo = editor.highlighter().incremental_edit_count();
    editor.redo();
    assert!(editor.highlighter().incremental_edit_count() > edits_before_redo);
    assert_ast_matches_fresh(&editor, "redo");

    // 6. Atomic batch replace_ranges_atomic (used by Replace All)
    editor.replace_ranges_atomic(vec![
        (Position::new(0, 7), Position::new(0, 13), "alpha_0".to_string()),
        (Position::new(1, 7), Position::new(1, 13), "alpha_1".to_string()),
    ]);
    assert!(editor.buffer().line(0).unwrap().contains("fn alpha_0"));
    assert!(editor.buffer().line(1).unwrap().contains("fn alpha_1"));
    assert_ast_matches_fresh(&editor, "replace_ranges_atomic");

    // 7. JavaScript incremental AST equivalence
    let mut js_editor = CodeEditor::new(
        "export function launch(speed) {\n    return speed * 2;\n}\n",
        Language::JavaScript,
    );
    js_editor.cursors_mut().set_single(Position::new(1, 21));
    js_editor.insert_text(" + 10");
    assert!(js_editor.highlighter().incremental_edit_count() >= 1);
    assert_ast_matches_fresh(&js_editor, "javascript insert");
}

#[test]
fn test_ten_languages_tokenization_multiline_state_and_folding() {
    // Verify extension & filename detection for all 10 languages
    assert_eq!(Language::from_extension("rs"), Language::Rust);
    assert_eq!(Language::from_extension("py"), Language::Python);
    assert_eq!(Language::from_extension("json"), Language::Json);
    assert_eq!(Language::from_extension("yaml"), Language::Yaml);
    assert_eq!(Language::from_extension("yml"), Language::Yaml);
    assert_eq!(Language::from_extension("toml"), Language::Toml);
    assert_eq!(Language::from_extension("sdf"), Language::XmlSdf);
    assert_eq!(Language::from_extension("urdf"), Language::XmlSdf);
    assert_eq!(Language::from_extension("xml"), Language::XmlSdf);
    assert_eq!(Language::from_extension("cpp"), Language::Cpp);
    assert_eq!(Language::from_extension("hpp"), Language::Cpp);
    assert_eq!(Language::from_extension("ts"), Language::TypeScript);
    assert_eq!(Language::from_extension("js"), Language::JavaScript);
    assert_eq!(Language::from_extension("md"), Language::Markdown);
    assert_eq!(Language::from_extension("sh"), Language::Shell);
    assert_eq!(Language::from_filename("Cargo.toml"), Language::Toml);
    assert_eq!(Language::from_filename("model.sdf"), Language::XmlSdf);
    assert_eq!(Language::from_filename("deploy.bash"), Language::Shell);

    // 1. Rust: doc comments, macro, raw string, control flow
    let rs = "/// Computes force\npub fn force(m: f64) -> f64 {\n    println!(r#\"raw\"#);\n    return m * 9.81;\n}";
    let rs_ed = CodeEditor::new(rs, Language::Rust);
    let rs_t0 = rs_ed.highlighter().highlight_line_ast(0, rs_ed.buffer().line(0).unwrap());
    assert!(rs_t0.iter().any(|t| t.token_type == TokenType::DocComment));
    let rs_t2 = rs_ed.highlighter().highlight_line_ast(2, rs_ed.buffer().line(2).unwrap());
    assert!(rs_t2.iter().any(|t| t.token_type == TokenType::Macro || t.text == "println"));
    let rs_t3 = rs_ed.highlighter().highlight_line_ast(3, rs_ed.buffer().line(3).unwrap());
    assert!(rs_t3.iter().any(|t| t.token_type == TokenType::ControlFlow && t.text == "return"));

    // 2. Python: decorator, def, triple-quoted multiline string, indentation fold
    let py = "@dataclass\ndef fly(alt: float):\n    \"\"\"Multi-line\n    docstring\n    \"\"\"\n    if alt > 0:\n        return True\n";
    let py_ed = CodeEditor::new(py, Language::Python);
    let py_t3 = py_ed.highlighter().highlight_line_ast(3, py_ed.buffer().line(3).unwrap());
    assert!(
        py_t3.iter().any(|t| t.token_type == TokenType::String),
        "Middle line of triple-quoted Python docstring must tokenize as String: {py_t3:?}"
    );
    let py_folds = py_ed.highlighter().compute_fold_ranges(py_ed.buffer());
    assert!(
        py_folds.iter().any(|f| f.start_line == 1 && f.end_line >= 5),
        "Python def block must produce indentation fold: {py_folds:?}"
    );

    // 3. JSON: properties, escapes, booleans, numbers, folds
    let json = "{\n  \"vehicle\": \"rover\\n1\",\n  \"armed\": true,\n  \"altitude\": 120.5\n}";
    let json_ed = CodeEditor::new(json, Language::Json);
    let j1 = json_ed.highlighter().highlight_line_ast(1, json_ed.buffer().line(1).unwrap());
    assert!(j1.iter().any(|t| t.token_type == TokenType::Property));
    assert!(j1.iter().any(|t| t.token_type == TokenType::String));
    let j2 = json_ed.highlighter().highlight_line_ast(2, json_ed.buffer().line(2).unwrap());
    assert!(j2.iter().any(|t| t.token_type == TokenType::Boolean && t.text == "true"));
    assert!(!json_ed.highlighter().compute_fold_ranges(json_ed.buffer()).is_empty());

    // 4. YAML: keys, anchors, block scalar, indentation folds
    let yaml = "defaults: &base\n  enabled: true\n  script: |\n    echo hello\n    echo world\nmission:\n  <<: *base\n";
    let yaml_ed = CodeEditor::new(yaml, Language::Yaml);
    let y0 = yaml_ed.highlighter().highlight_line_ast(0, yaml_ed.buffer().line(0).unwrap());
    assert!(y0.iter().any(|t| t.token_type == TokenType::Property && t.text == "defaults"));
    let y3 = yaml_ed.highlighter().highlight_line_ast(3, yaml_ed.buffer().line(3).unwrap());
    assert!(
        y3.iter().any(|t| t.token_type == TokenType::String),
        "YAML block scalar body must highlight as String: {y3:?}"
    );
    assert!(!yaml_ed.highlighter().compute_fold_ranges(yaml_ed.buffer()).is_empty());

    // 5. TOML: section headers, keys, multiline strings, folds
    let toml = "[package]\nname = \"strata\"\ndesc = \"\"\"\nline one\nline two\n\"\"\"\n[dependencies]\nserde = \"1.0\"\n";
    let toml_ed = CodeEditor::new(toml, Language::Toml);
    let t0 = toml_ed.highlighter().highlight_line_ast(0, toml_ed.buffer().line(0).unwrap());
    assert!(t0.iter().any(|t| t.token_type == TokenType::Heading || t.token_type == TokenType::Tag));
    let t3 = toml_ed.highlighter().highlight_line_ast(3, toml_ed.buffer().line(3).unwrap());
    assert!(t3.iter().any(|t| t.token_type == TokenType::String));
    assert!(!toml_ed.highlighter().compute_fold_ranges(toml_ed.buffer()).is_empty());

    // 6. XML/SDF: tags, attributes, multiline comments, tag-pair folds
    let sdf = "<?xml version=\"1.0\"?>\n<sdf version=\"1.9\">\n  <!-- multi\n       comment -->\n  <model name=\"rover\">\n    <pose>0 0 1 0 0 0</pose>\n  </model>\n</sdf>";
    let sdf_ed = CodeEditor::new(sdf, Language::XmlSdf);
    let s1 = sdf_ed.highlighter().highlight_line_ast(1, sdf_ed.buffer().line(1).unwrap());
    assert!(s1.iter().any(|t| t.token_type == TokenType::Tag && t.text.contains("sdf")));
    assert!(s1.iter().any(|t| t.token_type == TokenType::Attribute && t.text == "version"));
    let s3 = sdf_ed.highlighter().highlight_line_ast(3, sdf_ed.buffer().line(3).unwrap());
    assert!(s3.iter().any(|t| t.token_type == TokenType::Comment));
    let sdf_folds = sdf_ed.highlighter().compute_fold_ranges(sdf_ed.buffer());
    assert!(
        sdf_folds.iter().any(|f| f.start_line == 1 && f.end_line == 7),
        "SDF <sdf>..</sdf> must fold: {sdf_folds:?}"
    );
    assert!(
        sdf_folds.iter().any(|f| f.start_line == 4 && f.end_line == 6),
        "SDF <model>..</model> must fold: {sdf_folds:?}"
    );

    // 7. C/C++: preprocessor, namespace, template, block comment, folds
    let cpp = "#include <vector>\nnamespace sim {\n/* block\n   comment */\nclass Drone {\n    double mass = 1.5;\n};\n}";
    let cpp_ed = CodeEditor::new(cpp, Language::Cpp);
    let c0 = cpp_ed.highlighter().highlight_line_ast(0, cpp_ed.buffer().line(0).unwrap());
    assert!(c0.iter().any(|t| t.token_type == TokenType::Macro));
    let c3 = cpp_ed.highlighter().highlight_line_ast(3, cpp_ed.buffer().line(3).unwrap());
    assert!(c3.iter().any(|t| t.token_type == TokenType::Comment));
    assert!(!cpp_ed.highlighter().compute_fold_ranges(cpp_ed.buffer()).is_empty());

    // 8. TypeScript: interface, type annotations, arrow functions
    let ts = "export interface Pose {\n  lat: number;\n}\nconst norm = (x: number): number => x;\n";
    let ts_ed = CodeEditor::new(ts, Language::TypeScript);
    let ts0 = ts_ed.highlighter().highlight_line_ast(0, ts_ed.buffer().line(0).unwrap());
    assert!(ts0.iter().any(|t| t.token_type == TokenType::Keyword && t.text == "interface"));

    // 9. Markdown: headings, links, fenced code block with embedded Rust highlighting
    let md = "# Flight Plan\nSee [docs](https://example.com)\n```rust\nfn takeoff() -> bool { true }\n```\n";
    let md_ed = CodeEditor::new(md, Language::Markdown);
    let m0 = md_ed.highlighter().highlight_line_ast(0, md_ed.buffer().line(0).unwrap());
    assert!(m0.iter().any(|t| t.token_type == TokenType::Heading));
    let m1 = md_ed.highlighter().highlight_line_ast(1, md_ed.buffer().line(1).unwrap());
    assert!(m1.iter().any(|t| t.token_type == TokenType::Link));
    let m3 = md_ed.highlighter().highlight_line_ast(3, md_ed.buffer().line(3).unwrap());
    assert!(
        m3.iter().any(|t| t.token_type == TokenType::Keyword && t.text == "fn"),
        "Embedded rust fence in Markdown should highlight Rust keywords: {m3:?}"
    );
    assert!(!md_ed.highlighter().compute_fold_ranges(md_ed.buffer()).is_empty());

    // 10. Shell: shebang, variables, control flow, heredoc, folds
    let sh = "#!/usr/bin/env bash\nif [ -n \"$HOME\" ]; then\n  cat <<EOF\n  inside heredoc\nEOF\nfi\n";
    let sh_ed = CodeEditor::new(sh, Language::Shell);
    let sh1 = sh_ed.highlighter().highlight_line_ast(1, sh_ed.buffer().line(1).unwrap());
    assert!(sh1.iter().any(|t| t.token_type == TokenType::ControlFlow && t.text == "if"));
    let sh3 = sh_ed.highlighter().highlight_line_ast(3, sh_ed.buffer().line(3).unwrap());
    assert!(
        sh3.iter().any(|t| t.token_type == TokenType::String),
        "Shell heredoc body must highlight as String: {sh3:?}"
    );
    assert!(!sh_ed.highlighter().compute_fold_ranges(sh_ed.buffer()).is_empty());
}

#[test]
fn test_incremental_multiline_state_convergence_and_diagnostics() {
    // Multi-line comment propagation & early convergence on C++
    let mut cpp = String::from("int a = 1;\nint b = 2;\nint c = 3;\n");
    for i in 4..40 {
        cpp.push_str(&format!("int v{i} = {i};\n"));
    }
    let mut ed = CodeEditor::new(&cpp, Language::Cpp);

    // Line 10 initially starts with type `int`
    let before_toks = ed.highlighter().highlight_line_ast(10, ed.buffer().line(10).unwrap());
    assert!(before_toks.iter().any(|t| t.token_type == TokenType::Type && t.text == "int"));

    // Open an unclosed block comment on line 1 -> line 10 must become Comment
    ed.cursors_mut().set_single(Position::new(1, 0));
    ed.insert_text("/* ");
    let during_toks = ed.highlighter().highlight_line_ast(10, ed.buffer().line(10).unwrap());
    assert!(
        during_toks.iter().all(|t| t.token_type == TokenType::Comment),
        "Line 10 must turn into Comment when line 1 opens unclosed /*: {during_toks:?}"
    );

    // Close the block comment on line 2 -> line 10 must immediately recover to Type `int`
    ed.cursors_mut().set_single(Position::new(2, 0));
    ed.insert_text("*/ ");
    let after_toks = ed.highlighter().highlight_line_ast(10, ed.buffer().line(10).unwrap());
    assert!(
        after_toks.iter().any(|t| t.token_type == TokenType::Type && t.text == "int"),
        "Line 10 must recover after closing */ on line 2: {after_toks:?}"
    );

    // Syntax diagnostics: Rust AST error
    let ok_rs = CodeEditor::new("fn valid() -> i32 { 42 }", Language::Rust);
    assert!(ok_rs.highlighter().collect_syntax_diagnostics(ok_rs.buffer()).is_empty());

    let bad_rs = CodeEditor::new("fn broken( {", Language::Rust);
    assert!(
        !bad_rs.highlighter().collect_syntax_diagnostics(bad_rs.buffer()).is_empty(),
        "Broken Rust must emit Tree-Sitter syntax diagnostics"
    );

    // Syntax diagnostics: JSON malformed vs valid
    let ok_json = CodeEditor::new("{\"a\": [1, 2]}", Language::Json);
    assert!(ok_json.highlighter().collect_syntax_diagnostics(ok_json.buffer()).is_empty());
    let bad_json = CodeEditor::new("{\"a\": [1, 2}", Language::Json);
    assert!(!bad_json.highlighter().collect_syntax_diagnostics(bad_json.buffer()).is_empty());

    // Syntax diagnostics: XML/SDF mismatched closing tag
    let ok_sdf = CodeEditor::new("<sdf><model/></sdf>", Language::XmlSdf);
    assert!(ok_sdf.highlighter().collect_syntax_diagnostics(ok_sdf.buffer()).is_empty());
    let bad_sdf = CodeEditor::new("<sdf><model></world></sdf>", Language::XmlSdf);
    assert!(!bad_sdf.highlighter().collect_syntax_diagnostics(bad_sdf.buffer()).is_empty());
}

#[test]
fn test_utf8_multibyte_code_editor_insert_delete_undo_redo() {
    let mut ed = CodeEditor::new("let msg = \"🦀\";", Language::Rust);
    ed.cursors_mut().set_single(Position::new(0, 12)); // right after 🦀
    ed.insert_text("東京🚀");
    assert_eq!(ed.buffer().line(0), Some("let msg = \"🦀東京🚀\";"));
    assert_eq!(ed.cursors().primary().focus, Position::new(0, 15));

    // Delete forward over closing quote, then backspace over 🚀
    ed.delete_forward();
    assert_eq!(ed.buffer().line(0), Some("let msg = \"🦀東京🚀;"));
    ed.delete_backwards();
    assert_eq!(ed.buffer().line(0), Some("let msg = \"🦀東京;"));

    // Undo back to original cleanly without UTF-8 panic
    ed.undo();
    ed.undo();
    ed.undo();
    assert_eq!(ed.buffer().line(0), Some("let msg = \"🦀\";"));
    ed.redo();
    assert_eq!(ed.buffer().line(0), Some("let msg = \"🦀東京🚀\";"));

    // Rendering multi-byte syntax tokens must never panic on char boundaries
    let _bp = ed.render();
}

#[test]
fn test_find_replace_literal_case_whole_word_regex_and_atomic_replace_all() {
    let original = "let foo_1 = bar;\nlet Foo_2 = foobar;\nlet foo_3 = bar + BAR;\n";
    let mut ed = CodeEditor::new(original, Language::Rust);
    ed.open_replace();

    // 1. Case-insensitive literal search
    ed.set_find_query("bar");
    assert_eq!(ed.search().matches.len(), 4); // bar, foobar, bar, BAR
    assert_eq!(ed.search().status_label(), "1 of 4");

    // 2. Whole-word toggle excludes `foobar`
    let mut s = std::mem::take(ed.search_mut());
    s.toggle_whole_word(ed.buffer());
    *ed.search_mut() = s;
    assert_eq!(ed.search().matches.len(), 3); // bar, bar, BAR

    // 3. Case-sensitive toggle excludes `BAR`
    let mut s = std::mem::take(ed.search_mut());
    s.toggle_case_sensitive(ed.buffer());
    *ed.search_mut() = s;
    assert_eq!(ed.search().matches.len(), 2); // bar on line 0 and line 2

    // 4. Next/prev wraparound syncs cursor selection
    let m1 = ed.find_next().expect("second match");
    assert_eq!(m1.start.line, 2);
    assert_eq!(ed.cursors().primary().ordered_range(), (m1.start, m1.end));
    let m0 = ed.find_next().expect("wraps to first match");
    assert_eq!(m0.start.line, 0);
    let m_prev = ed.find_prev().expect("wraps backwards to last match");
    assert_eq!(m_prev.start.line, 2);

    // 5. Invalid regex never panics and surfaces regex_error cleanly
    let mut s = std::mem::take(ed.search_mut());
    s.use_regex = true;
    s.whole_word = false;
    s.case_sensitive = false;
    s.set_query("[unclosed", ed.buffer());
    *ed.search_mut() = s;
    assert!(ed.search().regex_error.is_some());
    assert_eq!(ed.search().status_label(), "Invalid regex");

    // 6. Regex capture-group Replace All in ONE atomic undo transaction
    ed.set_find_query(r"foo_(\d+)");
    assert_eq!(ed.search().matches.len(), 3);
    ed.set_replace_text("item_$1");
    let replaced = ed.replace_all();
    assert_eq!(replaced, 3);
    assert!(ed.text().contains("let item_1 = bar;"));
    assert!(ed.text().contains("let item_2 = foobar;"));
    assert!(ed.text().contains("let item_3 = bar + BAR;"));

    // Single undo reverts all 3 regex replacements at once
    ed.undo();
    assert_eq!(ed.text(), original);
}

#[test]
fn test_fold_aware_virtualization_and_row_projection() {
    let sdf = "<sdf version=\"1.9\">\n  <model name=\"a\">\n    <link name=\"base\"/>\n  </model>\n  <model name=\"b\">\n    <link name=\"rotor\"/>\n  </model>\n</sdf>";
    let mut ed = CodeEditor::new(sdf, Language::XmlSdf);
    assert_eq!(ed.visible_logical_lines().len(), 8);

    // Collapse first <model> (lines 1..=3 -> hides lines 2 and 3)
    assert!(ed.toggle_fold_at(1));
    assert!(ed.is_folded(1));
    assert_eq!(ed.folded_hidden_count(1), Some(2));
    assert_eq!(ed.visible_logical_lines(), vec![0, 1, 4, 5, 6, 7]);
    assert_eq!(ed.logical_to_display_row(2), None);
    assert_eq!(ed.logical_to_display_row(4), Some(2));
    assert_eq!(ed.display_row_to_logical(2), Some(4));

    // Fold all -> only outer <sdf> line 0 stays visible
    ed.fold_all();
    assert_eq!(ed.visible_logical_lines(), vec![0]);

    // Unfold all restores every line
    ed.unfold_all();
    assert_eq!(ed.visible_logical_lines().len(), 8);

    // Interactive render shows fold badge when folded
    let handle = CodeEditorHandle::new(sdf, Language::XmlSdf);
    handle.with_editor_mut(|e| {
        e.toggle_fold_at(1);
    });
    let bp = handle.render_interactive(&CodeEditorOptions::default());
    let debug_str = format!("{bp:?}");
    assert!(
        debug_str.contains("⋯ 2 lines"),
        "Collapsed fold pill badge must appear in interactive blueprint"
    );
    assert!(debug_str.contains("data-fold-toggle"));
}

#[test]
fn test_diagnostics_merging_syntax_and_external_lsp() {
    let mut ed = CodeEditor::new("fn broken( {", Language::Rust);
    assert!(
        ed.diagnostic_counts().errors >= 1,
        "Broken Rust should emit live syntax error"
    );

    // Attach external LSP warning and hint
    ed.set_external_diagnostics(vec![
        EditorDiagnostic::warning(
            Position::new(0, 0),
            Position::new(0, 2),
            "Unused function",
        )
        .with_code("W001")
        .with_source("rust-analyzer"),
        EditorDiagnostic::hint(Position::new(0, 3), Position::new(0, 9), "Consider docs"),
    ]);

    let counts = ed.diagnostic_counts();
    assert!(counts.errors >= 1);
    assert_eq!(counts.warnings, 1);
    assert_eq!(counts.hints, 1);
    assert_eq!(
        ed.diagnostics().max_severity_on_line(0),
        Some(DiagnosticSeverity::Error)
    );

    // Fix the syntax error -> live syntax error clears while external warning stays
    ed.set_text("fn fixed() -> i32 { 42 }");
    let after = ed.diagnostic_counts();
    assert_eq!(after.errors, 0);
    assert_eq!(after.warnings, 1);
    assert_eq!(
        ed.diagnostics().max_severity_on_line(0),
        Some(DiagnosticSeverity::Warning)
    );
}

#[test]
fn test_minimap_canvas_commands_scaling_and_scrub_to_scroll() {
    let mut large = String::new();
    for i in 0..20_000 {
        large.push_str(&format!("pub fn node_{i:05}(val: f64) -> f64 {{ val * 2.0 }}\n"));
    }
    let mut ed = CodeEditor::new(&large, Language::Rust);
    ed.open_find();
    ed.set_find_query("node_10000");
    ed.set_external_diagnostics(vec![EditorDiagnostic::error(
        Position::new(5_000, 0),
        Position::new(5_000, 10),
        "Simulated error",
    )]);

    let cfg = MinimapConfig::default();
    let cmds = render_minimap_canvas_commands(
        &ed,
        0.0,
        480.0,
        22.0,
        480.0,
        &cfg,
        &EditorTheme::obsidian_dark(),
    );
    assert!(!cmds.is_empty());
    assert!(
        cmds.len() < 2_500,
        "20k-line minimap must downsample to bounded CanvasCommands; actual {}",
        cmds.len()
    );

    // Monotonic click-to-scroll mapping
    let top_scroll = minimap_y_to_scroll_top(0.0, 480.0, ed.line_count(), 480.0, 22.0, &cfg);
    let mid_scroll = minimap_y_to_scroll_top(240.0, 480.0, ed.line_count(), 480.0, 22.0, &cfg);
    let bot_scroll = minimap_y_to_scroll_top(480.0, 480.0, ed.line_count(), 480.0, 22.0, &cfg);
    assert_eq!(top_scroll, 0.0);
    assert!(mid_scroll > top_scroll);
    assert!(bot_scroll > mid_scroll);
}

#[test]
fn test_interactive_code_editor_widget_keymap_mouse_rsx_and_css_variables() {
    use strata_dom::{Attribute, Event, EventType, NodeBlueprint, Tag};
    use strata_render::canvas2d::CanvasCommand;

    let handle = CodeEditorHandle::new("fn main() {\n    let x = 10;\n    let y = 20;\n}", Language::Rust);

    // 1. RSX drop-in instantiation with CSS-variable theme & Canvas2D minimap payload
    let view = strata_macro::rsx! {
        <CodeEditorWidget
            handle={handle.clone()}
            language={Language::Rust}
            theme={EditorTheme::obsidian_dark()}
            show_minimap={true}
            show_line_numbers={true}
            show_fold_gutter={true}
            show_status_bar={true}
            read_only={false}
            class="h-full w-full"
        />
    };

    let debug_bp = format!("{:?}", view.root);
    assert!(debug_bp.contains("var(--strata-editor-bg"));
    assert!(debug_bp.contains("var(--strata-editor-tok-keyword"));
    assert!(debug_bp.contains("data-editor-minimap"));
    assert!(debug_bp.contains("data-editor-statusbar"));

    // Verify minimap Tag::Canvas carries an Arc<Vec<CanvasCommand>> ScenePayload
    fn find_minimap_payload(node: &NodeBlueprint) -> bool {
        if let NodeBlueprint::Element {
            tag,
            attributes,
            children,
            ..
        } = node
        {
            if *tag == Tag::Canvas {
                for attr in attributes {
                    if let Attribute::ScenePayload(p) = attr {
                        if p.downcast_ref::<Vec<CanvasCommand>>().is_some()
                            || p.downcast_ref::<std::sync::Arc<Vec<CanvasCommand>>>()
                                .is_some()
                        {
                            return true;
                        }
                    }
                }
            }
            return children.iter().any(find_minimap_payload);
        }
        false
    }
    assert!(
        find_minimap_payload(&view.root),
        "Minimap Tag::Canvas must carry Vec<CanvasCommand> ScenePayload"
    );

    // 2. Keyboard interactions: Ctrl+F opens Find bar, Escape closes it, Ctrl+/ toggles comments
    handle.with_editor_mut(|ed| {
        let mut ctrl_f = Event::new(EventType::KeyDown);
        ctrl_f.ctrl_key = true;
        ctrl_f.key = "f".to_string();
        assert!(ed.handle_key_event(&ctrl_f));
        assert!(ed.search().is_open);
    });
    let with_find = handle.render_interactive(&CodeEditorOptions::default());
    assert!(format!("{with_find:?}").contains("data-find-replace-bar"));

    handle.with_editor_mut(|ed| {
        let mut esc = Event::new(EventType::KeyDown);
        esc.key = "Escape".to_string();
        assert!(ed.handle_key_event(&esc));
        assert!(!ed.search().is_open);

        // Toggle comment on lines 1..=2
        ed.cursors_mut().set_single(Position::new(1, 0));
        ed.cursors_mut().all_mut()[0] =
            Selection::range(Position::new(1, 0), Position::new(2, 5));
        let mut ctrl_slash = Event::new(EventType::KeyDown);
        ctrl_slash.ctrl_key = true;
        ctrl_slash.key = "/".to_string();
        assert!(ed.handle_key_event(&ctrl_slash));
        assert!(ed.buffer().line(1).unwrap().trim_start().starts_with("// "));
        assert!(ed.buffer().line(2).unwrap().trim_start().starts_with("// "));

        // Toggle again uncomments cleanly
        assert!(ed.handle_key_event(&ctrl_slash));
        assert_eq!(ed.buffer().line(1), Some("    let x = 10;"));

        // Tab / Shift+Tab multi-line indent & outdent
        ed.cursors_mut().set_single(Position::new(1, 0));
        ed.cursors_mut().all_mut()[0] =
            Selection::range(Position::new(1, 0), Position::new(2, 5));
        let mut tab = Event::new(EventType::KeyDown);
        tab.key = "Tab".to_string();
        assert!(ed.handle_key_event(&tab));
        assert_eq!(ed.buffer().line(1), Some("        let x = 10;"));
        tab.shift_key = true;
        assert!(ed.handle_key_event(&tab));
        assert_eq!(ed.buffer().line(1), Some("    let x = 10;"));

        // Pointer double-click selects word, Alt+Click adds second cursor
        ed.handle_pointer_down(72.0 + 36.0, 22.0 + 5.0, 2, false, false, 72.0);
        assert!(!ed.cursors().primary().is_collapsed());
        ed.handle_pointer_down(72.0 + 10.0, 5.0, 1, true, false, 72.0);
        assert_eq!(ed.cursors().count(), 2);

        // Read-only mode blocks mutations while permitting navigation & Find
        ed.set_read_only(true);
        let before = ed.text();
        let mut ch = Event::new(EventType::KeyDown);
        ch.key = "x".to_string();
        assert!(ed.handle_key_event(&ch));
        assert_eq!(ed.text(), before);
    });

    // 3. Light theme CSS variable preset emits daylight-light styling
    handle.set_theme(EditorTheme::daylight_light());
    let light_bp = handle.render_interactive(&CodeEditorOptions::default());
    assert!(format!("{light_bp:?}").contains("daylight-light"));
    assert!(EditorTheme::daylight_light()
        .to_css_variables_block()
        .contains("--strata-editor-bg: #ffffff"));

    // 4. Reactive hook persistence across two render passes
    strata_dom::reset_component_hooks();
    let v1 = strata_dom::with_root_component_scope(|| {
        let h = use_code_editor("alpha", Language::PlainText);
        h.with_editor_mut(|e| e.insert_text("beta "));
        h.text()
    });
    let v2 = strata_dom::with_root_component_scope(|| {
        let h = use_code_editor("alpha", Language::PlainText);
        h.text()
    });
    assert_eq!(v1, "beta alpha");
    assert_eq!(v2, "beta alpha");
}


