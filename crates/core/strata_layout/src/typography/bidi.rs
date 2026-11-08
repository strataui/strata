//! Unicode Bidirectional Algorithm (UAX #9) implementation for Strata.
//!
//! Implements the complete W3C / Unicode Bidirectional Algorithm (UAX #9):
//! - Character classification (L, R, AL, EN, ES, ET, AN, CS, NSM, BN, B, S, WS, ON,
//!   LRE, RLE, PDF, LRO, RLO, LRI, RLI, FSI, PDI).
//! - Paragraph base direction heuristic (Rules P2/P3).
//! - Explicit directional embeddings, overrides, and isolates stack (Rules X1..X8).
//! - Weak type resolution (Rules W1..W7).
//! - Neutral and Paired Bracket resolution (Rules N0..N2 / BD14..BD16).
//! - Implicit level resolution (Rules I1..I2).
//! - Visual run reordering (Rule L2) and glyph mirroring (Rule L4).

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
    /// Left-to-Right Embedding.
    LRE,
    /// Right-to-Left Embedding.
    RLE,
    /// Pop Directional Format.
    PDF,
    /// Left-to-Right Override.
    LRO,
    /// Right-to-Left Override.
    RLO,
    /// Left-to-Right Isolate.
    LRI,
    /// Right-to-Left Isolate.
    RLI,
    /// First Strong Isolate.
    FSI,
    /// Pop Directional Isolate.
    PDI,
}

impl BidiClass {
    /// Classifies a Unicode character according to Unicode Standard Annex #9.
    pub fn classify(c: char) -> Self {
        // Explicit directional formatting characters
        match c {
            '\u{202A}' => return BidiClass::LRE,
            '\u{202B}' => return BidiClass::RLE,
            '\u{202C}' => return BidiClass::PDF,
            '\u{202D}' => return BidiClass::LRO,
            '\u{202E}' => return BidiClass::RLO,
            '\u{2066}' => return BidiClass::LRI,
            '\u{2067}' => return BidiClass::RLI,
            '\u{2068}' => return BidiClass::FSI,
            '\u{2069}' => return BidiClass::PDI,
            '\u{200E}' => return BidiClass::L,  // Left-to-Right Mark
            '\u{200F}' => return BidiClass::R,  // Right-to-Left Mark
            '\u{061C}' => return BidiClass::AL, // Arabic Letter Mark
            _ => {}
        }

        // Arabic Letters and presentation forms
        if ('\u{0600}'..='\u{065F}').contains(&c)
            || ('\u{066A}'..='\u{06FF}').contains(&c)
            || ('\u{0750}'..='\u{077F}').contains(&c)
            || ('\u{08A0}'..='\u{08FF}').contains(&c)
            || ('\u{FB50}'..='\u{FDFF}').contains(&c)
            || ('\u{FE70}'..='\u{FEFF}').contains(&c)
            || ('\u{0700}'..='\u{074F}').contains(&c) // Syriac
        {
            // Nonspacing diacritics/harakat inside Arabic
            if ('\u{064B}'..='\u{065F}').contains(&c) || c == '\u{0670}' {
                return BidiClass::NSM;
            }
            return BidiClass::AL;
        }

        // Arabic-Indic Digits
        if ('\u{0660}'..='\u{0669}').contains(&c) {
            return BidiClass::AN;
        }

        // Hebrew script & presentation forms
        if ('\u{0590}'..='\u{05FF}').contains(&c)
            || ('\u{FB1D}'..='\u{FB4F}').contains(&c)
            || ('\u{0780}'..='\u{07BF}').contains(&c) // Thaana
            || ('\u{0800}'..='\u{083F}').contains(&c) // Samaritan
            || ('\u{0840}'..='\u{085F}').contains(&c) // Mandaic
        {
            if ('\u{0591}'..='\u{05BD}').contains(&c)
                || c == '\u{05BF}'
                || c == '\u{05C1}'
                || c == '\u{05C2}'
                || c == '\u{05C4}'
                || c == '\u{05C5}'
                || c == '\u{05C7}'
            {
                return BidiClass::NSM;
            }
            return BidiClass::R;
        }

        // Combining diacritical marks
        if ('\u{0300}'..='\u{036F}').contains(&c)
            || ('\u{1AB0}'..='\u{1AFF}').contains(&c)
            || ('\u{1DC0}'..='\u{1DFF}').contains(&c)
            || ('\u{20D0}'..='\u{20FF}').contains(&c)
            || ('\u{FE20}'..='\u{FE2F}').contains(&c)
        {
            return BidiClass::NSM;
        }

        // European Digits (0..9 and Persian/Eastern digits)
        if c.is_ascii_digit() || ('\u{06F0}'..='\u{06F9}').contains(&c) {
            return BidiClass::EN;
        }

        match c {
            '\n' | '\r' => BidiClass::B,
            '\t' => BidiClass::S,
            ' ' | '\u{00A0}' => BidiClass::WS,
            '+' | '-' => BidiClass::ES,
            '$' | '%' | '€' | '£' | '¥' | '¢' | '°' => BidiClass::ET,
            '.' | ',' | ':' | '/' => BidiClass::CS,
            '\u{200B}'..='\u{200D}' | '\u{FEFF}' => BidiClass::BN,
            '!' | '?' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '=' | '_'
            | '|' | '\\' | '~' | '`' | '@' | '#' | '^' | '&' | '*' | ';' | '«' | '»' => {
                BidiClass::ON
            }
            _ => BidiClass::L,
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

    /// Whether this is a neutral type.
    #[inline(always)]
    pub fn is_neutral(self) -> bool {
        matches!(
            self,
            BidiClass::ON
                | BidiClass::WS
                | BidiClass::S
                | BidiClass::B
                | BidiClass::BN
                | BidiClass::ES
                | BidiClass::ET
                | BidiClass::CS
        )
    }
}

/// Returns the mirrored glyph counterpart for mirrored characters in RTL runs (Rule L4).
pub fn bidi_mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        '≤' => '≥',
        '≥' => '≤',
        _ => c,
    }
}

