//! Unit conversion. Static tables, no network, ever.
//!
//! Currency is deliberately absent — it needs live rates, which is an outbound
//! request on the Bangless path and therefore a correctness bug (ADR-0002). It
//! waits for v0.9's Bangs; `docs/tbd/v0.4.md` carries the reasoning.
//!
//! Conversion goes through a base unit per dimension rather than a table of
//! pairs: pairs are O(n²) to write and O(n²) to get wrong.

/// What a unit measures. Two units only convert within one dimension, so
/// `40 kg to lb` works and `40 kg to cm` yields nothing rather than a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dim {
    Length,
    Mass,
    Temperature,
    Data,
    Time,
}

/// One unit, as an affine map onto its dimension's base.
///
/// `base = value * factor + offset`. The offset exists for exactly one
/// dimension: Fahrenheit is not a scaled Celsius, and a pure factor table gets
/// every temperature wrong by 32 degrees somewhere.
struct Unit {
    /// Every spelling accepted. First is not special; `label` decides display.
    names: &'static [&'static str],
    dim: Dim,
    factor: f64,
    offset: f64,
    /// How the converted value is written out — `lb`, not `pounds`.
    label: &'static str,
}

/// Metres, kilograms, Celsius, bytes and seconds are the bases.
///
/// Data is 1024-based: File Explorer reports 1024-based sizes under the same
/// "GB" label, and someone checking our answer against it has to see the same
/// number. The `KiB` spellings are accepted and mean the same.
const UNITS: &[Unit] = &[
    // Length
    unit(&["mm", "millimetre", "millimetres", "millimeter", "millimeters"], Dim::Length, 0.001, "mm"),
    unit(&["cm", "centimetre", "centimetres", "centimeter", "centimeters"], Dim::Length, 0.01, "cm"),
    unit(&["m", "metre", "metres", "meter", "meters"], Dim::Length, 1.0, "m"),
    unit(&["km", "kilometre", "kilometres", "kilometer", "kilometers"], Dim::Length, 1000.0, "km"),
    unit(&["in", "inch", "inches"], Dim::Length, 0.0254, "in"),
    unit(&["ft", "foot", "feet"], Dim::Length, 0.3048, "ft"),
    unit(&["yd", "yard", "yards"], Dim::Length, 0.9144, "yd"),
    unit(&["mi", "mile", "miles"], Dim::Length, 1609.344, "mi"),
    unit(&["nmi", "nauticalmile", "nauticalmiles"], Dim::Length, 1852.0, "nmi"),
    // Mass
    unit(&["mg", "milligram", "milligrams"], Dim::Mass, 1e-6, "mg"),
    unit(&["g", "gram", "grams"], Dim::Mass, 0.001, "g"),
    unit(&["kg", "kilogram", "kilograms", "kilo", "kilos"], Dim::Mass, 1.0, "kg"),
    unit(&["t", "tonne", "tonnes", "metricton"], Dim::Mass, 1000.0, "t"),
    unit(&["oz", "ounce", "ounces"], Dim::Mass, 0.028349523125, "oz"),
    unit(&["lb", "lbs", "pound", "pounds"], Dim::Mass, 0.45359237, "lb"),
    unit(&["st", "stone", "stones"], Dim::Mass, 6.35029318, "st"),
    // Temperature. The one dimension with an offset.
    unit(&["c", "celsius", "centigrade"], Dim::Temperature, 1.0, "\u{00b0}C"),
    Unit { names: &["f", "fahrenheit"], dim: Dim::Temperature, factor: 5.0 / 9.0, offset: -160.0 / 9.0, label: "\u{00b0}F" },
    Unit { names: &["k", "kelvin"], dim: Dim::Temperature, factor: 1.0, offset: -273.15, label: "K" },
    // Data
    unit(&["byte", "bytes"], Dim::Data, 1.0, "bytes"),
    unit(&["kb", "kib", "kilobyte", "kilobytes"], Dim::Data, 1024.0, "KB"),
    unit(&["mb", "mib", "megabyte", "megabytes"], Dim::Data, 1048576.0, "MB"),
    unit(&["gb", "gib", "gigabyte", "gigabytes"], Dim::Data, 1073741824.0, "GB"),
    unit(&["tb", "tib", "terabyte", "terabytes"], Dim::Data, 1099511627776.0, "TB"),
    unit(&["pb", "pib", "petabyte", "petabytes"], Dim::Data, 1125899906842624.0, "PB"),
    unit(&["bit", "bits"], Dim::Data, 0.125, "bits"),
    unit(&["kbit", "kilobit", "kilobits"], Dim::Data, 128.0, "kbit"),
    unit(&["mbit", "megabit", "megabits"], Dim::Data, 131072.0, "Mbit"),
    unit(&["gbit", "gigabit", "gigabits"], Dim::Data, 134217728.0, "Gbit"),
    // Time. Months and years are absent on purpose: both are ambiguous, and a
    // launcher answering "3 months" with a number derived from 30.44 days is
    // making up precision it does not have.
    unit(&["ms", "millisecond", "milliseconds"], Dim::Time, 0.001, "ms"),
    unit(&["s", "sec", "secs", "second", "seconds"], Dim::Time, 1.0, "s"),
    unit(&["min", "mins", "minute", "minutes"], Dim::Time, 60.0, "min"),
    unit(&["h", "hr", "hrs", "hour", "hours"], Dim::Time, 3600.0, "h"),
    unit(&["d", "day", "days"], Dim::Time, 86400.0, "d"),
    unit(&["wk", "week", "weeks"], Dim::Time, 604800.0, "wk"),
];

