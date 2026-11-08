//! Zero-dependency CSS stylesheet lexer, parser, and property value decoders.

use crate::selector::Selector;
use crate::types::{
    AnimationDirection, AnimationFillMode, AnimationIterationCount, AnimationPlayState,
    AnimationRange, AnimationRangeName, AnimationSpec, AnimationTimeline, BackfaceVisibility,
    BorderStyle, BoxShadow, Color, ColorStop, Cursor, Dimension, EdgeInsets, Gradient, GridTrack,
    GridTrackBreadth, PointerEvents, ScrollAxis, ScrollTimelineSource, TextDecoration,
    TextTransform, TransformOp, TransformOrigin, TransformStyle, TransitionBehavior,
    TransitionEasing, TransitionProperty, TransitionSpec, UserSelect, VerticalAlign, Visibility,
    WhiteSpace,
};

/// Single CSS property declaration (e.g. `background-color: #000 !important;`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub property: String,
    pub value: String,
    pub important: bool,
}

/// CSS style rule associating a list of selectors with a declaration block and optional layer order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
    pub layer_order: Option<u32>,
}

/// A single parsed stop inside a CSS `@keyframes` block.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeStopRule {
    /// Normalized offset along the animation timeline in [0.0, 1.0].
    pub offset: f32,
    /// Property declarations defined at this stop.
    pub declarations: Vec<Declaration>,
    /// Optional segment timing function specified via `animation-timing-function`.
    pub timing_function: Option<TransitionEasing>,
}

/// A complete parsed CSS `@keyframes` animation rule.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeRule {
    pub name: String,
    pub stops: Vec<KeyframeStopRule>,
}

/// A style rule inside a `@starting-style` at-rule block (W3C CSS Transitions Level 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartingStyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

/// Parsed CSS stylesheet containing ordered style rules, keyframe rules, starting-style rules, and registered properties.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct StyleSheet {
    pub rules: Vec<StyleRule>,
    pub keyframes: Vec<KeyframeRule>,
    pub starting_styles: Vec<StartingStyleRule>,
    pub registered_properties: Vec<crate::properties::RegisteredProperty>,
}

impl StyleSheet {
    /// Parses a complete CSS stylesheet string using default media context.
    pub fn parse(css: &str) -> Result<Self, String> {
        Self::parse_with_context(css, &crate::media::MediaContext::default())
    }

    /// Parses a complete CSS stylesheet string evaluating media queries against a specific device context.
    pub fn parse_with_context(
        css: &str,
        context: &crate::media::MediaContext,
    ) -> Result<Self, String> {
        let stripped = strip_css_comments(css);
        let mut rules = Vec::new();
        let mut keyframes = Vec::new();
        let mut starting_styles = Vec::new();
        let mut registered_properties = Vec::new();
        let mut layer_names: Vec<String> = Vec::new();
        let mut chars = stripped.chars().peekable();

        loop {
            skip_whitespace(&mut chars);
            if chars.peek().is_none() {
                break;
            }

            // Read selectors or at-rule header up to '{' or ';'
            let mut selector_buf = String::new();
            let mut found_brace = false;
            let mut in_quotes: Option<char> = None;

            for c in chars.by_ref() {
                if let Some(q) = in_quotes {
                    if c == q {
                        in_quotes = None;
                    }
                    selector_buf.push(c);
                } else if c == '"' || c == '\'' {
                    in_quotes = Some(c);
                    selector_buf.push(c);
                } else if c == '{' {
                    found_brace = true;
                    break;
                } else if c == ';' {
                    // Semicolon-terminated statement outside braces (e.g. @charset, @import, stray ;)
                    break;
                } else {
                    selector_buf.push(c);
                }
            }

            if !found_brace {
                // Semicolon-terminated at-rule (like @layer base, components; or @charset "UTF-8";)
                let trimmed_stmt = selector_buf.trim();
                if let Some(layer_names_str) = trimmed_stmt.strip_prefix("@layer") {
                    for name in layer_names_str.split(',') {
                        let name = name.trim();
                        if !name.is_empty() && !layer_names.contains(&name.to_string()) {
                            layer_names.push(name.to_string());
                        }
                    }
                }
                continue;
            }

            // Read body block up to matching '}' respecting quotes
            let mut decl_buf = String::new();
            let mut depth = 1;
            let mut in_decl_quotes: Option<char> = None;

            for c in chars.by_ref() {
                if let Some(q) = in_decl_quotes {
                    if c == q {
                        in_decl_quotes = None;
                    }
                    decl_buf.push(c);
                } else if c == '"' || c == '\'' {
                    in_decl_quotes = Some(c);
                    decl_buf.push(c);
                } else if c == '{' {
                    depth += 1;
                    decl_buf.push(c);
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    decl_buf.push(c);
                } else {
                    decl_buf.push(c);
                }
            }

            let trimmed_sel = selector_buf.trim();

            // Evaluate @media queries
            if let Some(media_query_str) = trimmed_sel.strip_prefix("@media") {
                if let Ok(mql) = crate::media::MediaQueryList::parse(media_query_str) {
                    if mql.matches(context) {
                        // Recursively parse inner rules in the matching media query block
                        if let Ok(inner_sheet) = Self::parse_with_context(&decl_buf, context) {
                            rules.extend(inner_sheet.rules);
                            keyframes.extend(inner_sheet.keyframes);
                            starting_styles.extend(inner_sheet.starting_styles);
                            registered_properties.extend(inner_sheet.registered_properties);
                        }
                    }
                }
                continue;
            }

            // Evaluate @layer
            if trimmed_sel.starts_with("@layer") {
                let layer_name = trimmed_sel.trim_start_matches("@layer").trim();
                let layer_id = if layer_name.is_empty() {
                    let id = layer_names.len() as u32;
                    layer_names.push(format!("__anon_{}", id));
                    id
                } else if let Some(idx) = layer_names.iter().position(|n| n == layer_name) {
                    idx as u32
                } else {
                    let id = layer_names.len() as u32;
                    layer_names.push(layer_name.to_string());
                    id
                };

                if let Ok(inner_sheet) = Self::parse_with_context(&decl_buf, context) {
                    for mut r in inner_sheet.rules {
                        if r.layer_order.is_none() {
                            r.layer_order = Some(layer_id);
                        }
                        rules.push(r);
                    }
                    keyframes.extend(inner_sheet.keyframes);
                    starting_styles.extend(inner_sheet.starting_styles);
                    registered_properties.extend(inner_sheet.registered_properties);
                }
                continue;
            }

            // Evaluate @container
            if trimmed_sel.starts_with("@container") {
                let query_str = trimmed_sel.trim_start_matches("@container").trim();
                if evaluate_container_query(query_str, context) {
                    if let Ok(inner_sheet) = Self::parse_with_context(&decl_buf, context) {
                        rules.extend(inner_sheet.rules);
                        keyframes.extend(inner_sheet.keyframes);
                        starting_styles.extend(inner_sheet.starting_styles);
                        registered_properties.extend(inner_sheet.registered_properties);
                    }
                }
                continue;
            }

            // Evaluate @supports
            if trimmed_sel.starts_with("@supports") {
                if let Ok(inner_sheet) = Self::parse_with_context(&decl_buf, context) {
                    rules.extend(inner_sheet.rules);
                    keyframes.extend(inner_sheet.keyframes);
                    starting_styles.extend(inner_sheet.starting_styles);
                    registered_properties.extend(inner_sheet.registered_properties);
                }
                continue;
            }

            // Evaluate @starting-style
            if trimmed_sel == "@starting-style" || trimmed_sel.starts_with("@starting-style") {
                if let Ok(inner_sheet) = Self::parse_with_context(&decl_buf, context) {
                    for r in inner_sheet.rules {
                        starting_styles.push(StartingStyleRule {
                            selectors: r.selectors,
                            declarations: r.declarations,
                        });
                    }
                    starting_styles.extend(inner_sheet.starting_styles);
                    keyframes.extend(inner_sheet.keyframes);
                    registered_properties.extend(inner_sheet.registered_properties);
                }
                continue;
            }

            // Evaluate @property --name
            if let Some(prop_name) = trimmed_sel.strip_prefix("@property") {
                let prop_name = prop_name.trim();
                if !prop_name.is_empty() {
                    let reg_prop =
                        crate::properties::parse_property_rule_body(prop_name, &decl_buf);
                    registered_properties.push(reg_prop);
                }
                continue;
            }

            // Evaluate @keyframes / @-webkit-keyframes
            if let Some(name) = trimmed_sel
                .strip_prefix("@keyframes")
                .or_else(|| trimmed_sel.strip_prefix("@-webkit-keyframes"))
            {
                let name = name.trim();
                if !name.is_empty() {
                    let kf_rule = parse_keyframe_rule_body(name, &decl_buf);
                    keyframes.push(kf_rule);
                }
                continue;
            }

            // Skip other unhandled at-rules (e.g. @font-face, @charset) in general cascade rules
            if trimmed_sel.starts_with('@') {
                continue;
            }

            // Parse comma-separated selectors respecting parentheses, brackets, and quotes
            let mut selectors = Vec::new();
            for sel_str in split_selector_list(&selector_buf) {
                let trimmed = sel_str.trim();
                if !trimmed.is_empty() {
                    if let Ok(sel) = Selector::parse(trimmed) {
                        selectors.push(sel);
                    }
                }
            }

            if selectors.is_empty() {
                continue;
            }

            // Check for nested @starting-style inside declaration body
            let mut final_decl_buf = decl_buf;
            if let Some(start_pos) = final_decl_buf.find("@starting-style") {
                if let Some(brace_pos) = final_decl_buf[start_pos..].find('{') {
                    let abs_brace = start_pos + brace_pos;
                    let mut depth = 1;
                    let mut end_pos = abs_brace + 1;
                    for (i, ch) in final_decl_buf[abs_brace + 1..].char_indices() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                end_pos = abs_brace + 1 + i;
                                break;
                            }
                        }
                    }
                    let inner_starting = &final_decl_buf[abs_brace + 1..end_pos];
                    let nested_decls = parse_declarations(inner_starting);
                    if !nested_decls.is_empty() && !selectors.is_empty() {
                        starting_styles.push(StartingStyleRule {
                            selectors: selectors.clone(),
                            declarations: nested_decls,
                        });
                    }
                    let mut stripped_body = String::new();
                    stripped_body.push_str(&final_decl_buf[..start_pos]);
                    if end_pos + 1 < final_decl_buf.len() {
                        stripped_body.push_str(&final_decl_buf[end_pos + 1..]);
                    }
                    final_decl_buf = stripped_body;
                }
            }

            // Parse declaration block
            let declarations = parse_declarations(&final_decl_buf);
            rules.push(StyleRule {
                selectors,
                declarations,
                layer_order: None,
            });
        }

        Ok(Self {
            rules,
            keyframes,
            starting_styles,
            registered_properties,
        })
    }
}

