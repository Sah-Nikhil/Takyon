//! Focus preferences as typed values, and the themed block page.
//!
//! Keys live in `prefs.rs` with the rest; this reads them.

use super::session::Strictness;

/// When a hurdle (shutdown hold, idle-sleep hold) applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hurdle {
    /// Strict sessions only. The default (ADR-0036).
    Strict,
    Always,
    Never,
}

impl Hurdle {
    pub fn parse(value: Option<&str>) -> Hurdle {
        match value {
            Some("always") => Hurdle::Always,
            Some("never") => Hurdle::Never,
            _ => Hurdle::Strict,
        }
    }

    pub fn applies(self, strictness: Strictness) -> bool {
        match self {
            Hurdle::Strict => strictness == Strictness::Strict,
            Hurdle::Always => true,
            Hurdle::Never => false,
        }
    }
}

/// Snooze length from `focus.snooze_minutes`. None = No Snooze. Default 3.
pub fn snooze_ms(value: Option<&str>) -> Option<u64> {
    match value.and_then(|v| v.trim().parse::<u64>().ok()) {
        Some(0) => None,
        Some(minutes) => Some(minutes * 60_000),
        None => Some(3 * 60_000),
    }
}

/// Colours a block page is painted in: the live theme half's three roles.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct PageTheme {
    pub plate: String,
    pub fg: String,
    pub accent: String,
}

/// The bundled block page with its theme block replaced. Roles that are not
/// plain colour values are refused, so nothing can close the `<style>`.
pub fn themed_page(theme: Option<&PageTheme>) -> String {
    let colour = |v: &str| {
        !v.is_empty()
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric() || " .()%#,-/".contains(c))
    };
    let Some(t) = theme.filter(|t| colour(&t.plate) && colour(&t.fg) && colour(&t.accent)) else {
        return PAGE.to_string();
    };
    let open = r#"<style id="takyon-theme">"#;
    let Some(start) = PAGE.find(open) else {
        return PAGE.to_string();
    };
    let Some(len) = PAGE[start..].find("</style>") else {
        return PAGE.to_string();
    };
    let block = format!(
        "{open}\n  :root {{\n    --plate: {};\n    --fg: {};\n    --accent: {};\n  }}\n",
        t.plate, t.fg, t.accent
    );
    format!("{}{block}{}", &PAGE[..start], &PAGE[start + len..])
}

/// `focus/blocked.html`, compiled in so no install path is needed.
pub const PAGE: &str = include_str!("../../focus/blocked.html");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_16_hurdles_default_to_strict_sessions_only() {
        assert_eq!(Hurdle::parse(None), Hurdle::Strict);
        assert_eq!(Hurdle::parse(Some("always")), Hurdle::Always);
        assert_eq!(Hurdle::parse(Some("never")), Hurdle::Never);
        assert_eq!(Hurdle::parse(Some("garbage")), Hurdle::Strict);
        assert!(Hurdle::Strict.applies(Strictness::Strict));
        assert!(!Hurdle::Strict.applies(Strictness::Soft));
        assert!(Hurdle::Always.applies(Strictness::Soft));
        assert!(!Hurdle::Never.applies(Strictness::Strict));
    }

    #[test]
    fn v0_16_snooze_is_three_minutes_unless_chosen() {
        assert_eq!(snooze_ms(None), Some(180_000));
        assert_eq!(snooze_ms(Some("1")), Some(60_000));
        assert_eq!(snooze_ms(Some("10")), Some(600_000));
        assert_eq!(snooze_ms(Some("0")), None);
        assert_eq!(snooze_ms(Some("x")), Some(180_000));
    }

    #[test]
    fn v0_16_the_block_page_takes_the_live_theme() {
        let theme = PageTheme {
            plate: "oklch(0.19 0.004 286)".into(),
            fg: "oklch(0.966 0.0025 286)".into(),
            accent: "oklch(0.762 0.118 251)".into(),
        };
        let page = themed_page(Some(&theme));
        assert!(page.contains("--plate: oklch(0.19 0.004 286);"));
        assert!(page.contains("--accent: oklch(0.762 0.118 251);"));
        assert!(
            !page.contains("oklch(0.196 0.0255 293)"),
            "Vela fallback replaced"
        );
        assert!(page.contains("<title>Blocked · Takyon</title>"));
    }

    #[test]
    fn v0_16_a_theme_that_could_break_out_of_the_style_block_is_ignored() {
        let evil = PageTheme {
            plate: "red;}</style><script>alert(1)</script>".into(),
            fg: "#fff".into(),
            accent: "#000".into(),
        };
        assert_eq!(themed_page(Some(&evil)), PAGE);
        assert_eq!(themed_page(None), PAGE);
    }
}
