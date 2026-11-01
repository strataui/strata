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
                    entity.push(chars.next().unwrap());
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
            _ => None,
        }
    }
}

/// Tokenizes an HTML source string into a sequential vector of `HtmlToken` items.
pub struct HtmlTokenizer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> HtmlTokenizer<'a> {
    pub fn new(html: &'a str) -> Self {
        Self {
            chars: html.chars().peekable(),
        }
    }

    pub fn tokenize(&mut self) -> Vec<HtmlToken> {
        let mut tokens = Vec::new();

        while self.chars.peek().is_some() {
            if let Some(token) = self.next_token() {
                tokens.push(token);
            }
        }

        tokens
    }

    fn next_token(&mut self) -> Option<HtmlToken> {
        match self.chars.peek() {
            Some(&'<') => {
                self.chars.next(); // Consume '<'
                self.lex_tag_or_comment()
            }
            Some(_) => self.lex_text(),
            None => None,
        }
    }

    fn lex_text(&mut self) -> Option<HtmlToken> {
        let mut raw_text = String::new();

        while let Some(&ch) = self.chars.peek() {
            if ch == '<' {
                break;
            }
            raw_text.push(self.chars.next().unwrap());
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
            while let Some(ch) = self.chars.next() {
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
                } else {
                    tag_name.push(self.chars.next().unwrap());
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
            tag_name.push(self.chars.next().unwrap());
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

        Some(HtmlToken::StartTag {
            name: tag_name.to_lowercase(),
            attributes,
            self_closing,
        })
    }

    fn lex_comment_content(&mut self) -> HtmlToken {
        let mut content = String::new();
        let mut dash_count = 0;

        while let Some(ch) = self.chars.next() {
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
            name.push(self.chars.next().unwrap());
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
                    while let Some(ch) = self.chars.next() {
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
                    while let Some(ch) = self.chars.next() {
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
                        s.push(self.chars.next().unwrap());
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