/// Parses a declaration block body into a list of `Declaration`s.
pub fn parse_declarations(block: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();

    for stmt in block.split(';') {
        let trimmed = stmt.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((prop, val)) = trimmed.split_once(':') {
            let property = prop.trim().to_ascii_lowercase();
            let mut val_str = val.trim();
            let mut important = false;

            if let Some(stripped) = val_str.strip_suffix("!important") {
                important = true;
                val_str = stripped.trim();
            } else if val_str.to_ascii_lowercase().ends_with("!important") {
                important = true;
                let len = val_str.len();
                val_str = val_str[..len - 10].trim();
            }

            declarations.push(Declaration {
                property,
                value: val_str.to_string(),
                important,
            });
        }
    }

    declarations
}

/// Parses a keyframe offset identifier (e.g. `from`, `to`, `0%`, `50%`, `100%`) into a normalized float in `[0.0, 1.0]`.
pub fn parse_keyframe_stop_offset(s: &str) -> Option<f32> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("from") {
        Some(0.0)
    } else if s.eq_ignore_ascii_case("to") {
        Some(1.0)
    } else if let Some(pct) = s.strip_suffix('%') {
        pct.trim()
            .parse::<f32>()
            .map(|p| (p / 100.0).clamp(0.0, 1.0))
            .ok()
    } else if let Ok(p) = s.parse::<f32>() {
        Some((p / 100.0).clamp(0.0, 1.0))
    } else {
        None
    }
}

/// Parses the declaration body of a `@keyframes` block into an ordered list of `KeyframeStopRule`s.
pub fn parse_keyframe_rule_body(name: &str, body: &str) -> KeyframeRule {
    let mut stops = Vec::new();
    let mut chars = body.chars().peekable();

    loop {
        skip_whitespace(&mut chars);
        if chars.peek().is_none() {
            break;
        }

        // Read stop offsets header (e.g. "0%, 100%" or "from") up to '{'
        let mut offset_buf = String::new();
        let mut found_brace = false;
        for c in chars.by_ref() {
            if c == '{' {
                found_brace = true;
                break;
            }
            offset_buf.push(c);
        }
        if !found_brace {
            break;
        }

        // Read stop body up to matching '}'
        let mut stop_body = String::new();
        let mut depth = 1;
        for c in chars.by_ref() {
            if c == '{' {
                depth += 1;
                stop_body.push(c);
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                stop_body.push(c);
            } else {
                stop_body.push(c);
            }
        }

        let decls = parse_declarations(&stop_body);
        let timing_function = decls
            .iter()
            .find(|d| d.property == "animation-timing-function")
            .and_then(|d| TransitionEasing::parse(&d.value));

        // Parse comma-separated offsets (e.g. "0%, 100%")
        for off_str in offset_buf.split(',') {
            if let Some(offset) = parse_keyframe_stop_offset(off_str) {
                stops.push(KeyframeStopRule {
                    offset,
                    declarations: decls.clone(),
                    timing_function,
                });
            }
        }
    }

    stops.sort_by(|a, b| {
        a.offset
            .partial_cmp(&b.offset)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    KeyframeRule {
        name: name.to_string(),
        stops,
    }
}

/// Strips all `/* ... */` comment blocks from a CSS string.
fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next(); // consume '*'
            while let Some(inner) = chars.next() {
                if inner == '*' && chars.peek() == Some(&'/') {
                    chars.next(); // consume '/'
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }

    out
}

fn skip_whitespace<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
}

/// Decodes CSS dimension values into `Dimension`.
pub fn parse_dimension(val: &str) -> Option<Dimension> {
    let s = val.trim().to_ascii_lowercase();
    if s == "auto" {
        return Some(Dimension::Auto);
    }
    if let Some(px) = s.strip_suffix("px") {
        if let Ok(n) = px.trim().parse::<f32>() {
            return Some(Dimension::Px(n));
        }
    }
    if let Some(pct) = s.strip_suffix('%') {
        if let Ok(n) = pct.trim().parse::<f32>() {
            return Some(Dimension::Percent(n));
        }
    }
    if let Some(rem) = s.strip_suffix("rem") {
        if let Ok(n) = rem.trim().parse::<f32>() {
            return Some(Dimension::Rem(n));
        }
    }
    if let Some(em) = s.strip_suffix("em") {
        if let Ok(n) = em.trim().parse::<f32>() {
            return Some(Dimension::Em(n));
        }
    }
    if let Some(vw) = s.strip_suffix("vw") {
        if let Ok(n) = vw.trim().parse::<f32>() {
            return Some(Dimension::Vw(n));
        }
    }
    if let Some(vh) = s.strip_suffix("vh") {
        if let Ok(n) = vh.trim().parse::<f32>() {
            return Some(Dimension::Vh(n));
        }
    }
    if let Ok(n) = s.parse::<f32>() {
        // Raw 0 or unitless zero is 0px
        if n == 0.0 {
            return Some(Dimension::Px(0.0));
        }
    }
    None
}

/// Maximum number of tracks expanded from a single template list.
const MAX_GRID_TRACKS: usize = 4096;

/// Parses a CSS `grid-template-columns` or `grid-template-rows` track list.
///
/// Accepted track forms are `auto`, `min-content`, `max-content`,
/// `fit-content(<length>)`, unitless `0`, `<n>px`, `<n>%`, and `<n>fr`.
/// Square-bracket line names such as `[content-start]` are stripped from the
/// token stream.
///
/// `repeat(<count>, <track list>)` expands the inner list `<count>` times.
/// `repeat(auto-fill, ...)` and `repeat(auto-fit, ...)` expand the inner list
/// exactly once because intrinsic repetition needs a layout pass; the tracks are
/// not duplicated to fill the container.
///
/// `minmax(<min>, <max>)` is reduced to a single track: a `1fr` maximum or a
/// `0`/`auto` minimum becomes `1fr`, a fixed minimum paired with an `auto`
/// maximum keeps the minimum, and every other combination keeps the maximum
/// track. Full minimum and maximum track sizing is deferred to the grid solver.
pub fn parse_grid_tracks(value: &str) -> Vec<GridTrack> {
    let mut tracks = Vec::new();
    for token in tokenize_grid_track_tokens(value) {
        expand_grid_track_token(&token, &mut tracks);
        if tracks.len() >= MAX_GRID_TRACKS {
            tracks.truncate(MAX_GRID_TRACKS);
            break;
        }
    }
    tracks
}

fn tokenize_grid_track_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0usize;
    let mut in_line_names = false;

    for ch in value.chars() {
        if in_line_names {
            if ch == ']' {
                in_line_names = false;
            }
            continue;
        }

        match ch {
            '[' if paren_depth == 0 => {
                in_line_names = true;
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
            }
            c if c.is_whitespace() && paren_depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn expand_grid_track_token(token: &str, out: &mut Vec<GridTrack>) {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return;
    }

    if let Some((name, inner)) = split_grid_function(trimmed) {
        if name.eq_ignore_ascii_case("repeat") {
            if let Some((count_part, list_part)) = split_top_level_comma(inner) {
                let inner_tokens = tokenize_grid_track_tokens(list_part);
                match count_part.trim().parse::<u32>() {
                    Ok(count) if count > 0 => {
                        for _ in 0..count {
                            for inner_token in &inner_tokens {
                                expand_grid_track_token(inner_token, out);
                                if out.len() >= MAX_GRID_TRACKS {
                                    return;
                                }
                            }
                        }
                    }
                    _ => {
                        // `auto-fill` / `auto-fit` and invalid counts expand once.
                        for inner_token in &inner_tokens {
                            expand_grid_track_token(inner_token, out);
                            if out.len() >= MAX_GRID_TRACKS {
                                return;
                            }
                        }
                    }
                }
            }
            return;
        }

        if name.eq_ignore_ascii_case("minmax") {
            if let Some((min_part, max_part)) = split_top_level_comma(inner) {
                out.push(resolve_minmax_track(min_part.trim(), max_part.trim()));
            }
            return;
        }
    }

    if let Some(track) = parse_single_grid_track(trimmed) {
        out.push(track);
    }
}

fn split_grid_function(token: &str) -> Option<(&str, &str)> {
    let open = token.find('(')?;
    if !token.ends_with(')') {
        return None;
    }
    Some((&token[..open], &token[open + 1..token.len() - 1]))
}

fn split_top_level_comma(value: &str) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some((&value[..index], &value[index + 1..])),
            _ => {}
        }
    }
    None
}

