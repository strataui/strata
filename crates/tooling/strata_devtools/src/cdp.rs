//! Remote WebSocket Chrome DevTools Protocol (CDP) Bridge, RFC 6455 Framing, and Live State Serializer.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// RFC 6455 WebSocket Frame Opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSocketOpcode {
    Continuation = 0x00,
    Text = 0x01,
    Binary = 0x02,
    Close = 0x08,
    Ping = 0x09,
    Pong = 0x0A,
}

impl WebSocketOpcode {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b & 0x0F {
            0x00 => Some(Self::Continuation),
            0x01 => Some(Self::Text),
            0x02 => Some(Self::Binary),
            0x08 => Some(Self::Close),
            0x09 => Some(Self::Ping),
            0x0A => Some(Self::Pong),
            _ => None,
        }
    }
}

/// Decoded RFC 6455 WebSocket frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketFrame {
    pub fin: bool,
    pub opcode: WebSocketOpcode,
    pub payload: Vec<u8>,
}

/// Encoder & Decoder for RFC 6455 WebSocket communication.
pub struct WebSocketCodec;

impl WebSocketCodec {
    /// Encodes a server-to-client unmasked WebSocket frame.
    pub fn encode_frame(opcode: WebSocketOpcode, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(payload.len() + 10);
        let first_byte = 0x80 | (opcode as u8); // FIN bit set
        out.push(first_byte);

        let len = payload.len();
        if len <= 125 {
            out.push(len as u8); // Unmasked (Mask bit = 0)
        } else if len <= 65535 {
            out.push(126);
            out.push((len >> 8) as u8);
            out.push((len & 0xFF) as u8);
        } else {
            out.push(127);
            for i in (0..8).rev() {
                out.push(((len as u64) >> (i * 8)) as u8);
            }
        }

        out.extend_from_slice(payload);
        out
    }

    /// Decodes a client-to-server (potentially masked) or server frame.
    /// Returns the decoded frame and total bytes consumed from the buffer.
    pub fn decode_frame(buf: &[u8]) -> Result<(WebSocketFrame, usize), &'static str> {
        if buf.len() < 2 {
            return Err("Incomplete frame header");
        }

        let first = buf[0];
        let second = buf[1];

        let fin = (first & 0x80) != 0;
        let opcode = WebSocketOpcode::from_u8(first).ok_or("Invalid opcode")?;
        let masked = (second & 0x80) != 0;
        let mut payload_len = (second & 0x7F) as usize;

        let mut offset = 2;
        if payload_len == 126 {
            if buf.len() < offset + 2 {
                return Err("Incomplete 16-bit payload length");
            }
            payload_len = ((buf[offset] as usize) << 8) | (buf[offset + 1] as usize);
            offset += 2;
        } else if payload_len == 127 {
            if buf.len() < offset + 8 {
                return Err("Incomplete 64-bit payload length");
            }
            let mut l: u64 = 0;
            for i in 0..8 {
                l = (l << 8) | (buf[offset + i] as u64);
            }
            payload_len = l as usize;
            offset += 8;
        }

        let mask_key = if masked {
            if buf.len() < offset + 4 {
                return Err("Incomplete masking key");
            }
            let key = [buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]];
            offset += 4;
            Some(key)
        } else {
            None
        };

        if buf.len() < offset + payload_len {
            return Err("Incomplete frame payload");
        }

        let mut payload = buf[offset..offset + payload_len].to_vec();
        if let Some(key) = mask_key {
            for (i, p) in payload.iter_mut().enumerate() {
                *p ^= key[i % 4];
            }
        }

        let consumed = offset + payload_len;
        Ok((
            WebSocketFrame {
                fin,
                opcode,
                payload,
            },
            consumed,
        ))
    }
}

/// Computes FIPS 180-1 SHA-1 digest in pure Rust.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h0 = 0x67452301u32;
    let mut h1 = 0xEFCDAB89u32;
    let mut h2 = 0x98BADCFEu32;
    let mut h3 = 0x10325476u32;
    let mut h4 = 0xC3D2E1F0u32;

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for (i, &w_i) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w_i);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

/// Pure-Rust Base64 string encoding.
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Computes standard RFC 6455 `Sec-WebSocket-Accept` header response value.
pub fn compute_sec_websocket_accept(key: &str) -> String {
    const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    let concat = format!("{}{}", key.trim(), WS_GUID);
    let digest = sha1(concat.as_bytes());
    base64_encode(&digest)
}

/// Safely escapes special JSON string characters.
pub fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Serialized Chrome DevTools DOM Node hierarchy.
#[derive(Debug, Clone, PartialEq)]
pub struct CdpDomNode {
    pub node_id: u64,
    pub backend_node_id: u64,
    pub parent_id: Option<u64>,
    pub node_type: u32,
    pub node_name: String,
    pub local_name: String,
    pub node_value: String,
    pub child_node_count: u32,
    pub children: Vec<CdpDomNode>,
    pub attributes: Vec<String>,
}

