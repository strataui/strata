# strata_i18n: Reactive Internationalization, ICU MessageFormat & Localization

The `strata_i18n` crate delivers a pure-Rust, zero-dependency internationalization and localization engine for the Strata framework. Engineered as a high-performance equivalent to `formatjs`, `react-intl`, and `react-i18next`, `strata_i18n` features Unicode CLDR pluralization rules, full ICU MessageFormat AST parsing, number, currency, percent, and relative-time formatters, Right-to-Left (RTL) text detection, and fine-grained reactive locale signals.

---

## Architectural Principles

Internationalization in Strata operates through compiled message trees and reactive signals:

```text
┌─────────────────────────────────────────────────────────────┐
│                   Active Locale Signal                      │
│                  use_locale() -> LocaleId                   │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                    TranslationCatalog                       │
│        (O(1) compiled ICU AST lookup by key & locale)       │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                 ICU Evaluation Engine                       │
│    • CLDR Plural Category Solver (=0, one, few, many, ...)  │
│    • Branch Selection (admin, editor, viewer, other)        │
│    • Variable Interpolation ({name}, {count})               │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                 Localized Output String                     │
│    "3 cluster nodes failed in us-east-01" / "تم فشل ٣ عقد" │
└─────────────────────────────────────────────────────────────┘
```

1. **Compile-Once AST Evaluation**: ICU templates are parsed into immutable `IcuMessage` abstract syntax trees upon registration. Formatting evaluates the pre-parsed AST directly without runtime string scanning or regular expressions.
2. **Reactivity Without Global Rebuilding**: Changing the active locale via `set_locale(new_locale)` invalidates only text nodes subscribed to the locale signal, preserving application state and input focus.
3. **True BCP 47 and RTL Awareness**: Parses language, script, and region subtags, automatically setting text directionality (`ltr` or `rtl`) for Arabic, Hebrew, Persian, and Urdu.

---

## Core Types and Data Models

### BCP 47 Language Tags: `LocaleId`

Parses, validates, and inspects standard BCP 47 language identifiers:

```rust
use strata_i18n::prelude::LocaleId;

// Standard Latin LTR locale
let en = LocaleId::parse("en-US").unwrap();
assert_eq!(en.language(), "en");
assert_eq!(en.region(), Some("US"));
assert!(!en.is_rtl());

// Arabic RTL locale
let ar = LocaleId::parse("ar-SA").unwrap();
assert_eq!(ar.language(), "ar");
assert_eq!(ar.region(), Some("SA"));
assert!(ar.is_rtl()); // Automatically returns true

// Complex subtag with script (Simplified Chinese)
let zh = LocaleId::parse("zh-Hans-CN").unwrap();
assert_eq!(zh.language(), "zh");
assert_eq!(zh.script(), Some("Hans"));
assert_eq!(zh.region(), Some("CN"));
```

#### Standard Factory Constructors

- `LocaleId::en_us()`: English (United States)
- `LocaleId::de_de()`: German (Germany)
- `LocaleId::fr_fr()`: French (France)
- `LocaleId::es_es()`: Spanish (Spain)
- `LocaleId::ja_jp()`: Japanese (Japan)
- `LocaleId::ar_sa()`: Arabic (Saudi Arabia, RTL)

---

### The Translation Catalog: `TranslationCatalog`

Maintains localized ICU templates indexed by key and target locale:

```rust
use std::collections::HashMap;
use strata_i18n::prelude::{LocaleId, TranslationCatalog};

let catalog = TranslationCatalog::new();
let en = LocaleId::en_us();
let de = LocaleId::de_de();

// Register English template
catalog.register(
    &en,
    "cluster.nodes_offline",
    "{count, plural, =0 {All nodes operational} one {# node offline in {region}} other {# nodes offline in {region}}}",
).unwrap();

// Register German template
catalog.register(
    &de,
    "cluster.nodes_offline",
    "{count, plural, =0 {Alle Knoten betriebsbereit} one {# Knoten offline in {region}} other {# Knoten offline in {region}}}",
).unwrap();

// Interpolate with arguments
let mut args = HashMap::new();
args.insert("count".to_string(), 0.into());
args.insert("region".to_string(), "us-east-01".into());

assert_eq!(catalog.translate("cluster.nodes_offline", &args, &en), "All nodes operational");
assert_eq!(catalog.translate("cluster.nodes_offline", &args, &de), "Alle Knoten betriebsbereit");

args.insert("count".to_string(), 3.into());
assert_eq!(catalog.translate("cluster.nodes_offline", &args, &en), "3 nodes offline in us-east-01");
assert_eq!(catalog.translate("cluster.nodes_offline", &args, &de), "3 Knoten offline in us-east-01");
```

---

## ICU MessageFormat Engine: `IcuMessage`

