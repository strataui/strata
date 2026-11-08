//! HTML5 stream tokenizer and character entity decoder.
//!
//! Converts raw HTML text into a structured stream of HTML tokens,
//! resolving named and numeric HTML character entities with zero external dependencies.

/// A token emitted by the HTML tokenizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlToken {
    /// Opening tag: `<tag attr="val">` or `<tag />`
    StartTag {
        name: String,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    },
    /// Closing tag: `</tag>`
    EndTag {
        name: String,
    },
    /// Raw character data with decoded entities.
    Text(String),
    /// Comment: `<!-- content -->`
    Comment(String),
    /// Document type declaration: `<!DOCTYPE ...>`
    DocType(String),
}

/// Decodes common named HTML entities and numeric decimal/hexadecimal character references.
pub fn decode_html_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }

    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            let mut found_semicolon = false;

            while let Some(&next_ch) = chars.peek() {
                if next_ch == ';' {
                    chars.next();
                    found_semicolon = true;
                    break;
                } else if next_ch.is_alphanumeric() || next_ch == '#' {
                    if let Some(ch) = chars.next() {
                        entity.push(ch);
                    }
                    if entity.len() > 10 {
                        break;
                    }
                } else {
                    break;
                }
            }

            if found_semicolon && !entity.is_empty() {
                if let Some(decoded) = resolve_entity(&entity) {
                    output.push_str(&decoded);
                    continue;
                }
            }

            // Fallback: not a recognized entity, emit literally
            output.push('&');
            output.push_str(&entity);
            if found_semicolon {
                output.push(';');
            }
        } else {
            output.push(ch);
        }
    }

    output
}

