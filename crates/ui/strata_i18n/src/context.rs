//! Reactive Internationalization Context and Hooks.

use std::cell::RefCell;
use std::collections::HashMap;
use strata_dom::reactive::Signal;

use crate::catalog::TranslationCatalog;
use crate::icu::IcuValue;
use crate::locale::LocaleId;

thread_local! {
    static CURRENT_CONTEXT: RefCell<Option<I18nContext>> = const { RefCell::new(None) };
}

/// Reactive Internationalization Context.
#[derive(Clone)]
pub struct I18nContext {
    locale_signal: Signal<LocaleId>,
    catalog: TranslationCatalog,
}

impl I18nContext {
    pub fn new(initial_locale: LocaleId, catalog: TranslationCatalog) -> Self {
        catalog.set_default_locale(initial_locale.clone());
        Self {
            locale_signal: Signal::new(initial_locale),
            catalog,
        }
    }

    pub fn locale(&self) -> Signal<LocaleId> {
        self.locale_signal.clone()
    }

    pub fn set_locale(&self, new_locale: LocaleId) {
        self.locale_signal.set(new_locale);
    }

    pub fn catalog(&self) -> &TranslationCatalog {
        &self.catalog
    }

    pub fn translate(&self, key: &str, args: &HashMap<String, IcuValue>) -> String {
        let active_locale = self.locale_signal.get();
        self.catalog.translate(key, args, &active_locale)
    }
}

/// Initialize the thread-local reactive i18n context.
pub fn init_i18n(initial_locale: LocaleId, catalog: TranslationCatalog) -> I18nContext {
    let ctx = I18nContext::new(initial_locale, catalog);
    CURRENT_CONTEXT.with(|c| {
        *c.borrow_mut() = Some(ctx.clone());
    });
    ctx
}

/// Get the active reactive i18n context.
pub fn use_i18n() -> I18nContext {
    CURRENT_CONTEXT.with(|c| {
        if let Some(ref ctx) = *c.borrow() {
            ctx.clone()
        } else {
            let default_ctx = I18nContext::new(LocaleId::en_us(), TranslationCatalog::new());
            *c.borrow_mut() = Some(default_ctx.clone());
            default_ctx
        }
    })
}

/// Hook to get the reactive locale signal.
pub fn use_locale() -> Signal<LocaleId> {
    use_i18n().locale()
}

/// Set active application locale reactively.
pub fn set_locale(new_locale: LocaleId) {
    use_i18n().set_locale(new_locale);
}

/// Translate a key using current active locale and args.
pub fn translate_key(key: &str, args: &[(&str, IcuValue)]) -> String {
    let ctx = use_i18n();
    let mut map = HashMap::new();
    for (k, v) in args {
        map.insert(k.to_string(), v.clone());
    }
    ctx.translate(key, &map)
}

/// Macro for ergonomic translation calls:
/// `t!("auth.welcome")` or `t!("cart.items", count => 42, user => "Alice")`
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::context::translate_key($key, &[])
    };
    ($key:expr, $($name:ident => $val:expr),* $(,)?) => {
        $crate::context::translate_key($key, &[
            $( (stringify!($name), $crate::icu::IcuValue::from($val)) ),*
        ])
    };
    ($key:expr, $($name:literal => $val:expr),* $(,)?) => {
        $crate::context::translate_key($key, &[
            $( ($name, $crate::icu::IcuValue::from($val)) ),*
        ])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reactive_i18n_context() {
        let catalog = TranslationCatalog::new();
        let en = LocaleId::en_us();
        let de = LocaleId::de_de();

        catalog
            .register(&en, "hello", "Hello, {name}!")
            .unwrap();
        catalog
            .register(&de, "hello", "Guten Tag, {name}!")
            .unwrap();

        init_i18n(en, catalog);

        assert_eq!(t!("hello", name => "Lara"), "Hello, Lara!");

        // Switch locale reactively
        set_locale(de);
        assert_eq!(t!("hello", name => "Lara"), "Guten Tag, Lara!");
    }
}
