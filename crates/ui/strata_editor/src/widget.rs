//! Drop-in interactive Monaco-class `<CodeEditorWidget>` component, reactive `CodeEditorHandle`,
//! and `use_code_editor` hook.
//!
//! Renders a virtualized, fold-aware viewport with line numbers, clickable fold chevrons,
//! diagnostic gutter indicators & inline squiggles, Find & Replace overlay bar, matching
//! bracket highlights, multi-cursor carets & selections, Canvas2D minimap with scrub-to-scroll,
//! Monaco status bar, and full `--strata-editor-*` CSS-variable theming.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use strata_dom::{
    use_ref, use_signal, Attribute, Event, EventType, NodeBlueprint, Signal, Tag, View,
};
use strata_macro::component;
use strata_render::canvas2d::CanvasCommand;

use crate::code_editor::CodeEditor;
use crate::diagnostics::{DiagnosticCounts, DiagnosticSeverity, EditorDiagnostic};
use crate::minimap::{render_minimap_canvas_commands, MinimapConfig};
use crate::syntax::Language;
use crate::theme::EditorTheme;

/// Visual and behavioral configuration for interactive `<CodeEditorWidget>` rendering.
#[derive(Clone)]
pub struct CodeEditorOptions {
    pub show_line_numbers: bool,
    pub show_fold_gutter: bool,
    pub show_minimap: bool,
    pub show_status_bar: bool,
    pub read_only: bool,
    pub font_size: f32,
    pub line_height: f32,
    pub viewport_height: f32,
    pub minimap: MinimapConfig,
    pub theme: Option<EditorTheme>,
    pub class: String,
    pub on_change: Option<Rc<dyn Fn(String)>>,
}

impl std::fmt::Debug for CodeEditorOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeEditorOptions")
            .field("show_line_numbers", &self.show_line_numbers)
            .field("show_fold_gutter", &self.show_fold_gutter)
            .field("show_minimap", &self.show_minimap)
            .field("show_status_bar", &self.show_status_bar)
            .field("read_only", &self.read_only)
            .field("font_size", &self.font_size)
            .field("line_height", &self.line_height)
            .field("viewport_height", &self.viewport_height)
            .field("minimap", &self.minimap)
            .field("theme", &self.theme)
            .field("class", &self.class)
            .field("on_change", &self.on_change.as_ref().map(|_| "<callback>"))
            .finish()
    }
}

impl Default for CodeEditorOptions {
    fn default() -> Self {
        Self {
            show_line_numbers: true,
            show_fold_gutter: true,
            show_minimap: true,
            show_status_bar: true,
            read_only: false,
            font_size: 13.0,
            line_height: 22.0,
            viewport_height: 480.0,
            minimap: MinimapConfig::default(),
            theme: None,
            class: String::new(),
            on_change: None,
        }
    }
}

/// Reactive shared handle to a [`CodeEditor`] instance.
///
/// Wraps `Rc<RefCell<CodeEditor>>` alongside a reactive `Signal<u64>` revision counter so
/// editing, scrolling, folding, and Find & Replace trigger immediate fine-grained UI updates
/// without cloning the underlying B-Tree text rope.
#[derive(Clone)]
pub struct CodeEditorHandle {
    inner: Rc<RefCell<CodeEditor>>,
    revision: Signal<u64>,
    minimap_dragging: Rc<std::cell::Cell<bool>>,
}

impl std::fmt::Debug for CodeEditorHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ed = self.inner.borrow();
        f.debug_struct("CodeEditorHandle")
            .field("language", &ed.language())
            .field("lines", &ed.line_count())
            .field("revision", &self.revision.get())
            .finish()
    }
}

impl CodeEditorHandle {
    /// Creates a new standalone handle initialized with `initial_text` and `language`.
    pub fn new(initial_text: &str, language: Language) -> Self {
        Self {
            inner: Rc::new(RefCell::new(CodeEditor::new(initial_text, language))),
            revision: Signal::new(0),
            minimap_dragging: Rc::new(std::cell::Cell::new(false)),
        }
    }

    fn from_parts(inner: Rc<RefCell<CodeEditor>>, revision: Signal<u64>) -> Self {
        Self {
            inner,
            revision,
            minimap_dragging: Rc::new(std::cell::Cell::new(false)),
        }
    }

    /// Reads the reactive revision signal (subscribes callers to editor updates).
    pub fn version(&self) -> u64 {
        self.revision.get()
    }

    /// Bumps the reactive revision counter to schedule a repaint.
    pub fn notify(&self) {
        let next = self.revision.get().wrapping_add(1);
        self.revision.set(next);
    }

    /// Executes a read-only closure against the inner [`CodeEditor`].
    pub fn with_editor<R>(&self, f: impl FnOnce(&CodeEditor) -> R) -> R {
        let _ = self.revision.get();
        let guard = self.inner.borrow();
        f(&guard)
    }

    /// Executes a mutating closure against the inner [`CodeEditor`] and notifies subscribers.
    pub fn with_editor_mut<R>(&self, f: impl FnOnce(&mut CodeEditor) -> R) -> R {
        let result = {
            let mut guard = self.inner.borrow_mut();
            f(&mut guard)
        };
        self.notify();
        result
    }