/// The offset-free case, which is every unit but two.
const fn unit(names: &'static [&'static str], dim: Dim, factor: f64, label: &'static str) -> Unit {
    Unit { names, dim, factor, offset: 0.0, label }
}

/// A conversion request, split out of the raw input.
pub struct Conversion<'a> {
    /// Everything left of the unit — an expression in its own right, so
    /// `(40+5) kg to lb` works.
    pub expression: &'a str,
    from: &'static Unit,
    to: &'static Unit,
}

impl Conversion<'_> {
    /// Convert an already-evaluated value, or refuse across dimensions.
    pub fn apply(&self, value: f64) -> Option<(f64, &'static str)> {
        if self.from.dim != self.to.dim {
            return None;
        }
        let base = value * self.from.factor + self.from.offset;
        let out = (base - self.to.offset) / self.to.factor;
        out.is_finite().then_some((out, self.to.label))
    }
}

/// Recognise `<expression> <unit> to|in <unit>`, or decline.
///
/// The keyword is required. Without it `40 kg` would have to be guessed at, and
/// guessing is what puts a calculator row on top of an app search.
pub fn split(input: &str) -> Option<Conversion<'_>> {
    let (left, right) = split_keyword(input)?;
    let to = lookup(right.trim())?;

    // The unit is the trailing run of letters, so `40kg` and `40 kg` are one
    // case. What remains is handed back to the expression parser untouched.
    let trimmed = left.trim_end();
    let cut = trimmed.trim_end_matches(char::is_alphabetic);
    if cut.len() == trimmed.len() {
        return None;
    }
    let from = lookup(&trimmed[cut.len()..])?;

    Some(Conversion { expression: cut, from, to })
}

/// Find the ` to ` or ` in ` that separates the two units.
///
/// `to` is tried first because `in` is *also* a unit — inches — so `5 ft in in`
/// has to split at the first ` in `, not the last.
fn split_keyword(input: &str) -> Option<(&str, &str)> {
    for keyword in [" to ", " in "] {
        if let Some(at) = find_ascii_case_insensitive(input, keyword) {
            return Some((&input[..at], &input[at + keyword.len()..]));
        }
    }
    None
}

/// The keywords are ASCII, so lowercasing byte-wise keeps every index valid for
/// slicing the original string.
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(needle)
}

