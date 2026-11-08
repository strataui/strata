//! Unicode Line Breaking Algorithm (UAX #14) and CSS white-space handling.
//!
//! Provides opportunistic break detection, Japanese Kinsoku Shori (prohibited start/end characters),
//! CJK ideographic wrapping, zero-width space handling, and line boundary segmentation.

use super::shaper::ShapedGlyph;
use strata_primitives::geom::LayoutUnit;

/// Unicode UAX #14 Line Breaking Class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineBreakClass {
    /// Mandatory Break (\n, \r, \u{000C}).
    BK,
    /// Carriage Return.
    CR,
    /// Line Feed.
    LF,
    /// Zero Width Space (\u{200B}).
    ZW,
    /// Word Joiner (\u{2060}, \u{FEFF}).
    WJ,
    /// Non-breaking Glue (\u{00A0}, \u{202F}).
    GL,
    /// Space (' ', '\t').
    SP,
    /// Open Punctuation (prohibited at line end: '(', '[', '{', '（', '「', '『', '【', '《').
    OP,
    /// Close Punctuation (prohibited at line start: ')', ']', '}', '）', '」', '』', '】', '》', '。', '、').
    CL,
    /// Close Parenthesis (prohibited at line start: ')', ']', '）', '］').
    CP,
    /// Exclamation / Interrogation (prohibited at line start: '!', '?', '！', '？').
    EX,
    /// Non-starter (prohibited at line start: small kana ぁ, ぃ, っ, ゃ, prolonged sound ー).
    NS,
    /// Ideographic / CJK character (allows break before and after other ideographs).
    ID,
    /// Ordinary Alphabetic character (Latin, Cyrillic, Greek, Arabic, Hebrew).
    AL,
    /// Numeric digit (0..=9).
    NU,
    /// Hyphen ('-').
    HY,
    /// Break After (em-dash '—', en-dash '–', soft hyphen \u{00AD}).
    BA,
    /// Break Before.
    BB,
    /// Inseparable (ellipsis '…', '‥').
    IN,
    /// Infix Numeric Separator (',', '.', ':', ';').
    IS,
    /// Prefix Numeric ('$', '£', '¥', '€', '¢').
    PR,
    /// Postfix Numeric ('%', '‰', '°').
    PO,
    /// Unknown / Other.
    XX,
}

/// Classifies a Unicode character according to Unicode Standard Annex #14.
pub fn classify_line_break(c: char) -> LineBreakClass {
    match c {
        '\n' => LineBreakClass::LF,
        '\r' => LineBreakClass::CR,
        '\u{000C}' => LineBreakClass::BK,
        '\u{200B}' => LineBreakClass::ZW,
        '\u{2060}' | '\u{FEFF}' => LineBreakClass::WJ,
        '\u{00A0}' | '\u{202F}' => LineBreakClass::GL,
        ' ' | '\t' => LineBreakClass::SP,

        // Open Punctuation (Gyomatsu Kinsoku - prohibited at end of line)
        '(' | '[' | '{' | '<' | '（' | '［' | '｛' | '〈' | '《' | '「' | '『' | '【' | '〔'
        | '“' | '‘' | '«' => LineBreakClass::OP,

        // Close Punctuation (Gyoto Kinsoku - prohibited at start of line)
        ')' | ']' | '}' | '>' | '）' | '］' | '｝' | '〉' | '》' | '」' | '』' | '】' | '〕'
        | '”' | '’' | '»' | '。' | '、' | '，' | '．' => LineBreakClass::CL,

        // Exclamations / Questions (Gyoto Kinsoku)
        '!' | '?' | '！' | '？' => LineBreakClass::EX,

        // Non-starter characters (Japanese Gyoto Kinsoku: small kana and prolonged sound marks)
        'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'っ' | 'ゃ' | 'ゅ' | 'ょ' | 'ゎ'
        | 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ' | 'ッ' | 'ャ' | 'ュ' | 'ョ' | 'ヮ' | 'ヶ' | 'ヵ'
        | 'ー' | '・' | '：' | '；' => LineBreakClass::NS,

        // Hyphens and break-after dashes
        '-' => LineBreakClass::HY,
        '—' | '–' | '\u{00AD}' => LineBreakClass::BA,

        // Inseparable ellipsis
        '…' | '‥' => LineBreakClass::IN,

        // Prefix and Postfix numeric
        '$' | '£' | '¥' | '€' | '¢' | '￥' => LineBreakClass::PR,
        '%' | '‰' | '°' => LineBreakClass::PO,

        // Infix numeric separators
        ',' | '.' | ':' | ';' => LineBreakClass::IS,

        // Numeric digits
        '0'..='9' | '٠'..='٩' => LineBreakClass::NU,

        // CJK Ideographs (Hiragana, Katakana, Kanji, Hanja, Hangul)
        '\u{4E00}'..='\u{9FFF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{20000}'..='\u{2A6DF}'
        | '\u{3040}'..='\u{309F}'
        | '\u{30A0}'..='\u{30FF}'
        | '\u{AC00}'..='\u{D7AF}' => LineBreakClass::ID,

        _ => LineBreakClass::AL,
    }
}

