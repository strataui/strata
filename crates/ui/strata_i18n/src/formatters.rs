//! Localized Formatting Suite: Numbers, Currencies, Percentages, and Relative Time.

use crate::locale::LocaleId;

/// Options for numeric formatting.
#[derive(Debug, Clone)]
pub struct NumberFormatOptions {
    pub min_fraction_digits: usize,
    pub max_fraction_digits: usize,
    pub use_grouping: bool,
}

impl Default for NumberFormatOptions {
    fn default() -> Self {
        Self {
            min_fraction_digits: 0,
            max_fraction_digits: 2,
            use_grouping: true,
        }
    }
}

/// Time unit for relative time formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

/// Format a number according to the specified locale.
pub fn format_number(val: f64, locale: &LocaleId, options: &NumberFormatOptions) -> String {
    let (group_sep, decimal_sep) = match locale.language() {
        "de" | "fr" | "it" | "es" | "pt" | "ru" => ('.', ','),
        _ => (',', '.'),
    };

    let is_negative = val < 0.0;
    let abs_val = val.abs();

    // Round according to max_fraction_digits
    let multiplier = 10f64.powi(options.max_fraction_digits as i32);
    let rounded = (abs_val * multiplier).round() / multiplier;

    let integer_part = rounded.trunc() as u64;
    let fraction_part = (rounded.fract() * multiplier).round() as u64;

    // Group integer part
    let int_str = integer_part.to_string();
    let grouped_int = if options.use_grouping && int_str.len() > 3 {
        let mut res = String::new();
        let chars: Vec<char> = int_str.chars().collect();
        let len = chars.len();
        for (i, &c) in chars.iter().enumerate() {
            if i > 0 && (len - i).is_multiple_of(3) {
                res.push(group_sep);
            }
            res.push(c);
        }
        res
    } else {
        int_str
    };

    // Format fraction part
    let mut frac_str = if options.max_fraction_digits > 0 {
        format!("{:0width$}", fraction_part, width = options.max_fraction_digits)
    } else {
        String::new()
    };

    // Trim trailing zeroes up to min_fraction_digits
    while frac_str.len() > options.min_fraction_digits && frac_str.ends_with('0') {
        frac_str.pop();
    }

    let mut result = String::new();
    if is_negative {
        result.push('-');
    }
    result.push_str(&grouped_int);
    if !frac_str.is_empty() {
        result.push(decimal_sep);
        result.push_str(&frac_str);
    }

    result
}

/// Format a currency amount according to the locale and currency code.
pub fn format_currency(amount: f64, currency: &str, locale: &LocaleId) -> String {
    let num_opts = NumberFormatOptions {
        min_fraction_digits: 2,
        max_fraction_digits: 2,
        use_grouping: true,
    };
    let formatted_num = format_number(amount, locale, &num_opts);

    let symbol = match currency.to_ascii_uppercase().as_str() {
        "USD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" => "¥",
        "CHF" => "CHF",
        _ => currency,
    };

    // Symbol placement convention
    match locale.language() {
        "en" => {
            if symbol == "$" || symbol == "£" || symbol == "¥" {
                if amount < 0.0 {
                    format!("-{}{}", symbol, &formatted_num[1..])
                } else {
                    format!("{symbol}{formatted_num}")
                }
            } else {
                format!("{formatted_num} {symbol}")
            }
        }
        "de" | "fr" | "es" | "it" => format!("{formatted_num} {symbol}"),
        _ => format!("{symbol} {formatted_num}"),
    }
}

/// Format a percentage ratio (e.g., 0.155 -> "15.5%").
pub fn format_percent(ratio: f64, locale: &LocaleId) -> String {
    let opts = NumberFormatOptions {
        min_fraction_digits: 0,
        max_fraction_digits: 1,
        use_grouping: true,
    };
    let num = format_number(ratio * 100.0, locale, &opts);
    match locale.language() {
        "fr" | "de" => format!("{num} %"),
        _ => format!("{num}%"),
    }
}

