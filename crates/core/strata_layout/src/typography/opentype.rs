//! Pure Rust OpenType / TrueType font parser, metrics extractor, and font database.
//!
//! Parses sfnt container tables: `head`, `hhea`, `maxp`, `OS/2`, `hmtx`, `cmap` (Formats 4 & 12),
//! and `kern` (Format 0) directly from binary font data without external dependencies.
//! Provides authentic font face metrics, glyph advance widths, and optical pair kerning.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use strata_primitives::geom::LayoutUnit;
use strata_primitives::AtomId;
use strata_style::types::{FontStyle, FontWeight};

use super::font_metrics::FontMetrics;

/// Errors that can occur when parsing OpenType / TrueType font files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenTypeParseError {
    UnexpectedEof,
    InvalidSignature(u32),
    TableNotFound(&'static str),
    InvalidTableData(&'static str),
}

impl std::fmt::Display for OpenTypeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "Unexpected EOF while parsing font binary"),
            Self::InvalidSignature(sig) => write!(f, "Invalid sfnt signature: 0x{:08X}", sig),
            Self::TableNotFound(tag) => write!(f, "Required font table not found: {}", tag),
            Self::InvalidTableData(msg) => write!(f, "Invalid table data: {}", msg),
        }
    }
}

impl std::error::Error for OpenTypeParseError {}

#[inline(always)]
fn read_u16_be(data: &[u8], offset: usize) -> Option<u16> {
    if offset + 2 <= data.len() {
        Some(u16::from_be_bytes([data[offset], data[offset + 1]]))
    } else {
        None
    }
}

#[inline(always)]
fn read_i16_be(data: &[u8], offset: usize) -> Option<i16> {
    if offset + 2 <= data.len() {
        Some(i16::from_be_bytes([data[offset], data[offset + 1]]))
    } else {
        None
    }
}

#[inline(always)]
fn read_u32_be(data: &[u8], offset: usize) -> Option<u32> {
    if offset + 4 <= data.len() {
        Some(u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]))
    } else {
        None
    }
}

/// Parsed OpenType / TrueType font face.
#[derive(Clone)]
pub struct OpenTypeFont {
    data: Arc<[u8]>,
    pub units_per_em: u16,
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
    pub typo_ascender: i16,
    pub typo_descender: i16,
    pub typo_line_gap: i16,
    pub x_height: Option<i16>,
    pub cap_height: Option<i16>,
    pub weight_class: u16,
    pub is_italic: bool,
    pub is_monospace: bool,
    pub num_glyphs: u16,
    pub number_of_h_metrics: u16,
    hmtx_offset: usize,
    cmap_subtable_offset: usize,
    cmap_format: u16,
    kern_subtable_offset: Option<usize>,
    pub gsub_offset: Option<usize>,
    pub supported_features: Vec<[u8; 4]>,
}

impl std::fmt::Debug for OpenTypeFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenTypeFont")
            .field("units_per_em", &self.units_per_em)
            .field("ascender", &self.ascender)
            .field("descender", &self.descender)
            .field("line_gap", &self.line_gap)
            .field("weight_class", &self.weight_class)
            .field("is_italic", &self.is_italic)
            .field("is_monospace", &self.is_monospace)
            .field("num_glyphs", &self.num_glyphs)
            .finish()
    }
}

