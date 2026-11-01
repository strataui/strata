//! Streaming parser for LLM-generated UI IR patches.
//! Supports chunked Server-Sent Events (SSE) and Newline-Delimited JSON (NDJSON) streams.

use crate::delta::AgentPatch;
use crate::sandbox::AgentUiSandbox;

/// Streaming parser maintaining an internal buffer across asynchronous token chunks.
#[derive(Debug, Clone, Default)]
pub struct StreamingUiParser {
    buffer: String,
    total_patches_parsed: usize,
}

impl StreamingUiParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            total_patches_parsed: 0,
        }
    }

    /// Feeds an incoming text chunk (from an SSE stream, HTTP response, or WebSocket)
    /// and extracts any fully formed `AgentPatch` instructions.
    pub fn feed_chunk(&mut self, chunk: &str) -> Vec<AgentPatch> {
        self.buffer.push_str(chunk);
        let mut patches = Vec::new();

        // Process all complete lines
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].trim_end_matches('\r').to_string();
            self.buffer.drain(..=newline_pos);

            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(':') {
                // Ignore empty lines or SSE comments (: keep-alive)
                continue;
            }

            // Check if SSE format (e.g. "data: { ... }")
            let payload = if let Some(stripped) = trimmed.strip_prefix("data:") {
                stripped.trim()
            } else {
                trimmed
            };

            if payload == "[DONE]" {
                continue;
            }

            if let Some(patch) = AgentPatch::parse_json(payload) {
                patches.push(patch);
                self.total_patches_parsed += 1;
            }
        }

        patches
    }

    /// Feeds a chunk and directly applies all parsed patches into an `AgentUiSandbox`.
    /// Returns the number of successfully applied patches.
    pub fn feed_chunk_and_apply(
        &mut self,
        chunk: &str,
        sandbox: &mut AgentUiSandbox,
    ) -> Result<usize, String> {
        let patches = self.feed_chunk(chunk);
        let count = patches.len();
        for patch in patches {
            sandbox.apply_patch(patch)?;
        }
        Ok(count)
    }

    /// Flushes any remaining complete JSON object in the buffer upon stream closure.
    pub fn flush(&mut self) -> Vec<AgentPatch> {
        let mut patches = Vec::new();
        let trimmed = self.buffer.trim();
        if !trimmed.is_empty() {
            let payload = if let Some(stripped) = trimmed.strip_prefix("data:") {
                stripped.trim()
            } else {
                trimmed
            };

            if payload != "[DONE]" {
                if let Some(patch) = AgentPatch::parse_json(payload) {
                    patches.push(patch);
                    self.total_patches_parsed += 1;
                }
            }
        }
        self.buffer.clear();
        patches
    }

    /// Resets the internal streaming buffer and counter.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.total_patches_parsed = 0;
    }

    /// Returns the total count of patches parsed since creation or last reset.
    pub fn total_patches(&self) -> usize {
        self.total_patches_parsed
    }

    /// Returns true if the parser currently holds incomplete trailing chunk bytes.
    pub fn has_buffered_data(&self) -> bool {
        !self.buffer.is_empty()
    }
}