/// Format relative time (e.g., -120 seconds -> "2 minutes ago").
pub fn format_relative_time(delta_value: i64, unit: TimeUnit, locale: &LocaleId) -> String {
    let is_past = delta_value <= 0;
    let abs_val = delta_value.unsigned_abs();

    let lang = locale.language();

    match (lang, is_past) {
        ("de", true) => match unit {
            TimeUnit::Second => if abs_val == 1 { "vor 1 Sekunde".into() } else { format!("vor {abs_val} Sekunden") },
            TimeUnit::Minute => if abs_val == 1 { "vor 1 Minute".into() } else { format!("vor {abs_val} Minuten") },
            TimeUnit::Hour => if abs_val == 1 { "vor 1 Stunde".into() } else { format!("vor {abs_val} Stunden") },
            TimeUnit::Day => if abs_val == 1 { "gestern".into() } else { format!("vor {abs_val} Tagen") },
            _ => format!("vor {abs_val} Einheiten"),
        },
        ("de", false) => match unit {
            TimeUnit::Second => format!("in {abs_val} Sekunden"),
            TimeUnit::Minute => format!("in {abs_val} Minuten"),
            TimeUnit::Hour => format!("in {abs_val} Stunden"),
            TimeUnit::Day => if abs_val == 1 { "morgen".into() } else { format!("in {abs_val} Tagen") },
            _ => format!("in {abs_val} Einheiten"),
        },
        ("fr", true) => match unit {
            TimeUnit::Second => format!("il y a {abs_val} secondes"),
            TimeUnit::Minute => format!("il y a {abs_val} minutes"),
            TimeUnit::Hour => format!("il y a {abs_val} heures"),
            TimeUnit::Day => if abs_val == 1 { "hier".into() } else { format!("il y a {abs_val} jours") },
            _ => format!("il y a {abs_val} unités"),
        },
        ("fr", false) => match unit {
            TimeUnit::Second => format!("dans {abs_val} secondes"),
            TimeUnit::Minute => format!("dans {abs_val} minutes"),
            TimeUnit::Hour => format!("dans {abs_val} heures"),
            TimeUnit::Day => if abs_val == 1 { "demain".into() } else { format!("dans {abs_val} jours") },
            _ => format!("dans {abs_val} unités"),
        },
        _ => {
            // English default
            if abs_val == 0 {
                return "just now".into();
            }
            let (singular, plural) = match unit {
                TimeUnit::Second => ("second", "seconds"),
                TimeUnit::Minute => ("minute", "minutes"),
                TimeUnit::Hour => ("hour", "hours"),
                TimeUnit::Day => ("day", "days"),
                TimeUnit::Week => ("week", "weeks"),
                TimeUnit::Month => ("month", "months"),
                TimeUnit::Year => ("year", "years"),
            };
            let word = if abs_val == 1 { singular } else { plural };

            if is_past {
                format!("{abs_val} {word} ago")
            } else {
                format!("in {abs_val} {word}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_number_formatting() {
        let en = LocaleId::en_us();
        let de = LocaleId::de_de();
        let opts = NumberFormatOptions {
            min_fraction_digits: 2,
            max_fraction_digits: 2,
            use_grouping: true,
        };

        assert_eq!(format_number(1234567.89, &en, &opts), "1,234,567.89");
        assert_eq!(format_number(1234567.89, &de, &opts), "1.234.567,89");
    }

    #[test]
    fn test_currency_formatting() {
        let en = LocaleId::en_us();
        let de = LocaleId::de_de();

        assert_eq!(format_currency(49.99, "USD", &en), "$49.99");
        assert_eq!(format_currency(49.99, "EUR", &de), "49,99 €");
    }

    #[test]
    fn test_percent_formatting() {
        let en = LocaleId::en_us();
        let de = LocaleId::de_de();

        assert_eq!(format_percent(0.125, &en), "12.5%");
        assert_eq!(format_percent(0.125, &de), "12,5 %");
    }

    #[test]
    fn test_relative_time_formatting() {
        let en = LocaleId::en_us();
        let de = LocaleId::de_de();

        assert_eq!(format_relative_time(-5, TimeUnit::Minute, &en), "5 minutes ago");
        assert_eq!(format_relative_time(2, TimeUnit::Hour, &en), "in 2 hours");
        assert_eq!(format_relative_time(-1, TimeUnit::Day, &de), "gestern");
        assert_eq!(format_relative_time(1, TimeUnit::Day, &de), "morgen");
    }
}
