//! Palette derivation for the `paper` preset.
//!
//! Each stock is three colours — paper, slip, ink — plus one accent and the two
//! semantic colours. Everything else is a mix **towards the paper**, and that
//! direction is what lets one set of ratios serve both modes: `soft` is always
//! "18% towards the paper", so it darkens on a light sheet and lightens on a
//! dark one. The ladder is therefore the same shape in all six combinations,
//! and a new stock only has to pick three colours.
//!
//! The light/dark axis is `ThemeMode`, deliberately not part of
//! [`PaperTheme`]: "which paper" and "day or night" are independent questions,
//! exactly like the preset and the global theme switch.

use ratatui::style::Color;

use super::*;
use crate::core::config::models::{PaperTheme, ThemeMode};
use crate::tui::app::view::sheet::Sheet;
use crate::tui::palette;

/// One resolved palette. The page, the rules, the text and the status colours
/// all come from here; no module may hard-code a paper colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PaperPalette {
    /// The page itself. Every cell of the frame is painted with it.
    pub bg: Color,
    /// The slip a dialog sits on: one step closer to the reader than `bg`.
    pub sheet: Color,
    /// Body text and the selected row.
    pub ink: Color,
    /// Secondary text: values, unselected rows.
    pub soft: Color,
    /// Tertiary: labels, hints.
    pub dim: Color,
    /// Quaternary: running heads, page numbers, footnotes.
    pub faint: Color,
    /// Section rules.
    pub rule: Color,
    /// Rules that only hint at a division.
    pub rule_soft: Color,
    /// The one accent: position, keys, destructive actions. Nothing else.
    pub accent: Color,
    /// Ready.
    pub ok: Color,
    /// Needs attention.
    pub warn: Color,
    /// `跟随终端`: the fills are withdrawn so the terminal's own background
    /// shows through. The ink stays this stock's, so the dark stock is still
    /// chosen in that mode.
    pub transparent: bool,
}

/// The three inks a stock starts from, before the ladder is mixed.
struct Base {
    bg: Color,
    sheet: Color,
    ink: Color,
    accent: Color,
    ok: Color,
    warn: Color,
}

/// Ratios that put each step of the ladder at its rank. `dim` is 0.34, not the
/// 0.40 a naive ladder would use: at 0.40 kraft falls to 3.97:1, below the
/// 4.5:1 the design requires of secondary text. The floor stays, the ratio
/// moves.
const SOFT: f64 = 0.18;
const DIM: f64 = 0.34;
const FAINT: f64 = 0.62;
const RULE_MIX: f64 = 0.78;
const RULE_SOFT_MIX: f64 = 0.88;

impl PaperTheme {
    /// The resolved palette for this stock under `mode`.
    ///
    /// `Terminal` keeps the dark stock: a `Reset` background cannot take part in
    /// a mix, and every rule and blend here is a mix towards the page. What it
    /// gives up is the paint itself, since `transparent` makes `fill` return
    /// `Reset` and `punch_through_terminal` hands back the page and the slip.
    pub(crate) fn palette(self, mode: ThemeMode) -> PaperPalette {
        let base = self.base(matches!(mode, ThemeMode::Dark | ThemeMode::Terminal));
        PaperPalette {
            transparent: mode == ThemeMode::Terminal,
            bg: base.bg,
            sheet: base.sheet,
            ink: base.ink,
            soft: palette::mix(base.ink, base.bg, SOFT),
            dim: palette::mix(base.ink, base.bg, DIM),
            faint: palette::mix(base.ink, base.bg, FAINT),
            rule: palette::mix(base.ink, base.bg, RULE_MIX),
            rule_soft: palette::mix(base.ink, base.bg, RULE_SOFT_MIX),
            accent: base.accent,
            ok: base.ok,
            warn: base.warn,
        }
    }

    /// Paper, slip, ink and the three semantic colours for one stock.
    ///
    /// The dark variants are not "the light ones turned down": they are the same
    /// page printed at night, so the ink is warm and the accent is lifted one
    /// step or it would disappear into the sheet.
    fn base(self, dark: bool) -> Base {
        match (self, dark) {
            (Self::Ivory, false) => Base {
                bg: Color::Rgb(0xF7, 0xF3, 0xE8),
                sheet: Color::Rgb(0xFC, 0xF9, 0xF1),
                ink: Color::Rgb(0x1A, 0x1A, 0x1A),
                accent: Color::Rgb(0x8C, 0x2F, 0x2F),
                ok: Color::Rgb(0x4F, 0x6B, 0x3A),
                warn: Color::Rgb(0x8C, 0x6B, 0x2F),
            },
            (Self::Ash, false) => Base {
                bg: Color::Rgb(0xEF, 0xEE, 0xE8),
                sheet: Color::Rgb(0xF7, 0xF6, 0xF1),
                ink: Color::Rgb(0x1B, 0x1B, 0x1B),
                accent: Color::Rgb(0x8C, 0x2F, 0x2F),
                ok: Color::Rgb(0x4F, 0x6B, 0x3A),
                warn: Color::Rgb(0x8C, 0x6B, 0x2F),
            },
            (Self::Kraft, false) => Base {
                bg: Color::Rgb(0xE7, 0xDA, 0xC3),
                sheet: Color::Rgb(0xF0, 0xE6, 0xD3),
                ink: Color::Rgb(0x22, 0x1C, 0x14),
                accent: Color::Rgb(0x77, 0x20, 0x1A),
                ok: Color::Rgb(0x4F, 0x6B, 0x3A),
                warn: Color::Rgb(0x8C, 0x6B, 0x2F),
            },
            (Self::Ivory, true) => Base {
                bg: Color::Rgb(0x14, 0x11, 0x0C),
                sheet: Color::Rgb(0x1D, 0x19, 0x13),
                ink: Color::Rgb(0xED, 0xE6, 0xD6),
                accent: Color::Rgb(0xD9, 0x7F, 0x6B),
                ok: Color::Rgb(0x9C, 0xC4, 0x6F),
                warn: Color::Rgb(0xD8, 0xAE, 0x6B),
            },
            (Self::Ash, true) => Base {
                bg: Color::Rgb(0x13, 0x13, 0x13),
                sheet: Color::Rgb(0x1C, 0x1C, 0x1C),
                ink: Color::Rgb(0xE6, 0xE6, 0xE2),
                accent: Color::Rgb(0xD9, 0x7F, 0x6B),
                ok: Color::Rgb(0x9C, 0xC4, 0x6F),
                warn: Color::Rgb(0xD8, 0xAE, 0x6B),
            },
            (Self::Kraft, true) => Base {
                bg: Color::Rgb(0x1A, 0x13, 0x0D),
                sheet: Color::Rgb(0x24, 0x1B, 0x12),
                ink: Color::Rgb(0xE3, 0xD3, 0xB8),
                accent: Color::Rgb(0xC9, 0x7A, 0x5E),
                ok: Color::Rgb(0x9C, 0xC4, 0x6F),
                warn: Color::Rgb(0xD8, 0xAE, 0x6B),
            },
        }
    }
}