fn parse_single_grid_track(token: &str) -> Option<GridTrack> {
    let lower = token.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }

    match lower.as_str() {
        "auto" => return Some(GridTrack::Auto),
        "min-content" => return Some(GridTrack::MinContent),
        "max-content" => return Some(GridTrack::MaxContent),
        "subgrid" => return Some(GridTrack::Subgrid),
        _ => {}
    }

    if lower.starts_with("fit-content(") && lower.ends_with(')') {
        return Some(GridTrack::MaxContent);
    }

    if lower == "0" || lower == "0.0" {
        return Some(GridTrack::Px(0.0));
    }

    if let Some(px) = lower.strip_suffix("px") {
        return px.trim().parse::<f32>().ok().map(GridTrack::Px);
    }
    if let Some(pct) = lower.strip_suffix('%') {
        return pct.trim().parse::<f32>().ok().map(GridTrack::Percent);
    }
    if let Some(fr) = lower.strip_suffix("fr") {
        return fr.trim().parse::<f32>().ok().map(GridTrack::Fr);
    }

    None
}

fn parse_grid_breadth(token: &str) -> Option<GridTrackBreadth> {
    let lower = token.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }

    match lower.as_str() {
        "auto" => return Some(GridTrackBreadth::Auto),
        "min-content" => return Some(GridTrackBreadth::MinContent),
        "max-content" => return Some(GridTrackBreadth::MaxContent),
        "0" | "0.0" => return Some(GridTrackBreadth::Px(0.0)),
        _ => {}
    }

    if let Some(px) = lower.strip_suffix("px") {
        return px.trim().parse::<f32>().ok().map(GridTrackBreadth::Px);
    }
    if let Some(pct) = lower.strip_suffix('%') {
        return pct.trim().parse::<f32>().ok().map(GridTrackBreadth::Percent);
    }
    if let Some(fr) = lower.strip_suffix("fr") {
        return fr.trim().parse::<f32>().ok().map(GridTrackBreadth::Fr);
    }

    None
}

fn resolve_minmax_track(min: &str, max: &str) -> GridTrack {
    let min_b = parse_grid_breadth(min).unwrap_or(GridTrackBreadth::Auto);
    let max_b = parse_grid_breadth(max).unwrap_or(GridTrackBreadth::Auto);
    GridTrack::MinMax(min_b, max_b)
}

/// Decodes CSS colors: `#hex`, `rgb(...)`, `rgba(...)`, `hsl(...)`, `hsla(...)`, or named colors.
pub fn parse_color(val: &str) -> Option<Color> {
    let s = val.trim();
    if s.starts_with('#') {
        return Color::parse_hex(s);
    }

    let lower = s.to_ascii_lowercase();

    if lower.starts_with("rgb(") && lower.ends_with(')') {
        let inner = &lower[4..lower.len() - 1];
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 3 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            return Some(Color::new(r, g, b, 255));
        }
    }

    if lower.starts_with("rgba(") && lower.ends_with(')') {
        let inner = &lower[5..lower.len() - 1];
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 4 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            let a = (parts[3].parse::<f32>().ok()?.clamp(0.0, 1.0) * 255.0).round() as u8;
            return Some(Color::new(r, g, b, a));
        }
    }

    // Common CSS color keywords
    match lower.as_str() {
        "transparent" => Some(Color::TRANSPARENT),
        "black" => Some(Color::BLACK),
        "white" => Some(Color::WHITE),
        "red" => Some(Color::RED),
        "green" => Some(Color::GREEN),
        "blue" => Some(Color::BLUE),
        _ => None,
    }
}

/// Decodes CSS 4-directional insets shorthand with font-size and root-font-size context.
pub fn parse_edge_insets_with_context(
    val: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<EdgeInsets<f32>> {
    let parts: Vec<f32> = val
        .split_whitespace()
        .filter_map(|p| match parse_dimension(p) {
            Some(Dimension::Px(px)) => Some(px),
            Some(Dimension::Em(em)) => Some(em * font_size),
            Some(Dimension::Rem(rem)) => Some(rem * root_font_size),
            Some(Dimension::Percent(pct)) => Some(font_size * (pct / 100.0)),
            _ => None,
        })
        .collect();

    match parts.len() {
        1 => Some(EdgeInsets::all(parts[0])),
        2 => Some(EdgeInsets::symmetric(parts[0], parts[1])),
        3 => Some(EdgeInsets::new(parts[0], parts[1], parts[2], parts[1])),
        4 => Some(EdgeInsets::new(parts[0], parts[1], parts[2], parts[3])),
        _ => None,
    }
}

/// Decodes CSS 4-directional insets shorthand with font-size context (e.g. `10px`, `1em 2em`, `10px 20px 30px 40px`).
pub fn parse_edge_insets_with_font_size(val: &str, font_size: f32) -> Option<EdgeInsets<f32>> {
    parse_edge_insets_with_context(val, font_size, 16.0)
}

/// Decodes CSS 4-directional insets shorthand (e.g. `10px`, `10px 20px`, `10px 20px 30px 40px`).
pub fn parse_edge_insets_px(val: &str) -> Option<EdgeInsets<f32>> {
    parse_edge_insets_with_context(val, 16.0, 16.0)
}

/// Parses CSS `text-decoration` or `text-decoration-line`.
pub fn parse_text_decoration(val: &str) -> Option<TextDecoration> {
    match val.trim().to_ascii_lowercase().as_str() {
        "none" => Some(TextDecoration::None),
        "underline" => Some(TextDecoration::Underline),
        "line-through" => Some(TextDecoration::LineThrough),
        "overline" => Some(TextDecoration::Overline),
        _ => None,
    }
}

/// Parses CSS `text-transform`.
pub fn parse_text_transform(val: &str) -> Option<TextTransform> {
    match val.trim().to_ascii_lowercase().as_str() {
        "none" => Some(TextTransform::None),
        "uppercase" => Some(TextTransform::Uppercase),
        "lowercase" => Some(TextTransform::Lowercase),
        "capitalize" => Some(TextTransform::Capitalize),
        _ => None,
    }
}

/// Parses CSS `white-space`.
pub fn parse_white_space(val: &str) -> Option<WhiteSpace> {
    match val.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(WhiteSpace::Normal),
        "nowrap" => Some(WhiteSpace::NoWrap),
        "pre" => Some(WhiteSpace::Pre),
        "pre-wrap" => Some(WhiteSpace::PreWrap),
        "pre-line" => Some(WhiteSpace::PreLine),
        _ => None,
    }
}

/// Parses CSS `vertical-align`.
pub fn parse_vertical_align(val: &str) -> Option<VerticalAlign> {
    match val.trim().to_ascii_lowercase().as_str() {
        "baseline" => Some(VerticalAlign::Baseline),
        "top" => Some(VerticalAlign::Top),
        "middle" => Some(VerticalAlign::Middle),
        "bottom" => Some(VerticalAlign::Bottom),
        "text-top" => Some(VerticalAlign::TextTop),
        "text-bottom" => Some(VerticalAlign::TextBottom),
        "sub" => Some(VerticalAlign::Sub),
        "super" => Some(VerticalAlign::Super),
        _ => None,
    }
}

/// Parses CSS `cursor`.
pub fn parse_cursor(val: &str) -> Option<Cursor> {
    match val.trim().to_ascii_lowercase().as_str() {
        "default" | "auto" => Some(Cursor::Default),
        "pointer" => Some(Cursor::Pointer),
        "text" => Some(Cursor::Text),
        "move" => Some(Cursor::Move),
        "not-allowed" => Some(Cursor::NotAllowed),
        "crosshair" => Some(Cursor::Crosshair),
        "grab" => Some(Cursor::Grab),
        "grabbing" => Some(Cursor::Grabbing),
        "col-resize" => Some(Cursor::ColResize),
        "row-resize" => Some(Cursor::RowResize),
        "wait" => Some(Cursor::Wait),
        "help" => Some(Cursor::Help),
        "none" => Some(Cursor::None),
        _ => None,
    }
}

/// Parses CSS `user-select`.
pub fn parse_user_select(val: &str) -> Option<UserSelect> {
    match val.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(UserSelect::Auto),
        "none" => Some(UserSelect::None),
        "text" => Some(UserSelect::Text),
        "all" => Some(UserSelect::All),
        _ => None,
    }
}

/// Parses CSS `pointer-events`.
pub fn parse_pointer_events(val: &str) -> Option<PointerEvents> {
    match val.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(PointerEvents::Auto),
        "none" => Some(PointerEvents::None),
        _ => None,
    }
}

/// Parses CSS `visibility`.
pub fn parse_visibility(val: &str) -> Option<Visibility> {
    match val.trim().to_ascii_lowercase().as_str() {
        "visible" => Some(Visibility::Visible),
        "hidden" => Some(Visibility::Hidden),
        "collapse" => Some(Visibility::Collapse),
        _ => None,
    }
}

/// Parses CSS `border-style`.
pub fn parse_border_style(val: &str) -> Option<BorderStyle> {
    match val.trim().to_ascii_lowercase().as_str() {
        "solid" => Some(BorderStyle::Solid),
        "dashed" => Some(BorderStyle::Dashed),
        "dotted" => Some(BorderStyle::Dotted),
        "double" => Some(BorderStyle::Double),
        "none" => Some(BorderStyle::None),
        "hidden" => Some(BorderStyle::Hidden),
        _ => None,
    }
}

/// Parses CSS `aspect-ratio` (e.g., `16 / 9`, `16/9`, `1.5`, `1`).
pub fn parse_aspect_ratio(val: &str) -> Option<f32> {
    let s = val.trim();
    if let Some((w, h)) = s.split_once('/') {
        let width = w.trim().parse::<f32>().ok()?;
        let height = h.trim().parse::<f32>().ok()?;
        if height > 0.0 {
            Some(width / height)
        } else {
            None
        }
    } else if let Ok(val) = s.parse::<f32>() {
        if val > 0.0 {
            Some(val)
        } else {
            None
        }
    } else {
        None
    }
}

