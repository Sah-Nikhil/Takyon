//! Turning an `f64` back into something a person asked for.
//!
//! Two jobs, and the first is not cosmetic: `12*1.18` is 14.159999999999998 in
//! binary floating point, and a launcher that answers that has not answered.
//! Rounding is where the result becomes correct, not where it becomes pretty.
//!
//! The second job is grouping, which is also what decides whether a bare number
//! is worth showing at all — see `mod.rs`.

/// Decimal places kept at or above 1.
///
/// Four not two: `40 kg` is 88.1849 lb, and 88.18 throws away precision someone
/// may have been converting *for*. Decimal places not significant figures, which
/// would round `123456789*2` to 246,914,000.
const DECIMALS: usize = 4;

/// Significant digits kept below 1, where four decimal places would flatten
/// `1/3000` to 0.0003.
const SIGNIFICANT: i32 = 6;

/// Outside this range, decimal notation is longer than the Palette row and less
/// readable than an exponent.
const SCIENTIFIC_ABOVE: f64 = 1e15;
const SCIENTIFIC_BELOW: f64 = 1e-6;

/// Format a result for display and for the clipboard alike.
///
/// One function, deliberately: "Copy answer" copying something other than what
/// the row shows is the kind of bug nobody reports because nobody believes it.
pub fn number(v: f64) -> Option<String> {
    if !v.is_finite() {
        return None;
    }
    if v == 0.0 {
        // Also catches -0.0, which `{:.4}` would otherwise render as "-0".
        return Some("0".to_string());
    }

    let magnitude = v.abs();
    if !(SCIENTIFIC_BELOW..SCIENTIFIC_ABOVE).contains(&magnitude) {
        return Some(format!("{v:e}"));
    }

    let text = if magnitude >= 1.0 {
        trim_zeros(&format!("{v:.DECIMALS$}"))
    } else {
        // `log10().floor()` is -1 for 0.5 and -4 for 0.0003, so this keeps six
        // digits from the first one that is not a zero.
        let exponent = magnitude.log10().floor() as i32;
        let places = (SIGNIFICANT - 1 - exponent).clamp(0, 17) as usize;
        trim_zeros(&format!("{v:.places$}"))
    };
    Some(group(&text))
}

/// Drop trailing zeros left by fixed-point formatting, and the point with them.
fn trim_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Insert thousands separators into the integer part only.
fn group(s: &str) -> String {
    let (sign, rest) = match s.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", s),
    };
    let (int, frac) = match rest.split_once('.') {
        Some((int, frac)) => (int, Some(frac)),
        None => (rest, None),
    };

    let mut grouped = String::with_capacity(int.len() + int.len() / 3);
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }

    match frac {
        Some(frac) => format!("{sign}{grouped}.{frac}"),
        None => format!("{sign}{grouped}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f64) -> String {
        number(v).expect("finite")
    }

    /// Step 1 of the manual verification script. The stored value is
    /// 14.159999999999998, so this is the assertion that the rounding rule is
    /// doing the job it exists for.
    #[test]
    fn v0_4_binary_floating_point_noise_is_rounded_away() {
        assert_eq!(f(12.0 * 1.18), "14.16");
        assert_eq!(f(0.1 + 0.2), "0.3");
        assert_eq!(f(12.0 + 12.0 * 0.3), "15.6");
    }

    /// The rule that decides whether a bare number is worth an Entry at all
    /// (`mod.rs`), so it is not decoration.
    #[test]
    fn v0_4_the_integer_part_is_grouped_and_the_fraction_is_not() {
        assert_eq!(f(2024.0), "2,024");
        assert_eq!(f(202.0), "202");
        assert_eq!(f(1234567.0), "1,234,567");
        assert_eq!(f(-9876.5), "-9,876.5");
        assert_eq!(f(1000.125), "1,000.125");
    }

    /// Significant figures would round this to 246,914,000. Decimal places are
    /// what keep a large integer exact.
    #[test]
    fn v0_4_a_large_integer_is_never_rounded() {
        assert_eq!(f(123456789.0 * 2.0), "246,913,578");
    }

    /// Below 1, four decimal places would flatten this to 0.0003 and `1/3` to
    /// 0.3333, so precision is kept from the first non-zero digit instead.
    #[test]
    fn v0_4_small_numbers_keep_six_significant_digits() {
        assert_eq!(f(1.0 / 3.0), "0.333333");
        assert_eq!(f(1.0 / 3000.0), "0.000333333");
    }

    #[test]
    fn v0_4_extreme_magnitudes_fall_back_to_an_exponent() {
        assert_eq!(f(1e20), "1e20");
        assert_eq!(f(1e-9), "1e-9");
    }

    /// `{:.4}` renders -0.0 as "-0", which reads as a bug in the calculator.
    #[test]
    fn v0_4_negative_zero_is_zero() {
        assert_eq!(f(0.0), "0");
        assert_eq!(f(-0.0), "0");
    }

    /// The Palette has no error row, so an unrepresentable result must be no
    /// result at all rather than a formatted `NaN`.
    #[test]
    fn v0_4_a_non_finite_value_has_no_formatting() {
        assert!(number(f64::NAN).is_none());
        assert!(number(f64::INFINITY).is_none());
    }
}
