//! W3C Content Security Policy (CSP) Level 3 Parser and Directive Enforcement Engine.
//!
//! Provides strict policy evaluation for:
//! - `default-src`
//! - `script-src`, `script-src-elem`, `script-src-attr`
//! - `style-src`, `style-src-elem`, `style-src-attr`
//! - `img-src`
//! - `connect-src`
//! - `font-src`
//! - `object-src`
//! - `media-src`
//! - `frame-src`
//! - `worker-src`
//! - `base-uri`
//! - `form-action`
//!
//! Supports source expressions: `'none'`, `'self'`, `'unsafe-inline'`, `'unsafe-eval'`,
//! `'strict-dynamic'`, `'nonce-...'`, `'sha256-...'`, scheme sources, and host/wildcard sources.

use std::collections::HashMap;

/// Standard CSP Level 3 directive kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DirectiveKind {
    DefaultSrc,
    ScriptSrc,
    ScriptSrcElem,
    ScriptSrcAttr,
    StyleSrc,
    StyleSrcElem,
    StyleSrcAttr,
    ImgSrc,
    ConnectSrc,
    FontSrc,
    ObjectSrc,
    MediaSrc,
    FrameSrc,
    WorkerSrc,
    BaseUri,
    FormAction,
}

impl DirectiveKind {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "default-src" => Some(Self::DefaultSrc),
            "script-src" => Some(Self::ScriptSrc),
            "script-src-elem" => Some(Self::ScriptSrcElem),
            "script-src-attr" => Some(Self::ScriptSrcAttr),
            "style-src" => Some(Self::StyleSrc),
            "style-src-elem" => Some(Self::StyleSrcElem),
            "style-src-attr" => Some(Self::StyleSrcAttr),
            "img-src" => Some(Self::ImgSrc),
            "connect-src" => Some(Self::ConnectSrc),
            "font-src" => Some(Self::FontSrc),
            "object-src" => Some(Self::ObjectSrc),
            "media-src" => Some(Self::MediaSrc),
            "frame-src" => Some(Self::FrameSrc),
            "worker-src" => Some(Self::WorkerSrc),
            "base-uri" => Some(Self::BaseUri),
            "form-action" => Some(Self::FormAction),
            _ => None,
        }
    }
}

/// CSP source expression token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceExpr {
    None,
    Wildcard,
    SelfSource,
    UnsafeInline,
    UnsafeEval,
    StrictDynamic,
    UnsafeHashes,
    Nonce(String),
    Hash {
        algorithm: String,
        digest: String,
    },
    Scheme(String),
    Host {
        scheme: Option<String>,
        host: String,
        port: Option<u16>,
        path: Option<String>,
        is_wildcard_subdomain: bool,
    },
}

/// Pure-Rust SHA-256 implementation (FIPS 180-4) for CSP inline script/style hash verification.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = data.to_vec();
    padded.push(0x80);
    while !(padded.len() + 8).is_multiple_of(64) {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_val = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_val
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_val = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_val);
    }

    let mut result = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

/// Base64 encoding helper for digest hashes.
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as usize;
        let b1 = if i + 1 < data.len() {
            data[i + 1] as usize
        } else {
            0
        };
        let b2 = if i + 2 < data.len() {
            data[i + 2] as usize
        } else {
            0
        };

        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) & 0x3f] as char);
        out.push(TABLE[(triple >> 12) & 0x3f] as char);

        if i + 1 < data.len() {
            out.push(TABLE[(triple >> 6) & 0x3f] as char);
        } else {
            out.push('=');
        }

        if i + 2 < data.len() {
            out.push(TABLE[triple & 0x3f] as char);
        } else {
            out.push('=');
        }

        i += 3;
    }
    out
}

/// Calculates the base64-encoded SHA-256 digest of arbitrary byte slices.
pub fn sha256_base64(data: &[u8]) -> String {
    let digest = sha256(data);
    base64_encode(&digest)
}

/// Content Security Policy (CSP Level 3) policy container.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContentSecurityPolicy {
    pub directives: HashMap<DirectiveKind, Vec<SourceExpr>>,
}