/// Splits a CSS value string by commas that are outside of parentheses (e.g. for comma-separated box-shadows or gradients).
pub fn split_comma_outside_parens(val: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut paren_depth = 0;
    let mut start = 0;

    for (i, ch) in val.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
            }
            ',' if paren_depth == 0 => {
                let trimmed = val[start..i].trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    let tail = val[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }

    parts
}

/// Parses CSS `box-shadow` property containing one or more comma-separated shadow definitions.
pub fn parse_box_shadow(val: &str) -> Vec<BoxShadow> {
    let trimmed = val.trim();
    if trimmed.eq_ignore_ascii_case("none") || trimmed.is_empty() {
        return Vec::new();
    }

    let mut shadows = Vec::new();
    for shadow_str in split_comma_outside_parens(trimmed) {
        if let Some(shadow) = parse_single_box_shadow(shadow_str) {
            shadows.push(shadow);
        }
    }

    shadows
}

fn parse_single_box_shadow(s: &str) -> Option<BoxShadow> {
    let mut tokens = Vec::new();
    let mut paren_depth = 0;
    let mut start = 0;

    for (i, ch) in s.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
            }
            ' ' | '\t' | '\n' | '\r' if paren_depth == 0 => {
                let tok = s[start..i].trim();
                if !tok.is_empty() {
                    tokens.push(tok);
                }
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        tokens.push(tail);
    }

    if tokens.is_empty() {
        return None;
    }

    let mut inset = false;
    let mut lengths = Vec::new();
    let mut color = None;

    for tok in tokens {
        if tok.eq_ignore_ascii_case("inset") {
            inset = true;
        } else if let Some(c) = parse_color(tok) {
            color = Some(c);
        } else if let Some(dim) = parse_dimension(tok) {
            let px = match dim {
                Dimension::Px(p) => p,
                Dimension::Em(em) => em * 16.0,
                Dimension::Rem(rem) => rem * 16.0,
                _ => 0.0,
            };
            lengths.push(px);
        }
    }

    if lengths.len() < 2 {
        return None;
    }

    let offset_x = lengths[0];
    let offset_y = lengths[1];
    let blur_radius = if lengths.len() >= 3 {
        lengths[2].max(0.0)
    } else {
        0.0
    };
    let spread_radius = if lengths.len() >= 4 { lengths[3] } else { 0.0 };
    let color = color.unwrap_or(Color::BLACK);

    Some(BoxShadow {
        offset_x,
        offset_y,
        blur_radius,
        spread_radius,
        color,
        inset,
    })
}

/// Parses CSS `linear-gradient(...)`, `radial-gradient(...)`, or `conic-gradient(...)`.
pub fn parse_gradient(val: &str) -> Option<Gradient> {
    let s = val.trim();
    if let Some(inner) = s
        .strip_prefix("linear-gradient(")
        .and_then(|t| t.strip_suffix(')'))
    {
        parse_linear_gradient_body(inner)
    } else if let Some(inner) = s
        .strip_prefix("radial-gradient(")
        .and_then(|t| t.strip_suffix(')'))
    {
        parse_radial_gradient_body(inner)
    } else if let Some(inner) = s
        .strip_prefix("conic-gradient(")
        .and_then(|t| t.strip_suffix(')'))
    {
        parse_conic_gradient_body(inner)
    } else {
        None
    }
}

fn parse_linear_gradient_body(inner: &str) -> Option<Gradient> {
    let parts = split_comma_outside_parens(inner);
    if parts.is_empty() {
        return None;
    }

    let mut angle_deg = 180.0f32; // Default: to bottom
    let mut stop_start_idx = 0;

    let first = parts[0].trim();
    if let Some(angle) = parse_gradient_angle_or_direction(first) {
        angle_deg = angle;
        stop_start_idx = 1;
    }

    let mut stops = Vec::new();
    for &part in &parts[stop_start_idx..] {
        if let Some(stop) = parse_color_stop(part) {
            stops.push(stop);
        }
    }

    if stops.len() < 2 {
        return None;
    }

    Some(Gradient::Linear { angle_deg, stops })
}

fn parse_radial_gradient_body(inner: &str) -> Option<Gradient> {
    let parts = split_comma_outside_parens(inner);
    if parts.is_empty() {
        return None;
    }

    let mut center = (0.5f32, 0.5f32);
    let mut stop_start_idx = 0;

    let first = parts[0].trim();
    if first.starts_with("circle") || first.starts_with("ellipse") || first.starts_with("at ") {
        if let Some(at_idx) = first.find("at ") {
            let pos_str = &first[at_idx + 3..].trim();
            center = parse_position_pair(pos_str);
        }
        stop_start_idx = 1;
    }

    let mut stops = Vec::new();
    for &part in &parts[stop_start_idx..] {
        if let Some(stop) = parse_color_stop(part) {
            stops.push(stop);
        }
    }

    if stops.len() < 2 {
        return None;
    }

    Some(Gradient::Radial { center, stops })
}

fn parse_conic_gradient_body(inner: &str) -> Option<Gradient> {
    let parts = split_comma_outside_parens(inner);
    if parts.is_empty() {
        return None;
    }

    let mut angle_deg = 0.0f32;
    let mut center = (0.5f32, 0.5f32);
    let mut stop_start_idx = 0;

    let first = parts[0].trim();
    if first.starts_with("from ") || first.starts_with("at ") {
        if let Some(from_idx) = first.find("from ") {
            let rem = &first[from_idx + 5..];
            let angle_token = rem.split_whitespace().next().unwrap_or("");
            if let Some(a) = parse_gradient_angle(angle_token) {
                angle_deg = a;
            }
        }
        if let Some(at_idx) = first.find("at ") {
            let pos_str = &first[at_idx + 3..].trim();
            center = parse_position_pair(pos_str);
        }
        stop_start_idx = 1;
    }

    let mut stops = Vec::new();
    for &part in &parts[stop_start_idx..] {
        if let Some(stop) = parse_color_stop(part) {
            stops.push(stop);
        }
    }

    if stops.len() < 2 {
        return None;
    }

    Some(Gradient::Conic {
        angle_deg,
        center,
        stops,
    })
}

fn parse_gradient_angle_or_direction(s: &str) -> Option<f32> {
    let lower = s.to_ascii_lowercase();
    match lower.as_str() {
        "to top" => Some(0.0),
        "to right" => Some(90.0),
        "to bottom" => Some(180.0),
        "to left" => Some(270.0),
        "to top right" | "to right top" => Some(45.0),
        "to bottom right" | "to right bottom" => Some(135.0),
        "to bottom left" | "to left bottom" => Some(225.0),
        "to top left" | "to left top" => Some(315.0),
        _ => parse_gradient_angle(&lower),
    }
}

fn parse_gradient_angle(s: &str) -> Option<f32> {
    if let Some(deg) = s.strip_suffix("deg") {
        deg.trim().parse::<f32>().ok()
    } else if let Some(rad) = s.strip_suffix("rad") {
        rad.trim().parse::<f32>().ok().map(|r| r.to_degrees())
    } else if let Some(turn) = s.strip_suffix("turn") {
        turn.trim().parse::<f32>().ok().map(|t| t * 360.0)
    } else {
        None
    }
}

fn parse_position_pair(s: &str) -> (f32, f32) {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.is_empty() {
        return (0.5, 0.5);
    }
    let x = match tokens[0] {
        "left" => 0.0,
        "center" => 0.5,
        "right" => 1.0,
        other => {
            if let Some(p) = other.strip_suffix('%').and_then(|t| t.parse::<f32>().ok()) {
                p / 100.0
            } else {
                0.5
            }
        }
    };
    let y = if tokens.len() >= 2 {
        match tokens[1] {
            "top" => 0.0,
            "center" => 0.5,
            "bottom" => 1.0,
            other => {
                if let Some(p) = other.strip_suffix('%').and_then(|t| t.parse::<f32>().ok()) {
                    p / 100.0
                } else {
                    0.5
                }
            }
        }
    } else {
        0.5
    };
    (x, y)
}

fn parse_color_stop(part: &str) -> Option<ColorStop> {
    let tokens: Vec<&str> = part.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let mut color = None;
    let mut position = None;

    for tok in tokens {
        if let Some(c) = parse_color(tok) {
            color = Some(c);
        } else if let Some(pct) = tok.strip_suffix('%').and_then(|t| t.parse::<f32>().ok()) {
            position = Some((pct / 100.0).clamp(0.0, 1.0));
        } else if let Some(Dimension::Percent(p)) = parse_dimension(tok) {
            position = Some((p / 100.0).clamp(0.0, 1.0));
        }
    }

    color.map(|c| ColorStop { color: c, position })
}

