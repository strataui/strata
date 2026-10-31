//! BCP 47 Locale Identifier and Directionality Engine.

use std::fmt;
use std::str::FromStr;

/// Standard BCP 47 Locale Identifier (e.g., `en-US`, `de-DE`, `ja-JP`, `ar-SA`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocaleId {
    language: String,
    script: Option<String>,
    region: Option<String>,
}

impl LocaleId {
    /// Create a new `LocaleId` with language, optional script, and optional region.
    pub fn new(language: impl Into<String>, script: Option<String>, region: Option<String>) -> Self {
        Self {
            language: language.into().to_ascii_lowercase(),
            script: script.map(|s| {
                let mut c = s.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_ascii_lowercase(),
                }
            }),
            region: region.map(|r| r.to_ascii_uppercase()),
        }
    }

    /// English (United States) - Default fallback
    pub fn en_us() -> Self {
        Self::new("en", None, Some("US".into()))
    }

    /// English (Generic)
    pub fn en() -> Self {
        Self::new("en", None, None)
    }

    /// German (Germany)
    pub fn de_de() -> Self {
        Self::new("de", None, Some("DE".into()))
    }

    /// French (France)
    pub fn fr_fr() -> Self {
        Self::new("fr", None, Some("FR".into()))
    }

    /// Japanese (Japan)
    pub fn ja_jp() -> Self {
        Self::new("ja", None, Some("JP".into()))
    }

    /// Arabic (Saudi Arabia - Right-to-Left)
    pub fn ar_sa() -> Self {
        Self::new("ar", None, Some("SA".into()))
    }

    /// Hebrew (Israel - Right-to-Left)
    pub fn he_il() -> Self {
        Self::new("he", None, Some("IL".into()))
    }

    /// Parse a BCP 47 string or POSIX locale (e.g., `en-US`, `en_US`, `zh-Hans-CN`).
    pub fn parse(s: &str) -> Result<Self, String> {
        let clean = s.trim().replace('_', "-");
        if clean.is_empty() {
            return Err("Locale string cannot be empty".into());
        }

        let parts: Vec<&str> = clean.split('-').collect();
        let lang = parts[0];
        if !lang.chars().all(|c| c.is_ascii_alphabetic()) || lang.len() < 2 || lang.len() > 3 {
            return Err(format!("Invalid language subtag: '{lang}'"));
        }

        let mut script = None;
        let mut region = None;

        for part in &parts[1..] {
            if part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic()) {
                script = Some(part.to_string());
            } else if (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
                || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
            {
                region = Some(part.to_string());
            }
        }

        Ok(Self::new(lang, script, region))
    }

    /// Get primary language subtag.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Get optional script subtag (e.g., `Hans`, `Latn`).
    pub fn script(&self) -> Option<&str> {
        self.script.as_deref()
    }

    /// Get optional region subtag (e.g., `US`, `DE`, `GB`).
    pub fn region(&self) -> Option<&str> {
        self.region.as_deref()
    }

    /// Whether this locale uses a Right-to-Left (RTL) writing system.
    pub fn is_rtl(&self) -> bool {
        matches!(
            self.language.as_str(),
            "ar" | "he" | "fa" | "ur" | "yi" | "ps" | "sd" | "ug"
        )
    }

    /// Candidate fallback chain for lookup (e.g., `de-CH` -> `de`).
    pub fn fallback_chain(&self) -> Vec<Self> {
        let mut chain = Vec::new();
        chain.push(self.clone());

        if self.region.is_some() || self.script.is_some() {
            chain.push(Self::new(&self.language, None, None));
        }

        chain
    }
}

impl fmt::Display for LocaleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.language)?;
        if let Some(ref script) = self.script {
            write!(f, "-{script}")?;
        }
        if let Some(ref region) = self.region {
            write!(f, "-{region}")?;
        }
        Ok(())
    }
}

impl FromStr for LocaleId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_parsing_and_formatting() {
        let loc = LocaleId::parse("en-US").unwrap();
        assert_eq!(loc.language(), "en");
        assert_eq!(loc.region(), Some("US"));
        assert!(!loc.is_rtl());
        assert_eq!(loc.to_string(), "en-US");

        let posix = LocaleId::parse("de_DE").unwrap();
        assert_eq!(posix.language(), "de");
        assert_eq!(posix.region(), Some("DE"));
        assert_eq!(posix.to_string(), "de-DE");

        let script_loc = LocaleId::parse("zh-Hans-CN").unwrap();
        assert_eq!(script_loc.language(), "zh");
        assert_eq!(script_loc.script(), Some("Hans"));
        assert_eq!(script_loc.region(), Some("CN"));
        assert_eq!(script_loc.to_string(), "zh-Hans-CN");
    }

    #[test]
    fn test_rtl_detection() {
        assert!(LocaleId::parse("ar-SA").unwrap().is_rtl());
        assert!(LocaleId::parse("he-IL").unwrap().is_rtl());
        assert!(LocaleId::parse("fa-IR").unwrap().is_rtl());
        assert!(!LocaleId::parse("en-GB").unwrap().is_rtl());
        assert!(!LocaleId::parse("fr-FR").unwrap().is_rtl());
    }

    #[test]
    fn test_fallback_chain() {
        let loc = LocaleId::parse("de-CH").unwrap();
        let chain = loc.fallback_chain();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].to_string(), "de-CH");
        assert_eq!(chain[1].to_string(), "de");
    }
}