    /// Returns the full document text.
    pub fn text(&self) -> String {
        self.with_editor(CodeEditor::text)
    }

    /// Replaces document contents and resets history.
    pub fn set_text(&self, text: &str) {
        self.with_editor_mut(|ed| ed.set_text(text));
    }

    /// Returns the active language.
    pub fn language(&self) -> Language {
        self.with_editor(CodeEditor::language)
    }

    /// Changes syntax language and re-parses the buffer.
    pub fn set_language(&self, language: Language) {
        self.with_editor_mut(|ed| ed.set_language(language));
    }

    /// Updates the active CSS-variable [`EditorTheme`].
    pub fn set_theme(&self, theme: EditorTheme) {
        self.with_editor_mut(|ed| ed.set_theme(theme));
    }

    /// Updates external/LSP diagnostics without clobbering live syntax diagnostics.
    pub fn set_diagnostics(&self, diagnostics: Vec<EditorDiagnostic>) {
        self.with_editor_mut(|ed| ed.set_external_diagnostics(diagnostics));
    }

    /// Returns diagnostic severity counts.
    pub fn diagnostic_counts(&self) -> DiagnosticCounts {
        self.with_editor(CodeEditor::diagnostic_counts)
    }

    /// Opens the Find bar.
    pub fn open_find(&self) {
        self.with_editor_mut(CodeEditor::open_find);
    }

    /// Opens the Find & Replace bar.
    pub fn open_replace(&self) {
        self.with_editor_mut(CodeEditor::open_replace);
    }

    /// Collapses all foldable blocks.
    pub fn fold_all(&self) {
        self.with_editor_mut(CodeEditor::fold_all);
    }

    /// Expands all folded blocks.
    pub fn unfold_all(&self) {
        self.with_editor_mut(CodeEditor::unfold_all);
    }

    /// Renders the complete interactive Monaco-class [`NodeBlueprint`] using `options`.
    pub fn render_interactive(&self, options: &CodeEditorOptions) -> NodeBlueprint {
        render_interactive_editor(self, options)
    }

    /// Renders the interactive editor wrapped in a Strata [`View`].
    pub fn render_view(&self, options: &CodeEditorOptions) -> View {
        View::new(self.render_interactive(options))
    }
}

/// Reactive Strata hook returning a persistent [`CodeEditorHandle`] across component re-renders.
pub fn use_code_editor(initial_text: &str, language: Language) -> CodeEditorHandle {
    let shared_ref = use_ref::<Option<Rc<RefCell<CodeEditor>>>>(None);
    let revision = use_signal(0u64);
    let shared = shared_ref.with_mut(|slot| {
        Rc::clone(slot.get_or_insert_with(|| Rc::new(RefCell::new(CodeEditor::new(initial_text, language)))))
    });
    CodeEditorHandle::from_parts(shared, revision)
}

/// Drop-in Monaco-class interactive `<CodeEditorWidget>` Strata component.
///
/// Supports virtualized rendering, incremental Tree-Sitter & 10-language syntax highlighting,
/// gutter folding, Find & Replace (`Ctrl+F` / `Ctrl+H`), Canvas2D minimap, LSP/syntax
/// diagnostics, multi-cursor keyboard/mouse editing, and `--strata-editor-*` CSS-variable theming.
#[component]
pub fn CodeEditorWidget(
    handle: CodeEditorHandle,
    language: Option<Language>,
    theme: Option<EditorTheme>,
    show_minimap: Option<bool>,
    show_line_numbers: Option<bool>,
    show_fold_gutter: Option<bool>,
    show_status_bar: Option<bool>,
    read_only: Option<bool>,
    font_size: Option<f32>,
    line_height: Option<f32>,
    viewport_height: Option<f32>,
    on_change: Option<Rc<dyn Fn(String)>>,
    class: Option<&'static str>,
) -> View {
    if let Some(lang) = language {
        let needs_switch = handle.inner.borrow().language() != lang;
        if needs_switch {
            handle.inner.borrow_mut().set_language(lang);
        }
    }
    if let Some(ro) = read_only {
        handle.inner.borrow_mut().set_read_only(ro);
    }

    let options = CodeEditorOptions {
        show_line_numbers: show_line_numbers.unwrap_or(true),
        show_fold_gutter: show_fold_gutter.unwrap_or(true),
        show_minimap: show_minimap.unwrap_or(true),
        show_status_bar: show_status_bar.unwrap_or(true),
        read_only: read_only.unwrap_or(false),
        font_size: font_size.unwrap_or(13.0),
        line_height: line_height.unwrap_or(22.0),
        viewport_height: viewport_height.unwrap_or(480.0),
        minimap: MinimapConfig::default(),
        theme,
        class: class.unwrap_or("").to_string(),
        on_change,
    };

    View::new(render_interactive_editor(&handle, &options))
}

/// Alias matching `<CodeEditorView ... />` naming convention.
pub use CodeEditorWidget as CodeEditorView;