/// Parses CSS `filter` and `backdrop-filter` function lists (e.g., `blur(8px) brightness(1.2)`).
pub fn parse_filter_list(val: &str) -> Option<crate::effects::BackdropFilter> {
    let s = val.trim();
    if s.eq_ignore_ascii_case("none") || s.is_empty() {
        return None;
    }

    let mut ops = Vec::new();
    let mut paren_depth = 0;
    let mut start = 0;
    let mut tokens = Vec::new();

    for (i, ch) in s.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
            }
            ' ' | '\t' if paren_depth == 0 => {
                let tok = s[start..i].trim();
                if !tok.is_empty() {
                    tokens.push(tok);
                }
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        tokens.push(tail);
    }

    for tok in tokens {
        if let Some(open) = tok.find('(') {
            if let Some(close) = tok.rfind(')') {
                let name = tok[..open].trim().to_ascii_lowercase();
                let arg = tok[open + 1..close].trim();
                match name.as_str() {
                    "blur" => {
                        if let Some(dim) = parse_dimension(arg) {
                            let px = match dim {
                                Dimension::Px(p) => p,
                                Dimension::Em(em) => em * 16.0,
                                Dimension::Rem(rem) => rem * 16.0,
                                _ => 0.0,
                            };
                            ops.push(crate::effects::FilterOp::Blur(px));
                        }
                    }
                    "brightness" => {
                        if let Ok(v) = arg.parse::<f32>() {
                            ops.push(crate::effects::FilterOp::Brightness(v));
                        } else if let Some(pct) =
                            arg.strip_suffix('%').and_then(|t| t.parse::<f32>().ok())
                        {
                            ops.push(crate::effects::FilterOp::Brightness(pct / 100.0));
                        }
                    }
                    "contrast" => {
                        if let Ok(v) = arg.parse::<f32>() {
                            ops.push(crate::effects::FilterOp::Contrast(v));
                        } else if let Some(pct) =
                            arg.strip_suffix('%').and_then(|t| t.parse::<f32>().ok())
                        {
                            ops.push(crate::effects::FilterOp::Contrast(pct / 100.0));
                        }
                    }
                    "grayscale" => {
                        if let Ok(v) = arg.parse::<f32>() {
                            ops.push(crate::effects::FilterOp::Grayscale(v));
                        } else if let Some(pct) =
                            arg.strip_suffix('%').and_then(|t| t.parse::<f32>().ok())
                        {
                            ops.push(crate::effects::FilterOp::Grayscale(pct / 100.0));
                        }
                    }
                    "invert" => {
                        if let Ok(v) = arg.parse::<f32>() {
                            ops.push(crate::effects::FilterOp::Invert(v));
                        } else if let Some(pct) =
                            arg.strip_suffix('%').and_then(|t| t.parse::<f32>().ok())
                        {
                            ops.push(crate::effects::FilterOp::Invert(pct / 100.0));
                        }
                    }
                    "opacity" => {
                        if let Ok(v) = arg.parse::<f32>() {
                            ops.push(crate::effects::FilterOp::Opacity(v));
                        } else if let Some(pct) =
                            arg.strip_suffix('%').and_then(|t| t.parse::<f32>().ok())
                        {
                            ops.push(crate::effects::FilterOp::Opacity(pct / 100.0));
                        }
                    }
                    "saturate" => {
                        if let Ok(v) = arg.parse::<f32>() {
                            ops.push(crate::effects::FilterOp::Saturate(v));
                        } else if let Some(pct) =
                            arg.strip_suffix('%').and_then(|t| t.parse::<f32>().ok())
                        {
                            ops.push(crate::effects::FilterOp::Saturate(pct / 100.0));
                        }
                    }
                    "sepia" => {
                        if let Ok(v) = arg.parse::<f32>() {
                            ops.push(crate::effects::FilterOp::Sepia(v));
                        } else if let Some(pct) =
                            arg.strip_suffix('%').and_then(|t| t.parse::<f32>().ok())
                        {
                            ops.push(crate::effects::FilterOp::Sepia(pct / 100.0));
                        }
                    }
                    "hue-rotate" => {
                        let arg_lower = arg.to_ascii_lowercase();
                        let deg = if let Some(d) = arg_lower.strip_suffix("deg") {
                            d.trim().parse::<f32>().unwrap_or(0.0)
                        } else if let Some(rad) = arg_lower.strip_suffix("rad") {
                            rad.trim().parse::<f32>().unwrap_or(0.0)
                                * (180.0 / std::f32::consts::PI)
                        } else if let Some(turn) = arg_lower.strip_suffix("turn") {
                            turn.trim().parse::<f32>().unwrap_or(0.0) * 360.0
                        } else {
                            arg_lower.parse::<f32>().unwrap_or(0.0)
                        };
                        ops.push(crate::effects::FilterOp::HueRotate(deg));
                    }
                    "drop-shadow" => {
                        if let Some(ds) = parse_filter_drop_shadow(arg) {
                            ops.push(ds);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if ops.is_empty() {
        None
    } else {
        Some(crate::effects::BackdropFilter::new(ops))
    }
}

/// Helper parsing drop-shadow arguments inside filter functions.
fn parse_filter_drop_shadow(arg: &str) -> Option<crate::effects::FilterOp> {
    let tokens: Vec<&str> = arg.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }
    let mut lengths = Vec::new();
    let mut color = None;

    for tok in tokens {
        if let Some(dim) = parse_dimension(tok) {
            let px = match dim {
                Dimension::Px(p) => p,
                Dimension::Em(em) => em * 16.0,
                Dimension::Rem(rem) => rem * 16.0,
                _ => 0.0,
            };
            lengths.push(px);
        } else if let Some(c) = parse_color(tok) {
            color = Some(c);
        }
    }

    if lengths.len() < 2 {
        return None;
    }

    let offset_x = lengths[0];
    let offset_y = lengths[1];
    let blur_radius = if lengths.len() >= 3 { lengths[2] } else { 0.0 };
    let color = color.unwrap_or(Color::BLACK);

    Some(crate::effects::FilterOp::DropShadow {
        offset_x,
        offset_y,
        blur_radius,
        color,
    })
}

/// Parses CSS `clip-path` property values into `ClipPath`.
pub fn parse_clip_path(val: &str) -> Option<crate::effects::ClipPath> {
    let s = val.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") {
        return Some(crate::effects::ClipPath::None);
    }

    let open = s.find('(')?;
    let close = s.rfind(')')?;
    if close <= open {
        return None;
    }

    let func_name = s[..open].trim().to_ascii_lowercase();
    let arg = s[open + 1..close].trim();

    match func_name.as_str() {
        "inset" => {
            let mut parts = arg.split("round");
            let insets_part = parts.next().unwrap_or("").trim();
            let round_part = parts.next().map(|r| r.trim());

            let inset_tokens: Vec<f32> = insets_part
                .split_whitespace()
                .filter_map(|t| match parse_dimension(t) {
                    Some(Dimension::Px(p)) => Some(p),
                    Some(Dimension::Em(em)) => Some(em * 16.0),
                    Some(Dimension::Rem(rem)) => Some(rem * 16.0),
                    _ => t.parse::<f32>().ok(),
                })
                .collect();

            let (top, right, bottom, left) = match inset_tokens.len() {
                1 => (
                    inset_tokens[0],
                    inset_tokens[0],
                    inset_tokens[0],
                    inset_tokens[0],
                ),
                2 => (
                    inset_tokens[0],
                    inset_tokens[1],
                    inset_tokens[0],
                    inset_tokens[1],
                ),
                3 => (
                    inset_tokens[0],
                    inset_tokens[1],
                    inset_tokens[2],
                    inset_tokens[1],
                ),
                4 => (
                    inset_tokens[0],
                    inset_tokens[1],
                    inset_tokens[2],
                    inset_tokens[3],
                ),
                _ => (0.0, 0.0, 0.0, 0.0),
            };

            let border_radius = if let Some(rp) = round_part {
                rp.split_whitespace()
                    .next()
                    .and_then(|t| match parse_dimension(t) {
                        Some(Dimension::Px(p)) => Some(p),
                        Some(Dimension::Em(em)) => Some(em * 16.0),
                        Some(Dimension::Rem(rem)) => Some(rem * 16.0),
                        _ => t.parse::<f32>().ok(),
                    })
                    .unwrap_or(0.0)
            } else {
                0.0
            };

            Some(crate::effects::ClipPath::Inset {
                top,
                right,
                bottom,
                left,
                border_radius,
            })
        }
        "circle" => {
            let mut radius = 50.0f32;
            let mut center_x = 50.0f32;
            let mut center_y = 50.0f32;

            if let Some(at_idx) = arg.to_ascii_lowercase().find(" at ") {
                let r_part = arg[..at_idx].trim();
                let center_part = arg[at_idx + 4..].trim();
                if let Some(r) = parse_dimension(r_part) {
                    radius = match r {
                        Dimension::Px(p) => p,
                        _ => 50.0,
                    };
                }
                let c_tokens: Vec<&str> = center_part.split_whitespace().collect();
                if c_tokens.len() >= 2 {
                    if let Some(cx) = parse_dimension(c_tokens[0]) {
                        center_x = match cx {
                            Dimension::Px(p) => p,
                            _ => 50.0,
                        };
                    }
                    if let Some(cy) = parse_dimension(c_tokens[1]) {
                        center_y = match cy {
                            Dimension::Px(p) => p,
                            _ => 50.0,
                        };
                    }
                }
            } else if !arg.is_empty() {
                if let Some(r) = parse_dimension(arg) {
                    radius = match r {
                        Dimension::Px(p) => p,
                        _ => 50.0,
                    };
                }
            }

            Some(crate::effects::ClipPath::Circle {
                radius,
                center_x,
                center_y,
            })
        }
        "ellipse" => {
            let mut radius_x = 50.0f32;
            let mut radius_y = 50.0f32;
            let mut center_x = 50.0f32;
            let mut center_y = 50.0f32;

            if let Some(at_idx) = arg.to_ascii_lowercase().find(" at ") {
                let r_part = arg[..at_idx].trim();
                let center_part = arg[at_idx + 4..].trim();
                let r_tokens: Vec<&str> = r_part.split_whitespace().collect();
                if r_tokens.len() >= 2 {
                    if let Some(rx) = parse_dimension(r_tokens[0]) {
                        radius_x = match rx {
                            Dimension::Px(p) => p,
                            _ => 50.0,
                        };
                    }
                    if let Some(ry) = parse_dimension(r_tokens[1]) {
                        radius_y = match ry {
                            Dimension::Px(p) => p,
                            _ => 50.0,
                        };
                    }
                }
                let c_tokens: Vec<&str> = center_part.split_whitespace().collect();
                if c_tokens.len() >= 2 {
                    if let Some(cx) = parse_dimension(c_tokens[0]) {
                        center_x = match cx {
                            Dimension::Px(p) => p,
                            _ => 50.0,
                        };
                    }
                    if let Some(cy) = parse_dimension(c_tokens[1]) {
                        center_y = match cy {
                            Dimension::Px(p) => p,
                            _ => 50.0,
                        };
                    }
                }
            } else if !arg.is_empty() {
                let r_tokens: Vec<&str> = arg.split_whitespace().collect();
                if r_tokens.len() >= 2 {
                    if let Some(rx) = parse_dimension(r_tokens[0]) {
                        radius_x = match rx {
                            Dimension::Px(p) => p,
                            _ => 50.0,
                        };
                    }
                    if let Some(ry) = parse_dimension(r_tokens[1]) {
                        radius_y = match ry {
                            Dimension::Px(p) => p,
                            _ => 50.0,
                        };
                    }
                }
            }

            Some(crate::effects::ClipPath::Ellipse {
                radius_x,
                radius_y,
                center_x,
                center_y,
            })
        }
        "polygon" => {
            let mut vertices = Vec::new();
            for pair in arg.split(',') {
                let pt_str = pair.trim();
                let coords: Vec<&str> = pt_str.split_whitespace().collect();
                if coords.len() >= 2 {
                    let x = parse_dimension(coords[0])
                        .map(|d| match d {
                            Dimension::Px(p) => p,
                            Dimension::Percent(pct) => pct,
                            _ => 0.0,
                        })
                        .unwrap_or(0.0);
                    let y = parse_dimension(coords[1])
                        .map(|d| match d {
                            Dimension::Px(p) => p,
                            Dimension::Percent(pct) => pct,
                            _ => 0.0,
                        })
                        .unwrap_or(0.0);
                    vertices.push(strata_primitives::geom::Point::new(x, y));
                }
            }
            if vertices.is_empty() {
                None
            } else {
                Some(crate::effects::ClipPath::Polygon(vertices))
            }
        }
        "path" => {
            let unquoted = arg.trim_matches(|c| c == '\'' || c == '"').trim();
            Some(crate::effects::ClipPath::Path(unquoted.to_string()))
        }
        _ => None,
    }
}

/// Parses a CSS time string into milliseconds (e.g. `200ms`, `0.3s`, `0`).
pub fn parse_time_ms(val: &str) -> Option<f32> {
    let s = val.trim().to_ascii_lowercase();
    if let Some(ms) = s.strip_suffix("ms") {
        ms.trim().parse::<f32>().ok()
    } else if let Some(sec) = s.strip_suffix('s') {
        sec.trim().parse::<f32>().map(|s| s * 1000.0).ok()
    } else if let Ok(n) = s.parse::<f32>() {
        if n == 0.0 {
            Some(0.0)
        } else {
            None
        }
    } else {
        None
    }
}

/// Splits a CSS value string by top-level commas, respecting parentheses.
pub fn split_comma_separated(s: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth: usize = 0;

    for c in s.chars() {
        if c == '(' {
            depth += 1;
            current.push(c);
        } else if c == ')' {
            depth = depth.saturating_sub(1);
            current.push(c);
        } else if c == ',' && depth == 0 {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                items.push(trimmed.to_string());
            }
            current = String::new();
        } else {
            current.push(c);
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        items.push(trimmed.to_string());
    }
    items
}

/// Splits a single transition specification by whitespace, respecting parentheses.
fn split_transition_tokens(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth: usize = 0;

    for c in s.chars() {
        if c == '(' {
            depth += 1;
            current.push(c);
        } else if c == ')' {
            depth = depth.saturating_sub(1);
            current.push(c);
        } else if c.is_whitespace() && depth == 0 {
            if !current.is_empty() {
                tokens.push(current);
                current = String::new();
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Parses a single CSS transition definition (e.g. `opacity 200ms ease-in-out 50ms allow-discrete`).
pub fn parse_single_transition(item: &str) -> Option<TransitionSpec> {
    let tokens = split_transition_tokens(item);
    if tokens.is_empty() {
        return None;
    }

    let mut property: Option<TransitionProperty> = None;
    let mut duration_ms: Option<f32> = None;
    let mut timing_function: Option<TransitionEasing> = None;
    let mut delay_ms: Option<f32> = None;
    let mut behavior: Option<TransitionBehavior> = None;

    for tok in tokens {
        let lower = tok.to_ascii_lowercase();

        // 1. Behavior: normal | allow-discrete
        if lower == "allow-discrete" {
            behavior = Some(TransitionBehavior::AllowDiscrete);
            continue;
        } else if lower == "normal" && behavior.is_none() {
            behavior = Some(TransitionBehavior::Normal);
            continue;
        }

        // 2. Timing function
        if let Some(easing) = TransitionEasing::parse(&tok) {
            if timing_function.is_none() {
                timing_function = Some(easing);
                continue;
            }
        }

        // 3. Time (duration or delay)
        if let Some(t) = parse_time_ms(&tok) {
            if duration_ms.is_none() {
                duration_ms = Some(t);
            } else if delay_ms.is_none() {
                delay_ms = Some(t);
            }
            continue;
        }

        // 4. Property
        if property.is_none() {
            property = Some(TransitionProperty::parse(&tok));
        }
    }

    Some(TransitionSpec {
        property: property.unwrap_or(TransitionProperty::All),
        duration_ms: duration_ms.unwrap_or(0.0),
        timing_function: timing_function.unwrap_or(TransitionEasing::Ease),
        delay_ms: delay_ms.unwrap_or(0.0),
        behavior: behavior.unwrap_or(TransitionBehavior::Normal),
    })
}

/// Parses a comma-separated `transition` shorthand property string into a list of `TransitionSpec`s.
pub fn parse_transitions(val: &str) -> Vec<TransitionSpec> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| parse_single_transition(&s))
        .collect()
}

/// Parses a comma-separated `transition-property` list.
pub fn parse_transition_properties(val: &str) -> Vec<TransitionProperty> {
    split_comma_separated(val)
        .into_iter()
        .map(|s| TransitionProperty::parse(&s))
        .collect()
}

/// Parses a comma-separated `transition-duration` list in milliseconds.
pub fn parse_transition_durations(val: &str) -> Vec<f32> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| parse_time_ms(&s))
        .collect()
}

/// Parses a comma-separated `transition-timing-function` list.
pub fn parse_transition_timing_functions(val: &str) -> Vec<TransitionEasing> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| TransitionEasing::parse(&s))
        .collect()
}

/// Parses a comma-separated `transition-delay` list in milliseconds.
pub fn parse_transition_delays(val: &str) -> Vec<f32> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| parse_time_ms(&s))
        .collect()
}

