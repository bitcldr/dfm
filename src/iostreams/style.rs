//! Style atoms and the `paint` helper.
//!
//! Each atom is a reusable terminal style. `paint` applies the style only when
//! color is enabled, otherwise returning the text unchanged — so the caller
//! decides per-stream whether ANSI is emitted.

use anstyle::{AnsiColor, Style};

/// Bold (no color).
pub(crate) const BOLD: Style = Style::new().bold();
/// Bold + green.
pub(crate) const BOLD_GREEN: Style = Style::new().bold().fg_color(Some(ansi(AnsiColor::Green)));
/// Bold + bright white.
pub(crate) const BOLD_WHITE: Style = Style::new()
    .bold()
    .fg_color(Some(ansi(AnsiColor::BrightWhite)));
/// Bold + cyan.
pub(crate) const BOLD_CYAN: Style = Style::new().bold().fg_color(Some(ansi(AnsiColor::Cyan)));
/// Yellow.
pub(crate) const YELLOW: Style = Style::new().fg_color(Some(ansi(AnsiColor::Yellow)));
/// Bright white.
pub(crate) const WHITE: Style = Style::new().fg_color(Some(ansi(AnsiColor::BrightWhite)));
/// Dim (bright black / grey).
pub(crate) const DIM: Style = Style::new().fg_color(Some(ansi(AnsiColor::BrightBlack)));
/// Bright red (for failure counts).
pub(crate) const HI_RED: Style = Style::new().fg_color(Some(ansi(AnsiColor::BrightRed)));

/// `const`-friendly conversion from an ANSI color to a `Color`.
const fn ansi(c: AnsiColor) -> anstyle::Color {
    anstyle::Color::Ansi(c)
}

/// The arrow glyph used between link source and target (U+2192).
pub(crate) const fn arrow() -> &'static str {
    "→"
}

/// Apply `style` to `text` when `enabled`, else return `text` unchanged.
///
/// Uses `anstyle`'s `render`/`render_reset` so the emitted SGR sequence is the
/// style's own representation, wrapping the styled token with a trailing reset.
pub(crate) fn paint(style: Style, text: &str, enabled: bool) -> String {
    if !enabled {
        return text.to_string();
    }
    format!("{}{text}{}", style.render(), style.render_reset())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_disabled_is_plain() {
        assert_eq!(paint(BOLD_GREEN, "Linked", false), "Linked");
    }

    #[test]
    fn paint_enabled_wraps_with_sgr_and_reset() {
        let s = paint(BOLD_GREEN, "Linked", true);
        assert!(s.starts_with('\u{1b}'), "should start with ESC: {s:?}");
        assert!(s.contains("Linked"));
        assert!(s.ends_with("\u{1b}[0m"), "should end with reset: {s:?}");
    }

    #[test]
    fn arrow_is_u2192() {
        assert_eq!(arrow(), "\u{2192}");
    }
}
