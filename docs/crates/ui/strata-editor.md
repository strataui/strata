# strata_editor: Headless Document Model, Rich-Text WYSIWYG & Code Editor

The `strata_editor` crate provides a unified, zero-DOM text manipulation and document authoring engine for Strata applications. Operating without browser contenteditable hacks or external web runtimes, it fulfills two complementary roles:
1. **Headless WYSIWYG Document Editor**: A structured block-and-mark document model equivalent to TipTap, Lexical, and Slate, with bidirectional GitHub Flavored Markdown serialization.
2. **High-Performance Code Editor Engine**: A multi-cursor, virtualized line buffer code editor engine equivalent to Monaco and Tree-Sitter, featuring incremental syntax highlighting, bracket matching, code folding, transactional undo/redo coalescing, CJK IME composition, and multi-format clipboard exchange.

---

## Architectural Overview

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        Strata Editor Core                              │
├───────────────────────────────────┬────────────────────────────────────┤
│     WYSIWYG Document Layer        │         Code Editor Engine         │
│  (TipTap / Lexical Equivalence)   │      (Monaco / Helix Equivalence)  │
├───────────────────────────────────┼────────────────────────────────────┤
│ • Document (Tree of BlockNodes)   │ • LineBuffer (Piece table / lines) │
│ • BlockKind (Heading, Code, ...)  │ • MultiCursor & Selection Set      │
│ • InlineSpan & InlineMark         │ • SyntaxHighlighter (Tree-Sitter)  │
│ • GFM Markdown Parser & Emitter   │ • HistoryStack (Coalesced undo)    │
│ • Transactional Document Mutations│ • ImeComposition (CJK input)       │
├───────────────────────────────────┴────────────────────────────────────┤
│                       Cross-Cutting Services                           │
│     ClipboardBridge (Plain, Markdown, HTML, RTF) • Reactive Signals    │
└────────────────────────────────────────────────────────────────────────┘
```

---

## WYSIWYG Document Model

The rich-text document architecture models documents as strict hierarchical ASTs, avoiding raw HTML string mutations and guaranteeing deterministic document state.

### Core Data Structures

```rust
use strata_editor::prelude::*;

// Create an empty document
let mut doc = Document::new();

// Add a level-1 heading
doc.add_block(BlockNode::heading("h_1", 1, "Incident Post-Mortem: Cluster Ingress Outage"));

// Add a paragraph with rich inline marks
let paragraph = BlockNode {
    id: "p_1".to_string(),
    kind: BlockKind::Paragraph,
    spans: vec![
        InlineSpan::plain("At 14:22 UTC, the primary ingress proxy suffered a "),
        InlineSpan::plain("cascading memory exhaustion").with_mark(InlineMark::Bold),
        InlineSpan::plain(" due to an unindexed telemetry filter query against "),
        InlineSpan::plain("clickhouse-cluster-01").with_mark(InlineMark::Code),
        InlineSpan::plain(". View root cause details in "),
        InlineSpan::plain("RCA-8491").with_mark(InlineMark::Link("https://incidents.internal/8491".into())),
        InlineSpan::plain("."),
    ],
};
doc.add_block(paragraph);

// Add a fenced code block
doc.add_block(BlockNode {
    id: "code_1".to_string(),
    kind: BlockKind::CodeBlock(Some("bash".into())),
    spans: vec![InlineSpan::plain("kubectl drain ingress-gateway-04 --ignore-daemonsets --delete-emptydir-data")],
});
```

#### Document AST Model

| Type | Variant / Field | Description |
| :--- | :--- | :--- |
| `Document` | `blocks: Vec<BlockNode>` | Ordered collection of top-level structural blocks. |
| `BlockNode` | `id: String`, `kind: BlockKind`, `spans: Vec<InlineSpan>` | Unique block node within the document tree. |
| `BlockKind` | `Paragraph` | Standard running text paragraph. |
| | `Heading(u8)` | Section heading (levels 1 through 6). |
| | `Blockquote` | Indented quotation or callout. |
| | `CodeBlock(Option<String>)` | Preformatted code block with optional syntax language. |
| | `BulletListItem` | Unordered list item. |
| | `NumberedListItem(usize)` | Ordered list item with numeric sequence index. |
| | `HorizontalRule` | Visual thematic break divider. |
| `InlineSpan` | `text: String`, `marks: Vec<InlineMark>` | Contiguous text run sharing uniform formatting marks. |
| `InlineMark` | `Bold` | Strong emphasis (`**text**`). |
| | `Italic` | Regular emphasis (`*text*`). |
| | `Code` | Inline monospace code fragment (`` `code` ``). |
| | `Underline` | Underlined text decoration. |
| | `Strike` | Strikethrough text (`~~text~~`). |
| | `Link(String)` | Hyperlink targeting an external URI or resource. |

---

### Markdown Serialization

Documents seamlessly convert to and from GitHub Flavored Markdown (GFM):

```rust
use strata_editor::prelude::{from_markdown, to_markdown, Document};