/// Parses a comma-separated `transition-behavior` list.
pub fn parse_transition_behaviors(val: &str) -> Vec<TransitionBehavior> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| TransitionBehavior::parse(&s))
        .collect()
}

/// Parses a single CSS animation definition (e.g. `spin 1s linear infinite`, `ping 1s cubic-bezier(0, 0, 0.2, 1) infinite`).
pub fn parse_single_animation(item: &str) -> Option<AnimationSpec> {
    let tokens = split_transition_tokens(item);
    if tokens.is_empty() {
        return None;
    }

    let mut name: Option<String> = None;
    let mut duration_ms: Option<f32> = None;
    let mut timing_function: Option<TransitionEasing> = None;
    let mut delay_ms: Option<f32> = None;
    let mut iteration_count: Option<AnimationIterationCount> = None;
    let mut direction: Option<AnimationDirection> = None;
    let mut fill_mode: Option<AnimationFillMode> = None;
    let mut play_state: Option<AnimationPlayState> = None;

    for tok in tokens {
        let lower = tok.to_ascii_lowercase();

        // 1. Play state: running | paused
        if let Some(ps) = AnimationPlayState::parse(&tok) {
            if play_state.is_none() {
                play_state = Some(ps);
                continue;
            }
        }

        // 2. Direction: normal | reverse | alternate | alternate-reverse
        if let Some(dir) = AnimationDirection::parse(&tok) {
            if direction.is_none() {
                direction = Some(dir);
                continue;
            }
        }

        // 3. Fill mode: forwards | backwards | both
        if (lower == "forwards" || lower == "backwards" || lower == "both")
            && fill_mode.is_none() {
                fill_mode = Some(AnimationFillMode::parse(&tok).unwrap());
                continue;
            }

        // 4. Timing function
        if let Some(easing) = TransitionEasing::parse(&tok) {
            if timing_function.is_none() {
                timing_function = Some(easing);
                continue;
            }
        }

        // 5. Time (duration or delay)
        if let Some(t) = parse_time_ms(&tok) {
            if duration_ms.is_none() {
                duration_ms = Some(t);
            } else if delay_ms.is_none() {
                delay_ms = Some(t);
            }
            continue;
        }

        // 6. Iteration count: infinite or number
        if lower == "infinite" {
            if iteration_count.is_none() {
                iteration_count = Some(AnimationIterationCount::Infinite);
                continue;
            }
        } else if let Ok(n) = tok.parse::<f32>() {
            if iteration_count.is_none() && n >= 0.0 {
                iteration_count = Some(AnimationIterationCount::Count(n));
                continue;
            }
        }

        // 7. Keyword 'none' can denote fill-mode none or animation-name none
        if lower == "none" {
            if fill_mode.is_none() {
                fill_mode = Some(AnimationFillMode::None);
            } else if name.is_none() {
                name = Some("none".to_string());
            }
            continue;
        }

        // 8. Otherwise, it is the animation name
        if name.is_none() {
            name = Some(tok.to_string());
        }
    }

    let anim_name = name.unwrap_or_default();
    if anim_name.is_empty() && duration_ms.is_none() {
        return None;
    }

    Some(AnimationSpec {
        name: anim_name,
        duration_ms: duration_ms.unwrap_or(0.0),
        timing_function: timing_function.unwrap_or(TransitionEasing::Ease),
        delay_ms: delay_ms.unwrap_or(0.0),
        iteration_count: iteration_count.unwrap_or(AnimationIterationCount::Count(1.0)),
        direction: direction.unwrap_or(AnimationDirection::Normal),
        fill_mode: fill_mode.unwrap_or(AnimationFillMode::None),
        play_state: play_state.unwrap_or(AnimationPlayState::Running),
        timeline: AnimationTimeline::Time,
        range: AnimationRange::default(),
    })
}

