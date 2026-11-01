//! LSP 3.17 Language Server lifecycle coordinator and request handler.

use std::collections::HashMap;
use crate::color::{find_document_colors, format_color_presentation};
use crate::completion::compute_completions;
use crate::diagnostics::validate_document;
use crate::hover::resolve_hover;
use crate::jsonrpc::{extract_json_number, extract_json_string, JsonRpcMessage};
use crate::parser::RsxDocument;
use crate::types::{ColorRgba, Position, Range, ServerCapabilities};

/// State of an active Strata LSP server session.
pub struct LspServer {
    pub documents: HashMap<String, RsxDocument>,
    pub initialized: bool,
    pub shutdown_requested: bool,
}

impl Default for LspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl LspServer {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            initialized: false,
            shutdown_requested: false,
        }
    }

    /// Handles an incoming JSON-RPC message, optionally producing a framed response or notification.
    pub fn handle_message(&mut self, msg: JsonRpcMessage) -> Option<String> {
        match msg {
            JsonRpcMessage::Request { id, method, params_json } => {
                let res = self.handle_request(id, &method, &params_json);
                Some(res)
            }
            JsonRpcMessage::Notification { method, params_json } => {
                self.handle_notification(&method, &params_json)
            }
            JsonRpcMessage::Response { .. } => None,
            JsonRpcMessage::Error { .. } => None,
        }
    }

    fn handle_request(&mut self, id: u64, method: &str, params: &str) -> String {
        match method {
            "initialize" => {
                self.initialized = true;
                let caps = ServerCapabilities::default();
                let result_json = format!(
                    "{{\"capabilities\":{{\"textDocumentSync\":{},\"completionProvider\":{{\"triggerCharacters\":[\"<\",\" \",\"-\",\"\\\"\",\":\"]}},\"hoverProvider\":{},\"colorProvider\":{}}}}}",
                    caps.text_document_sync, caps.hover_provider, caps.document_color_provider
                );
                JsonRpcMessage::format_response(id, &result_json)
            }
            "shutdown" => {
                self.shutdown_requested = true;
                JsonRpcMessage::format_response(id, "null")
            }
            "textDocument/completion" => {
                let uri = extract_json_string(params, "uri").unwrap_or_default();
                let line = extract_json_number(params, "line").unwrap_or(0) as u32;
                let character = extract_json_number(params, "character").unwrap_or(0) as u32;

                let items = if let Some(doc) = self.documents.get(&uri) {
                    compute_completions(doc, Position::new(line, character))
                } else {
                    Vec::new()
                };

                let mut items_json = String::from("[");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        items_json.push(',');
                    }
                    items_json.push_str(&format!(
                        "{{\"label\":\"{}\",\"kind\":{},\"detail\":{},\"documentation\":{}}}",
                        item.label,
                        item.kind as u8,
                        item.detail.as_ref().map(|d| format!("\"{}\"", d.replace('"', "\\\""))).unwrap_or_else(|| "null".to_string()),
                        item.documentation.as_ref().map(|d| format!("\"{}\"", d.replace('"', "\\\""))).unwrap_or_else(|| "null".to_string())
                    ));
                }
                items_json.push(']');

                JsonRpcMessage::format_response(id, &items_json)
            }
            "textDocument/hover" => {
                let uri = extract_json_string(params, "uri").unwrap_or_default();
                let line = extract_json_number(params, "line").unwrap_or(0) as u32;
                let character = extract_json_number(params, "character").unwrap_or(0) as u32;

                if let Some(doc) = self.documents.get(&uri) {
                    if let Some(hover) = resolve_hover(doc, Position::new(line, character)) {
                        let res = format!(
                            "{{\"contents\":{{\"kind\":\"markdown\",\"value\":\"{}\"}}}}",
                            hover.contents.replace('"', "\\\"").replace('\n', "\\n")
                        );
                        return JsonRpcMessage::format_response(id, &res);
                    }
                }

                JsonRpcMessage::format_response(id, "null")
            }
            "textDocument/documentColor" => {
                let uri = extract_json_string(params, "uri").unwrap_or_default();
                let colors = if let Some(doc) = self.documents.get(&uri) {
                    find_document_colors(doc)
                } else {
                    Vec::new()
                };

                let mut colors_json = String::from("[");
                for (i, info) in colors.iter().enumerate() {
                    if i > 0 {
                        colors_json.push(',');
                    }
                    colors_json.push_str(&format!(
                        "{{\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}},\"color\":{{\"red\":{:.3},\"green\":{:.3},\"blue\":{:.3},\"alpha\":{:.3}}}}}",
                        info.range.start.line, info.range.start.character,
                        info.range.end.line, info.range.end.character,
                        info.color.red, info.color.green, info.color.blue, info.color.alpha
                    ));
                }
                colors_json.push(']');

                JsonRpcMessage::format_response(id, &colors_json)
            }
            "textDocument/colorPresentation" => {
                let r = extract_json_number(params, "red").unwrap_or(0) as f32 / 255.0;
                let g = extract_json_number(params, "green").unwrap_or(0) as f32 / 255.0;
                let b = extract_json_number(params, "blue").unwrap_or(0) as f32 / 255.0;
                let a = extract_json_number(params, "alpha").unwrap_or(255) as f32 / 255.0;

                let color = ColorRgba { red: r, green: g, blue: b, alpha: a };
                let range = Range::default();
                let presentations = format_color_presentation(color, range);

                let mut pres_json = String::from("[");
                for (i, pres) in presentations.iter().enumerate() {
                    if i > 0 {
                        pres_json.push(',');
                    }
                    pres_json.push_str(&format!("{{\"label\":\"{}\"}}", pres.label));
                }
                pres_json.push(']');

                JsonRpcMessage::format_response(id, &pres_json)
            }
            _ => JsonRpcMessage::format_error(Some(id), -32601, &format!("Method '{}' not found", method)),
        }
    }

    fn handle_notification(&mut self, method: &str, params: &str) -> Option<String> {
        match method {
            "initialized" => None,
            "textDocument/didOpen" => {
                let uri = extract_json_string(params, "uri")?;
                let text = extract_json_string(params, "text").unwrap_or_default();
                let doc = RsxDocument::parse(&text);
                let diags = validate_document(&doc);
                self.documents.insert(uri.clone(), doc);
                Some(format_diagnostics_notification(&uri, &diags))
            }
            "textDocument/didChange" => {
                let uri = extract_json_string(params, "uri")?;
                let text = extract_json_string(params, "text").unwrap_or_default();
                let doc = RsxDocument::parse(&text);
                let diags = validate_document(&doc);
                self.documents.insert(uri.clone(), doc);
                Some(format_diagnostics_notification(&uri, &diags))
            }
            "textDocument/didClose" => {
                if let Some(uri) = extract_json_string(params, "uri") {
                    self.documents.remove(&uri);
                }
                None
            }
            _ => None,
        }
    }
}

fn format_diagnostics_notification(uri: &str, diagnostics: &[crate::types::Diagnostic]) -> String {
    let mut diags_json = String::from("[");
    for (i, diag) in diagnostics.iter().enumerate() {
        if i > 0 {
            diags_json.push(',');
        }
        diags_json.push_str(&format!(
            "{{\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}},\"severity\":{},\"source\":\"{}\",\"message\":\"{}\"}}",
            diag.range.start.line,
            diag.range.start.character,
            diag.range.end.line,
            diag.range.end.character,
            diag.severity as u8,
            diag.source.as_deref().unwrap_or("strata-lsp"),
            diag.message.replace('"', "\\\"")
        ));
    }
    diags_json.push(']');

    let params = format!("{{\"uri\":\"{}\",\"diagnostics\":{}}}", uri, diags_json);
    JsonRpcMessage::format_notification("textDocument/publishDiagnostics", &params)
}