fn resolve_entity(entity: &str) -> Option<String> {
    if let Some(stripped) = entity.strip_prefix('#') {
        if let Some(hex) = stripped.strip_prefix('x').or_else(|| stripped.strip_prefix('X')) {
            u32::from_str_radix(hex, 16)
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
        } else {
            stripped
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
        }
    } else {
        match entity {
            "amp" => Some("&".to_string()),
            "lt" => Some("<".to_string()),
            "gt" => Some(">".to_string()),
            "quot" => Some("\"".to_string()),
            "apos" => Some("'".to_string()),
            "nbsp" => Some("\u{00A0}".to_string()),
            "mdash" => Some("—".to_string()),
            "ndash" => Some("–".to_string()),
            "copy" => Some("©".to_string()),
            "reg" => Some("®".to_string()),
            "trade" => Some("™".to_string()),
            "bull" => Some("•".to_string()),
            "hellip" => Some("…".to_string()),
            "cent" => Some("¢".to_string()),
            "pound" => Some("£".to_string()),
            "yen" => Some("¥".to_string()),
            "euro" => Some("€".to_string()),
            "laquo" => Some("«".to_string()),
            "raquo" => Some("»".to_string()),
            "times" => Some("×".to_string()),
            "divide" => Some("÷".to_string()),
            "plusmn" => Some("±".to_string()),
            "minus" => Some("−".to_string()),
            "ge" => Some("≥".to_string()),
            "le" => Some("≤".to_string()),
            "ne" => Some("≠".to_string()),
            "equiv" => Some("≡".to_string()),
            "approx" | "asymp" => Some("≈".to_string()),
            "sim" => Some("∼".to_string()),
            "cong" => Some("≅".to_string()),
            "radic" => Some("√".to_string()),
            "infin" => Some("∞".to_string()),
            "sum" => Some("∑".to_string()),
            "prod" => Some("∏".to_string()),
            "int" => Some("∫".to_string()),
            "part" => Some("∂".to_string()),
            "forall" => Some("∀".to_string()),
            "exist" => Some("∃".to_string()),
            "empty" => Some("∅".to_string()),
            "isin" => Some("∈".to_string()),
            "notin" => Some("∉".to_string()),
            "ni" => Some("∋".to_string()),
            "sub" => Some("⊂".to_string()),
            "sup" => Some("⊃".to_string()),
            "nsub" => Some("⊄".to_string()),
            "sube" => Some("⊆".to_string()),
            "supe" => Some("⊇".to_string()),
            "cap" => Some("∩".to_string()),
            "cup" => Some("∪".to_string()),
            "and" => Some("∧".to_string()),
            "or" => Some("∨".to_string()),
            "not" => Some("¬".to_string()),
            "perp" => Some("⊥".to_string()),
            "sdot" => Some("⋅".to_string()),
            "ang" => Some("∠".to_string()),
            "larr" => Some("←".to_string()),
            "rarr" => Some("→".to_string()),
            "uarr" => Some("↑".to_string()),
            "darr" => Some("↓".to_string()),
            "harr" => Some("↔".to_string()),
            "crarr" => Some("↵".to_string()),
            "lArr" => Some("⇐".to_string()),
            "rArr" => Some("⇒".to_string()),
            "uArr" => Some("⇑".to_string()),
            "dArr" => Some("⇓".to_string()),
            "hArr" => Some("⇔".to_string()),
            "deg" => Some("°".to_string()),
            "middot" => Some("·".to_string()),
            "dagger" => Some("†".to_string()),
            "Dagger" => Some("‡".to_string()),
            "permil" => Some("‰".to_string()),
            "prime" => Some("′".to_string()),
            "Prime" => Some("″".to_string()),
            "lsaquo" => Some("‹".to_string()),
            "rsaquo" => Some("›".to_string()),
            "oline" => Some("‾".to_string()),
            "sect" => Some("§".to_string()),
            "para" => Some("¶".to_string()),
            "micro" => Some("µ".to_string()),
            "lceil" => Some("⌈".to_string()),
            "rceil" => Some("⌉".to_string()),
            "lfloor" => Some("⌊".to_string()),
            "rfloor" => Some("⌋".to_string()),
            "alpha" => Some("α".to_string()),
            "beta" => Some("β".to_string()),
            "gamma" => Some("γ".to_string()),
            "delta" => Some("δ".to_string()),
            "epsilon" => Some("ε".to_string()),
            "zeta" => Some("ζ".to_string()),
            "eta" => Some("η".to_string()),
            "theta" => Some("θ".to_string()),
            "iota" => Some("ι".to_string()),
            "kappa" => Some("κ".to_string()),
            "lambda" => Some("λ".to_string()),
            "mu" => Some("μ".to_string()),
            "nu" => Some("ν".to_string()),
            "xi" => Some("ξ".to_string()),
            "omicron" => Some("ο".to_string()),
            "pi" => Some("π".to_string()),
            "rho" => Some("ρ".to_string()),
            "sigma" => Some("σ".to_string()),
            "sigmaf" => Some("ς".to_string()),
            "tau" => Some("τ".to_string()),
            "upsilon" => Some("υ".to_string()),
            "phi" => Some("φ".to_string()),
            "chi" => Some("χ".to_string()),
            "psi" => Some("ψ".to_string()),
            "omega" => Some("ω".to_string()),
            "Alpha" => Some("Α".to_string()),
            "Beta" => Some("Β".to_string()),
            "Gamma" => Some("Γ".to_string()),
            "Delta" => Some("Δ".to_string()),
            "Epsilon" => Some("Ε".to_string()),
            "Zeta" => Some("Ζ".to_string()),
            "Eta" => Some("Η".to_string()),
            "Theta" => Some("Θ".to_string()),
            "Iota" => Some("Ι".to_string()),
            "Kappa" => Some("Κ".to_string()),
            "Lambda" => Some("Λ".to_string()),
            "Mu" => Some("Μ".to_string()),
            "Nu" => Some("Ν".to_string()),
            "Xi" => Some("Ξ".to_string()),
            "Omicron" => Some("Ο".to_string()),
            "Pi" => Some("Π".to_string()),
            "Rho" => Some("Ρ".to_string()),
            "Sigma" => Some("Σ".to_string()),
            "Tau" => Some("Τ".to_string()),
            "Upsilon" => Some("Υ".to_string()),
            "Phi" => Some("Φ".to_string()),
            "Chi" => Some("Χ".to_string()),
            "Psi" => Some("Ψ".to_string()),
            "Omega" => Some("Ω".to_string()),
            _ => None,
        }
    }
}

/// Tokenizes an HTML source string into a sequential vector of `HtmlToken` items.
pub struct HtmlTokenizer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    raw_text_tag: Option<String>,
    pending_token: Option<HtmlToken>,
}

impl<'a> HtmlTokenizer<'a> {
    pub fn new(html: &'a str) -> Self {
        Self {
            chars: html.chars().peekable(),
            raw_text_tag: None,
            pending_token: None,
        }
    }

    pub fn tokenize(&mut self) -> Vec<HtmlToken> {
        let mut tokens = Vec::new();

        while self.chars.peek().is_some() || self.pending_token.is_some() {
            if let Some(token) = self.next_token() {
                tokens.push(token);
            }
        }

        tokens
    }