impl OpenTypeFont {
    /// Parses an OpenType or TrueType font from memory.
    pub fn parse(bytes: &[u8]) -> Result<Self, OpenTypeParseError> {
        if bytes.len() < 12 {
            return Err(OpenTypeParseError::UnexpectedEof);
        }

        let sfnt_version = read_u32_be(bytes, 0).ok_or(OpenTypeParseError::UnexpectedEof)?;
        // TrueType 1.0 (0x00010000), OpenType CFF (0x4F54544F 'OTTO'), Apple TrueType (0x74727565 'true'), typ1 (0x74797031)
        if sfnt_version != 0x00010000
            && sfnt_version != 0x4F54544F
            && sfnt_version != 0x74727565
            && sfnt_version != 0x74797031
        {
            return Err(OpenTypeParseError::InvalidSignature(sfnt_version));
        }

        let num_tables = read_u16_be(bytes, 4).ok_or(OpenTypeParseError::UnexpectedEof)? as usize;
        if bytes.len() < 12 + num_tables * 16 {
            return Err(OpenTypeParseError::UnexpectedEof);
        }

        let mut head_offset = None;
        let mut hhea_offset = None;
        let mut maxp_offset = None;
        let mut os2_offset = None;
        let mut hmtx_offset = None;
        let mut cmap_offset = None;
        let mut kern_offset = None;
        let mut gsub_offset = None;

        for i in 0..num_tables {
            let entry_offset = 12 + i * 16;
            let tag = &bytes[entry_offset..entry_offset + 4];
            let offset = read_u32_be(bytes, entry_offset + 8).unwrap() as usize;

            match tag {
                b"head" => head_offset = Some(offset),
                b"hhea" => hhea_offset = Some(offset),
                b"maxp" => maxp_offset = Some(offset),
                b"OS/2" => os2_offset = Some(offset),
                b"hmtx" => hmtx_offset = Some(offset),
                b"cmap" => cmap_offset = Some(offset),
                b"kern" => kern_offset = Some(offset),
                b"GSUB" => gsub_offset = Some(offset),
                _ => {}
            }
        }

        let head_off = head_offset.ok_or(OpenTypeParseError::TableNotFound("head"))?;
        let hhea_off = hhea_offset.ok_or(OpenTypeParseError::TableNotFound("hhea"))?;
        let hmtx_off = hmtx_offset.ok_or(OpenTypeParseError::TableNotFound("hmtx"))?;
        let cmap_off = cmap_offset.ok_or(OpenTypeParseError::TableNotFound("cmap"))?;

        // 1. Read 'head'
        let units_per_em = read_u16_be(bytes, head_off + 18).unwrap_or(1000).max(16);
        let mac_style = read_u16_be(bytes, head_off + 44).unwrap_or(0);
        let head_bold = (mac_style & 1) != 0;
        let head_italic = (mac_style & 2) != 0;

        // 2. Read 'hhea'
        let ascender = read_i16_be(bytes, hhea_off + 4).unwrap_or(units_per_em as i16 * 4 / 5);
        let descender = read_i16_be(bytes, hhea_off + 6).unwrap_or(-(units_per_em as i16 / 5));
        let line_gap = read_i16_be(bytes, hhea_off + 8).unwrap_or(0);
        let number_of_h_metrics = read_u16_be(bytes, hhea_off + 34).unwrap_or(1);

        // 3. Read 'maxp'
        let num_glyphs = if let Some(off) = maxp_offset {
            read_u16_be(bytes, off + 4).unwrap_or(1)
        } else {
            number_of_h_metrics
        };

        // 4. Read 'OS/2' (if present)
        let mut weight_class = if head_bold { 700 } else { 400 };
        let mut is_italic = head_italic;
        let mut is_monospace = false;
        let mut typo_ascender = ascender;
        let mut typo_descender = descender;
        let mut typo_line_gap = line_gap;
        let mut x_height = None;
        let mut cap_height = None;

        if let Some(off) = os2_offset {
            if let Some(version) = read_u16_be(bytes, off) {
                if let Some(w) = read_u16_be(bytes, off + 4) {
                    if w > 0 {
                        weight_class = w;
                    }
                }
                if let Some(panose4) = bytes.get(off + 35) {
                    if *panose4 == 9 {
                        is_monospace = true;
                    }
                }
                let use_typo_metrics = if let Some(sel) = read_u16_be(bytes, off + 62) {
                    if (sel & 1) != 0 {
                        is_italic = true;
                    }
                    (sel & 0x0080) != 0
                } else {
                    false
                };
                if let Some(t_asc) = read_i16_be(bytes, off + 68) {
                    if (use_typo_metrics || typo_ascender == 0) && t_asc != 0 {
                        typo_ascender = t_asc;
                    }
                }
                if let Some(t_desc) = read_i16_be(bytes, off + 70) {
                    if (use_typo_metrics || typo_descender == 0) && t_desc != 0 {
                        typo_descender = t_desc;
                    }
                }
                if let Some(t_gap) = read_i16_be(bytes, off + 72) {
                    if use_typo_metrics || typo_line_gap == 0 {
                        typo_line_gap = t_gap;
                    }
                }
                if version >= 2 {
                    if let Some(xh) = read_i16_be(bytes, off + 86) {
                        if xh > 0 {
                            x_height = Some(xh);
                        }
                    }
                    if let Some(ch) = read_i16_be(bytes, off + 88) {
                        if ch > 0 {
                            cap_height = Some(ch);
                        }
                    }
                }
            }
        }

        // 5. Read 'cmap'
        let num_cmap_subtables =
            read_u16_be(bytes, cmap_off + 2).ok_or(OpenTypeParseError::UnexpectedEof)? as usize;
        let mut chosen_subtable = None;
        let mut chosen_format = 0;

        for s in 0..num_cmap_subtables {
            let record_off = cmap_off + 4 + s * 8;
            let platform_id = read_u16_be(bytes, record_off).unwrap_or(0);
            let encoding_id = read_u16_be(bytes, record_off + 2).unwrap_or(0);
            let sub_off = cmap_off + read_u32_be(bytes, record_off + 4).unwrap_or(0) as usize;

            let format = read_u16_be(bytes, sub_off).unwrap_or(0);
            // Format 12 (Unicode 32-bit UCS-4) is preferred
            if format == 12 && (platform_id == 0 || (platform_id == 3 && encoding_id == 10)) {
                chosen_subtable = Some(sub_off);
                chosen_format = 12;
                break;
            }
            // Format 4 (Unicode 16-bit BMP)
            if format == 4
                && (platform_id == 0 || (platform_id == 3 && (encoding_id == 1 || encoding_id == 0)))
                && chosen_format != 12
            {
                chosen_subtable = Some(sub_off);
                chosen_format = 4;
            }
        }

        let (cmap_subtable_offset, cmap_format) = match chosen_subtable {
            Some(off) => (off, chosen_format),
            None => return Err(OpenTypeParseError::TableNotFound("cmap Unicode subtable")),
        };

        // 6. Read 'kern' table (Format 0 pair kerning)
        let mut kern_subtable_offset = None;
        if let Some(k_off) = kern_offset {
            let version = read_u16_be(bytes, k_off).unwrap_or(1);
            let n_tables = read_u16_be(bytes, k_off + 2).unwrap_or(0);
            if version == 0 && n_tables > 0 {
                let sub_off = k_off + 4;
                let sub_version = read_u16_be(bytes, sub_off).unwrap_or(0);
                let coverage = read_u16_be(bytes, sub_off + 4).unwrap_or(0);
                // Format 0 (horizontal, kerning)
                if sub_version == 0 && (coverage & 1) != 0 && (coverage >> 8) == 0 {
                    kern_subtable_offset = Some(sub_off + 6);
                }
            }
        }

        // 7. Read 'GSUB' table (Glyph Substitution features: liga, calt, dlig, smcp)
        let mut supported_features = Vec::new();
        if let Some(gsub_off) = gsub_offset {
            if let Some(feature_list_rel) = read_u16_be(bytes, gsub_off + 6) {
                let feature_list_off = gsub_off + feature_list_rel as usize;
                if let Some(feature_count) = read_u16_be(bytes, feature_list_off) {
                    for fi in 0..feature_count as usize {
                        let rec_off = feature_list_off + 2 + fi * 6;
                        if rec_off + 4 <= bytes.len() {
                            let tag: [u8; 4] = [
                                bytes[rec_off],
                                bytes[rec_off + 1],
                                bytes[rec_off + 2],
                                bytes[rec_off + 3],
                            ];
                            if !supported_features.contains(&tag) {
                                supported_features.push(tag);
                            }
                        }
                    }
                }
            }
        }

        Ok(Self {
            data: Arc::from(bytes.to_vec()),
            units_per_em,
            ascender,
            descender,
            line_gap,
            typo_ascender,
            typo_descender,
            typo_line_gap,
            x_height,
            cap_height,
            weight_class,
            is_italic,
            is_monospace,
            num_glyphs,
            number_of_h_metrics,
            hmtx_offset: hmtx_off,
            cmap_subtable_offset,
            cmap_format,
            kern_subtable_offset,
            gsub_offset,
            supported_features,
        })
    }