let raw_markdown = r#"# Distributed Cache Eviction Policy

When node memory reaches **92% utilization**, the runtime triggers LRU eviction:
- Evict expired session keys
- Flush dead trace buffers
- Scale read replicas
"#;

// Parse Markdown into structured AST
let doc: Document = from_markdown(raw_markdown);
assert_eq!(doc.blocks.len(), 5);

// Emit normalized Markdown
let emitted_markdown = to_markdown(&doc);
assert!(emitted_markdown.contains("# Distributed Cache Eviction Policy"));
assert!(emitted_markdown.contains("**92% utilization**"));
```

---

## Code Editor Engine

The code editor subsystem is built around high-throughput string manipulation, multi-cursor navigation, Tree-Sitter grammar integration, and differential render blueprints.

### LineBuffer Operations

`LineBuffer` maintains contiguous text rows optimized for zero-copy line indexing and localized mutations:

```rust
use strata_editor::prelude::LineBuffer;

let mut buffer = LineBuffer::from_str("fn reconcile_cluster() {\n    verify_health();\n}");
assert_eq!(buffer.line_count(), 3);
assert_eq!(buffer.line(1), Some("    verify_health();"));

// Localized character insertion
buffer.insert_str(1, 4, "let _status = ");
assert_eq!(buffer.line(1), Some("    let _status = verify_health();"));

// Line splitting (Enter key)
buffer.split_line(1, 17);
assert_eq!(buffer.line_count(), 4);

// Line joining (Backspace at line start)
buffer.join_line_with_next(1);
assert_eq!(buffer.line_count(), 3);

// Multi-line range deletion
buffer.delete_range(0, 0, 1, 0); // Deletes first line
assert_eq!(buffer.line(0), Some("    let _status = verify_health();"));
```

---

### Multi-Cursor Selection Model

The editor supports multiple independent cursors and non-contiguous selection regions:

```rust
use strata_editor::prelude::{MultiCursor, Position, Selection};

let mut cursors = MultiCursor::new();
assert_eq!(cursors.count(), 1);

// Add auxiliary cursor positions for simultaneous editing
cursors.add_cursor(Position::new(5, 12));
cursors.add_cursor(Position::new(9, 12));
assert_eq!(cursors.count(), 3);

// Selections are directionally normalized
let sel = Selection::range(Position::new(10, 20), Position::new(10, 5));
assert_eq!(sel.ordered_range(), (Position::new(10, 5), Position::new(10, 20)));
assert!(!sel.is_collapsed());
```

---

### Syntax Highlighting & Language Grammars

The syntax highlighter parses token streams and calculates code folding boundaries:

```rust
use strata_editor::prelude::{Language, LineBuffer, Position, SyntaxHighlighter, TokenType};

let highlighter = SyntaxHighlighter::new();
let line = "pub async fn query_telemetry(node_id: &str) -> Result<Metrics, Error> {";

let tokens = highlighter.highlight_line(line, Language::Rust);

for token in tokens {
    match token.token_type {
        TokenType::Keyword => println!("Keyword: {}", token.text),   // "pub", "async", "fn"
        TokenType::Function => println!("Function: {}", token.text), // "query_telemetry"
        TokenType::Type => println!("Type: {}", token.text),         // "Metrics", "Error"
        _ => {}
    }
}

// Compute syntactic code folding regions
let code = "fn start() {\n    loop {\n        tick();\n    }\n}";
let buffer = LineBuffer::from_str(code);
let folds = highlighter.compute_fold_ranges(&buffer);
assert_eq!(folds[0].start_line, 0);
assert_eq!(folds[0].end_line, 4);