impl CdpDomNode {
    /// Serializes the node and its recursive children into standard CDP JSON.
    pub fn to_json(&self) -> String {
        let mut s = String::new();
        s.push('{');
        s.push_str(&format!(
            r#""nodeId":{},"backendNodeId":{},"nodeType":{},"nodeName":"{}","localName":"{}","nodeValue":"{}","childNodeCount":{}"#,
            self.node_id,
            self.backend_node_id,
            self.node_type,
            self.node_name,
            self.local_name,
            escape_json(&self.node_value),
            self.child_node_count
        ));
        if let Some(pid) = self.parent_id {
            s.push_str(&format!(r#","parentId":{}"#, pid));
        }
        if !self.attributes.is_empty() {
            s.push_str(r#","attributes":["#);
            for (i, attr) in self.attributes.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push('"');
                s.push_str(&escape_json(attr));
                s.push('"');
            }
            s.push(']');
        }
        if !self.children.is_empty() {
            s.push_str(r#","children":["#);
            for (i, child) in self.children.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&child.to_json());
            }
            s.push(']');
        }
        s.push('}');
        s
    }

    /// Finds a mutable reference to a node by its CDP node ID.
    pub fn find_node_mut(&mut self, target_id: u64) -> Option<&mut CdpDomNode> {
        if self.node_id == target_id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(n) = child.find_node_mut(target_id) {
                return Some(n);
            }
        }
        None
    }

    /// Finds a node by its CDP node ID.
    pub fn find_node(&self, target_id: u64) -> Option<&CdpDomNode> {
        if self.node_id == target_id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(n) = child.find_node(target_id) {
                return Some(n);
            }
        }
        None
    }

    /// Updates the text/node value of the target node.
    pub fn set_node_value(&mut self, target_id: u64, val: &str) -> bool {
        if let Some(node) = self.find_node_mut(target_id) {
            node.node_value = val.to_string();
            true
        } else {
            false
        }
    }

    /// Sets or adds an attribute key-value pair on the target node.
    pub fn set_attribute(&mut self, target_id: u64, name: &str, val: &str) -> bool {
        if let Some(node) = self.find_node_mut(target_id) {
            let mut found = false;
            let mut i = 0;
            while i < node.attributes.len() {
                if node.attributes[i] == name {
                    if i + 1 < node.attributes.len() {
                        node.attributes[i + 1] = val.to_string();
                    } else {
                        node.attributes.push(val.to_string());
                    }
                    found = true;
                    break;
                }
                i += 2;
            }
            if !found {
                node.attributes.push(name.to_string());
                node.attributes.push(val.to_string());
            }
            true
        } else {
            false
        }
    }

    /// Removes an attribute from the target node by name.
    pub fn remove_attribute(&mut self, target_id: u64, name: &str) -> bool {
        if let Some(node) = self.find_node_mut(target_id) {
            let mut i = 0;
            while i < node.attributes.len() {
                if node.attributes[i] == name {
                    node.attributes.remove(i);
                    if i < node.attributes.len() {
                        node.attributes.remove(i);
                    }
                    return true;
                }
                i += 2;
            }
            false
        } else {
            false
        }
    }

    /// Retrieves an attribute value on this node.
    pub fn get_attribute(&self, name: &str) -> Option<String> {
        let mut i = 0;
        while i < self.attributes.len() {
            if self.attributes[i] == name {
                return self.attributes.get(i + 1).cloned();
            }
            i += 2;
        }
        None
    }

    /// Finds the first descendant matching a simple selector (`.class`, `#id`, or tag).
    pub fn query_selector(&self, selector: &str) -> Option<u64> {
        let sel = selector.trim();
        if let Some(class_name) = sel.strip_prefix('.') {
            self.find_first_matching(&|n| {
                n.get_attribute("class")
                    .map(|c| c.split_whitespace().any(|cls| cls == class_name))
                    .unwrap_or(false)
            })
        } else if let Some(id_name) = sel.strip_prefix('#') {
            self.find_first_matching(&|n| {
                n.get_attribute("id").as_deref() == Some(id_name)
            })
        } else {
            let tag_lower = sel.to_lowercase();
            self.find_first_matching(&|n| n.local_name == tag_lower || n.node_name.to_lowercase() == tag_lower)
        }
    }

    /// Collects all descendants matching a simple selector.
    pub fn query_selector_all(&self, selector: &str) -> Vec<u64> {
        let sel = selector.trim();
        let mut results = Vec::new();
        if let Some(class_name) = sel.strip_prefix('.') {
            self.collect_matching(
                &|n| {
                    n.get_attribute("class")
                        .map(|c| c.split_whitespace().any(|cls| cls == class_name))
                        .unwrap_or(false)
                },
                &mut results,
            );
        } else if let Some(id_name) = sel.strip_prefix('#') {
            self.collect_matching(
                &|n| n.get_attribute("id").as_deref() == Some(id_name),
                &mut results,
            );
        } else {
            let tag_lower = sel.to_lowercase();
            self.collect_matching(
                &|n| n.local_name == tag_lower || n.node_name.to_lowercase() == tag_lower,
                &mut results,
            );
        }
        results
    }

    fn find_first_matching(&self, pred: &dyn Fn(&CdpDomNode) -> bool) -> Option<u64> {
        if pred(self) {
            return Some(self.node_id);
        }
        for child in &self.children {
            if let Some(id) = child.find_first_matching(pred) {
                return Some(id);
            }
        }
        None
    }

    fn collect_matching(&self, pred: &dyn Fn(&CdpDomNode) -> bool, out: &mut Vec<u64>) {
        if pred(self) {
            out.push(self.node_id);
        }
        for child in &self.children {
            child.collect_matching(pred, out);
        }
    }
}