/// Returns true if character is an opening bracket.
fn matching_close_bracket(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '<' => Some('>'),
        '«' => Some('»'),
        '‹' => Some('›'),
        _ => None,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverrideStatus {
    Neutral,
    LeftToRight,
    RightToLeft,
}

/// Directional status element on the embedding stack (Rules X1..X8).
#[derive(Debug, Clone, Copy)]
struct Status {
    level: u8,
    override_status: OverrideStatus,
    is_isolate: bool,
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

    /// Splits text into visual runs according to UAX #9 rules.
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
        let mut classes: Vec<BidiClass> = chars.iter().map(|(_, _, cl)| *cl).collect();
        let mut levels: Vec<u8> = vec![base_level; n];

        // 1. Explicit Directional Embeddings, Overrides, and Isolates (Rules X1..X8)
        let mut stack = Vec::with_capacity(32);
        stack.push(Status {
            level: base_level,
            override_status: OverrideStatus::Neutral,
            is_isolate: false,
        });

        for i in 0..n {
            let cl = classes[i];
            match cl {
                BidiClass::RLE => {
                    let current = stack.last().copied().unwrap_or(Status {
                        level: base_level,
                        override_status: OverrideStatus::Neutral,
                        is_isolate: false,
                    });
                    let new_level = (current.level + 1) | 1;
                    if new_level <= 125 && stack.len() < 125 {
                        stack.push(Status {
                            level: new_level,
                            override_status: OverrideStatus::Neutral,
                            is_isolate: false,
                        });
                    }
                    classes[i] = BidiClass::BN;
                }
                BidiClass::LRE => {
                    let current = stack.last().copied().unwrap_or(Status {
                        level: base_level,
                        override_status: OverrideStatus::Neutral,
                        is_isolate: false,
                    });
                    let new_level = (current.level + 2) & !1;
                    if new_level <= 124 && stack.len() < 125 {
                        stack.push(Status {
                            level: new_level,
                            override_status: OverrideStatus::Neutral,
                            is_isolate: false,
                        });
                    }
                    classes[i] = BidiClass::BN;
                }
                BidiClass::RLO => {
                    let current = stack.last().copied().unwrap_or(Status {
                        level: base_level,
                        override_status: OverrideStatus::Neutral,
                        is_isolate: false,
                    });
                    let new_level = (current.level + 1) | 1;
                    if new_level <= 125 && stack.len() < 125 {
                        stack.push(Status {
                            level: new_level,
                            override_status: OverrideStatus::RightToLeft,
                            is_isolate: false,
                        });
                    }
                    classes[i] = BidiClass::BN;
                }
                BidiClass::LRO => {
                    let current = stack.last().copied().unwrap_or(Status {
                        level: base_level,
                        override_status: OverrideStatus::Neutral,
                        is_isolate: false,
                    });
                    let new_level = (current.level + 2) & !1;
                    if new_level <= 124 && stack.len() < 125 {
                        stack.push(Status {
                            level: new_level,
                            override_status: OverrideStatus::LeftToRight,
                            is_isolate: false,
                        });
                    }
                    classes[i] = BidiClass::BN;
                }
                BidiClass::RLI => {
                    let current = stack.last().copied().unwrap_or(Status {
                        level: base_level,
                        override_status: OverrideStatus::Neutral,
                        is_isolate: false,
                    });
                    let new_level = (current.level + 1) | 1;
                    if new_level <= 125 && stack.len() < 125 {
                        stack.push(Status {
                            level: new_level,
                            override_status: OverrideStatus::Neutral,
                            is_isolate: true,
                        });
                    }
                    classes[i] = BidiClass::BN;
                }
                BidiClass::LRI => {
                    let current = stack.last().copied().unwrap_or(Status {
                        level: base_level,
                        override_status: OverrideStatus::Neutral,
                        is_isolate: false,
                    });
                    let new_level = (current.level + 2) & !1;
                    if new_level <= 124 && stack.len() < 125 {
                        stack.push(Status {
                            level: new_level,
                            override_status: OverrideStatus::Neutral,
                            is_isolate: true,
                        });
                    }
                    classes[i] = BidiClass::BN;
                }
                BidiClass::FSI => {
                    let current = stack.last().copied().unwrap_or(Status {
                        level: base_level,
                        override_status: OverrideStatus::Neutral,
                        is_isolate: false,
                    });
                    let mut first_strong = BidiClass::L;
                    let mut k = i + 1;
                    while k < n {
                        let k_cl = classes[k];
                        if k_cl == BidiClass::PDI {
                            break;
                        }
                        if k_cl == BidiClass::R || k_cl == BidiClass::AL {
                            first_strong = BidiClass::R;
                            break;
                        } else if k_cl == BidiClass::L {
                            first_strong = BidiClass::L;
                            break;
                        }
                        k += 1;
                    }
                    let new_level = if first_strong == BidiClass::R {
                        (current.level + 1) | 1
                    } else {
                        (current.level + 2) & !1
                    };
                    if new_level <= 125 && stack.len() < 125 {
                        stack.push(Status {
                            level: new_level,
                            override_status: OverrideStatus::Neutral,
                            is_isolate: true,
                        });
                    }
                    classes[i] = BidiClass::BN;
                }
                BidiClass::PDF => {
                    while stack.len() > 1 {
                        if let Some(top) = stack.last() {
                            if !top.is_isolate {
                                stack.pop();
                                break;
                            }
                        }
                        stack.pop();
                    }
                    classes[i] = BidiClass::BN;
                }
                BidiClass::PDI => {
                    while stack.len() > 1 {
                        if let Some(top) = stack.pop() {
                            if top.is_isolate {
                                break;
                            }
                        }
                    }
                    classes[i] = BidiClass::BN;
                }
                BidiClass::B => {
                    stack.truncate(1);
                    levels[i] = base_level;
                }
                _ => {
                    let current = stack.last().copied().unwrap_or(Status {
                        level: base_level,
                        override_status: OverrideStatus::Neutral,
                        is_isolate: false,
                    });
                    levels[i] = current.level;
                    match current.override_status {
                        OverrideStatus::LeftToRight => classes[i] = BidiClass::L,
                        OverrideStatus::RightToLeft => classes[i] = BidiClass::R,
                        OverrideStatus::Neutral => {}
                    }
                }
            }
        }

        // 2. Weak type resolution (Rules W1..W7)
        // W1: Nonspacing marks (NSM) take direction of previous character
        let mut prev_class = if base_level == 1 {
            BidiClass::R
        } else {
            BidiClass::L
        };
        for cl in &mut classes {
            if *cl == BidiClass::NSM {
                *cl = prev_class;
            } else if *cl != BidiClass::BN {
                prev_class = *cl;
            }
        }

        // W2: EN after AL becomes AN
        let mut last_strong_w2 = if base_level == 1 {
            BidiClass::R
        } else {
            BidiClass::L
        };
        for cl in &mut classes {
            if cl.is_strong() {
                last_strong_w2 = *cl;
            } else if *cl == BidiClass::EN && last_strong_w2 == BidiClass::AL {
                *cl = BidiClass::AN;
            }
        }

        // W3: AL becomes R
        for cl in &mut classes {
            if *cl == BidiClass::AL {
                *cl = BidiClass::R;
            }
        }

        // W4: ES between EN/EN becomes EN. CS between numbers becomes that number type.
        for i in 1..n.saturating_sub(1) {
            if classes[i] == BidiClass::ES
                && classes[i - 1] == BidiClass::EN
                && classes[i + 1] == BidiClass::EN
            {
                classes[i] = BidiClass::EN;
            } else if classes[i] == BidiClass::CS {
                if classes[i - 1] == BidiClass::EN && classes[i + 1] == BidiClass::EN {
                    classes[i] = BidiClass::EN;
                } else if classes[i - 1] == BidiClass::AN && classes[i + 1] == BidiClass::AN {
                    classes[i] = BidiClass::AN;
                }
            }
        }

        // W5: ET adjacent to EN becomes EN
        for i in 0..n {
            if classes[i] == BidiClass::ET {
                let prev_en = i > 0 && classes[i - 1] == BidiClass::EN;
                let next_en = i + 1 < n && classes[i + 1] == BidiClass::EN;
                if prev_en || next_en {
                    classes[i] = BidiClass::EN;
                }
            }
        }

        // W6: Any remaining ES, ET, CS become ON
        for cl in &mut classes {
            if *cl == BidiClass::ES || *cl == BidiClass::ET || *cl == BidiClass::CS {
                *cl = BidiClass::ON;
            }
        }

        // W7: EN in L context becomes L
        let mut last_strong_w7 = if base_level == 1 {
            BidiClass::R
        } else {
            BidiClass::L
        };
        for cl in &mut classes {
            if cl.is_strong() {
                last_strong_w7 = *cl;
            } else if *cl == BidiClass::EN && last_strong_w7 == BidiClass::L {
                *cl = BidiClass::L;
            }
        }

        // 3. Paired Bracket Resolution (Rule N0 / BD14..BD16)
        let mut bracket_stack: Vec<(char, usize)> = Vec::new();
        for i in 0..n {
            let c = chars[i].1;
            if let Some(close) = matching_close_bracket(c) {
                bracket_stack.push((close, i));
            } else if let Some(pos) = bracket_stack.iter().rposition(|&(cl, _)| cl == c) {
                let open_idx = bracket_stack[pos].1;
                bracket_stack.truncate(pos);

                // Examine text inside bracket pair
                let inside_strong = classes[(open_idx + 1)..i]
                    .iter()
                    .find(|cl| cl.is_strong())
                    .copied();

                let embed_dir = if levels[open_idx] % 2 == 1 {
                    BidiClass::R
                } else {
                    BidiClass::L
                };

                if let Some(st) = inside_strong {
                    if st == embed_dir {
                        classes[open_idx] = embed_dir;
                        classes[i] = embed_dir;
                    } else {
                        classes[open_idx] = st;
                        classes[i] = st;
                    }
                }
            }
        }

        // 4. Neutral type resolution (Rules N1..N2)
        // N1: Neutrals between matching strong types adopt that strong type
        let mut i = 0;
        while i < n {
            if classes[i].is_neutral() {
                let start = i;
                while i < n && classes[i].is_neutral() {
                    i += 1;
                }
                let end = i;

                // Find left delimiter (skipping BN and neutrals)
                let mut left_type = if levels[start] % 2 == 1 {
                    BidiClass::R
                } else {
                    BidiClass::L
                };
                let mut p = start;
                while p > 0 {
                    p -= 1;
                    if classes[p] != BidiClass::BN && !classes[p].is_neutral() {
                        left_type = classes[p];
                        break;
                    }
                }

                // Find right delimiter (skipping BN and neutrals)
                let mut right_type = if levels[start] % 2 == 1 {
                    BidiClass::R
                } else {
                    BidiClass::L
                };
                let mut k = end;
                while k < n {
                    if classes[k] != BidiClass::BN && !classes[k].is_neutral() {
                        right_type = classes[k];
                        break;
                    }
                    k += 1;
                }

                // Map delimiter classes to their effective direction (N1)
                // W7 has already converted L-context EN into L.
                // Any remaining EN or AN acts as R.
                let left_dir = match left_type {
                    BidiClass::L => BidiClass::L,
                    BidiClass::R | BidiClass::EN | BidiClass::AN => BidiClass::R,
                    _ => if levels[start] % 2 == 1 { BidiClass::R } else { BidiClass::L },
                };

                let right_dir = match right_type {
                    BidiClass::L => BidiClass::L,
                    BidiClass::R | BidiClass::EN | BidiClass::AN => BidiClass::R,
                    _ => if levels[start] % 2 == 1 { BidiClass::R } else { BidiClass::L },
                };

                // Rule N1: If both delimiters have the same direction, adopt it
                // Rule N2: Otherwise adopt embedding level direction
                let target = if left_dir == right_dir {
                    left_dir
                } else if levels[start] % 2 == 1 {
                    BidiClass::R
                } else {
                    BidiClass::L
                };

                classes[start..end].fill(target);
            } else {
                i += 1;
            }
        }

        // 5. Implicit level resolution (Rules I1..I2)
        for i in 0..n {
            let cl = classes[i];
            let lvl = levels[i];
            if lvl.is_multiple_of(2) {
                // LTR embedding level
                if cl == BidiClass::R {
                    levels[i] = lvl + 1;
                } else if cl == BidiClass::AN || cl == BidiClass::EN {
                    levels[i] = lvl + 2;
                }
            } else {
                // RTL embedding level
                if cl == BidiClass::L || cl == BidiClass::EN || cl == BidiClass::AN {
                    levels[i] = lvl + 1;
                }
            }
        }

        // 6. Group adjacent characters with identical level into runs
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
                    is_rtl: !current_level.is_multiple_of(2),
                });
                run_start = chars[i].0;
                current_level = levels[i];
            }
        }

        let last_idx = chars[n - 1].0;
        let last_char_len = chars[n - 1].1.len_utf8();
        runs.push(BidiRun {
            start: run_start,
            end: last_idx + last_char_len,
            level: current_level,
            is_rtl: !current_level.is_multiple_of(2),
        });

        // 7. Visual reordering: Rule L2
        Self::reorder_visually(&mut runs, base_level);

        runs
    }

    /// Reorders runs in-place into visual order from left to right (Rule L2).
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