impl ContentSecurityPolicy {
    /// Creates an empty, unrestricted policy.
    pub fn new() -> Self {
        Self {
            directives: HashMap::new(),
        }
    }

    /// Parses a standard `Content-Security-Policy` header value.
    pub fn parse(header: &str) -> Self {
        let mut policy = Self::new();
        for dir_str in header.split(';') {
            let trimmed = dir_str.trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut parts = trimmed.split_whitespace();
            if let Some(dir_name) = parts.next() {
                if let Some(kind) = DirectiveKind::from_str(dir_name) {
                    let mut sources = Vec::new();
                    for token in parts {
                        if let Some(expr) = Self::parse_source_expr(token) {
                            sources.push(expr);
                        }
                    }
                    policy.directives.insert(kind, sources);
                }
            }
        }
        policy
    }

    fn parse_source_expr(token: &str) -> Option<SourceExpr> {
        let trimmed = token.trim();
        let lower = trimmed.to_ascii_lowercase();

        if lower == "'none'" {
            return Some(SourceExpr::None);
        }
        if lower == "*" {
            return Some(SourceExpr::Wildcard);
        }
        if lower == "'self'" {
            return Some(SourceExpr::SelfSource);
        }
        if lower == "'unsafe-inline'" {
            return Some(SourceExpr::UnsafeInline);
        }
        if lower == "'unsafe-eval'" {
            return Some(SourceExpr::UnsafeEval);
        }
        if lower == "'strict-dynamic'" {
            return Some(SourceExpr::StrictDynamic);
        }
        if lower == "'unsafe-hashes'" {
            return Some(SourceExpr::UnsafeHashes);
        }

        if lower.starts_with("'nonce-") && lower.ends_with('\'') {
            let nonce_val = &trimmed[7..trimmed.len() - 1];
            return Some(SourceExpr::Nonce(nonce_val.to_string()));
        }

        if (lower.starts_with("'sha256-")
            || lower.starts_with("'sha384-")
            || lower.starts_with("'sha512-"))
            && lower.ends_with('\'')
        {
            let dash_idx = trimmed.find('-')?;
            let algo = &trimmed[1..dash_idx];
            let hash_val = &trimmed[dash_idx + 1..trimmed.len() - 1];
            return Some(SourceExpr::Hash {
                algorithm: algo.to_ascii_lowercase(),
                digest: hash_val.to_string(),
            });
        }

        if lower == "http:" || lower == "https:" || lower == "data:" || lower == "blob:" {
            return Some(SourceExpr::Scheme(lower));
        }

        // Host expression parsing
        let (scheme, rest) = if let Some(idx) = trimmed.find("://") {
            let s = &trimmed[..idx];
            let r = &trimmed[idx + 3..];
            (Some(s.to_ascii_lowercase()), r)
        } else {
            (None, trimmed)
        };

        let (host_port, path) = if let Some(slash_idx) = rest.find('/') {
            (&rest[..slash_idx], Some(rest[slash_idx..].to_string()))
        } else {
            (rest, None)
        };

        let (host, port) = if let Some(colon_idx) = host_port.find(':') {
            let h = &host_port[..colon_idx];
            let p = host_port[colon_idx + 1..].parse::<u16>().ok();
            (h, p)
        } else {
            (host_port, None)
        };

        let is_wildcard = host.starts_with("*.");
        let normalized_host = if is_wildcard {
            host[2..].to_ascii_lowercase()
        } else {
            host.to_ascii_lowercase()
        };

        Some(SourceExpr::Host {
            scheme,
            host: normalized_host,
            port,
            path,
            is_wildcard_subdomain: is_wildcard,
        })
    }