    /// Checks whether this font supports a specific OpenType feature tag (e.g. `*b"liga"`, `*b"calt"`, `*b"dlig"`, `*b"smcp"`).
    #[inline]
    pub fn has_feature(&self, tag: &[u8; 4]) -> bool {
        self.supported_features.contains(tag)
    }

    /// Resolves character to glyph ID via cmap subtable.
    pub fn glyph_id(&self, c: char) -> u16 {
        let cp = c as u32;
        if self.cmap_format == 12 {
            // Format 12: 32-bit sequential groups
            let num_groups = read_u32_be(&self.data, self.cmap_subtable_offset + 12).unwrap_or(0) as usize;
            let groups_start = self.cmap_subtable_offset + 16;

            let mut low = 0;
            let mut high = num_groups;
            while low < high {
                let mid = (low + high) / 2;
                let entry_off = groups_start + mid * 12;
                let start_char = read_u32_be(&self.data, entry_off).unwrap_or(u32::MAX);
                let end_char = read_u32_be(&self.data, entry_off + 4).unwrap_or(0);
                let start_glyph = read_u32_be(&self.data, entry_off + 8).unwrap_or(0);

                if cp < start_char {
                    high = mid;
                } else if cp > end_char {
                    low = mid + 1;
                } else {
                    return (start_glyph + (cp - start_char)) as u16;
                }
            }
            0
        } else if self.cmap_format == 4 {
            if cp > 0xFFFF {
                return 0;
            }
            let seg_count =
                (read_u16_be(&self.data, self.cmap_subtable_offset + 6).unwrap_or(0) / 2) as usize;
            if seg_count == 0 {
                return 0;
            }

            let end_codes_start = self.cmap_subtable_offset + 14;
            let start_codes_start = end_codes_start + seg_count * 2 + 2;
            let id_deltas_start = start_codes_start + seg_count * 2;
            let id_range_offsets_start = id_deltas_start + seg_count * 2;

            for i in 0..seg_count {
                let end_code = read_u16_be(&self.data, end_codes_start + i * 2).unwrap_or(0) as u32;
                if end_code >= cp {
                    let start_code =
                        read_u16_be(&self.data, start_codes_start + i * 2).unwrap_or(0) as u32;
                    if start_code <= cp {
                        let id_delta =
                            read_i16_be(&self.data, id_deltas_start + i * 2).unwrap_or(0) as i32;
                        let id_range_offset =
                            read_u16_be(&self.data, id_range_offsets_start + i * 2).unwrap_or(0)
                                as usize;

                        if id_range_offset == 0 {
                            return ((cp as i32 + id_delta) & 0xFFFF) as u16;
                        } else {
                            let glyph_offset = id_range_offsets_start
                                + i * 2
                                + id_range_offset
                                + (cp - start_code) as usize * 2;
                            if let Some(glyph_id) = read_u16_be(&self.data, glyph_offset) {
                                if glyph_id != 0 {
                                    return ((glyph_id as i32 + id_delta) & 0xFFFF) as u16;
                                }
                            }
                        }
                    }
                    break;
                }
            }
            0
        } else {
            0
        }
    }