/// Builds a Chrome DevTools DOM tree from a `UiIrGraph`.
pub fn build_cdp_tree_from_ir(graph: &strata_dom::UiIrGraph) -> Option<CdpDomNode> {
    let root_id = graph.root_id?;
    let mut next_cdp_id = 2u64;
    let root_child = build_cdp_node_from_ir_recursive(graph, root_id, 1, &mut next_cdp_id);

    Some(CdpDomNode {
        node_id: 1,
        backend_node_id: 1,
        parent_id: None,
        node_type: 9, // DOCUMENT_NODE
        node_name: "#document".to_string(),
        local_name: String::new(),
        node_value: String::new(),
        child_node_count: 1,
        children: vec![root_child],
        attributes: Vec::new(),
    })
}

fn build_cdp_node_from_ir_recursive(
    graph: &strata_dom::UiIrGraph,
    ir_id: strata_dom::NodeId,
    parent_cdp_id: u64,
    next_cdp_id: &mut u64,
) -> CdpDomNode {
    let node = graph.get(ir_id).copied().unwrap_or_default();
    let cdp_id = *next_cdp_id;
    *next_cdp_id += 1;

    let (node_name, local_name, node_type) = match node.tag {
        strata_dom::IrTag::Text => ("#text".to_string(), String::new(), 3),
        strata_dom::IrTag::Div => ("DIV".to_string(), "div".to_string(), 1),
        strata_dom::IrTag::Span => ("SPAN".to_string(), "span".to_string(), 1),
        strata_dom::IrTag::Button => ("BUTTON".to_string(), "button".to_string(), 1),
        strata_dom::IrTag::Input => ("INPUT".to_string(), "input".to_string(), 1),
        strata_dom::IrTag::Image => ("IMG".to_string(), "img".to_string(), 1),
        strata_dom::IrTag::Svg => ("svg".to_string(), "svg".to_string(), 1),
        strata_dom::IrTag::Canvas3D => ("CANVAS".to_string(), "canvas".to_string(), 1),
        strata_dom::IrTag::ScrollView
        | strata_dom::IrTag::FlexContainer
        | strata_dom::IrTag::GridContainer
        | strata_dom::IrTag::ComponentRoot => ("DIV".to_string(), "div".to_string(), 1),
    };

    let (x, y, w, h) = node.layout_rect.to_f32();
    let attributes = vec![
        "data-ir-id".to_string(),
        format!("{}", ir_id.0),
        "data-rect".to_string(),
        format!("{:.1},{:.1},{:.1},{:.1}", x, y, w, h),
    ];

    let mut children = Vec::new();
    let mut curr_child = node.first_child;
    while !curr_child.is_null() {
        let child_cdp = build_cdp_node_from_ir_recursive(graph, curr_child, cdp_id, next_cdp_id);
        let next_sib = graph
            .get(curr_child)
            .map(|c| c.next_sibling)
            .unwrap_or(strata_dom::NodeId::NULL);
        children.push(child_cdp);
        curr_child = next_sib;
    }

    CdpDomNode {
        node_id: cdp_id,
        backend_node_id: cdp_id,
        parent_id: Some(parent_cdp_id),
        node_type,
        node_name,
        local_name,
        node_value: String::new(),
        child_node_count: children.len() as u32,
        children,
        attributes,
    }
}

/// Builds a Chrome DevTools DOM tree from a `NodeBlueprint`.
pub fn build_cdp_tree_from_blueprint(blueprint: &strata_dom::NodeBlueprint) -> CdpDomNode {
    let mut next_cdp_id = 2u64;
    let root_child = build_cdp_node_from_blueprint_recursive(blueprint, 1, &mut next_cdp_id);

    CdpDomNode {
        node_id: 1,
        backend_node_id: 1,
        parent_id: None,
        node_type: 9,
        node_name: "#document".to_string(),
        local_name: String::new(),
        node_value: String::new(),
        child_node_count: 1,
        children: vec![root_child],
        attributes: Vec::new(),
    }
}