/// Returns true if character is prohibited at the start of a line (Japanese Gyoto Kinsoku).
#[inline]
pub fn is_prohibited_start(c: char) -> bool {
    matches!(
        classify_line_break(c),
        LineBreakClass::CL | LineBreakClass::CP | LineBreakClass::EX | LineBreakClass::NS
    )
}

/// Returns true if character is prohibited at the end of a line (Japanese Gyomatsu Kinsoku).
#[inline]
pub fn is_prohibited_end(c: char) -> bool {
    matches!(classify_line_break(c), LineBreakClass::OP | LineBreakClass::PR)
}

/// Determines whether a soft line break is permitted between two adjacent characters per UAX #14.
pub fn can_break_between_chars(prev: char, curr: char) -> bool {
    let p_class = classify_line_break(prev);
    let c_class = classify_line_break(curr);

    // 1. Mandatory breaks
    if matches!(p_class, LineBreakClass::BK | LineBreakClass::CR | LineBreakClass::LF) {
        return true;
    }

    // 2. Prohibited breaks (WJ, GL, Kinsoku Shori)
    if p_class == LineBreakClass::WJ || c_class == LineBreakClass::WJ {
        return false;
    }
    if p_class == LineBreakClass::GL || c_class == LineBreakClass::GL {
        return false;
    }
    // Never break before close punctuation, exclamations, or non-starters (Gyoto Kinsoku)
    if matches!(
        c_class,
        LineBreakClass::CL | LineBreakClass::CP | LineBreakClass::EX | LineBreakClass::NS
    ) {
        return false;
    }
    // Never break after open punctuation or currency prefix (Gyomatsu Kinsoku)
    if matches!(p_class, LineBreakClass::OP | LineBreakClass::PR) {
        return false;
    }

    // 3. Zero Width Space allows break after
    if p_class == LineBreakClass::ZW {
        return true;
    }

    // 4. Space allows break after
    if p_class == LineBreakClass::SP {
        return true;
    }

    // 5. CJK Ideographic breaks (ID x ID, ID x AL, AL x ID)
    if p_class == LineBreakClass::ID && c_class == LineBreakClass::ID {
        return true;
    }
    if p_class == LineBreakClass::ID && (c_class == LineBreakClass::AL || c_class == LineBreakClass::NU) {
        return true;
    }
    if (p_class == LineBreakClass::AL || p_class == LineBreakClass::NU) && c_class == LineBreakClass::ID {
        return true;
    }

    // 6. Close punctuation followed by an ideograph or word allows break
    if matches!(p_class, LineBreakClass::CL | LineBreakClass::CP)
        && (c_class == LineBreakClass::ID || c_class == LineBreakClass::AL)
    {
        return true;
    }

    // 7. Break after hyphen or dash
    if matches!(p_class, LineBreakClass::HY | LineBreakClass::BA) {
        return true;
    }

    false
}

/// CSS white-space property mode controlling whitespace collapsing and wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteSpaceMode {
    /// Collapses consecutive whitespace; wraps on line breaks and when line exceeds width.
    #[default]
    Normal,
    /// Collapses consecutive whitespace; suppresses all soft line wrapping.
    NoWrap,
    /// Preserves all whitespace and newlines; suppresses soft line wrapping.
    Pre,
    /// Preserves all whitespace and newlines; wraps lines when exceeding container width.
    PreWrap,
    /// Collapses consecutive whitespace; preserves explicit newlines; wraps on line bounds.
    PreLine,
}

impl WhiteSpaceMode {
    /// Whether consecutive whitespace characters are collapsed into a single space.
    #[inline(always)]
    pub fn collapses_spaces(self) -> bool {
        matches!(
            self,
            WhiteSpaceMode::Normal | WhiteSpaceMode::NoWrap | WhiteSpaceMode::PreLine
        )
    }

    /// Whether soft wrapping is allowed when line exceeds container width.
    #[inline(always)]
    pub fn allows_soft_wrap(self) -> bool {
        matches!(
            self,
            WhiteSpaceMode::Normal | WhiteSpaceMode::PreWrap | WhiteSpaceMode::PreLine
        )
    }

    /// Whether explicit newline characters (\n) force a line break.
    #[inline(always)]
    pub fn preserves_newlines(self) -> bool {
        matches!(
            self,
            WhiteSpaceMode::Pre | WhiteSpaceMode::PreWrap | WhiteSpaceMode::PreLine
        )
    }
}

/// A segmented line of shaped glyphs resulting from the line breaking pass.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    /// The shaped glyphs placed on this line.
    pub glyphs: Vec<ShapedGlyph>,
    /// Total horizontal advance width of the line excluding trailing whitespace.
    pub content_width: LayoutUnit,
    /// Advance width of trailing whitespace.
    pub trailing_space_width: LayoutUnit,
    /// Whether this line ended with an explicit mandatory break (\n).
    pub is_hard_break: bool,
}

/// Unicode UAX #14 Line Breaker.
#[derive(Debug, Clone, Default)]
pub struct LineBreaker;

impl LineBreaker {
    pub fn new() -> Self {
        Self
    }

