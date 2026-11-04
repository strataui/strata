//! Reader appearance themes, color palettes, and typographic controls.

/// Configurable aesthetic theme and typographic scale for reading articles.
#[derive(Debug, Clone, PartialEq)]
pub struct ReaderTheme {
    /// Human-readable theme identifier.
    pub name: String,
    /// Tailwind class for page background.
    pub bg_class: String,
    /// Tailwind class for main reading text.
    pub text_class: String,
    /// Tailwind class for headers (`h1`-`h6`).
    pub heading_class: String,
    /// Tailwind class for links and interactive anchors.
    pub link_class: String,
    /// Tailwind class for dividers, table borders, and callout left lines.
    pub border_class: String,
    /// Tailwind class for infoboxes, sidebars, and cards.
    pub card_bg_class: String,
    /// Tailwind class for inline and block code snippets.
    pub code_bg_class: String,
    /// Relative font size multiplier (default 1.0).
    pub font_size_scale: f32,
    /// Max reading column width in pixels (e.g. 760).
    pub max_width_px: u32,
}

impl Default for ReaderTheme {
    fn default() -> Self {
        Self::dark()
    }
}

impl ReaderTheme {
    /// Clean high-contrast dark theme (default).
    pub fn dark() -> Self {
        Self {
            name: "Dark".to_string(),
            bg_class: "bg-neutral-950".to_string(),
            text_class: "text-slate-300".to_string(),
            heading_class: "text-slate-100".to_string(),
            link_class: "text-indigo-400 hover:text-indigo-300".to_string(),
            border_class: "border-slate-800".to_string(),
            card_bg_class: "bg-slate-900".to_string(),
            code_bg_class: "bg-slate-900".to_string(),
            font_size_scale: 1.0,
            max_width_px: 760,
        }
    }

    /// Pure pitch-black theme optimized for OLED/AMOLED displays.
    pub fn amoled() -> Self {
        Self {
            name: "Amoled".to_string(),
            bg_class: "bg-black".to_string(),
            text_class: "text-zinc-300".to_string(),
            heading_class: "text-zinc-100".to_string(),
            link_class: "text-blue-400 hover:text-blue-300".to_string(),
            border_class: "border-zinc-800".to_string(),
            card_bg_class: "bg-zinc-950".to_string(),
            code_bg_class: "bg-zinc-950".to_string(),
            font_size_scale: 1.0,
            max_width_px: 760,
        }
    }

    /// Warm, low-strain paper sepia theme for prolonged reading sessions.
    pub fn sepia() -> Self {
        Self {
            name: "Sepia".to_string(),
            bg_class: "bg-[#fbf0d9]".to_string(),
            text_class: "text-[#5f4b32]".to_string(),
            heading_class: "text-[#2c2214]".to_string(),
            link_class: "text-[#a35229] hover:text-[#7f3e1d]".to_string(),
            border_class: "border-[#e0d3bc]".to_string(),
            card_bg_class: "bg-[#f2e5c9]".to_string(),
            code_bg_class: "bg-[#eeddc0]".to_string(),
            font_size_scale: 1.0,
            max_width_px: 760,
        }
    }

    /// Classic crisp light theme.
    pub fn light() -> Self {
        Self {
            name: "Light".to_string(),
            bg_class: "bg-white".to_string(),
            text_class: "text-slate-700".to_string(),
            heading_class: "text-slate-900".to_string(),
            link_class: "text-blue-600 hover:text-blue-500".to_string(),
            border_class: "border-slate-200".to_string(),
            card_bg_class: "bg-slate-50".to_string(),
            code_bg_class: "bg-slate-100".to_string(),
            font_size_scale: 1.0,
            max_width_px: 760,
        }
    }

    /// Solarized dark palette with precise cyan and blue accents.
    pub fn solarized() -> Self {
        Self {
            name: "Solarized".to_string(),
            bg_class: "bg-[#002b36]".to_string(),
            text_class: "text-[#839496]".to_string(),
            heading_class: "text-[#93a1a1]".to_string(),
            link_class: "text-[#268bd2] hover:text-[#2aa198]".to_string(),
            border_class: "border-[#073642]".to_string(),
            card_bg_class: "bg-[#073642]".to_string(),
            code_bg_class: "bg-[#073642]".to_string(),
            font_size_scale: 1.0,
            max_width_px: 760,
        }
    }

    /// Resolves the primary canvas background color as RGBA.
    pub fn bg_color_rgba(&self) -> strata_primitives::color::Rgba {
        match self.name.as_str() {
            "Amoled" => strata_primitives::color::Rgba::new(0, 0, 0, 255),
            "Sepia" => strata_primitives::color::Rgba::new(251, 240, 217, 255),
            "Light" => strata_primitives::color::Rgba::new(255, 255, 255, 255),
            "Solarized" => strata_primitives::color::Rgba::new(0, 43, 54, 255),
            _ => strata_primitives::color::Rgba::new(10, 10, 10, 255), // Dark
        }
    }

    /// Resolves the default text color as RGBA.
    pub fn text_color_rgba(&self) -> strata_primitives::color::Rgba {
        match self.name.as_str() {
            "Amoled" => strata_primitives::color::Rgba::new(212, 212, 216, 255),
            "Sepia" => strata_primitives::color::Rgba::new(95, 75, 50, 255),
            "Light" => strata_primitives::color::Rgba::new(51, 65, 85, 255),
            "Solarized" => strata_primitives::color::Rgba::new(131, 148, 150, 255),
            _ => strata_primitives::color::Rgba::new(203, 213, 225, 255), // Dark
        }
    }
}