/// Builds the full interactive Monaco-class DOM blueprint (with live event listeners,
/// virtualized fold-aware rows, Find & Replace overlay, Canvas2D minimap, and status bar).
pub fn render_interactive_editor(
    handle: &CodeEditorHandle,
    options: &CodeEditorOptions,
) -> NodeBlueprint {
    // Subscribe to reactive revision changes.
    let _rev = handle.version();

    // Synchronize layout metrics & fold cache once per render pass.
    {
        let mut ed = handle.inner.borrow_mut();
        ed.set_read_only(options.read_only);
        ed.set_line_height(options.line_height);
        ed.set_viewport_height(options.viewport_height);
        ed.set_char_width((options.font_size * 0.62).max(5.5));
        let _ = ed.ensure_folds_cached();
    }

    let ed = handle.inner.borrow();
    let theme = options.theme.clone().unwrap_or_else(|| ed.theme().clone());
    let effective_lh = options.line_height.max(14.0);
    let effective_vh = options.viewport_height.max(80.0);
    let scroll_top = ed.scroll_top();
    let gutter_width = compute_gutter_width(options);

    let visible_lines = ed.visible_logical_lines();
    let total_visible = visible_lines.len().max(1);
    let first_visible = (scroll_top / effective_lh).floor() as usize;
    let visible_count = (effective_vh / effective_lh).ceil() as usize;
    let start_idx = first_visible.saturating_sub(2).min(total_visible);
    let end_idx = (first_visible + visible_count + 2).min(total_visible);

    let top_spacer_h = start_idx as f32 * effective_lh;
    let bottom_spacer_h = total_visible.saturating_sub(end_idx) as f32 * effective_lh;

    let primary_pos = ed.cursors().primary().focus;
    let matching_bracket = ed
        .highlighter()
        .find_matching_bracket(ed.buffer(), primary_pos);
    let all_folds = ed.fold_ranges();
    let active_search_idx = ed.search().active_match_index;

    let mut row_nodes = Vec::with_capacity((end_idx - start_idx) + 2);

    if top_spacer_h > 0.0 {
        row_nodes.push(NodeBlueprint::element(
            Tag::Div,
            vec!["w-full pointer-events-none shrink-0".to_string()],
            vec![Attribute::property(
                "style",
                format!("height: {:.1}px;", top_spacer_h),
            )],
            Vec::new(),
        ));
    }

    for &logical_line in &visible_lines[start_idx..end_idx] {
        let Some(line_str) = ed.buffer().line(logical_line) else {
            continue;
        };
        let is_active_line = logical_line == primary_pos.line;
        let line_diag_sev = ed.diagnostics().max_severity_on_line(logical_line);
        let line_diags = ed.diagnostics().diagnostics_on_line(logical_line);
        let is_fold_start = all_folds
            .iter()
            .any(|f| f.start_line == logical_line && f.end_line > f.start_line);
        let hidden_count = ed.folded_hidden_count(logical_line);

        let mut row_children = Vec::with_capacity(4);

        // 1. Diagnostic severity gutter dot + Fold chevron + Line number gutter
        if options.show_line_numbers || options.show_fold_gutter {
            let mut gutter_items = Vec::with_capacity(3);

            // Diagnostic dot
            let diag_dot = if let Some(sev) = line_diag_sev {
                let tooltip = line_diags
                    .first()
                    .map(|d| d.message.clone())
                    .unwrap_or_default();
                NodeBlueprint::element(
                    Tag::Span,
                    vec!["inline-block w-2 h-2 rounded-full mr-1 shrink-0".to_string()],
                    vec![
                        Attribute::property("data-diag-severity", sev.label()),
                        Attribute::property("title", tooltip),
                        Attribute::property(
                            "style",
                            format!("background-color: {};", theme.diagnostic_css(sev)),
                        ),
                    ],
                    Vec::new(),
                )
            } else {
                NodeBlueprint::element(
                    Tag::Span,
                    vec!["inline-block w-2 h-2 mr-1 shrink-0".to_string()],
                    Vec::new(),
                    Vec::new(),
                )
            };
            gutter_items.push(diag_dot);

            // Line number
            if options.show_line_numbers {
                let num_color = if is_active_line {
                    &theme.gutter_active_fg
                } else {
                    &theme.gutter_fg
                };
                gutter_items.push(NodeBlueprint::element(
                    Tag::Span,
                    vec!["w-9 text-right pr-1.5 select-none font-mono text-xs shrink-0".to_string()],
                    vec![Attribute::property("style", format!("color: {};", num_color))],
                    vec![NodeBlueprint::text((logical_line + 1).to_string())],
                ));
            }

            // Clickable fold chevron
            if options.show_fold_gutter {
                if is_fold_start {
                    let is_folded = ed.is_folded(logical_line);
                    let chevron = if is_folded { "▸" } else { "▾" };
                    let h_fold = handle.clone();
                    gutter_items.push(NodeBlueprint::element(
                        Tag::Button,
                        vec![
                            "w-4 h-4 flex items-center justify-center text-[11px] select-none cursor-pointer bg-transparent border-0 p-0 shrink-0"
                                .to_string(),
                        ],
                        vec![
                            Attribute::property("data-fold-toggle", logical_line.to_string()),
                            Attribute::property(
                                "title",
                                if is_folded { "Expand fold" } else { "Collapse fold" },
                            ),
                            Attribute::property(
                                "style",
                                format!("color: {};", theme.gutter_active_fg),
                            ),
                            Attribute::event_listener(EventType::Click, move |e: Event| {
                                e.stop_propagation();
                                h_fold.with_editor_mut(|editor| {
                                    editor.toggle_fold_at(logical_line);
                                });
                            }),
                        ],
                        vec![NodeBlueprint::text(chevron)],
                    ));
                } else {
                    gutter_items.push(NodeBlueprint::element(
                        Tag::Span,
                        vec!["w-4 shrink-0 select-none".to_string()],
                        Vec::new(),
                        Vec::new(),
                    ));
                }
            }

            row_children.push(NodeBlueprint::element(
                Tag::Div,
                vec!["flex items-center px-1.5 select-none shrink-0 border-r".to_string()],
                vec![Attribute::property(
                    "style",
                    format!(
                        "width: {:.0}px; background-color: {}; border-color: {};",
                        gutter_width, theme.gutter_bg, theme.border
                    ),
                )],
                gutter_items,
            ));
        }

        // 2. Syntax-colored code content with search matches, bracket match, carets, diagnostics, and fold badge
        let tokens = ed.highlighter().highlight_line_ast(logical_line, line_str);
        let search_matches = ed.search().matches_on_line(logical_line);
        let line_selections: Vec<(usize, usize)> = ed
            .cursors()
            .all()
            .iter()
            .filter_map(|sel| {
                let (s, e) = sel.ordered_range();
                if sel.is_collapsed() || logical_line < s.line || logical_line > e.line {
                    return None;
                }
                let start_c = if logical_line == s.line { s.col } else { 0 };
                let end_c = if logical_line == e.line {
                    e.col
                } else {
                    line_str.chars().count()
                };
                Some((start_c, end_c))
            })
            .collect();
        let line_carets: Vec<usize> = ed
            .cursors()
            .all()
            .iter()
            .filter(|sel| sel.focus.line == logical_line)
            .map(|sel| sel.focus.col)
            .collect();

        let mut code_spans = build_themed_line_spans(
            line_str,
            &tokens,
            &theme,
            &search_matches,
            active_search_idx,
            &line_selections,
            matching_bracket.filter(|p| p.line == logical_line).map(|p| p.col),
            &line_carets,
            line_diag_sev,
        );

        // Collapsed fold pill `⋯ N lines`
        if let Some(hidden) = hidden_count {
            let h_unfold = handle.clone();
            code_spans.push(NodeBlueprint::element(
                Tag::Button,
                vec![
                    "ml-2 px-1.5 py-0.5 rounded text-[11px] font-mono cursor-pointer border-0"
                        .to_string(),
                ],
                vec![
                    Attribute::property("data-fold-badge", logical_line.to_string()),
                    Attribute::property(
                        "style",
                        format!(
                            "background-color: {}; color: {};",
                            theme.fold_badge_bg, theme.fold_badge_fg
                        ),
                    ),
                    Attribute::event_listener(EventType::Click, move |e: Event| {
                        e.stop_propagation();
                        h_unfold.with_editor_mut(|editor| {
                            editor.toggle_fold_at(logical_line);
                        });
                    }),
                ],
                vec![NodeBlueprint::text(format!("⋯ {hidden} lines"))],
            ));
        }

        let code_cell_style = if let Some(sev) = line_diag_sev {
            format!(
                "text-decoration: underline wavy {};",
                theme.diagnostic_css(sev)
            )
        } else {
            String::new()
        };

        let mut code_attrs = vec![Attribute::property("data-line-index", logical_line.to_string())];
        if !code_cell_style.is_empty() {
            code_attrs.push(Attribute::property("data-has-diagnostic", "true"));
        }

        row_children.push(NodeBlueprint::element(
            Tag::Span,
            vec!["flex-1 pl-2.5 pr-4 whitespace-pre font-mono relative".to_string()],
            code_attrs,
            code_spans,
        ));

        let row_bg = if is_active_line {
            format!(
                "height: {:.1}px; line-height: {:.1}px; background-color: {};",
                effective_lh, effective_lh, theme.line_highlight
            )
        } else {
            format!(
                "height: {:.1}px; line-height: {:.1}px;",
                effective_lh, effective_lh
            )
        };

        row_nodes.push(NodeBlueprint::element(
            Tag::Div,
            vec!["flex items-center w-full shrink-0".to_string()],
            vec![
                Attribute::property("data-editor-row", logical_line.to_string()),
                Attribute::property("style", row_bg),
            ],
            row_children,
        ));
    }

    if bottom_spacer_h > 0.0 {
        row_nodes.push(NodeBlueprint::element(
            Tag::Div,
            vec!["w-full pointer-events-none shrink-0".to_string()],
            vec![Attribute::property(
                "style",
                format!("height: {:.1}px;", bottom_spacer_h),
            )],
            Vec::new(),
        ));
    }

    // Main virtualized viewport pane with pointer + wheel events
    let h_down = handle.clone();
    let h_move = handle.clone();
    let h_up = handle.clone();
    let h_wheel = handle.clone();
    let viewport_node = NodeBlueprint::element(
        Tag::Div,
        vec!["flex-1 min-w-0 h-full overflow-hidden select-none relative".to_string()],
        vec![
            Attribute::property("data-editor-viewport", "true"),
            Attribute::property(
                "style",
                format!(
                    "height: {:.1}px; font-size: {:.1}px;",
                    effective_vh, options.font_size
                ),
            ),
            Attribute::event_listener(EventType::MouseDown, move |e: Event| {
                h_down.with_editor_mut(|editor| {
                    editor.handle_pointer_down(
                        e.offset_x as f32,
                        e.offset_y as f32,
                        e.detail,
                        e.alt_key,
                        e.shift_key,
                        gutter_width,
                    );
                });
            }),
            Attribute::event_listener(EventType::MouseMove, move |e: Event| {
                if e.buttons & 1 != 0 {
                    h_move.with_editor_mut(|editor| {
                        editor.handle_pointer_drag(
                            e.offset_x as f32,
                            e.offset_y as f32,
                            gutter_width,
                        );
                    });
                }
            }),
            Attribute::event_listener(EventType::MouseUp, move |_e: Event| {
                h_up.with_editor_mut(CodeEditor::handle_pointer_up);
            }),
            Attribute::event_listener(EventType::Wheel, move |e: Event| {
                e.prevent_default();
                h_wheel.with_editor_mut(|editor| {
                    editor.handle_wheel(e.delta_y as f32);
                });
            }),
        ],
        row_nodes,
    );

    // Optional Canvas2D minimap
    let mut body_children = vec![viewport_node];
    if options.show_minimap {
        let minimap_cfg = options.minimap;
        let commands: Arc<Vec<CanvasCommand>> = Arc::new(render_minimap_canvas_commands(
            &ed,
            scroll_top,
            effective_vh,
            effective_lh,
            effective_vh,
            &minimap_cfg,
            &theme,
        ));
        let h_mm_down = handle.clone();
        let h_mm_move = handle.clone();
        let h_mm_up = handle.clone();
        let mm_drag_down = handle.minimap_dragging.clone();
        let mm_drag_move = handle.minimap_dragging.clone();
        let mm_drag_up = handle.minimap_dragging.clone();

        let minimap_node = NodeBlueprint::element(
            Tag::Canvas,
            vec!["shrink-0 border-l cursor-pointer".to_string()],
            vec![
                Attribute::property("data-editor-minimap", "true"),
                Attribute::property("width", format!("{:.0}", minimap_cfg.width)),
                Attribute::property("height", format!("{:.0}", effective_vh)),
                Attribute::property(
                    "style",
                    format!(
                        "width: {:.0}px; height: {:.0}px; background-color: {}; border-color: {};",
                        minimap_cfg.width, effective_vh, theme.minimap_bg, theme.border
                    ),
                ),
                Attribute::scene_payload_arc(commands),
                Attribute::event_listener(EventType::MouseDown, move |e: Event| {
                    e.stop_propagation();
                    mm_drag_down.set(true);
                    h_mm_down.with_editor_mut(|editor| {
                        editor.handle_minimap_pointer(
                            e.offset_y as f32,
                            effective_vh,
                            &minimap_cfg,
                        );
                    });
                }),
                Attribute::event_listener(EventType::MouseMove, move |e: Event| {
                    if mm_drag_move.get() && (e.buttons & 1 != 0) {
                        h_mm_move.with_editor_mut(|editor| {
                            editor.handle_minimap_pointer(
                                e.offset_y as f32,
                                effective_vh,
                                &minimap_cfg,
                            );
                        });
                    }
                }),
                Attribute::event_listener(EventType::MouseUp, move |_e: Event| {
                    mm_drag_up.set(false);
                    let _ = &h_mm_up;
                }),
            ],
            Vec::new(),
        );
        body_children.push(minimap_node);
    }

    // Optional Find & Replace bar overlay
    let mut root_children = Vec::with_capacity(3);
    if ed.search().is_open {
        root_children.push(build_find_replace_bar(
            handle,
            &ed,
            &theme,
            options.on_change.clone(),
        ));
    }

    // Split body (viewport + minimap)
    root_children.push(NodeBlueprint::element(
        Tag::Div,
        vec!["flex flex-1 min-h-0 w-full overflow-hidden".to_string()],
        Vec::new(),
        body_children,
    ));

    // Optional Monaco-style status bar
    if options.show_status_bar {
        root_children.push(build_status_bar(&ed, &theme));
    }

    // Drop read borrow before wiring root keyboard listener
    let lang_name = ed.language().display_name();
    drop(ed);

    let h_key = handle.clone();
    let on_change_key = options.on_change.clone();
    let root_classes = if options.class.is_empty() {
        "strata-code-editor flex flex-col font-mono rounded-lg border overflow-hidden relative outline-none"
            .to_string()
    } else {
        format!(
            "strata-code-editor flex flex-col font-mono rounded-lg border overflow-hidden relative outline-none {}",
            options.class
        )
    };

    NodeBlueprint::element(
        Tag::Div,
        vec![root_classes],
        vec![
            Attribute::property("role", "textbox"),
            Attribute::property("aria-multiline", "true"),
            Attribute::property("aria-label", format!("Code editor ({lang_name})")),
            Attribute::property("tabindex", "0"),
            Attribute::property("data-strata-code-editor", "true"),
            Attribute::property("data-editor-theme", theme.name.clone()),
            Attribute::property(
                "style",
                format!(
                    "background-color: {}; color: {}; border-color: {};",
                    theme.bg, theme.fg, theme.border
                ),
            ),
            Attribute::event_listener(EventType::KeyDown, move |e: Event| {
                let mut mutated_text: Option<String> = None;
                let handled = h_key.with_editor_mut(|editor| {
                    let before_rev_len = editor.buffer().total_text_byte_len();
                    let handled = editor.handle_key_event(&e);
                    if handled
                        && on_change_key.is_some()
                        && (editor.buffer().total_text_byte_len() != before_rev_len
                            || matches!(
                                e.key.as_str(),
                                "Enter" | "Backspace" | "Delete" | "Tab" | "z" | "Z" | "y" | "Y" | "/"
                            )
                            || e.key.chars().count() == 1)
                    {
                        mutated_text = Some(editor.text());
                    }
                    handled
                });
                if handled {
                    e.prevent_default();
                    e.stop_propagation();
                    if let (Some(cb), Some(text)) = (&on_change_key, mutated_text) {
                        cb(text);
                    }
                }
            }),
        ],
        root_children,
    )
}