// Bracket matching
let match_pos = highlighter.find_matching_bracket(&buffer, Position::new(0, 11));
assert_eq!(match_pos, Some(Position::new(4, 0)));
```

#### Supported Syntax Grammars

`Language::Rust`, `Language::Python`, `Language::JavaScript`, `Language::TypeScript`, `Language::Json`, `Language::Markdown`, `Language::Sql`, `Language::Toml`, `Language::Html`, `Language::Css`, `Language::C`, `Language::Cpp`, `Language::Go`.

---

### Transaction History & Undo Coalescing

The `HistoryStack` tracks structural mutations with millisecond-level keystroke coalescing, grouping rapid character entry into single undoable operations:

```rust
use strata_editor::prelude::{HistoryStack, LineBuffer, Position, Selection};

let mut history = HistoryStack::default();
let mut buffer = LineBuffer::new();

let cursor_initial = vec![Selection::point(Position::new(0, 0))];
let cursor_after_k = vec![Selection::point(Position::new(0, 1))];
let cursor_after_u = vec![Selection::point(Position::new(0, 2))];

// First character insertion
history.record_insert(Position::new(0, 0), "k", cursor_initial.clone(), cursor_after_k.clone());
buffer.insert_char(0, 0, 'k');

// Immediate second insertion (coalesces into single transaction within 300ms window)
history.record_insert(Position::new(0, 1), "u", cursor_after_k.clone(), cursor_after_u.clone());
buffer.insert_char(0, 1, 'u');

// Undo restores state prior to the coalesced sequence
let restored_cursors = history.undo(&mut buffer).expect("Undo succeeds");
assert_eq!(buffer.line(0), Some(""));
assert_eq!(restored_cursors, cursor_initial);

// Redo re-applies full coalesced transaction
let redone_cursors = history.redo(&mut buffer).expect("Redo succeeds");
assert_eq!(buffer.line(0), Some("ku"));
assert_eq!(redone_cursors, cursor_after_u);
```

---

### Multi-Format Clipboard Exchange

When copying selections or entire documents, `ClipboardBridge` exports a multi-MIME payload suitable for OS-level clipboard integration:

```rust
use strata_editor::prelude::{ClipboardBridge, Document, LineBuffer, Selection, Position};

let buffer = LineBuffer::from_str("use strata::telemetry::*;\nlet metric = 42;");
let selection = Selection::range(Position::new(0, 0), Position::new(1, 16));

let payload = ClipboardBridge::from_selection(&buffer, selection);

// Inspect multi-format payload
assert_eq!(payload.plain_text, "use strata::telemetry::*;\nlet metric = 42;");
assert!(payload.html.unwrap().contains("<pre><code>use strata::telemetry"));
assert!(payload.rtf.unwrap().contains(r"{\rtf1"));
```

---

### CJK IME Composition Engine

Handles complex Input Method Editor (IME) composition lifecycles for Japanese, Chinese, and Korean characters without cursor drift or partial commit bugs:

```rust
use strata_editor::prelude::{ImeComposition, ImeState};

let mut ime = ImeComposition::new();
assert!(!ime.is_composing());

// IME sequence begins
ime.start();
assert!(ime.is_composing());

// User inputs preedit phonetics
ime.update("tokyo", 5);
if let ImeState::Composing { ref preedit_text, cursor_offset, .. } = *ime.state() {
    assert_eq!(preedit_text, "tokyo");
    assert_eq!(cursor_offset, 5);
}

// Composition commits final candidate glyph
let committed = ime.commit();
assert_eq!(committed, Some("tokyo".to_string()));
assert_eq!(*ime.state(), ImeState::Inactive);
```

---

## Integrating `CodeEditor` Component

```rust
use strata_dom::prelude::*;
use strata_editor::prelude::{CodeEditor, Language};

#[component]
pub fn ConfigEditorView(initial_config: &'static str) -> View {
    let mut editor = CodeEditor::new(initial_config, Language::Toml);

    // Add keyboard shortcuts and handlers
    editor.cursors_mut().add_cursor(strata_editor::prelude::Position::new(0, 0));

    rsx! {
        <div class="h-full w-full bg-neutral-950 font-mono text-xs border border-neutral-800 rounded-lg overflow-hidden shadow-2xl">
            <header class="flex items-center justify-between px-4 py-2 bg-neutral-900 border-b border-neutral-800 text-neutral-400">
                <span>"cluster-config.toml"</span>
                <span class="text-neutral-500">"TOML • UTF-8"</span>
            </header>
            <div class="p-2 overflow-auto">
                {editor.render()}
            </div>
        </div>
    }
}
```