fn build_cdp_node_from_blueprint_recursive(
    blueprint: &strata_dom::NodeBlueprint,
    parent_cdp_id: u64,
    next_cdp_id: &mut u64,
) -> CdpDomNode {
    let cdp_id = *next_cdp_id;
    *next_cdp_id += 1;

    match blueprint {
        strata_dom::NodeBlueprint::Element {
            tag,
            classes,
            attributes: raw_attrs,
            children: raw_children,
            ..
        } => {
            let tag_name = format!("{:?}", tag).to_uppercase();
            let local_name = format!("{:?}", tag).to_lowercase();
            let mut attrs = Vec::new();
            if !classes.is_empty() {
                attrs.push("class".to_string());
                attrs.push(classes.join(" "));
            }
            for attr in raw_attrs {
                if let strata_dom::Attribute::Property { key, value } = attr {
                    attrs.push(key.clone());
                    attrs.push(value.clone());
                }
            }

            let mut children = Vec::with_capacity(raw_children.len());
            for child in raw_children {
                children.push(build_cdp_node_from_blueprint_recursive(
                    child,
                    cdp_id,
                    next_cdp_id,
                ));
            }

            CdpDomNode {
                node_id: cdp_id,
                backend_node_id: cdp_id,
                parent_id: Some(parent_cdp_id),
                node_type: 1,
                node_name: tag_name,
                local_name,
                node_value: String::new(),
                child_node_count: children.len() as u32,
                children,
                attributes: attrs,
            }
        }
        strata_dom::NodeBlueprint::Text(text) => CdpDomNode {
            node_id: cdp_id,
            backend_node_id: cdp_id,
            parent_id: Some(parent_cdp_id),
            node_type: 3,
            node_name: "#text".to_string(),
            local_name: String::new(),
            node_value: text.clone(),
            child_node_count: 0,
            children: Vec::new(),
            attributes: Vec::new(),
        },
        strata_dom::NodeBlueprint::DynamicText(getter) => CdpDomNode {
            node_id: cdp_id,
            backend_node_id: cdp_id,
            parent_id: Some(parent_cdp_id),
            node_type: 3,
            node_name: "#text".to_string(),
            local_name: String::new(),
            node_value: getter(),
            child_node_count: 0,
            children: Vec::new(),
            attributes: Vec::new(),
        },
        strata_dom::NodeBlueprint::Fragment(children_bps) => {
            let mut children = Vec::with_capacity(children_bps.len());
            for child in children_bps {
                children.push(build_cdp_node_from_blueprint_recursive(
                    child,
                    cdp_id,
                    next_cdp_id,
                ));
            }
            CdpDomNode {
                node_id: cdp_id,
                backend_node_id: cdp_id,
                parent_id: Some(parent_cdp_id),
                node_type: 11,
                node_name: "#document-fragment".to_string(),
                local_name: String::new(),
                node_value: String::new(),
                child_node_count: children.len() as u32,
                children,
                attributes: Vec::new(),
            }
        }
        strata_dom::NodeBlueprint::Empty => CdpDomNode {
            node_id: cdp_id,
            backend_node_id: cdp_id,
            parent_id: Some(parent_cdp_id),
            node_type: 1,
            node_name: "DIV".to_string(),
            local_name: "div".to_string(),
            node_value: String::new(),
            child_node_count: 0,
            children: Vec::new(),
            attributes: vec!["style".to_string(), "display: none".to_string()],
        },
    }
}