fn compute_gutter_width(options: &CodeEditorOptions) -> f32 {
    let mut w = 12.0; // padding + diagnostic dot
    if options.show_line_numbers {
        w += 42.0;
    }
    if options.show_fold_gutter {
        w += 18.0;
    }
    w
}

#[allow(clippy::too_many_arguments)]
fn build_themed_line_spans(
    line_text: &str,
    tokens: &[crate::syntax::SyntaxToken],
    theme: &EditorTheme,
    search_matches: &[(usize, &crate::search::SearchMatch)],
    active_search_idx: usize,
    selections: &[(usize, usize)],
    matching_bracket_col: Option<usize>,
    carets: &[usize],
    line_diag_sev: Option<DiagnosticSeverity>,
) -> Vec<NodeBlueprint> {
    let mut nodes = Vec::new();
    let line_chars = line_text.chars().count();

    if tokens.is_empty() {
        if !line_text.is_empty() {
            nodes.push(NodeBlueprint::text(line_text));
        }
    } else {
        let mut last_byte = 0usize;
        for tok in tokens {
            let start_b = tok.start_col.min(line_text.len());
            let end_b = tok.end_col.min(line_text.len());
            if start_b > last_byte
                && line_text.is_char_boundary(last_byte)
                && line_text.is_char_boundary(start_b)
            {
                nodes.push(NodeBlueprint::text(&line_text[last_byte..start_b]));
            }

            let tok_char_start = if line_text.is_char_boundary(start_b) {
                line_text[..start_b].chars().count()
            } else {
                0
            };
            let tok_char_end = tok_char_start + tok.text.chars().count();

            let mut style = format!("color: {};", theme.token_css(tok.token_type));
            if matches!(
                tok.token_type,
                crate::syntax::TokenType::Keyword
                    | crate::syntax::TokenType::ControlFlow
                    | crate::syntax::TokenType::Heading
            ) {
                style.push_str(" font-weight: 600;");
            } else if matches!(
                tok.token_type,
                crate::syntax::TokenType::Comment | crate::syntax::TokenType::DocComment
            ) {
                style.push_str(" font-style: italic;");
            }

            // Highlight selection / active search match / bracket pair directly on overlapping tokens
            if selections
                .iter()
                .any(|&(s, e)| tok_char_start < e && tok_char_end > s)
            {
                style.push_str(&format!(" background-color: {};", theme.selection_bg));
            } else if let Some((idx, _)) = search_matches
                .iter()
                .find(|(_, m)| tok_char_start < m.end.col && tok_char_end > m.start.col)
            {
                let bg = if *idx == active_search_idx {
                    &theme.find_active_bg
                } else {
                    &theme.find_match_bg
                };
                style.push_str(&format!(" background-color: {}; border-radius: 2px;", bg));
            } else if let Some(b_col) = matching_bracket_col {
                if b_col >= tok_char_start && b_col < tok_char_end {
                    style.push_str(&format!(
                        " background-color: {}; border-radius: 2px;",
                        theme.bracket_match_bg
                    ));
                }
            }

            if let Some(sev) = line_diag_sev {
                style.push_str(&format!(
                    " border-bottom: 2px dotted {};",
                    theme.diagnostic_css(sev)
                ));
            }

            nodes.push(NodeBlueprint::element(
                Tag::Span,
                Vec::<String>::new(),
                vec![Attribute::property("style", style)],
                vec![NodeBlueprint::text(tok.text.clone())],
            ));
            last_byte = end_b.max(last_byte);
        }
        if last_byte < line_text.len() && line_text.is_char_boundary(last_byte) {
            nodes.push(NodeBlueprint::text(&line_text[last_byte..]));
        }
    }

    // Render crisp multi-cursor caret bars for any caret anchored on this line
    for &caret_col in carets {
        let clamped_col = caret_col.min(line_chars);
        nodes.push(NodeBlueprint::element(
            Tag::Span,
            vec!["inline-block w-[2px] h-[16px] align-middle pointer-events-none".to_string()],
            vec![
                Attribute::property("data-editor-caret-col", clamped_col.to_string()),
                Attribute::property(
                    "style",
                    format!("background-color: {};", theme.caret),
                ),
            ],
            Vec::new(),
        ));
    }

    nodes
}

