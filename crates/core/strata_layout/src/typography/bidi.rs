//! Unicode Bidirectional Algorithm (UAX#9) implementation for Strata.
//!
//! Provides bidirectional character classification, paragraph direction heuristic,
//! weak and neutral type resolution, embedding level determination, and visual run reordering.

/// Unicode bidirectional character type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidiClass {
    /// Left-to-Right (e.g. Latin, Cyrillic, Greek, most symbols).
    L,
    /// Right-to-Left (e.g. Hebrew script).
    R,
    /// Right-to-Left Arabic (Arabic alphabet).
    AL,
    /// European Number (0..=9).
    EN,
    /// European Number Separator (+, -).
    ES,
    /// European Number Terminator ($, %, degree).
    ET,
    /// Arabic Number (Eastern Arabic-Indic digits ٠..٩).
    AN,
    /// Common Number Separator (comma, period, colon).
    CS,
    /// Nonspacing Mark (accents, diacritics).
    NSM,
    /// Boundary Neutral (control characters, zero-width chars).
    BN,
    /// Paragraph Separator (\n, \r).
    B,
    /// Segment Separator (tab).
    S,
    /// Whitespace (space).
    WS,
    /// Other Neutral (brackets, punctuation, mathematical symbols).
    ON,
}

impl BidiClass {
    /// Classifies a Unicode character according to Unicode Standard Annex #9.
    pub fn classify(c: char) -> Self {
        // Arabic Letters and presentation forms
        if (c >= '\u{0600}' && c <= '\u{065F}')
            || (c >= '\u{066A}' && c <= '\u{06FF}')
            || (c >= '\u{0750}' && c <= '\u{077F}')
            || (c >= '\u{FB50}' && c <= '\u{FDFF}')
            || (c >= '\u{FE70}' && c <= '\u{FEFF}')
        {
            return BidiClass::AL;
        }

        // Arabic-Indic Digits
        if c >= '\u{0660}' && c <= '\u{0669}' {
            return BidiClass::AN;
        }

        // Hebrew script
        if (c >= '\u{0590}' && c <= '\u{05FF}') || (c >= '\u{FB1D}' && c <= '\u{FB4F}') {
            return BidiClass::R;
        }

        // European Digits
        if c >= '0' && c <= '9' {
            return BidiClass::EN;
        }

        match c {
            '\n' | '\r' => BidiClass::B,
            '\t' => BidiClass::S,
            ' ' | '\u{00A0}' => BidiClass::WS,
            '+' | '-' => BidiClass::ES,
            '$' | '%' | '€' | '£' | '¥' => BidiClass::ET,
            '.' | ',' | ':' | '/' => BidiClass::CS,
            '\u{200B}'..='\u{200D}' | '\u{FEFF}' => BidiClass::BN,
            '!' | '?' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '=' | '_' => {
                BidiClass::ON
            }
            _ => {
                // Default Latin and standard script code points
                BidiClass::L
            }
        }
    }

    /// Whether this is a strong directional type (L, R, or AL).
    #[inline(always)]
    pub fn is_strong(self) -> bool {
        matches!(self, BidiClass::L | BidiClass::R | BidiClass::AL)
    }

    /// Whether this is a number type (EN or AN).
    #[inline(always)]
    pub fn is_number(self) -> bool {
        matches!(self, BidiClass::EN | BidiClass::AN)
    }
}

/// A contiguous segment of text sharing a uniform resolved embedding level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BidiRun {
    /// Byte start offset in original text.
    pub start: usize,
    /// Byte end offset in original text (exclusive).
    pub end: usize,
    /// Resolved embedding level (even = LTR, odd = RTL).
    pub level: u8,
    /// True if this run renders right-to-left.
    pub is_rtl: bool,
}

/// Bidirectional layout algorithm engine.
#[derive(Debug, Clone, Default)]
pub struct BidiEngine;

impl BidiEngine {
    pub fn new() -> Self {
        Self
    }

    /// Detects paragraph base direction (0 = LTR, 1 = RTL).
    /// Rule P2/P3: The first character of type L, AL, or R determines base level.
    pub fn detect_base_level(text: &str) -> u8 {
        for c in text.chars() {
            let class = BidiClass::classify(c);
            if class == BidiClass::AL || class == BidiClass::R {
                return 1;
            } else if class == BidiClass::L {
                return 0;
            }
        }
        0 // Default to LTR if no strong characters are present
    }