/// Serializes a `ComputedStyle` into standard CSS name/value pairs.
pub fn format_computed_style(style: &strata_style::ComputedStyle) -> Vec<(String, String)> {
    let mut props = Vec::with_capacity(32);
    props.push(("display".to_string(), format!("{:?}", style.display).to_lowercase()));
    props.push(("position".to_string(), format!("{:?}", style.position).to_lowercase()));
    props.push(("box-sizing".to_string(), format!("{:?}", style.box_sizing).to_lowercase()));

    // Dimensions
    props.push(("width".to_string(), format_dimension(style.width)));
    props.push(("height".to_string(), format_dimension(style.height)));
    props.push(("min-width".to_string(), format_dimension(style.min_width)));
    props.push(("max-width".to_string(), format_dimension(style.max_width)));
    props.push(("min-height".to_string(), format_dimension(style.min_height)));
    props.push(("max-height".to_string(), format_dimension(style.max_height)));

    // Margin
    props.push(("margin-top".to_string(), format!("{:.1}px", style.margin.top)));
    props.push(("margin-right".to_string(), format!("{:.1}px", style.margin.right)));
    props.push(("margin-bottom".to_string(), format!("{:.1}px", style.margin.bottom)));
    props.push(("margin-left".to_string(), format!("{:.1}px", style.margin.left)));

    // Padding
    props.push(("padding-top".to_string(), format!("{:.1}px", style.padding.top)));
    props.push(("padding-right".to_string(), format!("{:.1}px", style.padding.right)));
    props.push(("padding-bottom".to_string(), format!("{:.1}px", style.padding.bottom)));
    props.push(("padding-left".to_string(), format!("{:.1}px", style.padding.left)));

    // Insets
    props.push(("top".to_string(), format_dimension(style.insets.top)));
    props.push(("right".to_string(), format_dimension(style.insets.right)));
    props.push(("bottom".to_string(), format_dimension(style.insets.bottom)));
    props.push(("left".to_string(), format_dimension(style.insets.left)));

    // Flexbox
    props.push(("flex-direction".to_string(), format!("{:?}", style.flex_direction).to_lowercase()));
    props.push(("flex-grow".to_string(), format!("{:.2}", style.flex_grow)));
    props.push(("flex-shrink".to_string(), format!("{:.2}", style.flex_shrink)));
    props.push(("justify-content".to_string(), format!("{:?}", style.justify_content).to_lowercase()));
    props.push(("align-items".to_string(), format!("{:?}", style.align_items).to_lowercase()));
    props.push(("gap".to_string(), format!("{:.1}px", style.gap)));

    // Color & Typography
    props.push(("color".to_string(), format_color(style.color)));
    if let Some(bg) = style.background_color {
        props.push(("background-color".to_string(), format_color(bg)));
    } else {
        props.push(("background-color".to_string(), "transparent".to_string()));
    }
    props.push(("font-size".to_string(), format!("{:.1}px", style.font_size)));
    props.push(("opacity".to_string(), format!("{:.2}", style.opacity)));

    if let Some(z) = style.z_index {
        props.push(("z-index".to_string(), format!("{}", z)));
    }

    props
}

fn format_dimension(dim: strata_style::types::Dimension) -> String {
    match dim {
        strata_style::types::Dimension::Auto => "auto".to_string(),
        strata_style::types::Dimension::Px(px) => format!("{:.1}px", px),
        strata_style::types::Dimension::Percent(pct) => format!("{:.1}%", pct),
        strata_style::types::Dimension::Vw(vw) => format!("{:.1}vw", vw),
        strata_style::types::Dimension::Vh(vh) => format!("{:.1}vh", vh),
        strata_style::types::Dimension::Rem(rem) => format!("{:.1}rem", rem),
        strata_style::types::Dimension::Em(em) => format!("{:.1}em", em),
        _ => "auto".to_string(),
    }
}

fn format_color(color: strata_primitives::color::Rgba) -> String {
    format!(
        "rgba({}, {}, {}, {:.2})",
        color.r,
        color.g,
        color.b,
        color.a as f32 / 255.0
    )
}

/// Thread-safe live DevTools state synchronized from runtime.
#[derive(Debug, Clone, Default)]
pub struct DevToolsState {
    pub dom_tree: Option<CdpDomNode>,
    pub node_styles: HashMap<u64, Vec<(String, String)>>,
    pub runtime_signals: HashMap<String, String>,
    pub frame_count: u64,
    pub layout_count: u64,
    pub layout_duration_sec: f64,
    pub recalc_style_count: u64,
    pub recalc_style_duration_sec: f64,
    pub js_heap_used_bytes: usize,
    pub js_heap_total_bytes: usize,
    pub highlighted_node_id: Option<u64>,
}

/// Incoming Chrome DevTools Protocol JSON-RPC request.
#[derive(Debug, Clone, PartialEq)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    pub params: String,
}

impl CdpRequest {
    /// Zero-dependency parser for standard CDP JSON-RPC requests: `{"id": 1, "method": "DOM.getDocument", ...}`.
    pub fn parse(raw_json: &str) -> Result<Self, &'static str> {
        let trimmed = raw_json.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return Err("Invalid JSON object");
        }

        let id = Self::extract_u64_field(trimmed, "\"id\"").unwrap_or(0);
        let method = Self::extract_string_field(trimmed, "\"method\"").unwrap_or_default();

        Ok(Self {
            id,
            method,
            params: trimmed.to_string(),
        })
    }

    pub fn get_u64_param(&self, key: &str) -> Option<u64> {
        let field_key = format!("\"{}\"", key);
        Self::extract_u64_field(&self.params, &field_key)
    }

    pub fn get_string_param(&self, key: &str) -> Option<String> {
        let field_key = format!("\"{}\"", key);
        Self::extract_string_field(&self.params, &field_key)
    }

    fn extract_string_field(json: &str, field_key: &str) -> Option<String> {
        let key_idx = json.find(field_key)?;
        let rest = &json[key_idx + field_key.len()..];
        let colon_idx = rest.find(':')?;
        let val_rest = rest[colon_idx + 1..].trim_start();
        if !val_rest.starts_with('"') {
            return None;
        }
        let end_quote = val_rest[1..].find('"')?;
        Some(val_rest[1..1 + end_quote].to_string())
    }

    fn extract_u64_field(json: &str, field_key: &str) -> Option<u64> {
        let key_idx = json.find(field_key)?;
        let rest = &json[key_idx + field_key.len()..];
        let colon_idx = rest.find(':')?;
        let val_rest = rest[colon_idx + 1..].trim_start();
        let mut digits = String::new();
        for ch in val_rest.chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
            } else if !digits.is_empty() {
                break;
            }
        }
        digits.parse::<u64>().ok()
    }
}