fn build_find_replace_bar(
    handle: &CodeEditorHandle,
    ed: &CodeEditor,
    theme: &EditorTheme,
    on_change: Option<Rc<dyn Fn(String)>>,
) -> NodeBlueprint {
    let search = ed.search();
    let status = search.status_label();

    let h_query = handle.clone();
    let h_case = handle.clone();
    let h_word = handle.clone();
    let h_regex = handle.clone();
    let h_prev = handle.clone();
    let h_next = handle.clone();
    let h_close = handle.clone();

    let toggle_btn = |label: &str, active: bool, attr: &str, on_click: Box<dyn Fn(Event)>| {
        let style = if active {
            format!(
                "background-color: {}; color: {}; border-color: {};",
                theme.selection_bg, theme.fg, theme.caret
            )
        } else {
            format!(
                "background-color: transparent; color: {}; border-color: {};",
                theme.gutter_fg, theme.panel_border
            )
        };
        NodeBlueprint::element(
            Tag::Button,
            vec!["px-1.5 py-0.5 text-xs font-mono rounded border cursor-pointer".to_string()],
            vec![
                Attribute::property("data-find-action", attr),
                Attribute::property("style", style),
                Attribute::event_listener(EventType::Click, move |e: Event| {
                    e.stop_propagation();
                    on_click(e);
                }),
            ],
            vec![NodeBlueprint::text(label)],
        )
    };

    let find_row = NodeBlueprint::element(
        Tag::Div,
        vec!["flex items-center gap-1.5".to_string()],
        Vec::new(),
        vec![
            NodeBlueprint::element(
                Tag::Input,
                vec!["flex-1 min-w-[140px] px-2 py-0.5 text-xs font-mono rounded border".to_string()],
                vec![
                    Attribute::property("type", "text"),
                    Attribute::property("placeholder", "Find..."),
                    Attribute::property("value", search.query.clone()),
                    Attribute::property("data-find-input", "query"),
                    Attribute::property(
                        "style",
                        format!(
                            "background-color: {}; color: {}; border-color: {};",
                            theme.bg, theme.fg, theme.panel_border
                        ),
                    ),
                    Attribute::value_listener(EventType::Input, move |val: String| {
                        h_query.with_editor_mut(|editor| editor.set_find_query(val));
                    }),
                ],
                Vec::new(),
            ),
            toggle_btn(
                "Aa",
                search.case_sensitive,
                "case",
                Box::new(move |_| {
                    h_case.with_editor_mut(|editor| {
                        let mut s = std::mem::take(editor.search_mut());
                        s.toggle_case_sensitive(editor.buffer());
                        *editor.search_mut() = s;
                    });
                }),
            ),
            toggle_btn(
                "Ab",
                search.whole_word,
                "whole-word",
                Box::new(move |_| {
                    h_word.with_editor_mut(|editor| {
                        let mut s = std::mem::take(editor.search_mut());
                        s.toggle_whole_word(editor.buffer());
                        *editor.search_mut() = s;
                    });
                }),
            ),
            toggle_btn(
                ".*",
                search.use_regex,
                "regex",
                Box::new(move |_| {
                    h_regex.with_editor_mut(|editor| {
                        let mut s = std::mem::take(editor.search_mut());
                        s.toggle_regex(editor.buffer());
                        *editor.search_mut() = s;
                    });
                }),
            ),
            NodeBlueprint::element(
                Tag::Span,
                vec!["text-xs font-mono px-1 min-w-[64px] text-center select-none".to_string()],
                vec![
                    Attribute::property("data-find-status", "true"),
                    Attribute::property("style", format!("color: {};", theme.gutter_active_fg)),
                ],
                vec![NodeBlueprint::text(status)],
            ),
            toggle_btn(
                "↑",
                false,
                "prev",
                Box::new(move |_| {
                    h_prev.with_editor_mut(|editor| {
                        let _ = editor.find_prev();
                    });
                }),
            ),
            toggle_btn(
                "↓",
                false,
                "next",
                Box::new(move |_| {
                    h_next.with_editor_mut(|editor| {
                        let _ = editor.find_next();
                    });
                }),
            ),
            toggle_btn(
                "×",
                false,
                "close",
                Box::new(move |_| {
                    h_close.with_editor_mut(CodeEditor::close_find);
                }),
            ),
        ],
    );

    let mut rows = vec![find_row];

    if search.show_replace {
        let h_repl_in = handle.clone();
        let h_repl_one = handle.clone();
        let h_repl_all = handle.clone();
        let cb_one = on_change.clone();
        let cb_all = on_change;

        rows.push(NodeBlueprint::element(
            Tag::Div,
            vec!["flex items-center gap-1.5 mt-1".to_string()],
            Vec::new(),
            vec![
                NodeBlueprint::element(
                    Tag::Input,
                    vec!["flex-1 min-w-[140px] px-2 py-0.5 text-xs font-mono rounded border".to_string()],
                    vec![
                        Attribute::property("type", "text"),
                        Attribute::property("placeholder", "Replace..."),
                        Attribute::property("value", search.replacement.clone()),
                        Attribute::property("data-find-input", "replace"),
                        Attribute::property(
                            "style",
                            format!(
                                "background-color: {}; color: {}; border-color: {};",
                                theme.bg, theme.fg, theme.panel_border
                            ),
                        ),
                        Attribute::value_listener(EventType::Input, move |val: String| {
                            h_repl_in.with_editor_mut(|editor| editor.set_replace_text(val));
                        }),
                    ],
                    Vec::new(),
                ),
                toggle_btn(
                    "Replace",
                    false,
                    "replace",
                    Box::new(move |_| {
                        let updated = h_repl_one.with_editor_mut(|editor| {
                            if editor.replace_current() {
                                Some(editor.text())
                            } else {
                                None
                            }
                        });
                        if let (Some(cb), Some(text)) = (&cb_one, updated) {
                            cb(text);
                        }
                    }),
                ),
                toggle_btn(
                    "All",
                    false,
                    "replace-all",
                    Box::new(move |_| {
                        let updated = h_repl_all.with_editor_mut(|editor| {
                            if editor.replace_all() > 0 {
                                Some(editor.text())
                            } else {
                                None
                            }
                        });
                        if let (Some(cb), Some(text)) = (&cb_all, updated) {
                            cb(text);
                        }
                    }),
                ),
            ],
        ));
    }

    NodeBlueprint::element(
        Tag::Div,
        vec!["px-2.5 py-1.5 border-b flex flex-col shrink-0".to_string()],
        vec![
            Attribute::property("data-find-replace-bar", "true"),
            Attribute::property(
                "style",
                format!(
                    "background-color: {}; border-color: {};",
                    theme.panel_bg, theme.panel_border
                ),
            ),
        ],
        rows,
    )
}

