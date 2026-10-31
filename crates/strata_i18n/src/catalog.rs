//! Translation Catalog and Dictionary Management.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::icu::{IcuMessage, IcuValue};
use crate::locale::LocaleId;

/// Thread-safe in-memory translation catalog.
#[derive(Debug, Clone, Default)]
pub struct TranslationCatalog {
    inner: Arc<RwLock<CatalogInner>>,
}

#[derive(Debug, Default)]
struct CatalogInner {
    // locale -> (key -> compiled ICU message)
    locales: HashMap<LocaleId, HashMap<String, IcuMessage>>,
    default_locale: Option<LocaleId>,
}

impl TranslationCatalog {
    /// Create a new empty translation catalog.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(CatalogInner {
                locales: HashMap::new(),
                default_locale: Some(LocaleId::en_us()),
            })),
        }
    }

    /// Set default fallback locale.
    pub fn set_default_locale(&self, locale: LocaleId) {
        let mut inner = self.inner.write().unwrap();
        inner.default_locale = Some(locale);
    }

    /// Register a single translation template for a locale.
    pub fn register(&self, locale: &LocaleId, key: &str, template: &str) -> Result<(), String> {
        let msg = IcuMessage::parse(template)?;
        let mut inner = self.inner.write().unwrap();
        inner
            .locales
            .entry(locale.clone())
            .or_default()
            .insert(key.to_string(), msg);
        Ok(())
    }

    /// Bulk register key-value translations for a locale.
    pub fn register_bulk<'a, I>(&self, locale: &LocaleId, entries: I) -> Result<(), String>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        for (k, v) in entries {
            self.register(locale, k, v)?;
        }
        Ok(())
    }

    /// Translate a key with given arguments and requested locale.
    pub fn translate(
        &self,
        key: &str,
        args: &HashMap<String, IcuValue>,
        locale: &LocaleId,
    ) -> String {
        let inner = self.inner.read().unwrap();

        // 1. Try exact locale and its fallback chain
        for loc in locale.fallback_chain() {
            if let Some(dict) = inner.locales.get(&loc) {
                if let Some(msg) = dict.get(key) {
                    return msg.format(args, loc.language());
                }
            }
        }

        // 2. Try default fallback locale if configured
        if let Some(ref def_loc) = inner.default_locale {
            if def_loc != locale {
                for loc in def_loc.fallback_chain() {
                    if let Some(dict) = inner.locales.get(&loc) {
                        if let Some(msg) = dict.get(key) {
                            return msg.format(args, loc.language());
                        }
                    }
                }
            }
        }

        // 3. Fallback to key itself if not found
        key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_translations_and_fallbacks() {
        let catalog = TranslationCatalog::new();
        let en = LocaleId::en_us();
        let de = LocaleId::de_de();

        catalog
            .register(&en, "greeting", "Hello, {name}!")
            .unwrap();
        catalog
            .register(&en, "items", "{count, plural, one {# item} other {# items}}")
            .unwrap();
        catalog
            .register(&de, "greeting", "Hallo, {name}!")
            .unwrap();

        let mut args = HashMap::new();
        args.insert("name".to_string(), "Sam".into());

        assert_eq!(catalog.translate("greeting", &args, &en), "Hello, Sam!");
        assert_eq!(catalog.translate("greeting", &args, &de), "Hallo, Sam!");

        // Missing key in German falls back to English default
        args.insert("count".to_string(), 5.into());
        assert_eq!(catalog.translate("items", &args, &de), "5 items");
    }
}