/// Outgoing Chrome DevTools Protocol JSON-RPC response.
#[derive(Debug, Clone, PartialEq)]
pub struct CdpResponse {
    pub id: u64,
    pub result: Option<String>,
    pub error: Option<String>,
}

impl CdpResponse {
    pub fn success(id: u64, result_json: impl Into<String>) -> Self {
        Self {
            id,
            result: Some(result_json.into()),
            error: None,
        }
    }

    pub fn error(id: u64, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(message.into()),
        }
    }

    /// Serializes response to standard JSON.
    pub fn to_json(&self) -> String {
        if let Some(err) = &self.error {
            format!(
                r#"{{"id":{},"error":{{"code":-32601,"message":"{}"}}}}"#,
                self.id, err
            )
        } else {
            let res = self.result.as_deref().unwrap_or("{}");
            format!(r#"{{"id":{},"result":{}}}"#, self.id, res)
        }
    }
}

/// Chrome DevTools Protocol Domain Dispatcher.
#[derive(Debug, Clone, Default)]
pub struct CdpDispatcher {
    pub state: Arc<RwLock<DevToolsState>>,
}

impl CdpDispatcher {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(DevToolsState::default())),
        }
    }

    pub fn with_state(state: Arc<RwLock<DevToolsState>>) -> Self {
        Self { state }
    }

    /// Updates live inspector state from a `UiIrGraph` and node styles.
    pub fn update_from_ir(
        &self,
        graph: &strata_dom::UiIrGraph,
        styles: &HashMap<u64, strata_style::ComputedStyle>,
        frame_count: u64,
        layout_count: u64,
    ) {
        if let Ok(mut state) = self.state.write() {
            state.dom_tree = build_cdp_tree_from_ir(graph);
            state.frame_count = frame_count;
            state.layout_count = layout_count;
            state.recalc_style_count = layout_count;

            state.node_styles.clear();
            for (&id, style) in styles {
                state.node_styles.insert(id, format_computed_style(style));
            }
        }
    }

    /// Updates live inspector state from a `NodeBlueprint` and node styles.
    pub fn update_from_blueprint(
        &self,
        blueprint: &strata_dom::NodeBlueprint,
        styles: &HashMap<u64, strata_style::ComputedStyle>,
        frame_count: u64,
        layout_count: u64,
    ) {
        if let Ok(mut state) = self.state.write() {
            state.dom_tree = Some(build_cdp_tree_from_blueprint(blueprint));
            state.frame_count = frame_count;
            state.layout_count = layout_count;
            state.recalc_style_count = layout_count;

            state.node_styles.clear();
            for (&id, style) in styles {
                state.node_styles.insert(id, format_computed_style(style));
            }
        }
    }

    /// Sets explicit performance metrics on the dispatcher state.
    pub fn set_metrics(
        &self,
        frame_count: u64,
        layout_count: u64,
        layout_duration_sec: f64,
        recalc_style_duration_sec: f64,
        heap_used_bytes: usize,
    ) {
        if let Ok(mut state) = self.state.write() {
            state.frame_count = frame_count;
            state.layout_count = layout_count;
            state.layout_duration_sec = layout_duration_sec;
            state.recalc_style_duration_sec = recalc_style_duration_sec;
            state.js_heap_used_bytes = heap_used_bytes;
        }
    }

    /// Records a live reactive signal or component state property for DevTools inspection.
    pub fn update_runtime_signal(&self, name: &str, value: &str) {
        if let Ok(mut state) = self.state.write() {
            state.runtime_signals.insert(name.to_string(), value.to_string());
        }
    }

    /// Dispatches incoming CDP requests to supported domains (`DOM`, `CSS`, `Performance`, `Page`, `Runtime`).
    pub fn handle_request(&self, request: &CdpRequest) -> CdpResponse {
        match request.method.as_str() {
            "DOM.enable" | "CSS.enable" | "Page.enable" | "Network.enable" | "Performance.enable"
            | "Runtime.enable" => CdpResponse::success(request.id, "{}"),
            "DOM.getDocument" => {
                let state = self.state.read().unwrap();
                if let Some(tree) = &state.dom_tree {
                    CdpResponse::success(request.id, format!(r#"{{"root":{}}}"#, tree.to_json()))
                } else {
                    CdpResponse::success(
                        request.id,
                        r##"{"root":{"nodeId":1,"nodeType":9,"nodeName":"#document","children":[{"nodeId":2,"nodeType":1,"nodeName":"strata-root","childNodeCount":0}]}}"##,
                    )
                }
            }
            "DOM.highlightNode" => {
                let node_id = request.get_u64_param("nodeId");
                if let Ok(mut state) = self.state.write() {
                    state.highlighted_node_id = node_id;
                }
                CdpResponse::success(request.id, r#"{"highlighted":true}"#)
            }
            "DOM.hideHighlight" => {
                if let Ok(mut state) = self.state.write() {
                    state.highlighted_node_id = None;
                }
                CdpResponse::success(request.id, "{}")
            }
            "CSS.getComputedStyleForNode" => {
                let node_id = request.get_u64_param("nodeId").unwrap_or(2);
                let state = self.state.read().unwrap();
                if let Some(styles) = state.node_styles.get(&node_id) {
                    let mut json = String::from(r#"{"computedStyle":["#);
                    for (i, (name, val)) in styles.iter().enumerate() {
                        if i > 0 {
                            json.push(',');
                        }
                        json.push_str(&format!(
                            r#"{{"name":"{}","value":"{}"}}"#,
                            name,
                            escape_json(val)
                        ));
                    }
                    json.push_str("]}");
                    CdpResponse::success(request.id, json)
                } else {
                    CdpResponse::success(
                        request.id,
                        r#"{"computedStyle":[{"name":"display","value":"flex"},{"name":"box-sizing","value":"border-box"}]}"#,
                    )
                }
            }
            "Performance.getMetrics" => {
                let state = self.state.read().unwrap();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64();
                let task_dur = state.layout_duration_sec + state.recalc_style_duration_sec;
                let metrics_json = format!(
                    r#"{{"metrics":[{{"name":"Timestamp","value":{:.4}}},{{"name":"Frames","value":{}}},{{"name":"LayoutCount","value":{}}},{{"name":"LayoutDuration","value":{:.6}}},{{"name":"RecalcStyleCount","value":{}}},{{"name":"RecalcStyleDuration","value":{:.6}}},{{"name":"ScriptDuration","value":0.0001}},{{"name":"TaskDuration","value":{:.6}}},{{"name":"JSHeapUsedSize","value":{}}},{{"name":"JSHeapTotalSize","value":{}}}]}}"#,
                    now,
                    state.frame_count.max(1),
                    state.layout_count.max(1),
                    state.layout_duration_sec,
                    state.recalc_style_count.max(1),
                    state.recalc_style_duration_sec,
                    task_dur,
                    state.js_heap_used_bytes.max(1024 * 1024),
                    state.js_heap_total_bytes.max(2 * 1024 * 1024),
                );
                CdpResponse::success(request.id, metrics_json)
            }
            "Page.getResourceTree" => CdpResponse::success(
                request.id,
                r#"{"frameTree":{"frame":{"id":"1","mimeType":"text/html","securityOrigin":"strata://app","url":"strata://app"}}}"#,
            ),
            "Runtime.evaluate" => {
                let expr = request.get_string_param("expression").unwrap_or_default();
                let result_val = if expr.contains("version") {
                    "Strata Reactive Engine v0.1.0"
                } else if expr.contains("metrics") {
                    let s = self.state.read().unwrap();
                    return CdpResponse::success(
                        request.id,
                        format!(
                            r#"{{"result":{{"type":"string","value":"frames={}, layouts={}"}}}}"#,
                            s.frame_count, s.layout_count
                        ),
                    );
                } else {
                    "Strata Reactive Engine v0.1.0"
                };
                CdpResponse::success(
                    request.id,
                    format!(r#"{{"result":{{"type":"string","value":"{}"}}}}"#, result_val),
                )
            }
            "DOM.setNodeValue" => {
                let node_id = request.get_u64_param("nodeId").unwrap_or(0);
                let value = request.get_string_param("value").unwrap_or_default();
                if let Ok(mut state) = self.state.write() {
                    if let Some(tree) = &mut state.dom_tree {
                        tree.set_node_value(node_id, &value);
                    }
                }
                CdpResponse::success(request.id, "{}")
            }
            "DOM.setAttributeValue" => {
                let node_id = request.get_u64_param("nodeId").unwrap_or(0);
                let name = request.get_string_param("name").unwrap_or_default();
                let value = request.get_string_param("value").unwrap_or_default();
                if let Ok(mut state) = self.state.write() {
                    if let Some(tree) = &mut state.dom_tree {
                        tree.set_attribute(node_id, &name, &value);
                    }
                }
                CdpResponse::success(request.id, "{}")
            }
            "DOM.removeAttribute" => {
                let node_id = request.get_u64_param("nodeId").unwrap_or(0);
                let name = request.get_string_param("name").unwrap_or_default();
                if let Ok(mut state) = self.state.write() {
                    if let Some(tree) = &mut state.dom_tree {
                        tree.remove_attribute(node_id, &name);
                    }
                }
                CdpResponse::success(request.id, "{}")
            }
            "DOM.querySelector" => {
                let selector = request.get_string_param("selector").unwrap_or_default();
                let state = self.state.read().unwrap();
                let found_id = state.dom_tree.as_ref().and_then(|t| t.query_selector(&selector)).unwrap_or(0);
                CdpResponse::success(request.id, format!(r#"{{"nodeId":{}}}"#, found_id))
            }
            "DOM.querySelectorAll" => {
                let selector = request.get_string_param("selector").unwrap_or_default();
                let state = self.state.read().unwrap();
                let found_ids = state.dom_tree.as_ref().map(|t| t.query_selector_all(&selector)).unwrap_or_default();
                let mut json = String::from(r#"{"nodeIds":["#);
                for (i, id) in found_ids.iter().enumerate() {
                    if i > 0 { json.push(','); }
                    json.push_str(&id.to_string());
                }
                json.push_str("]}");
                CdpResponse::success(request.id, json)
            }
            "DOM.requestChildNodes" => {
                CdpResponse::success(request.id, "{}")
            }
            "CSS.setStyleTexts" => {
                let node_id = request.get_u64_param("nodeId").unwrap_or(2);
                let text = request.get_string_param("text").unwrap_or_default();
                let mut updated_props = Vec::new();
                for decl in text.split(';') {
                    let decl = decl.trim();
                    if let Some((k, v)) = decl.split_once(':') {
                        let prop_name = k.trim().to_string();
                        let prop_val = v.trim().to_string();
                        if !prop_name.is_empty() {
                            updated_props.push((prop_name, prop_val));
                        }
                    }
                }

                if let Ok(mut state) = self.state.write() {
                    let styles = state.node_styles.entry(node_id).or_default();
                    for (k, v) in &updated_props {
                        if let Some(existing) = styles.iter_mut().find(|(name, _)| name == k) {
                            existing.1 = v.clone();
                        } else {
                            styles.push((k.clone(), v.clone()));
                        }
                    }
                }

                let mut json = String::from(r#"{"styles":[{"cssProperties":["#);
                for (i, (name, val)) in updated_props.iter().enumerate() {
                    if i > 0 { json.push(','); }
                    json.push_str(&format!(r#"{{"name":"{}","value":"{}"}}"#, name, escape_json(val)));
                }
                json.push_str(r#"],"shorthandEntries":[]}]}"#);
                CdpResponse::success(request.id, json)
            }
            "CSS.getMatchedStylesForNode" => {
                CdpResponse::success(request.id, r#"{"matchedCSSRules":[],"pseudoElements":[],"inherited":[],"cssKeyframesRules":[]}"#)
            }
            "Runtime.getProperties" => {
                let state = self.state.read().unwrap();
                let mut props_json = String::from(r#"{"result":["#);
                let mut first = true;
                for (key, val) in &state.runtime_signals {
                    if !first { props_json.push(','); }
                    first = false;
                    props_json.push_str(&format!(
                        r#"{{"name":"{}","value":{{"type":"string","value":"{}"}},"writable":true,"enumerable":true,"configurable":true}}"#,
                        escape_json(key),
                        escape_json(val)
                    ));
                }
                if !first { props_json.push(','); }
                props_json.push_str(&format!(
                    r#"{{"name":"frameCount","value":{{"type":"number","value":{}}},"writable":false,"enumerable":true,"configurable":true}},{{"name":"layoutCount","value":{{"type":"number","value":{}}},"writable":false,"enumerable":true,"configurable":true}}"#,
                    state.frame_count, state.layout_count
                ));
                props_json.push_str("]}");
                CdpResponse::success(request.id, props_json)
            }
            other => CdpResponse::error(request.id, format!("Method '{}' not implemented", other)),
        }
    }

    /// Handles a raw WebSocket frame containing a CDP JSON-RPC text message and returns the response frame.
    pub fn handle_ws_frame(&self, frame: &WebSocketFrame) -> Option<WebSocketFrame> {
        if frame.opcode != WebSocketOpcode::Text {
            return None;
        }

        let text = std::str::from_utf8(&frame.payload).ok()?;
        let req = CdpRequest::parse(text).ok()?;
        let resp = self.handle_request(&req);
        let resp_json = resp.to_json();

        Some(WebSocketFrame {
            fin: true,
            opcode: WebSocketOpcode::Text,
            payload: resp_json.into_bytes(),
        })
    }
}
