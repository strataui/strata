//! Streaming UI IR delta patches for token-by-token LLM generative UI updates.

use std::collections::HashMap;

/// Individual atomic patch instruction in a generative UI stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentPatch {
    InsertNode {
        node_id: u64,
        parent_id: Option<u64>,
        tag: String,
        props: HashMap<String, String>,
        text: Option<String>,
    },
    MutateProps {
        node_id: u64,
        key: String,
        value: String,
    },
    UpdateText {
        node_id: u64,
        text: String,
    },
    RemoveNode {
        node_id: u64,
    },
}

impl AgentPatch {
    /// Formats patch as a compact JSON string suitable for LLM streaming.
    pub fn to_json(&self) -> String {
        match self {
            AgentPatch::InsertNode { node_id, parent_id, tag, props, text } => {
                let parent_str = parent_id.map(|p| p.to_string()).unwrap_or_else(|| "null".to_string());
                let text_str = text.as_ref().map(|t| format!("\"{}\"", t)).unwrap_or_else(|| "null".to_string());
                let mut props_arr = Vec::new();
                for (k, v) in props {
                    props_arr.push(format!("\"{}\":\"{}\"", k, v));
                }
                format!(
                    "{{\"op\":\"insert\",\"id\":{},\"parent\":{},\"tag\":\"{}\",\"props\":{{{}}},\"text\":{}}}",
                    node_id, parent_str, tag, props_arr.join(","), text_str
                )
            }
            AgentPatch::MutateProps { node_id, key, value } => {
                format!(
                    "{{\"op\":\"mutate\",\"id\":{},\"key\":\"{}\",\"val\":\"{}\"}}",
                    node_id, key, value
                )
            }
            AgentPatch::UpdateText { node_id, text } => {
                format!("{{\"op\":\"text\",\"id\":{},\"text\":\"{}\"}}", node_id, text)
            }
            AgentPatch::RemoveNode { node_id } => {
                format!("{{\"op\":\"remove\",\"id\":{}}}", node_id)
            }
        }
    }

    /// Fast zero-dependency parser for an individual JSON patch object.
    pub fn parse_json(json: &str) -> Option<Self> {
        let trimmed = json.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return None;
        }

        let op = extract_string_field(trimmed, "op")?;
        match op.as_str() {
            "insert" => {
                let node_id = extract_number_field(trimmed, "id")?;
                let parent_id = extract_number_field(trimmed, "parent");
                let tag = extract_string_field(trimmed, "tag").unwrap_or_else(|| "div".to_string());
                let text = extract_string_field(trimmed, "text");
                let mut props = HashMap::new();

                // Extract simple props if present
                if let Some(props_start) = trimmed.find("\"props\":{") {
                    let after = &trimmed[props_start + "\"props\":{".len()..];
                    if let Some(props_end) = after.find('}') {
                        let props_body = &after[..props_end];
                        for pair in props_body.split(',') {
                            let parts: Vec<&str> = pair.split(':').collect();
                            if parts.len() == 2 {
                                let k = parts[0].trim().trim_matches(|c| c == '"' || c == '{' || c == '}');
                                let v = parts[1].trim().trim_matches(|c| c == '"' || c == '{' || c == '}');
                                if !k.is_empty() {
                                    props.insert(k.to_string(), v.to_string());
                                }
                            }
                        }
                    }
                }

                Some(AgentPatch::InsertNode {
                    node_id,
                    parent_id,
                    tag,
                    props,
                    text,
                })
            }
            "mutate" => {
                let node_id = extract_number_field(trimmed, "id")?;
                let key = extract_string_field(trimmed, "key")?;
                let value = extract_string_field(trimmed, "val")?;
                Some(AgentPatch::MutateProps {
                    node_id,
                    key,
                    value,
                })
            }
            "text" => {
                let node_id = extract_number_field(trimmed, "id")?;
                let text = extract_string_field(trimmed, "text")?;
                Some(AgentPatch::UpdateText { node_id, text })
            }
            "remove" => {
                let node_id = extract_number_field(trimmed, "id")?;
                Some(AgentPatch::RemoveNode { node_id })
            }
            _ => None,
        }
    }
}

fn extract_string_field(json: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\":", field);
    let pos = json.find(&pattern)?;
    let after = json[pos + pattern.len()..].trim_start();
    if let Some(stripped) = after.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some(stripped[..end].to_string())
    } else {
        None
    }
}

fn extract_number_field(json: &str, field: &str) -> Option<u64> {
    let pattern = format!("\"{}\":", field);
    let pos = json.find(&pattern)?;
    let after = json[pos + pattern.len()..].trim_start();
    if after.starts_with("null") {
        return None;
    }
    let mut num_str = String::new();
    for ch in after.chars() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
        } else {
            break;
        }
    }
    num_str.parse().ok()
}