    /// Breaks a stream of shaped glyphs into visual lines fitting within `max_width`.
    pub fn break_lines(
        &self,
        glyphs: &[ShapedGlyph],
        max_width: Option<LayoutUnit>,
        mode: WhiteSpaceMode,
    ) -> Vec<TextLine> {
        if glyphs.is_empty() {
            return Vec::new();
        }

        let max_w = max_width.unwrap_or(LayoutUnit(i32::MAX));
        let allows_soft_wrap = mode.allows_soft_wrap() && max_width.is_some();
        let collapses_spaces = mode.collapses_spaces();

        let mut lines = Vec::new();
        let mut current_glyphs = Vec::new();
        let mut current_width = LayoutUnit::ZERO;

        let mut last_break_idx = None;
        let mut prev_was_space = false;
        let mut prev_char: Option<char> = None;
        let mut idx = 0;

        while idx < glyphs.len() {
            let glyph = glyphs[idx];
            let curr_char = char::from_u32(glyph.glyph_id).unwrap_or(' ');

            // 1. Check for hard newline character (\n is glyph_id 10)
            if glyph.glyph_id == 10 || glyph.glyph_id == 13 {
                let (content_w, trailing_w) = Self::calculate_widths(&current_glyphs);
                lines.push(TextLine {
                    glyphs: std::mem::take(&mut current_glyphs),
                    content_width: content_w,
                    trailing_space_width: trailing_w,
                    is_hard_break: true,
                });
                current_width = LayoutUnit::ZERO;
                last_break_idx = None;
                prev_was_space = false;
                prev_char = None;
                idx += 1;
                continue;
            }

            // 2. Whitespace collapsing
            if collapses_spaces && glyph.is_space {
                if prev_was_space || current_glyphs.is_empty() {
                    idx += 1;
                    continue;
                }
                prev_was_space = true;
            } else {
                prev_was_space = false;
            }

            // 3. Record UAX #14 break opportunities between characters
            if let Some(p_ch) = prev_char {
                if can_break_between_chars(p_ch, curr_char) {
                    last_break_idx = Some(current_glyphs.len());
                }
            }

            // 4. Check for line overflow and break opportunity
            let advance = glyph.x_advance;
            let fits = current_width + advance <= max_w;

            if !fits && allows_soft_wrap && !current_glyphs.is_empty() {
                if let Some(break_pos) = last_break_idx {
                    if break_pos > 0 && break_pos < current_glyphs.len() {
                        let remainder: Vec<ShapedGlyph> = current_glyphs.drain(break_pos..).collect();
                        let (content_w, trailing_w) = Self::calculate_widths(&current_glyphs);

                        lines.push(TextLine {
                            glyphs: std::mem::take(&mut current_glyphs),
                            content_width: content_w,
                            trailing_space_width: trailing_w,
                            is_hard_break: false,
                        });

                        current_glyphs = remainder;
                        current_width = current_glyphs
                            .iter()
                            .map(|g| g.x_advance)
                            .fold(LayoutUnit::ZERO, |a, b| a + b);
                        last_break_idx = None;
                    } else {
                        // Forced break on overflow
                        let (content_w, trailing_w) = Self::calculate_widths(&current_glyphs);
                        lines.push(TextLine {
                            glyphs: std::mem::take(&mut current_glyphs),
                            content_width: content_w,
                            trailing_space_width: trailing_w,
                            is_hard_break: false,
                        });
                        current_width = LayoutUnit::ZERO;
                        last_break_idx = None;
                    }
                } else {
                    // Forced emergency break on overflow
                    let (content_w, trailing_w) = Self::calculate_widths(&current_glyphs);
                    lines.push(TextLine {
                        glyphs: std::mem::take(&mut current_glyphs),
                        content_width: content_w,
                        trailing_space_width: trailing_w,
                        is_hard_break: false,
                    });
                    current_width = LayoutUnit::ZERO;
                    last_break_idx = None;
                }
            }

            current_width = current_width + advance;
            current_glyphs.push(glyph);
            prev_char = Some(curr_char);
            idx += 1;
        }

        // Flush remaining line
        if !current_glyphs.is_empty() {
            let (content_w, trailing_w) = Self::calculate_widths(&current_glyphs);
            lines.push(TextLine {
                glyphs: current_glyphs,
                content_width: content_w,
                trailing_space_width: trailing_w,
                is_hard_break: false,
            });
        }

        lines
    }

    /// Computes content width and trailing space width for a set of glyphs.
    fn calculate_widths(glyphs: &[ShapedGlyph]) -> (LayoutUnit, LayoutUnit) {
        if glyphs.is_empty() {
            return (LayoutUnit::ZERO, LayoutUnit::ZERO);
        }

        let mut trailing_w = LayoutUnit::ZERO;

        for g in glyphs.iter().rev() {
            if g.is_space {
                trailing_w = trailing_w + g.x_advance;
            } else {
                break;
            }
        }

        let total_w = glyphs
            .iter()
            .map(|g| g.x_advance)
            .fold(LayoutUnit::ZERO, |a, b| a + b);
        let content_w = total_w - trailing_w;

        (content_w, trailing_w)
    }
}