    /// Splits text into visual runs according to UAX#9 rules.
    pub fn resolve_runs(&self, text: &str) -> Vec<BidiRun> {
        if text.is_empty() {
            return Vec::new();
        }

        let base_level = Self::detect_base_level(text);
        let chars: Vec<(usize, char, BidiClass)> = text
            .char_indices()
            .map(|(idx, c)| (idx, c, BidiClass::classify(c)))
            .collect();

        let n = chars.len();
        let mut resolved_classes: Vec<BidiClass> = chars.iter().map(|(_, _, cl)| *cl).collect();
        let mut levels: Vec<u8> = vec![base_level; n];

        // 1. Weak type resolution (Simplified W1-W7)
        let mut last_strong = if base_level == 1 { BidiClass::R } else { BidiClass::L };
        for i in 0..n {
            let c = resolved_classes[i];
            if c.is_strong() {
                last_strong = c;
            } else if c == BidiClass::EN {
                if last_strong == BidiClass::AL {
                    resolved_classes[i] = BidiClass::AN;
                } else if last_strong == BidiClass::L {
                    resolved_classes[i] = BidiClass::L;
                }
            }
        }

        // Change AL to R
        for cl in &mut resolved_classes {
            if *cl == BidiClass::AL {
                *cl = BidiClass::R;
            }
        }

        // 2. Resolve neutral types (N1-N2)
        for i in 0..n {
            let cl = resolved_classes[i];
            if cl == BidiClass::ON || cl == BidiClass::WS || cl == BidiClass::ES || cl == BidiClass::CS {
                // If adjacent to strong, adopt strong; else adopt base
                resolved_classes[i] = last_strong;
            }
        }

        // 3. Assign embedding levels (I1-I2)
        for i in 0..n {
            let cl = resolved_classes[i];
            if base_level % 2 == 0 {
                // LTR base
                if cl == BidiClass::R {
                    levels[i] = base_level + 1;
                } else if cl == BidiClass::AN || cl == BidiClass::EN {
                    levels[i] = base_level + 2;
                } else {
                    levels[i] = base_level;
                }
            } else {
                // RTL base
                if cl == BidiClass::L || cl == BidiClass::EN || cl == BidiClass::AN {
                    levels[i] = base_level + 1;
                } else {
                    levels[i] = base_level;
                }
            }
        }

        // 4. Group adjacent characters with identical level into runs
        let mut runs = Vec::new();
        let mut run_start = chars[0].0;
        let mut current_level = levels[0];

        for i in 1..n {
            if levels[i] != current_level {
                let run_end = chars[i].0;
                runs.push(BidiRun {
                    start: run_start,
                    end: run_end,
                    level: current_level,
                    is_rtl: current_level % 2 != 0,
                });
                run_start = chars[i].0;
                current_level = levels[i];
            }
        }

        // Final run
        let last_idx = chars[n - 1].0;
        let last_char_len = chars[n - 1].1.len_utf8();
        runs.push(BidiRun {
            start: run_start,
            end: last_idx + last_char_len,
            level: current_level,
            is_rtl: current_level % 2 != 0,
        });

        // 5. Visual reordering: Rule L2
        // Reverse contiguous sequences of runs at highest level down to lowest odd level
        Self::reorder_visually(&mut runs, base_level);

        runs
    }

    /// Reorders runs in-place into visual order from left to right.
    fn reorder_visually(runs: &mut [BidiRun], base_level: u8) {
        if runs.len() <= 1 {
            return;
        }

        let max_level = runs.iter().map(|r| r.level).max().unwrap_or(0);
        let min_odd_level = base_level | 1;

        for level in (min_odd_level..=max_level).rev() {
            let mut i = 0;
            while i < runs.len() {
                if runs[i].level >= level {
                    let start = i;
                    while i < runs.len() && runs[i].level >= level {
                        i += 1;
                    }
                    let end = i;
                    runs[start..end].reverse();
                } else {
                    i += 1;
                }
            }
        }
    }
}