/// Parses a comma-separated `animation` shorthand property string into a list of `AnimationSpec`s.
pub fn parse_animations(val: &str) -> Vec<AnimationSpec> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| parse_single_animation(&s))
        .collect()
}

/// Parses a comma-separated `animation-name` list.
pub fn parse_animation_names(val: &str) -> Vec<String> {
    split_comma_separated(val)
        .into_iter()
        .map(|s| s.trim().to_string())
        .collect()
}

/// Parses a comma-separated `animation-duration` list in milliseconds.
pub fn parse_animation_durations(val: &str) -> Vec<f32> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| parse_time_ms(&s))
        .collect()
}

/// Parses a comma-separated `animation-timing-function` list.
pub fn parse_animation_timing_functions(val: &str) -> Vec<TransitionEasing> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| TransitionEasing::parse(&s))
        .collect()
}

/// Parses a comma-separated `animation-delay` list in milliseconds.
pub fn parse_animation_delays(val: &str) -> Vec<f32> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| parse_time_ms(&s))
        .collect()
}

/// Parses a comma-separated `animation-iteration-count` list.
pub fn parse_animation_iteration_counts(val: &str) -> Vec<AnimationIterationCount> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| AnimationIterationCount::parse(&s))
        .collect()
}

/// Parses a comma-separated `animation-direction` list.
pub fn parse_animation_directions(val: &str) -> Vec<AnimationDirection> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| AnimationDirection::parse(&s))
        .collect()
}

/// Parses a comma-separated `animation-fill-mode` list.
pub fn parse_animation_fill_modes(val: &str) -> Vec<AnimationFillMode> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| AnimationFillMode::parse(&s))
        .collect()
}

/// Parses a comma-separated `animation-play-state` list.
pub fn parse_animation_play_states(val: &str) -> Vec<AnimationPlayState> {
    split_comma_separated(val)
        .into_iter()
        .filter_map(|s| AnimationPlayState::parse(&s))
        .collect()
}

/// Parses a CSS angle string into radians (e.g. `45deg`, `0.5rad`, `1turn`, `100grad`, `0`).
pub fn parse_angle_rad(val: &str) -> Option<f32> {
    let s = val.trim().to_ascii_lowercase();
    if let Some(deg) = s.strip_suffix("deg") {
        deg.trim().parse::<f32>().map(|d| d.to_radians()).ok()
    } else if let Some(rad) = s.strip_suffix("rad") {
        rad.trim().parse::<f32>().ok()
    } else if let Some(turn) = s.strip_suffix("turn") {
        turn.trim()
            .parse::<f32>()
            .map(|t| t * 2.0 * std::f32::consts::PI)
            .ok()
    } else if let Some(grad) = s.strip_suffix("grad") {
        grad.trim()
            .parse::<f32>()
            .map(|g| g * (std::f32::consts::PI / 200.0))
            .ok()
    } else if let Ok(n) = s.parse::<f32>() {
        if n == 0.0 {
            Some(0.0)
        } else {
            None
        }
    } else {
        None
    }
}

/// Parses an individual CSS transform function (e.g. `rotate(45deg)` or `translate(10px, 20px)`).
pub fn parse_transform_op(func_name: &str, args_str: &str) -> Option<TransformOp> {
    let parts: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
    match func_name {
        "translate" => {
            if parts.is_empty() {
                return None;
            }
            let dx = parse_dimension(parts[0])?;
            let dy = if parts.len() > 1 {
                parse_dimension(parts[1])?
            } else {
                Dimension::Px(0.0)
            };
            Some(TransformOp::Translate(dx, dy))
        }
        "translatex" => {
            let dx = parse_dimension(parts.first()?)?;
            Some(TransformOp::Translate(dx, Dimension::Px(0.0)))
        }
        "translatey" => {
            let dy = parse_dimension(parts.first()?)?;
            Some(TransformOp::Translate(Dimension::Px(0.0), dy))
        }
        "translatez" => {
            let dz = parse_dimension(parts.first()?)?;
            Some(TransformOp::Translate3d(
                Dimension::Px(0.0),
                Dimension::Px(0.0),
                dz,
            ))
        }
        "translate3d" => {
            if parts.len() < 3 {
                return None;
            }
            let dx = parse_dimension(parts[0])?;
            let dy = parse_dimension(parts[1])?;
            let dz = parse_dimension(parts[2])?;
            Some(TransformOp::Translate3d(dx, dy, dz))
        }
        "scale" => {
            if parts.is_empty() {
                return None;
            }
            let sx = parts[0].parse::<f32>().ok()?;
            let sy = if parts.len() > 1 {
                parts[1].parse::<f32>().ok()?
            } else {
                sx
            };
            Some(TransformOp::Scale(sx, sy))
        }
        "scalex" => {
            let sx = parts.first()?.parse::<f32>().ok()?;
            Some(TransformOp::Scale(sx, 1.0))
        }
        "scaley" => {
            let sy = parts.first()?.parse::<f32>().ok()?;
            Some(TransformOp::Scale(1.0, sy))
        }
        "scalez" => {
            let sz = parts.first()?.parse::<f32>().ok()?;
            Some(TransformOp::Scale3d(1.0, 1.0, sz))
        }
        "scale3d" => {
            if parts.len() < 3 {
                return None;
            }
            let sx = parts[0].parse::<f32>().ok()?;
            let sy = parts[1].parse::<f32>().ok()?;
            let sz = parts[2].parse::<f32>().ok()?;
            Some(TransformOp::Scale3d(sx, sy, sz))
        }
        "rotate" => {
            let rad = parse_angle_rad(parts.first()?)?;
            Some(TransformOp::Rotate(rad))
        }
        "rotatez" => {
            let rad = parse_angle_rad(parts.first()?)?;
            Some(TransformOp::RotateZ(rad))
        }
        "rotatex" => {
            let rad = parse_angle_rad(parts.first()?)?;
            Some(TransformOp::RotateX(rad))
        }
        "rotatey" => {
            let rad = parse_angle_rad(parts.first()?)?;
            Some(TransformOp::RotateY(rad))
        }
        "rotate3d" => {
            if parts.len() < 4 {
                return None;
            }
            let x = parts[0].parse::<f32>().ok()?;
            let y = parts[1].parse::<f32>().ok()?;
            let z = parts[2].parse::<f32>().ok()?;
            let rad = parse_angle_rad(parts[3])?;
            Some(TransformOp::Rotate3d(x, y, z, rad))
        }
        "skew" => {
            if parts.is_empty() {
                return None;
            }
            let ax = parse_angle_rad(parts[0])?;
            let ay = if parts.len() > 1 {
                parse_angle_rad(parts[1])?
            } else {
                0.0
            };
            Some(TransformOp::Skew(ax, ay))
        }
        "skewx" => {
            let ax = parse_angle_rad(parts.first()?)?;
            Some(TransformOp::Skew(ax, 0.0))
        }
        "skewy" => {
            let ay = parse_angle_rad(parts.first()?)?;
            Some(TransformOp::Skew(0.0, ay))
        }
        "perspective" => {
            let d = match parse_dimension(parts.first()?)? {
                Dimension::Px(px) => px,
                Dimension::Rem(rem) => rem * 16.0,
                Dimension::Em(em) => em * 16.0,
                _ => return None,
            };
            Some(TransformOp::Perspective(d))
        }
        "matrix" => {
            if parts.len() < 6 {
                return None;
            }
            let mut m = [0.0f32; 6];
            for i in 0..6 {
                m[i] = parts[i].parse::<f32>().ok()?;
            }
            Some(TransformOp::Matrix(m))
        }
        "matrix3d" => {
            if parts.len() < 16 {
                return None;
            }
            let mut m = [0.0f32; 16];
            for i in 0..16 {
                m[i] = parts[i].parse::<f32>().ok()?;
            }
            Some(TransformOp::Matrix3d(m))
        }
        _ => None,
    }
}

/// Parses a complete CSS `transform` property value into a list of `TransformOp`s.
pub fn parse_transform_list(val: &str) -> Vec<TransformOp> {
    let trimmed = val.trim();
    if trimmed.eq_ignore_ascii_case("none") {
        return Vec::new();
    }

    let mut ops = Vec::new();
    let mut chars = trimmed.chars().peekable();

    while chars.peek().is_some() {
        skip_whitespace(&mut chars);
        if chars.peek().is_none() {
            break;
        }

        // Read function name up to '('
        let mut name = String::new();
        while let Some(&c) = chars.peek() {
            if c == '(' {
                chars.next();
                break;
            }
            if c.is_whitespace() {
                chars.next();
                continue;
            }
            name.push(c);
            chars.next();
        }

        if name.is_empty() {
            break;
        }

        // Read arguments up to ')'
        let mut args = String::new();
        let mut depth = 1;
        for c in chars.by_ref() {
            if c == '(' {
                depth += 1;
                args.push(c);
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                args.push(c);
            } else {
                args.push(c);
            }
        }

        let func_lower = name.trim().to_ascii_lowercase();
        if let Some(op) = parse_transform_op(&func_lower, &args) {
            ops.push(op);
        }
    }

    ops
}

