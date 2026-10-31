//! Strata I18n: Reactive Internationalization, ICU MessageFormat, and Localization.
//!
//! Provides a pure-Rust, zero-dependency equivalent to modern React i18n libraries
//! (`react-i18next`, `formatjs`, `react-intl`) with fine-grained reactive signals,
//! CLDR pluralization, ICU message parsing, localized formatters, and Right-to-Left (RTL) detection.

pub mod catalog;
pub mod context;
pub mod formatters;
pub mod icu;
pub mod locale;

pub use catalog::TranslationCatalog;
pub use context::{init_i18n, set_locale, translate_key, use_i18n, use_locale, I18nContext};
pub use formatters::{
    format_currency, format_number, format_percent, format_relative_time, NumberFormatOptions,
    TimeUnit,
};
pub use icu::{IcuMessage, IcuNode, IcuValue, PluralCategory};
pub use locale::LocaleId;

pub mod prelude {
    pub use crate::catalog::TranslationCatalog;
    pub use crate::context::{
        init_i18n, set_locale, translate_key, use_i18n, use_locale, I18nContext,
    };
    pub use crate::formatters::{
        format_currency, format_number, format_percent, format_relative_time, NumberFormatOptions,
        TimeUnit,
    };
    pub use crate::icu::{IcuMessage, IcuValue, PluralCategory};
    pub use crate::locale::LocaleId;
    pub use crate::t;
}