fn lookup(name: &str) -> Option<&'static Unit> {
    let name = name.trim().to_ascii_lowercase();
    UNITS.iter().find(|u| u.names.contains(&name.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(input: &str, value: f64) -> Option<(f64, &'static str)> {
        split(input)?.apply(value)
    }

    /// Step 2 of the manual verification script. 40 kg is 88.1849 lb; the plan's
    /// draft said 88.18, which is the same number with two digits thrown away —
    /// `fmt::DECIMALS` carries why we keep four.
    #[test]
    fn v0_4_the_worked_conversion_from_the_plan() {
        let (v, label) = convert("40 kg to lb", 40.0).unwrap();
        assert!((v - 88.1849).abs() < 1e-4, "{v}");
        assert_eq!(label, "lb");
    }

    /// The unit may be attached to the number or spaced off it. Both are what
    /// people actually type, and they must not be two code paths.
    #[test]
    fn v0_4_the_unit_may_be_attached_to_the_number() {
        let spaced = split("40 kg to lb").unwrap();
        let attached = split("40kg to lb").unwrap();
        assert_eq!(spaced.expression.trim(), "40");
        assert_eq!(attached.expression.trim(), "40");
    }

    /// Temperature is affine, not scaled. A factor-only table gets every one of
    /// these wrong, and gets 0 °C exactly right, which is how it survives a
    /// careless test.
    #[test]
    fn v0_4_temperature_conversion_carries_its_offset() {
        let (v, label) = convert("100 c to f", 100.0).unwrap();
        assert!((v - 212.0).abs() < 1e-9, "{v}");
        assert_eq!(label, "\u{00b0}F");

        assert!((convert("32 f to c", 32.0).unwrap().0 - 0.0).abs() < 1e-9);
        assert!((convert("-40 c to f", -40.0).unwrap().0 + 40.0).abs() < 1e-9);
        assert!((convert("0 c to k", 0.0).unwrap().0 - 273.15).abs() < 1e-9);
    }

    /// A conversion nobody can perform must yield nothing, not a number derived
    /// from two unrelated scales.
    #[test]
    fn v0_4_units_from_different_dimensions_do_not_convert() {
        assert!(convert("40 kg to cm", 40.0).is_none());
        assert!(convert("5 c to gb", 5.0).is_none());
    }

    /// 1024-based, matching what File Explorer reports under the same label.
    #[test]
    fn v0_4_data_sizes_are_1024_based_like_explorer() {
        assert_eq!(convert("1 gb to mb", 1.0).unwrap().0, 1024.0);
        assert_eq!(convert("1 gib to mib", 1.0).unwrap().0, 1024.0);
        assert_eq!(convert("1 byte to bits", 1.0).unwrap().0, 8.0);
    }

    #[test]
    fn v0_4_time_converts_within_the_units_that_are_unambiguous() {
        assert_eq!(convert("90 min to h", 90.0).unwrap().0, 1.5);
        assert_eq!(convert("2 wk to d", 2.0).unwrap().0, 14.0);
        // Months and years are absent deliberately; neither has a fixed length.
        assert!(split("3 months to days").is_none());
    }

    /// `in` is both the keyword and a unit. Splitting at the last occurrence
    /// instead of the first turns this into a lookup of "ft in".
    #[test]
    fn v0_4_in_works_as_a_keyword_even_when_the_target_is_inches() {
        assert!((convert("5 ft in in", 5.0).unwrap().0 - 60.0).abs() < 1e-9);
        assert!((convert("40 cm in inches", 40.0).unwrap().0 - 15.7480).abs() < 1e-3);
    }

    /// The left side stays an expression, so arithmetic and conversion compose
    /// rather than being two separate features.
    #[test]
    fn v0_4_the_left_side_is_still_an_expression() {
        assert_eq!(split("(40+5) kg to lb").unwrap().expression.trim(), "(40+5)");
    }

    /// Without the keyword there is nothing to convert, and guessing is what puts
    /// a calculator row on top of an app search.
    #[test]
    fn v0_4_a_quantity_with_no_keyword_is_not_a_conversion() {
        assert!(split("40 kg").is_none());
        assert!(split("100").is_none());
    }

    /// An unknown unit is a refusal. This is the same rule the parser applies to
    /// unknown words, and it is what keeps `notion to do` out of the calculator.
    #[test]
    fn v0_4_an_unknown_unit_is_refused() {
        assert!(split("40 kg to bananas").is_none());
        assert!(split("40 widgets to lb").is_none());
        assert!(split("notion to do").is_none());
    }

    /// Currency is not here, and its absence is the ADR-0002 guarantee for this
    /// phase rather than an oversight. `docs/tbd/v0.4.md`, owned by v0.9.
    #[test]
    fn v0_4_no_currency_unit_exists_at_all() {
        for name in ["usd", "inr", "eur", "gbp", "dollar", "dollars", "rupee"] {
            assert!(lookup(name).is_none(), "{name} is a unit");
        }
        assert!(split("100 usd to inr").is_none());
    }

    /// Two units sharing a spelling means one is unreachable, and which one
    /// depends on table order — invisible in review, maddening in use.
    #[test]
    fn v0_4_no_spelling_is_claimed_by_two_units() {
        let mut seen = std::collections::HashSet::new();
        for u in UNITS {
            for name in u.names {
                assert!(seen.insert(*name), "{name} is defined twice");
            }
        }
    }
}