    /// Resolves the effective sources for a requested directive kind, falling back per W3C CSP Level 3.
    pub fn effective_sources(&self, primary: DirectiveKind) -> Option<&[SourceExpr]> {
        if let Some(sources) = self.directives.get(&primary) {
            return Some(sources);
        }

        // Full W3C CSP Level 3 directive fallback cascade:
        // - script-src-elem -> script-src -> default-src
        // - script-src-attr -> script-src -> default-src
        // - style-src-elem -> style-src -> default-src
        // - style-src-attr -> style-src -> default-src
        // - worker-src -> script-src -> default-src
        // - frame-src -> default-src
        // - img-src, connect-src, font-src, object-src, media-src -> default-src
        let fallbacks: &[DirectiveKind] = match primary {
            DirectiveKind::ScriptSrcElem | DirectiveKind::ScriptSrcAttr => {
                &[DirectiveKind::ScriptSrc, DirectiveKind::DefaultSrc]
            }
            DirectiveKind::StyleSrcElem | DirectiveKind::StyleSrcAttr => {
                &[DirectiveKind::StyleSrc, DirectiveKind::DefaultSrc]
            }
            DirectiveKind::WorkerSrc => &[DirectiveKind::ScriptSrc, DirectiveKind::DefaultSrc],
            DirectiveKind::ScriptSrc
            | DirectiveKind::StyleSrc
            | DirectiveKind::ImgSrc
            | DirectiveKind::ConnectSrc
            | DirectiveKind::FontSrc
            | DirectiveKind::ObjectSrc
            | DirectiveKind::MediaSrc
            | DirectiveKind::FrameSrc => &[DirectiveKind::DefaultSrc],
            DirectiveKind::DefaultSrc | DirectiveKind::BaseUri | DirectiveKind::FormAction => &[],
        };

        for &fallback in fallbacks {
            if let Some(sources) = self.directives.get(&fallback) {
                return Some(sources);
            }
        }

        None
    }

    /// Evaluates whether an inline script is permitted to execute.
    pub fn allows_script_inline(&self, code: &str, nonce: Option<&str>) -> bool {
        let sources = match self.effective_sources(DirectiveKind::ScriptSrcElem) {
            Some(s) => s,
            None => return true, // No restriction
        };

        if sources.iter().any(|s| matches!(s, SourceExpr::None)) {
            return false;
        }

        // Check nonce match
        if let Some(n) = nonce {
            if sources.iter().any(|s| match s {
                SourceExpr::Nonce(expected) => expected == n,
                _ => false,
            }) {
                return true;
            }
        }

        // Check hash match
        let code_hash = sha256_base64(code.as_bytes());
        if sources.iter().any(|s| match s {
            SourceExpr::Hash { algorithm, digest } if algorithm == "sha256" => digest == &code_hash,
            _ => false,
        }) {
            return true;
        }

        // Check unsafe-inline (only if no nonce or hash is specified in policy)
        let has_nonce_or_hash = sources.iter().any(|s| {
            matches!(
                s,
                SourceExpr::Nonce(_) | SourceExpr::Hash { .. } | SourceExpr::StrictDynamic
            )
        });

        if !has_nonce_or_hash
            && sources
                .iter()
                .any(|s| matches!(s, SourceExpr::UnsafeInline))
        {
            return true;
        }

        false
    }

    /// Evaluates whether an external script URL is permitted.
    pub fn allows_script_url(
        &self,
        page_origin: &str,
        script_url: &str,
        nonce: Option<&str>,
    ) -> bool {
        let sources = match self.effective_sources(DirectiveKind::ScriptSrcElem) {
            Some(s) => s,
            None => return true,
        };

        if sources.iter().any(|s| matches!(s, SourceExpr::None)) {
            return false;
        }

        if let Some(n) = nonce {
            if sources.iter().any(|s| match s {
                SourceExpr::Nonce(expected) => expected == n,
                _ => false,
            }) {
                return true;
            }
        }

        // W3C CSP Level 3 §strict-dynamic:
        // When 'strict-dynamic' is present in the policy, host and scheme sources
        // are ignored. Only scripts with valid nonces/hashes or dynamically created scripts are allowed.
        let has_strict_dynamic = sources
            .iter()
            .any(|s| matches!(s, SourceExpr::StrictDynamic));
        if has_strict_dynamic {
            return false;
        }

        self.matches_source_list(sources, page_origin, script_url)
    }