    fn next_token(&mut self) -> Option<HtmlToken> {
        if let Some(tok) = self.pending_token.take() {
            return Some(tok);
        }

        if let Some(tag) = self.raw_text_tag.clone() {
            return self.lex_raw_text(&tag);
        }

        match self.chars.peek() {
            Some(&'<') => {
                self.chars.next(); // Consume '<'
                self.lex_tag_or_comment()
            }
            Some(_) => self.lex_text(),
            None => None,
        }
    }

    fn lex_raw_text(&mut self, tag: &str) -> Option<HtmlToken> {
        let mut content = String::new();
        let tag_lower = tag.to_ascii_lowercase();

        while self.chars.peek().is_some() {
            if let Some(&'<') = self.chars.peek() {
                let mut lookahead = self.chars.clone();
                lookahead.next(); // consume '<'
                if let Some('/') = lookahead.next() {
                    let mut closing_name = String::new();
                    while let Some(&c) = lookahead.peek() {
                        if c.is_alphanumeric() || c == '-' || c == '_' {
                            closing_name.push(c);
                            lookahead.next();
                        } else {
                            break;
                        }
                    }
                    if closing_name.eq_ignore_ascii_case(&tag_lower) {
                        while let Some(&c) = lookahead.peek() {
                            if c.is_whitespace() {
                                lookahead.next();
                            } else {
                                break;
                            }
                        }
                        if let Some('>') = lookahead.next() {
                            self.chars = lookahead;
                            self.raw_text_tag = None;
                            if content.is_empty() {
                                return Some(HtmlToken::EndTag { name: tag_lower });
                            } else {
                                self.pending_token = Some(HtmlToken::EndTag { name: tag_lower });
                                return Some(HtmlToken::Text(content));
                            }
                        }
                    }
                }
            }
            if let Some(ch) = self.chars.next() {
                content.push(ch);
            }
        }

        self.raw_text_tag = None;
        if content.is_empty() {
            None
        } else {
            Some(HtmlToken::Text(content))
        }
    }

    fn lex_text(&mut self) -> Option<HtmlToken> {
        let mut raw_text = String::new();

        while let Some(&ch) = self.chars.peek() {
            if ch == '<' {
                break;
            }
            if let Some(ch) = self.chars.next() {
                raw_text.push(ch);
            }
        }

        if raw_text.is_empty() {
            None
        } else {
            let decoded = decode_html_entities(&raw_text);
            Some(HtmlToken::Text(decoded))
        }
    }

    fn lex_tag_or_comment(&mut self) -> Option<HtmlToken> {
        // 1. Comment or DocType starting with '!'
        if let Some(&'!') = self.chars.peek() {
            self.chars.next(); // consume '!'

            // Check if comment: <!--
            if let Some(&'-') = self.chars.peek() {
                self.chars.next();
                if let Some(&'-') = self.chars.peek() {
                    self.chars.next();
                    return Some(self.lex_comment_content());
                }
            }

            // Otherwise DOCTYPE or directive
            let mut doctype = String::new();
            for ch in self.chars.by_ref() {
                if ch == '>' {
                    break;
                }
                doctype.push(ch);
            }
            return Some(HtmlToken::DocType(doctype.trim().to_string()));
        }

        // 2. End tag starting with '/'
        if let Some(&'/') = self.chars.peek() {
            self.chars.next(); // consume '/'
            let mut tag_name = String::new();
            while let Some(&ch) = self.chars.peek() {
                if ch == '>' {
                    self.chars.next();
                    break;
                } else if ch.is_whitespace() {
                    self.chars.next();
                } else if let Some(ch) = self.chars.next() {
                    tag_name.push(ch);
                }
            }
            return Some(HtmlToken::EndTag {
                name: tag_name.to_lowercase(),
            });
        }

        // 3. Start tag or self-closing tag
        let mut tag_name = String::new();
        while let Some(&ch) = self.chars.peek() {
            if ch == '>' || ch == '/' || ch.is_whitespace() {
                break;
            }
            if let Some(ch) = self.chars.next() {
                tag_name.push(ch);
            }
        }

        let mut attributes = Vec::new();
        let mut self_closing = false;

        loop {
            self.skip_whitespace();
            match self.chars.peek() {
                Some(&'>') => {
                    self.chars.next();
                    break;
                }
                Some(&'/') => {
                    self.chars.next();
                    self.skip_whitespace();
                    if let Some(&'>') = self.chars.peek() {
                        self.chars.next();
                        self_closing = true;
                        break;
                    }
                }
                Some(_) => {
                    if let Some((k, v)) = self.lex_attribute() {
                        attributes.push((k, v));
                    } else {
                        // Skip unparsable character
                        self.chars.next();
                    }
                }
                None => break,
            }
        }

        let name_lower = tag_name.to_lowercase();
        if !self_closing && matches!(name_lower.as_str(), "style" | "script" | "textarea" | "title") {
            self.raw_text_tag = Some(name_lower.clone());
        }

        Some(HtmlToken::StartTag {
            name: name_lower,
            attributes,
            self_closing,
        })
    }

