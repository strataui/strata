use strata_i18n::prelude::*;
use std::collections::HashMap;

#[test]
fn test_bcp47_parsing_and_rtl() {
    let en = LocaleId::parse("en-US").unwrap();
    assert_eq!(en.language(), "en");
    assert_eq!(en.region(), Some("US"));
    assert!(!en.is_rtl());

    let ar = LocaleId::parse("ar-SA").unwrap();
    assert_eq!(ar.language(), "ar");
    assert_eq!(ar.region(), Some("SA"));
    assert!(ar.is_rtl());

    let he = LocaleId::parse("he-IL").unwrap();
    assert!(he.is_rtl());

    let zh = LocaleId::parse("zh-Hans-CN").unwrap();
    assert_eq!(zh.language(), "zh");
    assert_eq!(zh.script(), Some("Hans"));
    assert_eq!(zh.region(), Some("CN"));
}

#[test]
fn test_icu_message_plurals_and_formatting() {
    let catalog = TranslationCatalog::new();
    let en = LocaleId::en_us();
    let de = LocaleId::de_de();
    let _ar = LocaleId::ar_sa();

    catalog
        .register(
            &en,
            "cart.items",
            "{count, plural, =0 {Cart is empty} one {# item in cart} other {# items in cart}}",
        )
        .unwrap();

    catalog
        .register(
            &de,
            "cart.items",
            "{count, plural, =0 {Warenkorb ist leer} one {# Artikel im Warenkorb} other {# Artikel im Warenkorb}}",
        )
        .unwrap();

    let mut args = HashMap::new();
    args.insert("count".to_string(), 0.into());
    assert_eq!(catalog.translate("cart.items", &args, &en), "Cart is empty");
    assert_eq!(catalog.translate("cart.items", &args, &de), "Warenkorb ist leer");

    args.insert("count".to_string(), 1.into());
    assert_eq!(catalog.translate("cart.items", &args, &en), "1 item in cart");
    assert_eq!(catalog.translate("cart.items", &args, &de), "1 Artikel im Warenkorb");

    args.insert("count".to_string(), 15.into());
    assert_eq!(catalog.translate("cart.items", &args, &en), "15 items in cart");
    assert_eq!(catalog.translate("cart.items", &args, &de), "15 Artikel im Warenkorb");
}

#[test]
fn test_icu_select_and_nested_messages() {
    let catalog = TranslationCatalog::new();
    let en = LocaleId::en_us();

    let template = "{role, select, admin {Administrator {name} has access.} editor {Editor {name} has write access.} other {User {name} has read-only access.}}";
    catalog.register(&en, "rbac.message", template).unwrap();

    let mut args = HashMap::new();
    args.insert("name".to_string(), "Ada".into());

    args.insert("role".to_string(), "admin".into());
    assert_eq!(
        catalog.translate("rbac.message", &args, &en),
        "Administrator Ada has access."
    );

    args.insert("role".to_string(), "editor".into());
    assert_eq!(
        catalog.translate("rbac.message", &args, &en),
        "Editor Ada has write access."
    );

    args.insert("role".to_string(), "guest".into());
    assert_eq!(
        catalog.translate("rbac.message", &args, &en),
        "User Ada has read-only access."
    );
}

#[test]
fn test_number_and_currency_formatters() {
    let en = LocaleId::en_us();
    let de = LocaleId::de_de();

    let opts = NumberFormatOptions {
        min_fraction_digits: 2,
        max_fraction_digits: 2,
        use_grouping: true,
    };

    assert_eq!(format_number(9876543.21, &en, &opts), "9,876,543.21");
    assert_eq!(format_number(9876543.21, &de, &opts), "9.876.543,21");

    assert_eq!(format_currency(1250.50, "USD", &en), "$1,250.50");
    assert_eq!(format_currency(1250.50, "EUR", &de), "1.250,50 €");

    assert_eq!(format_percent(0.854, &en), "85.4%");
    assert_eq!(format_percent(0.854, &de), "85,4 %");

    assert_eq!(format_relative_time(-45, TimeUnit::Second, &en), "45 seconds ago");
    assert_eq!(format_relative_time(3, TimeUnit::Hour, &en), "in 3 hours");
    assert_eq!(format_relative_time(-2, TimeUnit::Hour, &de), "vor 2 Stunden");
}

#[test]
fn test_reactive_locale_switching_and_macro() {
    let catalog = TranslationCatalog::new();
    let en = LocaleId::en_us();
    let de = LocaleId::de_de();

    catalog.register(&en, "nav.dashboard", "Dashboard").unwrap();
    catalog.register(&de, "nav.dashboard", "Übersicht").unwrap();
    catalog
        .register(&en, "welcome.user", "Welcome back, {user}!")
        .unwrap();
    catalog
        .register(&de, "welcome.user", "Willkommen zurück, {user}!")
        .unwrap();

    init_i18n(en, catalog);

    assert_eq!(t!("nav.dashboard"), "Dashboard");
    assert_eq!(t!("welcome.user", user => "Taylor"), "Welcome back, Taylor!");

    // Switch locale reactively to German
    set_locale(de);

    assert_eq!(t!("nav.dashboard"), "Übersicht");
    assert_eq!(t!("welcome.user", user => "Taylor"), "Willkommen zurück, Taylor!");
}
