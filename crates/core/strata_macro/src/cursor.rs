//! Token stream cursor for zero-dependency parsing of Rust proc_macro tokens.

use proc_macro::{Delimiter, Group, TokenStream, TokenTree};

#[derive(Clone)]
pub struct TokenCursor {
    tokens: Vec<TokenTree>,
    pos: usize,
}

impl TokenCursor {
    pub fn new(stream: TokenStream) -> Self {
        Self {
            tokens: stream.into_iter().collect(),
            pos: 0,
        }
    }

    pub fn peek(&self) -> Option<&TokenTree> {
        self.tokens.get(self.pos)
    }

    pub fn peek_n(&self, offset: usize) -> Option<&TokenTree> {
        self.tokens.get(self.pos + offset)
    }

    pub fn next(&mut self) -> Option<TokenTree> {
        if self.pos < self.tokens.len() {
            let t = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }

    pub fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub fn peek_punct(&self, expected: char) -> bool {
        match self.peek() {
            Some(TokenTree::Punct(p)) => p.as_char() == expected,
            _ => false,
        }
    }

    pub fn peek_punct_at(&self, offset: usize, expected: char) -> bool {
        match self.peek_n(offset) {
            Some(TokenTree::Punct(p)) => p.as_char() == expected,
            _ => false,
        }
    }

    pub fn consume_punct(&mut self, expected: char) -> bool {
        if self.peek_punct(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub fn peek_ident(&self) -> Option<String> {
        match self.peek() {
            Some(TokenTree::Ident(id)) => Some(id.to_string()),
            _ => None,
        }
    }

    pub fn consume_ident(&mut self) -> Option<String> {
        if let Some(id) = self.peek_ident() {
            self.pos += 1;
            Some(id)
        } else {
            None
        }
    }

    pub fn peek_group(&self, delim: Delimiter) -> Option<&Group> {
        match self.peek() {
            Some(TokenTree::Group(g)) if g.delimiter() == delim => Some(g),
            _ => None,
        }
    }

    pub fn consume_group(&mut self, delim: Delimiter) -> Option<Group> {
        if self.peek_group(delim).is_some() {
            match self.next() {
                Some(TokenTree::Group(g)) => Some(g),
                _ => unreachable!(),
            }
        } else {
            None
        }
    }

    pub fn peek_literal_string(&self) -> Option<String> {
        match self.peek() {
            Some(TokenTree::Literal(lit)) => {
                let s = lit.to_string();
                if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                    Some(s[1..s.len() - 1].to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn consume_literal_string(&mut self) -> Option<String> {
        if let Some(s) = self.peek_literal_string() {
            self.pos += 1;
            Some(s)
        } else {
            None
        }
    }

    pub fn current_pos(&self) -> usize {
        self.pos
    }

    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }
}