    fn lex_comment_content(&mut self) -> HtmlToken {
        let mut content = String::new();
        let mut dash_count = 0;

        for ch in self.chars.by_ref() {
            if ch == '-' {
                dash_count += 1;
            } else if ch == '>' && dash_count >= 2 {
                // End of comment
                let len = content.len();
                if len >= dash_count {
                    content.truncate(len - dash_count);
                }
                break;
            } else {
                dash_count = 0;
            }
            content.push(ch);
        }

        HtmlToken::Comment(content.trim().to_string())
    }

    fn lex_attribute(&mut self) -> Option<(String, String)> {
        let mut name = String::new();
        while let Some(&ch) = self.chars.peek() {
            if ch == '=' || ch == '>' || ch == '/' || ch.is_whitespace() {
                break;
            }
            if let Some(ch) = self.chars.next() {
                name.push(ch);
            }
        }

        if name.is_empty() {
            return None;
        }

        self.skip_whitespace();
        let name_lower = name.to_lowercase();

        if let Some(&'=') = self.chars.peek() {
            self.chars.next(); // consume '='
            self.skip_whitespace();

            let val = match self.chars.peek() {
                Some(&'"') => {
                    self.chars.next();
                    let mut s = String::new();
                    for ch in self.chars.by_ref() {
                        if ch == '"' {
                            break;
                        }
                        s.push(ch);
                    }
                    decode_html_entities(&s)
                }
                Some(&'\'') => {
                    self.chars.next();
                    let mut s = String::new();
                    for ch in self.chars.by_ref() {
                        if ch == '\'' {
                            break;
                        }
                        s.push(ch);
                    }
                    decode_html_entities(&s)
                }
                Some(_) => {
                    let mut s = String::new();
                    while let Some(&ch) = self.chars.peek() {
                        if ch.is_whitespace() || ch == '>' || ch == '/' {
                            break;
                        }
                        if let Some(ch) = self.chars.next() {
                            s.push(ch);
                        }
                    }
                    decode_html_entities(&s)
                }
                None => String::new(),
            };

            Some((name_lower, val))
        } else {
            // Boolean attribute (e.g. `disabled`, `checked`)
            Some((name_lower.clone(), name_lower))
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&ch) = self.chars.peek() {
            if ch.is_whitespace() {
                self.chars.next();
            } else {
                break;
            }
        }
    }
}

/// An incremental streaming HTML tokenizer that accepts incoming chunks of text
/// and yields complete tokens as soon as boundary delimiters are parsed.
#[derive(Debug, Default, Clone)]
pub struct StreamTokenizer {
    buffer: String,
}

