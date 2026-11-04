//! CSS Media Queries Level 4 Parser and Evaluator.
//!
//! Supports:
//! - Media types: `screen`, `all`, `print`, `speech`
//! - Media features: `min-width`, `max-width`, `min-height`, `max-height`, `prefers-color-scheme`, `orientation`
//! - Complex queries: `and`, `not`, comma-separated OR lists

/// Color scheme preference (dark vs light).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

/// Device or viewport orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// Primary CSS media type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    All,
    Screen,
    Print,
    Speech,
}

/// Individual media feature condition within a media query.
#[derive(Debug, Clone, PartialEq)]
pub enum MediaFeature {
    MinWidth(f32),
    MaxWidth(f32),
    MinHeight(f32),
    MaxHeight(f32),
    PrefersColorScheme(ColorScheme),
    Orientation(Orientation),
}

impl MediaFeature {
    /// Evaluates this media feature against the active device context.
    pub fn matches(&self, ctx: &MediaContext) -> bool {
        match self {
            MediaFeature::MinWidth(min_w) => ctx.viewport_width >= *min_w,
            MediaFeature::MaxWidth(max_w) => ctx.viewport_width <= *max_w,
            MediaFeature::MinHeight(min_h) => ctx.viewport_height >= *min_h,
            MediaFeature::MaxHeight(max_h) => ctx.viewport_height <= *max_h,
            MediaFeature::PrefersColorScheme(scheme) => match scheme {
                ColorScheme::Dark => ctx.is_dark,
                ColorScheme::Light => !ctx.is_dark,
            },
            MediaFeature::Orientation(orient) => match orient {
                Orientation::Portrait => ctx.viewport_height >= ctx.viewport_width,
                Orientation::Landscape => ctx.viewport_width > ctx.viewport_height,
            },
        }
    }
}

/// Single CSS media query clause (e.g. `screen and (min-width: 768px)` or `not print`).
#[derive(Debug, Clone, PartialEq)]
pub struct MediaQuery {
    pub negated: bool,
    pub media_type: MediaType,
    pub features: Vec<MediaFeature>,
}

impl MediaQuery {
    /// Evaluates whether this single media query condition is satisfied.
    pub fn matches(&self, ctx: &MediaContext) -> bool {
        let type_matched = match self.media_type {
            MediaType::All | MediaType::Screen => true,
            MediaType::Print | MediaType::Speech => false,
        };

        let features_matched = self.features.iter().all(|f| f.matches(ctx));
        let outcome = type_matched && features_matched;

        if self.negated {
            !outcome
        } else {
            outcome
        }
    }

    /// Parses a single media query clause.
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(Self {
                negated: false,
                media_type: MediaType::All,
                features: Vec::new(),
            });
        }

        let mut negated = false;
        let mut rest = trimmed;

        if let Some(after_not) = rest.strip_prefix("not").or_else(|| rest.strip_prefix("NOT")) {
            if after_not.starts_with(char::is_whitespace) {
                negated = true;
                rest = after_not.trim();
            }
        }

        // Optional `only` prefix per CSS syntax
        if let Some(after_only) = rest.strip_prefix("only").or_else(|| rest.strip_prefix("ONLY")) {
            if after_only.starts_with(char::is_whitespace) {
                rest = after_only.trim();
            }
        }

        let mut media_type = MediaType::All;
        let mut features = Vec::new();

        // Split by 'and' or 'AND'
        let parts: Vec<&str> = rest.split(" and ").flat_map(|p| p.split(" AND ")).map(|s| s.trim()).collect();

        for (idx, part) in parts.iter().enumerate() {
            if part.starts_with('(') && part.ends_with(')') {
                let inner = &part[1..part.len() - 1].trim();
                let feat = parse_feature(inner)?;
                features.push(feat);
            } else if idx == 0 {
                media_type = match part.to_ascii_lowercase().as_str() {
                    "all" => MediaType::All,
                    "screen" => MediaType::Screen,
                    "print" => MediaType::Print,
                    "speech" => MediaType::Speech,
                    _ => return Err(format!("Unrecognized media type '{}'", part)),
                };
            } else {
                return Err(format!("Unexpected media query token '{}'", part));
            }
        }

        Ok(Self {
            negated,
            media_type,
            features,
        })
    }
}