/// Parses a CSS `transform-origin` property string.
pub fn parse_transform_origin(val: &str) -> Option<TransformOrigin> {
    let parts: Vec<&str> = val.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let (x, y) = if parts.len() == 1 {
        let p = parts[0].to_ascii_lowercase();
        match p.as_str() {
            "top" => (
                Some(Dimension::Percent(50.0)),
                Some(Dimension::Percent(0.0)),
            ),
            "bottom" => (
                Some(Dimension::Percent(50.0)),
                Some(Dimension::Percent(100.0)),
            ),
            "left" => (
                Some(Dimension::Percent(0.0)),
                Some(Dimension::Percent(50.0)),
            ),
            "right" => (
                Some(Dimension::Percent(100.0)),
                Some(Dimension::Percent(50.0)),
            ),
            "center" => (
                Some(Dimension::Percent(50.0)),
                Some(Dimension::Percent(50.0)),
            ),
            _ => {
                let d = parse_dimension(parts[0])?;
                (Some(d), Some(Dimension::Percent(50.0)))
            }
        }
    } else {
        let p0 = parts[0].to_ascii_lowercase();
        let p1 = parts[1].to_ascii_lowercase();

        if p0 == "top" || p0 == "bottom" {
            let y_val = if p0 == "top" {
                Dimension::Percent(0.0)
            } else {
                Dimension::Percent(100.0)
            };
            let x_val = match p1.as_str() {
                "left" => Some(Dimension::Percent(0.0)),
                "right" => Some(Dimension::Percent(100.0)),
                "center" => Some(Dimension::Percent(50.0)),
                _ => parse_dimension(parts[1]),
            };
            (x_val, Some(y_val))
        } else if p1 == "top" || p1 == "bottom" {
            let y_val = if p1 == "top" {
                Dimension::Percent(0.0)
            } else {
                Dimension::Percent(100.0)
            };
            let x_val = match p0.as_str() {
                "left" => Some(Dimension::Percent(0.0)),
                "right" => Some(Dimension::Percent(100.0)),
                "center" => Some(Dimension::Percent(50.0)),
                _ => parse_dimension(parts[0]),
            };
            (x_val, Some(y_val))
        } else {
            let x_val = match p0.as_str() {
                "left" => Some(Dimension::Percent(0.0)),
                "right" => Some(Dimension::Percent(100.0)),
                "center" => Some(Dimension::Percent(50.0)),
                _ => parse_dimension(parts[0]),
            };
            let y_val = match p1.as_str() {
                "top" => Some(Dimension::Percent(0.0)),
                "bottom" => Some(Dimension::Percent(100.0)),
                "center" => Some(Dimension::Percent(50.0)),
                _ => parse_dimension(parts[1]),
            };
            (x_val, y_val)
        }
    };

    let z = if parts.len() > 2 {
        match parse_dimension(parts[2]) {
            Some(Dimension::Px(px)) => px,
            _ => 0.0,
        }
    } else {
        0.0
    };

    Some(TransformOrigin {
        x: x.unwrap_or(Dimension::Percent(50.0)),
        y: y.unwrap_or(Dimension::Percent(50.0)),
        z,
    })
}

/// Parses a CSS `transform-style` property string.
pub fn parse_transform_style(val: &str) -> Option<TransformStyle> {
    match val.trim().to_ascii_lowercase().as_str() {
        "flat" => Some(TransformStyle::Flat),
        "preserve-3d" => Some(TransformStyle::Preserve3d),
        _ => None,
    }
}

/// Parses a CSS `backface-visibility` property string.
pub fn parse_backface_visibility(val: &str) -> Option<BackfaceVisibility> {
    match val.trim().to_ascii_lowercase().as_str() {
        "visible" => Some(BackfaceVisibility::Visible),
        "hidden" => Some(BackfaceVisibility::Hidden),
        _ => None,
    }
}

/// Parses a CSS `animation-timeline` property value per W3C Scroll-Driven Animations Level 1.
pub fn parse_animation_timeline(val: &str) -> AnimationTimeline {
    let trimmed = val.trim();
    if trimmed.eq_ignore_ascii_case("auto") {
        return AnimationTimeline::Time;
    }
    if trimmed.eq_ignore_ascii_case("none") {
        return AnimationTimeline::Named("none".to_string());
    }

    if let Some(args) = trimmed
        .strip_prefix("scroll(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let args = args.trim();
        let mut source = ScrollTimelineSource::Nearest;
        let mut axis = ScrollAxis::Block;

        for part in args.split_whitespace() {
            match part.to_ascii_lowercase().as_str() {
                "nearest" => source = ScrollTimelineSource::Nearest,
                "root" => source = ScrollTimelineSource::Root,
                "self" => source = ScrollTimelineSource::SelfNode,
                "block" | "y" => axis = ScrollAxis::Block,
                "inline" | "x" => axis = ScrollAxis::Inline,
                "vertical" => axis = ScrollAxis::Vertical,
                "horizontal" => axis = ScrollAxis::Horizontal,
                _ => {}
            }
        }
        return AnimationTimeline::Scroll(source, axis);
    }

    if let Some(args) = trimmed
        .strip_prefix("view(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let args = args.trim();
        let mut axis = ScrollAxis::Block;
        let mut inset_start = 0.0;
        let mut inset_end = 0.0;

        for part in args.split_whitespace() {
            match part.to_ascii_lowercase().as_str() {
                "block" | "y" => axis = ScrollAxis::Block,
                "inline" | "x" => axis = ScrollAxis::Inline,
                "vertical" => axis = ScrollAxis::Vertical,
                "horizontal" => axis = ScrollAxis::Horizontal,
                other => {
                    if let Ok(px) = other.trim_end_matches("px").parse::<f32>() {
                        if inset_start == 0.0 {
                            inset_start = px;
                            inset_end = px;
                        } else {
                            inset_end = px;
                        }
                    } else if let Ok(pct) = other.trim_end_matches('%').parse::<f32>() {
                        if inset_start == 0.0 {
                            inset_start = pct / 100.0;
                            inset_end = pct / 100.0;
                        } else {
                            inset_end = pct / 100.0;
                        }
                    }
                }
            }
        }
        return AnimationTimeline::View(axis, inset_start, inset_end);
    }

    AnimationTimeline::Named(trimmed.to_string())
}

/// Parses a CSS `animation-range` property value per W3C Scroll-Driven Animations.
pub fn parse_animation_range(val: &str) -> AnimationRange {
    let trimmed = val.trim();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return AnimationRange::default();
    }

    let parse_name = |s: &str| match s.to_ascii_lowercase().as_str() {
        "normal" => AnimationRangeName::Normal,
        "cover" => AnimationRangeName::Cover,
        "contain" => AnimationRangeName::Contain,
        "entry" => AnimationRangeName::Entry,
        "exit" => AnimationRangeName::Exit,
        _ => AnimationRangeName::Normal,
    };

    let parse_offset = |s: &str| -> Option<f32> {
        if let Some(pct) = s.strip_suffix('%') {
            pct.parse::<f32>().map(|p| p / 100.0).ok()
        } else {
            s.parse::<f32>().ok()
        }
    };

    let name = parse_name(parts[0]);
    let mut start_offset = 0.0;
    let mut end_offset = 1.0;

    if parts.len() == 2 {
        if let Some(off) = parse_offset(parts[1]) {
            start_offset = off;
            end_offset = 1.0;
        }
    } else if parts.len() >= 3 {
        if let Some(s) = parse_offset(parts[1]) {
            start_offset = s;
        }
        if let Some(e) = parse_offset(parts[2]) {
            end_offset = e;
        }
    }

    AnimationRange {
        name,
        start_offset,
        end_offset,
    }
}

/// Parses a CSS `view-transition-name` property value per W3C View Transitions Module.
pub fn parse_view_transition_name(val: &str) -> Option<String> {
    let trimmed = val.trim();
    if trimmed.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Splits a comma-separated CSS selector list while respecting parentheses, brackets, and quotes.
pub fn split_selector_list(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev_idx = 0;

    for (idx, ch) in input.char_indices() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '(' if !in_single_quote && !in_double_quote => paren_depth += 1,
            ')' if !in_single_quote && !in_double_quote => {
                paren_depth = paren_depth.saturating_sub(1);
            }
            '[' if !in_single_quote && !in_double_quote => bracket_depth += 1,
            ']' if !in_single_quote && !in_double_quote => {
                bracket_depth = bracket_depth.saturating_sub(1);
            }
            ',' if !in_single_quote
                && !in_double_quote
                && paren_depth == 0
                && bracket_depth == 0 =>
            {
                result.push(&input[prev_idx..idx]);
                prev_idx = idx + 1;
            }
            _ => {}
        }
    }
    if prev_idx < input.len() {
        result.push(&input[prev_idx..]);
    }
    result
}

/// Evaluates a `@container` query condition against the active media and container context.
fn evaluate_container_query(query_str: &str, context: &crate::media::MediaContext) -> bool {
    let container_w = context.container_width.unwrap_or(context.viewport_width);
    let container_h = context.container_height.unwrap_or(context.viewport_height);

    let parts: Vec<&str> = query_str
        .split(" and ")
        .flat_map(|p| p.split(" AND "))
        .map(|s| s.trim())
        .collect();

    for part in parts {
        let trimmed = part.trim();
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            let inner = trimmed[1..trimmed.len() - 1].trim();
            if let Some((feat, val)) = inner.split_once(':') {
                let feat = feat.trim().to_ascii_lowercase();
                let val_str = val.trim();
                let px = if let Some(px_str) = val_str.strip_suffix("px") {
                    px_str.trim().parse::<f32>().unwrap_or(0.0)
                } else if let Some(rem_str) = val_str.strip_suffix("rem") {
                    rem_str.trim().parse::<f32>().unwrap_or(0.0) * 16.0
                } else {
                    val_str.parse::<f32>().unwrap_or(0.0)
                };

                match feat.as_str() {
                    "min-width" if container_w < px => return false,
                    "max-width" if container_w > px => return false,
                    "min-height" if container_h < px => return false,
                    "max-height" if container_h > px => return false,
                    "width" if (container_w - px).abs() > 0.5 => return false,
                    "height" if (container_h - px).abs() > 0.5 => return false,
                    _ => {}
                }
            }
        }
    }
    true
}