    /// Validates a resource request against the policy, returning Ok(()) if permitted or an error message.
    pub fn enforce(
        &self,
        directive: DirectiveKind,
        page_origin: &str,
        target_url: &str,
        nonce: Option<&str>,
    ) -> Result<(), String> {
        let sources = match self.effective_sources(directive) {
            Some(s) => s,
            None => return Ok(()),
        };

        if sources.iter().any(|s| matches!(s, SourceExpr::None)) {
            return Err(format!(
                "Blocked by CSP: directive {:?} specifies 'none'",
                directive
            ));
        }

        if let Some(n) = nonce {
            if sources.iter().any(|s| match s {
                SourceExpr::Nonce(expected) => expected == n,
                _ => false,
            }) {
                return Ok(());
            }
        }

        if matches!(
            directive,
            DirectiveKind::ScriptSrc | DirectiveKind::ScriptSrcElem
        ) && sources
            .iter()
            .any(|s| matches!(s, SourceExpr::StrictDynamic))
        {
            return Err(
                "Blocked by CSP: 'strict-dynamic' active without matching nonce/hash".into(),
            );
        }

        if self.matches_source_list(sources, page_origin, target_url) {
            Ok(())
        } else {
            Err(format!(
                "Blocked by CSP: URL {} violates {:?}",
                target_url, directive
            ))
        }
    }

    /// Evaluates whether `eval()` and `new Function()` dynamic JavaScript execution is permitted.
    pub fn allows_eval(&self) -> bool {
        let sources = match self.effective_sources(DirectiveKind::ScriptSrc) {
            Some(s) => s,
            None => return true,
        };

        if sources.iter().any(|s| matches!(s, SourceExpr::None)) {
            return false;
        }

        sources.iter().any(|s| matches!(s, SourceExpr::UnsafeEval))
    }

    /// Evaluates whether an inline style block is permitted.
    pub fn allows_style_inline(&self, style_css: &str, nonce: Option<&str>) -> bool {
        let sources = match self.effective_sources(DirectiveKind::StyleSrcElem) {
            Some(s) => s,
            None => return true,
        };

        if sources.iter().any(|s| matches!(s, SourceExpr::None)) {
            return false;
        }

        if let Some(n) = nonce {
            if sources.iter().any(|s| match s {
                SourceExpr::Nonce(expected) => expected == n,
                _ => false,
            }) {
                return true;
            }
        }

        let style_hash = sha256_base64(style_css.as_bytes());
        if sources.iter().any(|s| match s {
            SourceExpr::Hash { algorithm, digest } if algorithm == "sha256" => {
                digest == &style_hash
            }
            _ => false,
        }) {
            return true;
        }

        sources
            .iter()
            .any(|s| matches!(s, SourceExpr::UnsafeInline))
    }

    /// Evaluates whether an external stylesheet URL is permitted.
    pub fn allows_style_url(&self, page_origin: &str, style_url: &str) -> bool {
        let sources = match self.effective_sources(DirectiveKind::StyleSrcElem) {
            Some(s) => s,
            None => return true,
        };

        if sources.iter().any(|s| matches!(s, SourceExpr::None)) {
            return false;
        }

        self.matches_source_list(sources, page_origin, style_url)
    }

    /// Evaluates whether a network connect request (fetch, XMLHttpRequest, WebSocket) is permitted.
    pub fn allows_connect_url(&self, page_origin: &str, target_url: &str) -> bool {
        let sources = match self.effective_sources(DirectiveKind::ConnectSrc) {
            Some(s) => s,
            None => return true,
        };

        if sources.iter().any(|s| matches!(s, SourceExpr::None)) {
            return false;
        }

        self.matches_source_list(sources, page_origin, target_url)
    }

    /// Evaluates whether an image resource URL is permitted.
    pub fn allows_image_url(&self, page_origin: &str, target_url: &str) -> bool {
        let sources = match self.effective_sources(DirectiveKind::ImgSrc) {
            Some(s) => s,
            None => return true,
        };

        if sources.iter().any(|s| matches!(s, SourceExpr::None)) {
            return false;
        }

        self.matches_source_list(sources, page_origin, target_url)
    }