    /// Retrieves glyph advance width in font design units from `hmtx`.
    #[inline]
    pub fn glyph_advance_units(&self, gid: u16) -> u16 {
        if self.number_of_h_metrics == 0 {
            return self.units_per_em;
        }
        if gid < self.number_of_h_metrics {
            let offset = self.hmtx_offset + (gid as usize) * 4;
            read_u16_be(&self.data, offset).unwrap_or(self.units_per_em)
        } else {
            let offset = self.hmtx_offset + (self.number_of_h_metrics as usize - 1) * 4;
            read_u16_be(&self.data, offset).unwrap_or(self.units_per_em)
        }
    }

    /// Advance width in ems (fraction of font size).
    #[inline]
    pub fn glyph_advance_em(&self, gid: u16) -> f32 {
        self.glyph_advance_units(gid) as f32 / self.units_per_em.max(1) as f32
    }

    /// Advance width in CSS pixels for a given font size.
    #[inline]
    pub fn glyph_advance(&self, gid: u16, font_size: f32) -> f32 {
        self.glyph_advance_em(gid) * font_size
    }

    /// Computes advance width in CSS pixels for a character.
    #[inline]
    pub fn char_advance(&self, c: char, font_size: f32) -> f32 {
        let gid = self.glyph_id(c);
        self.glyph_advance(gid, font_size)
    }