Supports complex pluralization offsets, explicit matching rules, and categorical branch selectors (`select`).

### Categorical Select Branching

Allows role-based, status-based, or severity-based textual branching:

```rust
use std::collections::HashMap;
use strata_i18n::prelude::{IcuMessage, LocaleId};

let template = "{role, select, \
    admin {Admin {name} has full root access to the control plane.} \
    operator {Operator {name} has deployment privileges.} \
    viewer {Viewer {name} has read-only telemetry access.} \
    other {User {name} has unassigned permissions.}}";

let msg = IcuMessage::parse(template).expect("Valid ICU template");
let locale = LocaleId::en_us();

let mut args = HashMap::new();
args.insert("name".to_string(), "Taylor".into());

args.insert("role".to_string(), "admin".into());
assert_eq!(msg.format(&args, &locale), "Admin Taylor has full root access to the control plane.");

args.insert("role".to_string(), "viewer".into());
assert_eq!(msg.format(&args, &locale), "Viewer Taylor has read-only telemetry access.");
```

---

## Number, Currency, and Time Formatters

Localizes numbers, currency amounts, percentages, and relative time deltas according to regional cultural standards:

```rust
use strata_i18n::prelude::{
    format_currency, format_number, format_percent, format_relative_time,
    LocaleId, NumberFormatOptions, TimeUnit,
};

let en = LocaleId::en_us();
let de = LocaleId::de_de();

// 1. Localized Number Formatting
let opts = NumberFormatOptions {
    min_fraction_digits: 2,
    max_fraction_digits: 2,
    use_grouping: true,
};
assert_eq!(format_number(1450250.75, &en, &opts), "1,450,250.75");
assert_eq!(format_number(1450250.75, &de, &opts), "1.450.250,75");

// 2. Currency Formatting
assert_eq!(format_currency(4820.50, "USD", &en), "$4,820.50");
assert_eq!(format_currency(4820.50, "EUR", &de), "4.820,50 €");

// 3. Percent Formatting
assert_eq!(format_percent(0.9995, &en), "99.95%");
assert_eq!(format_percent(0.9995, &de), "99,95%");

// 4. Relative Time Formatting (Incidents and Audit Logs)
assert_eq!(format_relative_time(-15, TimeUnit::Minute, &en), "15 minutes ago");
assert_eq!(format_relative_time(2, TimeUnit::Hour, &en), "in 2 hours");
assert_eq!(format_relative_time(-1, TimeUnit::Day, &de), "vor 1 Tag");
```

---

## Reactive Context and Macro Ergonomics

Initialize global internationalization context during application bootstrap:

```rust
use strata_i18n::prelude::*;

// Bootstrap during runtime startup
let catalog = TranslationCatalog::new();
// ... register translations ...
init_i18n(catalog, LocaleId::en_us());

// Retrieve reactive locale signal in UI components
let locale = use_locale();
println!("Active locale: {}", locale.get().language());

// Switch locale globally
set_locale(LocaleId::de_de());
```

### The `t!` Declarative Translation Macro

```rust
use strata_dom::prelude::*;
use strata_i18n::prelude::*;

#[component]
pub fn ServerAlertBanner(node_name: &'static str, offline_count: usize) -> View {
    rsx! {
        <div class="p-3 bg-rose-950/40 border border-rose-800 rounded-lg text-rose-300 text-xs font-mono">
            {t!("cluster.nodes_offline", count => offline_count, region => node_name)}
        </div>
    }
}
```

---

## RTL Layout Integration

When a locale is activated whose `is_rtl()` flag is `true` (e.g. Arabic, Hebrew), Strata automatically flips flexbox flow directions, text alignments, and directional icons:

```rust
use strata_dom::prelude::*;
use strata_i18n::prelude::*;

#[component]
pub fn LocalizedHeader() -> View {
    let locale = use_locale();
    let is_rtl = locale.get().is_rtl();

    rsx! {
        <header
            class="flex items-center justify-between p-4 bg-neutral-900 border-b border-neutral-800"
            style={if is_rtl { "direction: rtl;" } else { "direction: ltr;" }}
        >
            <h1 class="text-sm font-bold text-neutral-200">
                {t!("app.title")}
            </h1>
            <div class="flex items-center gap-2">
                <button
                    class="px-3 py-1 bg-neutral-800 text-xs rounded text-neutral-300 hover:bg-neutral-700"
                    onclick={move |_| set_locale(LocaleId::ar_sa())}
                >
                    "العربية"
                </button>
                <button
                    class="px-3 py-1 bg-neutral-800 text-xs rounded text-neutral-300 hover:bg-neutral-700"
                    onclick={move |_| set_locale(LocaleId::en_us())}
                >
                    "English"
                </button>
            </div>
        </header>
    }
}
```