    /// Evaluates whether a font resource URL is permitted.
    pub fn allows_font_url(&self, page_origin: &str, target_url: &str) -> bool {
        let sources = match self.effective_sources(DirectiveKind::FontSrc) {
            Some(s) => s,
            None => return true,
        };

        if sources.iter().any(|s| matches!(s, SourceExpr::None)) {
            return false;
        }

        self.matches_source_list(sources, page_origin, target_url)
    }

    fn matches_source_list(
        &self,
        sources: &[SourceExpr],
        page_origin: &str,
        target_url: &str,
    ) -> bool {
        if sources.iter().any(|s| matches!(s, SourceExpr::Wildcard)) {
            return true;
        }

        let (target_scheme, target_host, target_port) = match Self::parse_origin_parts(target_url) {
            Some(parts) => parts,
            None => {
                // Relative URL matches page origin
                if !target_url.starts_with("http://") && !target_url.starts_with("https://") {
                    return true;
                }
                return false;
            }
        };

        let page_parts = Self::parse_origin_parts(page_origin);

        for expr in sources {
            match expr {
                SourceExpr::SelfSource => {
                    if let Some((ref p_sch, ref p_host, ref p_port)) = page_parts {
                        if p_sch == &target_scheme
                            && p_host == &target_host
                            && p_port == &target_port
                        {
                            return true;
                        }
                    }
                }
                SourceExpr::Scheme(sch) => {
                    let clean_sch = sch.trim_end_matches(':');
                    if target_scheme == clean_sch {
                        return true;
                    }
                }
                SourceExpr::Host {
                    scheme,
                    host,
                    port,
                    is_wildcard_subdomain,
                    ..
                } => {
                    if let Some(s) = scheme {
                        if s != &target_scheme {
                            continue;
                        }
                    }
                    if let Some(p) = port {
                        if Some(*p) != target_port {
                            continue;
                        }
                    }

                    if *is_wildcard_subdomain {
                        if target_host == *host || target_host.ends_with(&format!(".{host}")) {
                            return true;
                        }
                    } else if target_host == *host {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    fn parse_origin_parts(url: &str) -> Option<(String, String, Option<u16>)> {
        let idx = url.find("://")?;
        let (scheme, rest) = (url[..idx].to_ascii_lowercase(), &url[idx + 3..]);

        let authority = if let Some(slash_idx) = rest.find('/') {
            &rest[..slash_idx]
        } else {
            rest
        };

        let (host, port) = if let Some(colon_idx) = authority.find(':') {
            let h = &authority[..colon_idx];
            let p = authority[colon_idx + 1..].parse::<u16>().ok();
            (h.to_ascii_lowercase(), p)
        } else {
            let default_port = match scheme.as_str() {
                "http" => Some(80),
                "https" => Some(443),
                _ => None,
            };
            (authority.to_ascii_lowercase(), default_port)
        };

        Some((scheme, host, port))
    }
}

/// Extracts and parses a Content Security Policy from HTML `<meta http-equiv="Content-Security-Policy" ...>` tags.
pub fn extract_meta_csp(html: &str) -> Option<ContentSecurityPolicy> {
    let lower = html.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(meta_idx) = lower[pos..].find("<meta") {
        let abs_start = pos + meta_idx;
        let tag_end = match lower[abs_start..].find('>') {
            Some(end) => abs_start + end,
            None => break,
        };
        let tag_str = &html[abs_start..tag_end];
        let tag_lower = tag_str.to_ascii_lowercase();
        if tag_lower.contains("http-equiv") && tag_lower.contains("content-security-policy") {
            if let Some(content_idx) = tag_lower.find("content=") {
                let after_content = &tag_str[content_idx + 8..];
                let trimmed = after_content.trim_start();
                if let Some(quote) = trimmed.chars().next() {
                    if quote == '"' || quote == '\'' {
                        if let Some(end_quote) = trimmed[1..].find(quote) {
                            let policy_str = &trimmed[1..1 + end_quote];
                            return Some(ContentSecurityPolicy::parse(policy_str));
                        }
                    }
                }
            }
        }
        pos = tag_end + 1;
    }
    None
}

/// Convenience alias for ContentSecurityPolicy.
pub type CspPolicy = ContentSecurityPolicy;