    /// Retrieves pair kerning adjustment between two glyphs in font design units.
    pub fn pair_kerning_units(&self, left_gid: u16, right_gid: u16) -> i16 {
        let sub_off = match self.kern_subtable_offset {
            Some(off) => off,
            None => return 0,
        };

        let n_pairs = read_u16_be(&self.data, sub_off).unwrap_or(0) as usize;
        let pairs_start = sub_off + 8;
        let target_pair = ((left_gid as u32) << 16) | (right_gid as u32);

        let mut low = 0;
        let mut high = n_pairs;
        while low < high {
            let mid = (low + high) / 2;
            let pair_off = pairs_start + mid * 6;
            let pair = read_u32_be(&self.data, pair_off).unwrap_or(u32::MAX);
            if pair < target_pair {
                low = mid + 1;
            } else if pair > target_pair {
                high = mid;
            } else {
                return read_i16_be(&self.data, pair_off + 4).unwrap_or(0);
            }
        }
        0
    }

    /// Pair kerning adjustment in ems.
    #[inline]
    pub fn pair_kerning_em(&self, left_gid: u16, right_gid: u16) -> f32 {
        self.pair_kerning_units(left_gid, right_gid) as f32 / self.units_per_em.max(1) as f32
    }

    /// Pair kerning adjustment in CSS pixels for a given font size.
    #[inline]
    pub fn pair_kerning(&self, left_gid: u16, right_gid: u16, font_size: f32) -> f32 {
        self.pair_kerning_em(left_gid, right_gid) * font_size
    }

    /// Generates exact `FontMetrics` scaled to CSS pixels.
    pub fn font_metrics(&self, font_size: f32) -> FontMetrics {
        let scale = font_size / self.units_per_em.max(1) as f32;
        let ascent = ((self.typo_ascender as f32 * scale).max(0.0) * LayoutUnit::SCALE).round() / LayoutUnit::SCALE;
        let descent = (((-self.typo_descender).max(0) as f32 * scale) * LayoutUnit::SCALE).round() / LayoutUnit::SCALE;
        let line_gap = ((self.typo_line_gap.max(0) as f32 * scale) * LayoutUnit::SCALE).round() / LayoutUnit::SCALE;

        let x_height = match self.x_height {
            Some(xh) if xh > 0 => ((xh as f32 * scale) * LayoutUnit::SCALE).round() / LayoutUnit::SCALE,
            _ => (ascent * 0.52 * LayoutUnit::SCALE).round() / LayoutUnit::SCALE,
        };

        let cap_height = match self.cap_height {
            Some(ch) if ch > 0 => ((ch as f32 * scale) * LayoutUnit::SCALE).round() / LayoutUnit::SCALE,
            _ => (ascent * 0.72 * LayoutUnit::SCALE).round() / LayoutUnit::SCALE,
        };

        FontMetrics {
            ascent: LayoutUnit::from_f32(ascent),
            descent: LayoutUnit::from_f32(descent),
            line_gap: LayoutUnit::from_f32(line_gap),
            x_height: LayoutUnit::from_f32(x_height),
            cap_height: LayoutUnit::from_f32(cap_height),
            units_per_em: self.units_per_em,
            font_size,
        }
    }
}

/// Global Thread-Safe Font Database managing OpenType font faces.
pub struct FontDatabase {
    fonts: RwLock<HashMap<(String, u16, bool), Arc<OpenTypeFont>>>,
}

impl Default for FontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl FontDatabase {
    pub fn new() -> Self {
        let db = Self {
            fonts: RwLock::new(HashMap::new()),
        };
        db.load_standard_system_fonts();
        db
    }

    /// Registers font data for a specific family name.
    pub fn register_font_data(
        &self,
        family: &str,
        data: &[u8],
    ) -> Result<Arc<OpenTypeFont>, OpenTypeParseError> {
        let font = Arc::new(OpenTypeFont::parse(data)?);
        let key = (
            family.to_lowercase(),
            font.weight_class,
            font.is_italic,
        );
        if let Ok(mut lock) = self.fonts.write() {
            lock.insert(key, font.clone());
        }
        Ok(font)
    }