fn build_status_bar(ed: &CodeEditor, theme: &EditorTheme) -> NodeBlueprint {
    let primary = ed.cursors().primary().focus;
    let counts = ed.diagnostic_counts();
    let cursor_badge = if ed.cursors().count() > 1 {
        format!(
            "Ln {}, Col {} ({} cursors)",
            primary.line + 1,
            primary.col + 1,
            ed.cursors().count()
        )
    } else {
        format!("Ln {}, Col {}", primary.line + 1, primary.col + 1)
    };

    NodeBlueprint::element(
        Tag::Div,
        vec![
            "flex items-center justify-between px-3 py-1 text-[11px] font-mono border-t select-none shrink-0"
                .to_string(),
        ],
        vec![
            Attribute::property("data-editor-statusbar", "true"),
            Attribute::property(
                "style",
                format!(
                    "background-color: {}; color: {}; border-color: {};",
                    theme.gutter_bg, theme.gutter_fg, theme.border
                ),
            ),
        ],
        vec![
            NodeBlueprint::element(
                Tag::Div,
                vec!["flex items-center gap-3".to_string()],
                Vec::new(),
                vec![
                    NodeBlueprint::element(
                        Tag::Span,
                        Vec::<String>::new(),
                        vec![Attribute::property(
                            "style",
                            format!("color: {};", theme.diag_error),
                        )],
                        vec![NodeBlueprint::text(format!("✕ {}", counts.errors))],
                    ),
                    NodeBlueprint::element(
                        Tag::Span,
                        Vec::<String>::new(),
                        vec![Attribute::property(
                            "style",
                            format!("color: {};", theme.diag_warning),
                        )],
                        vec![NodeBlueprint::text(format!("▲ {}", counts.warnings))],
                    ),
                    NodeBlueprint::text(cursor_badge),
                ],
            ),
            NodeBlueprint::element(
                Tag::Div,
                vec!["flex items-center gap-3".to_string()],
                Vec::new(),
                vec![
                    NodeBlueprint::text(format!("Spaces: {}", ed.tab_size())),
                    NodeBlueprint::text("UTF-8"),
                    NodeBlueprint::element(
                        Tag::Span,
                        vec!["font-semibold".to_string()],
                        vec![Attribute::property(
                            "style",
                            format!("color: {};", theme.gutter_active_fg),
                        )],
                        vec![NodeBlueprint::text(ed.language().display_name())],
                    ),
                ],
            ),
        ],
    )
}