impl App {
    /// The palette this frame is printed with. Cheap enough to recompute per
    /// frame; a cache would outlive the config it derives from.
    pub(in crate::tui::app) fn paper_palette(&self) -> PaperPalette {
        let ui = &self.config.state.ui;
        let mode = if self.light_terminal() {
            ThemeMode::Light
        } else {
            ui.theme_mode
        };
        let mut palette = ui.paper_theme.palette(mode);
        // `mode` is `Light` here when the terminal is light; the sheet still
        // follows the terminal, so its fills stay withdrawn.
        palette.transparent = ui.theme_mode == ThemeMode::Terminal;
        palette
    }
}

impl Sheet for PaperPalette {
    fn fills(&self) -> [Option<Color>; 4] {
        [Some(self.bg), Some(self.sheet), None, None]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel_luminance(value: u8) -> f64 {
        let value = f64::from(value) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(color: Color) -> f64 {
        match color {
            Color::Rgb(r, g, b) => {
                0.2126 * channel_luminance(r)
                    + 0.7152 * channel_luminance(g)
                    + 0.0722 * channel_luminance(b)
            }
            _ => 0.0,
        }
    }

    fn contrast(a: Color, b: Color) -> f64 {
        let (left, right) = (luminance(a), luminance(b));
        let (lighter, darker) = if left >= right {
            (left, right)
        } else {
            (right, left)
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    /// The six combinations, generated rather than listed, so a fourth stock
    /// cannot be added without being checked.
    fn combinations() -> impl Iterator<Item = (PaperTheme, ThemeMode)> {
        PaperTheme::ALL.into_iter().flat_map(|theme| {
            [ThemeMode::Light, ThemeMode::Dark]
                .into_iter()
                .map(move |mode| (theme, mode))
        })
    }

    #[test]
    fn every_stock_keeps_the_ink_ladder_readable() {
        for (theme, mode) in combinations() {
            let pal = theme.palette(mode);
            let at = |color| contrast(color, pal.bg);
            assert!(
                at(pal.ink) >= 12.0,
                "{theme:?}/{mode:?} ink {:.2}",
                at(pal.ink)
            );
            assert!(
                at(pal.soft) >= 7.0,
                "{theme:?}/{mode:?} soft {:.2}",
                at(pal.soft)
            );
            assert!(
                at(pal.dim) >= 4.5,
                "{theme:?}/{mode:?} dim {:.2}",
                at(pal.dim)
            );
            assert!(
                at(pal.faint) >= 2.2,
                "{theme:?}/{mode:?} faint {:.2}",
                at(pal.faint)
            );
            assert!(
                at(pal.accent) >= 3.0,
                "{theme:?}/{mode:?} accent {:.2}",
                at(pal.accent)
            );
            // Status words are printed too, so colour is a restatement rather
            // than the carrier. That puts these on the same floor as the accent
            // (3.0), not the 4.5 that body text needs: the ochre that belongs to
            // kraft is 3.57:1 on its own paper and still reads as a word.
            assert!(at(pal.ok) >= 3.0, "{theme:?}/{mode:?} ok {:.2}", at(pal.ok));
            assert!(
                at(pal.warn) >= 3.0,
                "{theme:?}/{mode:?} warn {:.2}",
                at(pal.warn)
            );
            // `rule` sits below `faint` by design: it divides, it does not speak.
            assert!(
                at(pal.rule) < at(pal.faint),
                "{theme:?}/{mode:?} rule is louder than faint"
            );
        }
    }

    #[test]
    fn terminal_mode_prints_the_night_sheet() {
        for theme in PaperTheme::ALL {
            let mut expected = theme.palette(ThemeMode::Dark);
            expected.transparent = true;
            assert_eq!(
                theme.palette(ThemeMode::Terminal),
                expected,
                "{theme:?}: terminal must not invent a third palette"
            );
        }
    }

    #[test]
    fn the_slip_stays_closer_to_the_reader_than_the_page() {
        for (theme, mode) in combinations() {
            let pal = theme.palette(mode);
            assert!(
                luminance(pal.sheet) > luminance(pal.bg),
                "{theme:?}/{mode:?}: the sheet must lift off the page"
            );
        }
    }
}