    /// Queries the best matching OpenType font face for family, weight, and style.
    pub fn query_font(
        &self,
        family_name: &str,
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<Arc<OpenTypeFont>> {
        let target_weight = match weight {
            FontWeight::Thin => 100,
            FontWeight::ExtraLight => 200,
            FontWeight::Light => 300,
            FontWeight::Normal => 400,
            FontWeight::Medium => 500,
            FontWeight::SemiBold => 600,
            FontWeight::Bold => 700,
            FontWeight::ExtraBold => 800,
            FontWeight::Black => 900,
        };
        let is_italic = matches!(style, FontStyle::Italic | FontStyle::Oblique);
        let family_lower = family_name.to_lowercase();

        if let Ok(lock) = self.fonts.read() {
            // Exact match
            if let Some(font) = lock.get(&(family_lower.clone(), target_weight, is_italic)) {
                return Some(font.clone());
            }

            // Closest weight match with same italic
            let mut best_match: Option<(u16, Arc<OpenTypeFont>)> = None;
            for ((fam, w, it), font) in lock.iter() {
                if fam == &family_lower && *it == is_italic {
                    let diff = (*w as i32 - target_weight as i32).unsigned_abs() as u16;
                    match best_match {
                        Some((best_diff, _)) if diff < best_diff => {
                            best_match = Some((diff, font.clone()));
                        }
                        None => {
                            best_match = Some((diff, font.clone()));
                        }
                        _ => {}
                    }
                }
            }
            if let Some((_, font)) = best_match {
                return Some(font);
            }

            // Fallback: any font for family
            for ((fam, _, _), font) in lock.iter() {
                if fam == &family_lower {
                    return Some(font.clone());
                }
            }
        }

        // Try dynamically loading from disk
        if let Some(font) = self.try_load_system_font(&family_lower, target_weight, is_italic) {
            return Some(font);
        }

        None
    }

    fn try_load_system_font(
        &self,
        family: &str,
        target_weight: u16,
        is_italic: bool,
    ) -> Option<Arc<OpenTypeFont>> {
        let is_bold = target_weight >= 600;
        let candidates = match family {
            "dejavu sans" | "sans-serif" | "sans" => {
                if is_bold && is_italic {
                    vec!["/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf"]
                } else if is_bold {
                    vec!["/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"]
                } else if is_italic {
                    vec!["/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf"]
                } else {
                    vec!["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]
                }
            }
            "dejavu sans mono" | "monospace" | "mono" => {
                if is_bold {
                    vec!["/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"]
                } else {
                    vec!["/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"]
                }
            }
            "dejavu serif" | "serif" => {
                if is_bold {
                    vec!["/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf"]
                } else {
                    vec!["/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf"]
                }
            }
            _ => vec![],
        };

        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(font) = self.register_font_data(family, &bytes) {
                    return Some(font);
                }
            }
        }
        None
    }

    fn load_standard_system_fonts(&self) {
        let system_fonts = [
            ("dejavu sans", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
            ("dejavu sans", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
            ("dejavu sans mono", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
            ("dejavu sans mono", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"),
            ("dejavu serif", "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf"),
            ("dejavu serif", "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf"),
        ];

        for (family, path) in system_fonts {
            if let Ok(bytes) = std::fs::read(path) {
                let _ = self.register_font_data(family, &bytes);
            }
        }
    }
}

/// Global shared font database instance.
static GLOBAL_FONT_DATABASE: OnceLock<FontDatabase> = OnceLock::new();

/// Returns reference to the global font database.
pub fn font_database() -> &'static FontDatabase {
    GLOBAL_FONT_DATABASE.get_or_init(FontDatabase::new)
}

/// Helper to lookup an OpenType font for an AtomId font family.
pub fn lookup_font_for_style(
    family: AtomId,
    weight: FontWeight,
    style: FontStyle,
) -> Option<Arc<OpenTypeFont>> {
    let db = font_database();
    if let Some(family_name) = strata_primitives::atom::resolve(family) {
        if let Some(font) = db.query_font(&family_name, weight, style) {
            return Some(font);
        }
    }

    if family == AtomId::FONT_MONO {
        if let Some(font) = db.query_font("dejavu sans mono", weight, style) {
            return Some(font);
        }
    }

    db.query_font("dejavu sans", weight, style)
}