/// Comma-separated list of media queries, matching if any condition holds (CSS OR logic).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaQueryList {
    pub queries: Vec<MediaQuery>,
}

impl MediaQueryList {
    /// Evaluates whether any media query in this list matches the active context.
    pub fn matches(&self, ctx: &MediaContext) -> bool {
        if self.queries.is_empty() {
            return true;
        }
        self.queries.iter().any(|q| q.matches(ctx))
    }

    /// Parses a comma-separated list of media queries (e.g. `screen and (max-width: 600px), print`).
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(Self { queries: Vec::new() });
        }

        let mut queries = Vec::new();
        for item in trimmed.split(',') {
            let item_trimmed = item.trim();
            if !item_trimmed.is_empty() {
                queries.push(MediaQuery::parse(item_trimmed)?);
            }
        }

        Ok(Self { queries })
    }
}

/// Device context used to evaluate media query conditions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaContext {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub is_dark: bool,
}

impl Default for MediaContext {
    fn default() -> Self {
        Self {
            viewport_width: 1280.0,
            viewport_height: 800.0,
            is_dark: true,
        }
    }
}

fn parse_feature(input: &str) -> Result<MediaFeature, String> {
    let (prop, val) = match input.split_once(':') {
        Some((p, v)) => (p.trim().to_ascii_lowercase(), v.trim()),
        None => return Err(format!("Invalid media feature without colon: '{}'", input)),
    };

    match prop.as_str() {
        "min-width" => {
            let px = parse_px_dimension(val)?;
            Ok(MediaFeature::MinWidth(px))
        }
        "max-width" => {
            let px = parse_px_dimension(val)?;
            Ok(MediaFeature::MaxWidth(px))
        }
        "min-height" => {
            let px = parse_px_dimension(val)?;
            Ok(MediaFeature::MinHeight(px))
        }
        "max-height" => {
            let px = parse_px_dimension(val)?;
            Ok(MediaFeature::MaxHeight(px))
        }
        "prefers-color-scheme" => match val.to_ascii_lowercase().as_str() {
            "dark" => Ok(MediaFeature::PrefersColorScheme(ColorScheme::Dark)),
            "light" => Ok(MediaFeature::PrefersColorScheme(ColorScheme::Light)),
            _ => Err(format!("Unknown prefers-color-scheme value '{}'", val)),
        },
        "orientation" => match val.to_ascii_lowercase().as_str() {
            "portrait" => Ok(MediaFeature::Orientation(Orientation::Portrait)),
            "landscape" => Ok(MediaFeature::Orientation(Orientation::Landscape)),
            _ => Err(format!("Unknown orientation value '{}'", val)),
        },
        _ => Err(format!("Unsupported media feature property '{}'", prop)),
    }
}

fn parse_px_dimension(val: &str) -> Result<f32, String> {
    let cleaned = val.trim().to_ascii_lowercase();
    if let Some(px_str) = cleaned.strip_suffix("px") {
        px_str.trim().parse::<f32>().map_err(|e| e.to_string())
    } else if let Some(em_str) = cleaned.strip_suffix("em") {
        let em = em_str.trim().parse::<f32>().map_err(|e| e.to_string())?;
        Ok(em * 16.0)
    } else if let Some(rem_str) = cleaned.strip_suffix("rem") {
        let rem = rem_str.trim().parse::<f32>().map_err(|e| e.to_string())?;
        Ok(rem * 16.0)
    } else {
        cleaned.parse::<f32>().map_err(|e| e.to_string())
    }
}