impl StreamTokenizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds a chunk of incoming text bytes/characters and drains all complete tokens.
    pub fn feed_chunk(&mut self, chunk: &str) -> Vec<HtmlToken> {
        self.buffer.push_str(chunk);
        self.drain_complete_tokens(false)
    }

    /// Signals end of stream, draining all remaining buffered content as tokens.
    pub fn finish(&mut self) -> Vec<HtmlToken> {
        self.drain_complete_tokens(true)
    }

    fn drain_complete_tokens(&mut self, is_eof: bool) -> Vec<HtmlToken> {
        let mut tokens = Vec::new();
        loop {
            if self.buffer.is_empty() {
                break;
            }

            if self.buffer.starts_with('<') {
                if self.buffer.starts_with("<!--") {
                    if let Some(end_idx) = self.buffer.find("-->") {
                        let comment_chunk = &self.buffer[..end_idx + 3];
                        let mut tok = HtmlTokenizer::new(comment_chunk);
                        if let Some(t) = tok.tokenize().into_iter().next() {
                            tokens.push(t);
                        }
                        self.buffer.drain(..end_idx + 3);
                        continue;
                    } else if !is_eof {
                        break;
                    }
                }

                if let Some(end_idx) = self.buffer.find('>') {
                    let tag_chunk = self.buffer[..=end_idx].to_string();
                    let mut tok = HtmlTokenizer::new(&tag_chunk);
                    let parsed = tok.tokenize();
                    let is_raw_text = parsed.iter().any(|t| match t {
                        HtmlToken::StartTag { name, .. } => matches!(name.to_ascii_lowercase().as_str(), "script" | "style" | "title"),
                        _ => false,
                    });

                    if is_raw_text {
                        let tag_name = match &parsed[0] {
                            HtmlToken::StartTag { name, .. } => name.to_ascii_lowercase(),
                            _ => "".to_string(),
                        };
                        let closing_tag = format!("</{}>", tag_name);
                        if let Some(close_idx) = self.buffer.to_ascii_lowercase().find(&closing_tag) {
                            let total_len = close_idx + closing_tag.len();
                            let raw_chunk = self.buffer[..total_len].to_string();
                            let mut raw_tok = HtmlTokenizer::new(&raw_chunk);
                            tokens.extend(raw_tok.tokenize());
                            self.buffer.drain(..total_len);
                            continue;
                        } else if !is_eof {
                            break;
                        }
                    }

                    tokens.extend(parsed);
                    self.buffer.drain(..=end_idx);
                } else if !is_eof {
                    break;
                } else {
                    tokens.push(HtmlToken::Text(decode_html_entities(&self.buffer)));
                    self.buffer.clear();
                    break;
                }
            } else {
                if let Some(next_tag) = self.buffer.find('<') {
                    let text_chunk = self.buffer[..next_tag].to_string();
                    if !is_eof && text_chunk.contains('&') {
                        if let Some(last_amp) = text_chunk.rfind('&') {
                            if !text_chunk[last_amp..].contains(';') && text_chunk.len() - last_amp <= 10 {
                                if last_amp > 0 {
                                    let sub_chunk = &text_chunk[..last_amp];
                                    let decoded = decode_html_entities(sub_chunk);
                                    if !decoded.is_empty() {
                                        tokens.push(HtmlToken::Text(decoded));
                                    }
                                    self.buffer.drain(..last_amp);
                                }
                                break;
                            }
                        }
                    }
                    let decoded = decode_html_entities(&text_chunk);
                    if !decoded.is_empty() {
                        tokens.push(HtmlToken::Text(decoded));
                    }
                    self.buffer.drain(..next_tag);
                } else if !is_eof {
                    if let Some(last_amp) = self.buffer.rfind('&') {
                        if !self.buffer[last_amp..].contains(';') && self.buffer.len() - last_amp <= 10 {
                            if last_amp > 0 {
                                let sub_chunk = self.buffer[..last_amp].to_string();
                                let decoded = decode_html_entities(&sub_chunk);
                                if !decoded.is_empty() {
                                    tokens.push(HtmlToken::Text(decoded));
                                }
                                self.buffer.drain(..last_amp);
                            }
                            break;
                        }
                    }
                    let decoded = decode_html_entities(&self.buffer);
                    if !decoded.is_empty() {
                        tokens.push(HtmlToken::Text(decoded));
                    }
                    self.buffer.clear();
                    break;
                } else {
                    let decoded = decode_html_entities(&self.buffer);
                    if !decoded.is_empty() {
                        tokens.push(HtmlToken::Text(decoded));
                    }
                    self.buffer.clear();
                    break;
                }
            }
        }
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_entity_decoding() {
        assert_eq!(decode_html_entities("&amp;"), "&");
        assert_eq!(decode_html_entities("&lt;div&gt;"), "<div>");
        assert_eq!(decode_html_entities("&quot;hello&quot;"), "\"hello\"");
        assert_eq!(decode_html_entities("&ge; 0.95"), "≥ 0.95");
        assert_eq!(decode_html_entities("&le; 100"), "≤ 100");
        assert_eq!(decode_html_entities("10 &times; 20 = 200"), "10 × 20 = 200");
        assert_eq!(decode_html_entities("&alpha; &rarr; &omega;"), "α → ω");
        assert_eq!(decode_html_entities("&#8805;"), "≥");
        assert_eq!(decode_html_entities("&#x2265;"), "≥");
        assert_eq!(decode_html_entities("&plusmn;5&deg;"), "±5°");
    }
}

