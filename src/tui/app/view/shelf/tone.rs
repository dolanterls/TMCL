//! The `shelf` palette: five bindings, each with a dark and a light face.
//!
//! The scheme is one axis and `ThemeMode` the other, the same split `paper`
//! uses, so adding a binding means adding two faces here. `Terminal` reuses a
//! scheme's dark face and withdraws its fills; a terminal has no single
//! background to derive a sheet from.

use ratatui::style::Color;

use crate::core::config::models::{ShelfTheme, ThemeMode};
use crate::tui::app::view::sheet::Sheet;
use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub text: Color,
    pub dim: Color,
    pub faint: Color,
    pub accent: Color,
    pub accent_light: Color,
    pub on_accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub error: Color,
    /// Top-left and bottom-right ends of the wordmark ramp.
    pub wordmark_light: Color,
    pub wordmark_deep: Color,
    /// `跟随终端`: the fills are withdrawn so the terminal's own background
    /// shows through. The ink stays this sheet's.
    pub transparent: bool,
}

/// One face before it becomes a [`Palette`]: the thirteen slots, without the
/// transparency flag that only `Terminal` flips.
#[derive(Debug, Clone, Copy)]
struct Ink {
    bg: Color,
    surface: Color,
    text: Color,
    dim: Color,
    faint: Color,
    accent: Color,
    accent_light: Color,
    on_accent: Color,
    ok: Color,
    warn: Color,
    error: Color,
    wordmark_light: Color,
    wordmark_deep: Color,
}

impl Ink {
    const fn sheet(self, transparent: bool) -> Palette {
        Palette {
            bg: self.bg,
            surface: self.surface,
            text: self.text,
            dim: self.dim,
            faint: self.faint,
            accent: self.accent,
            accent_light: self.accent_light,
            on_accent: self.on_accent,
            ok: self.ok,
            warn: self.warn,
            error: self.error,
            wordmark_light: self.wordmark_light,
            wordmark_deep: self.wordmark_deep,
            transparent,
        }
    }
}

/// Lavender on indigo: the sheet shelf shipped with, and still the default.
mod lavender {
    use ratatui::style::Color;

    use super::Ink;

    pub(super) const DARK: Ink = Ink {
        bg: Color::Rgb(0x12, 0x10, 0x1C),
        surface: Color::Rgb(0x1E, 0x1A, 0x2E),
        text: Color::Rgb(0xE8, 0xE4, 0xF4),
        // Deliberately lighter than tuiba's `#6F6888`: that value is 3.59:1 on
        // this background, below the 4.5:1 floor the rest of TMCL uses.
        dim: Color::Rgb(0x8A, 0x82, 0xA8),
        faint: Color::Rgb(0x5C, 0x54, 0x74),
        accent: Color::Rgb(0xA8, 0x96, 0xFF),
        accent_light: Color::Rgb(0xC4, 0xB6, 0xFF),
        on_accent: Color::Rgb(0x12, 0x10, 0x1C),
        ok: Color::Rgb(0x8C, 0xE0, 0xA0),
        warn: Color::Rgb(0xF0, 0xB8, 0x6C),
        error: Color::Rgb(0xF0, 0x6E, 0x78),
        // Not `accent_light` → `accent` directly: those two are 1.24:1 apart in
        // luminance and the ramp would read as a flat fill. Lifting the top into
        // `text` and sinking the bottom a fifth into `bg` gives the mark a light
        // source instead of a second heading.
        wordmark_light: Color::Rgb(0xD4, 0xCB, 0xFA),
        wordmark_deep: Color::Rgb(0x8A, 0x7B, 0xD2),
    };

    /// The same lavender, moved onto paper: the accent darkens to keep its
    /// 4.5:1, `on_accent` flips to white, and the ramp sinks far enough to read
    /// on a white page (the dark sheet's `#D4CBFA` is 2.4:1 here).
    pub(super) const LIGHT: Ink = Ink {
        bg: Color::Rgb(0xF4, 0xF1, 0xFB),
        surface: Color::Rgb(0xFF, 0xFF, 0xFF),
        text: Color::Rgb(0x1B, 0x17, 0x30),
        dim: Color::Rgb(0x5F, 0x58, 0x78),
        faint: Color::Rgb(0x8A, 0x82, 0xA0),
        accent: Color::Rgb(0x6A, 0x4F, 0xE0),
        accent_light: Color::Rgb(0x8F, 0x79, 0xF2),
        on_accent: Color::Rgb(0xFF, 0xFF, 0xFF),
        ok: Color::Rgb(0x2E, 0x7D, 0x46),
        warn: Color::Rgb(0x96, 0x60, 0x1A),
        error: Color::Rgb(0xB3, 0x26, 0x1E),
        wordmark_light: Color::Rgb(0x7B, 0x5C, 0xE8),
        wordmark_deep: Color::Rgb(0x4A, 0x32, 0xA8),
    };
}

/// Moss: a forest page with a mint accent. The coolest sheet in the preset and
/// the quietest one to read for a long stretch.
mod moss {
    use ratatui::style::Color;

    use super::Ink;

    pub(super) const DARK: Ink = Ink {
        bg: Color::Rgb(0x0F, 0x17, 0x13),
        surface: Color::Rgb(0x18, 0x23, 0x1D),
        text: Color::Rgb(0xE6, 0xF2, 0xEA),
        dim: Color::Rgb(0x93, 0xAC, 0x9D),
        faint: Color::Rgb(0x5E, 0x73, 0x68),
        accent: Color::Rgb(0x86, 0xD9, 0xA8),
        accent_light: Color::Rgb(0xA9, 0xE8, 0xC3),
        on_accent: Color::Rgb(0x0F, 0x17, 0x13),
        ok: Color::Rgb(0x8C, 0xE0, 0xA0),
        warn: Color::Rgb(0xE8, 0xB9, 0x6C),
        error: Color::Rgb(0xF0, 0x7A, 0x78),
        wordmark_light: Color::Rgb(0xCB, 0xEB, 0xD8),
        wordmark_deep: Color::Rgb(0x7F, 0xA9, 0x8F),
    };

    pub(super) const LIGHT: Ink = Ink {
        bg: Color::Rgb(0xF1, 0xF7, 0xF2),
        surface: Color::Rgb(0xFF, 0xFF, 0xFF),
        text: Color::Rgb(0x14, 0x23, 0x1B),
        dim: Color::Rgb(0x4F, 0x65, 0x58),
        faint: Color::Rgb(0x85, 0x99, 0x8C),
        accent: Color::Rgb(0x2C, 0x7A, 0x55),
        accent_light: Color::Rgb(0x3E, 0x9A, 0x6C),
        on_accent: Color::Rgb(0xFF, 0xFF, 0xFF),
        ok: Color::Rgb(0x2E, 0x7D, 0x46),
        warn: Color::Rgb(0x8A, 0x5A, 0x16),
        error: Color::Rgb(0xB3, 0x26, 0x1E),
        // 3.20:1, the tightest ramp end in the preset; the floor is 3.0.
        wordmark_light: Color::Rgb(0x3E, 0x9A, 0x6C),
        wordmark_deep: Color::Rgb(0x1F, 0x5C, 0x3F),
    };
}

/// Slate: blue-grey page, ice-blue accent. Reads like a technical manual, and
/// is the furthest from lavender's hue without leaving the cool half.
mod slate {
    use ratatui::style::Color;

    use super::Ink;

    pub(super) const DARK: Ink = Ink {
        bg: Color::Rgb(0x0E, 0x14, 0x1B),
        surface: Color::Rgb(0x18, 0x21, 0x2B),
        text: Color::Rgb(0xE4, 0xED, 0xF6),
        dim: Color::Rgb(0x93, 0xA6, 0xB8),
        faint: Color::Rgb(0x5E, 0x71, 0x82),
        accent: Color::Rgb(0x88, 0xB8, 0xE8),
        accent_light: Color::Rgb(0xAD, 0xD0, 0xF2),
        on_accent: Color::Rgb(0x0E, 0x14, 0x1B),
        ok: Color::Rgb(0x8C, 0xE0, 0xA0),
        warn: Color::Rgb(0xED, 0xBB, 0x72),
        error: Color::Rgb(0xF2, 0x7A, 0x80),
        wordmark_light: Color::Rgb(0xCF, 0xE2, 0xF4),
        wordmark_deep: Color::Rgb(0x7F, 0x9F, 0xBE),
    };

    pub(super) const LIGHT: Ink = Ink {
        bg: Color::Rgb(0xF1, 0xF5, 0xF9),
        surface: Color::Rgb(0xFF, 0xFF, 0xFF),
        text: Color::Rgb(0x16, 0x20, 0x2B),
        dim: Color::Rgb(0x51, 0x64, 0x75),
        faint: Color::Rgb(0x87, 0x99, 0xA8),
        accent: Color::Rgb(0x2A, 0x6B, 0xA8),
        accent_light: Color::Rgb(0x3E, 0x86, 0xC6),
        on_accent: Color::Rgb(0xFF, 0xFF, 0xFF),
        ok: Color::Rgb(0x2E, 0x7D, 0x46),
        warn: Color::Rgb(0x8A, 0x5A, 0x16),
        error: Color::Rgb(0xB3, 0x26, 0x1E),
        wordmark_light: Color::Rgb(0x3E, 0x86, 0xC6),
        wordmark_deep: Color::Rgb(0x1E, 0x4E, 0x7C),
    };
}

/// Leather: a brown page with a copper accent. `warn` is a yellower amber than
/// the stock one because the stock amber and the copper accent share a hue at
/// the same weight, and a warning would read as a heading.
mod leather {
    use ratatui::style::Color;

    use super::Ink;

    pub(super) const DARK: Ink = Ink {
        bg: Color::Rgb(0x17, 0x11, 0x0C),
        surface: Color::Rgb(0x24, 0x1B, 0x13),
        text: Color::Rgb(0xF2, 0xE8, 0xDA),
        dim: Color::Rgb(0xB3, 0x9C, 0x82),
        faint: Color::Rgb(0x75, 0x62, 0x4B),
        accent: Color::Rgb(0xE0, 0x8A, 0x4E),
        accent_light: Color::Rgb(0xF0, 0xB2, 0x7C),
        on_accent: Color::Rgb(0x17, 0x11, 0x0C),
        ok: Color::Rgb(0x93, 0xD3, 0xA6),
        warn: Color::Rgb(0xE7, 0xCE, 0x72),
        error: Color::Rgb(0xEF, 0x7C, 0x77),
        wordmark_light: Color::Rgb(0xF0, 0xD6, 0xAE),
        wordmark_deep: Color::Rgb(0xB0, 0x8A, 0x5E),
    };

    pub(super) const LIGHT: Ink = Ink {
        bg: Color::Rgb(0xF8, 0xF3, 0xEA),
        surface: Color::Rgb(0xFF, 0xFF, 0xFF),
        text: Color::Rgb(0x24, 0x1A, 0x10),
        dim: Color::Rgb(0x6B, 0x59, 0x46),
        faint: Color::Rgb(0x9C, 0x88, 0x71),
        accent: Color::Rgb(0x9A, 0x4E, 0x12),
        accent_light: Color::Rgb(0xB4, 0x64, 0x1F),
        on_accent: Color::Rgb(0xFF, 0xFF, 0xFF),
        ok: Color::Rgb(0x2E, 0x7D, 0x46),
        warn: Color::Rgb(0x7A, 0x5A, 0x0F),
        error: Color::Rgb(0xB3, 0x26, 0x1E),
        wordmark_light: Color::Rgb(0xB4, 0x64, 0x1F),
        wordmark_deep: Color::Rgb(0x6B, 0x3A, 0x0C),
    };
}

/// Rose: a plum page under a rose accent. `error` is a full red rather than a
/// tint of the accent, so a failed task is not mistaken for a highlighted one.
mod rose {
    use ratatui::style::Color;

    use super::Ink;

    pub(super) const DARK: Ink = Ink {
        bg: Color::Rgb(0x17, 0x0F, 0x16),
        surface: Color::Rgb(0x25, 0x1A, 0x24),
        text: Color::Rgb(0xF4, 0xE7, 0xF1),
        dim: Color::Rgb(0xB5, 0x9A, 0xAD),
        faint: Color::Rgb(0x76, 0x5E, 0x70),
        accent: Color::Rgb(0xE5, 0x8F, 0xB8),
        accent_light: Color::Rgb(0xF2, 0xB4, 0xD0),
        on_accent: Color::Rgb(0x17, 0x0F, 0x16),
        ok: Color::Rgb(0x93, 0xD3, 0xA6),
        warn: Color::Rgb(0xE9, 0xC0, 0x7A),
        error: Color::Rgb(0xF0, 0x72, 0x7C),
        wordmark_light: Color::Rgb(0xF2, 0xCD, 0xE0),
        wordmark_deep: Color::Rgb(0xB0, 0x7E, 0x9A),
    };

    pub(super) const LIGHT: Ink = Ink {
        bg: Color::Rgb(0xF9, 0xF1, 0xF6),
        surface: Color::Rgb(0xFF, 0xFF, 0xFF),
        text: Color::Rgb(0x24, 0x16, 0x24),
        dim: Color::Rgb(0x6B, 0x57, 0x68),
        faint: Color::Rgb(0x9C, 0x84, 0x97),
        accent: Color::Rgb(0xA3, 0x2E, 0x6E),
        accent_light: Color::Rgb(0xBF, 0x47, 0x86),
        on_accent: Color::Rgb(0xFF, 0xFF, 0xFF),
        ok: Color::Rgb(0x2E, 0x7D, 0x46),
        warn: Color::Rgb(0x7A, 0x5A, 0x0F),
        error: Color::Rgb(0xB3, 0x26, 0x1E),
        wordmark_light: Color::Rgb(0xBF, 0x47, 0x86),
        wordmark_deep: Color::Rgb(0x7A, 0x1F, 0x50),
    };
}

/// The three faces, indexed by [`ShelfTheme::ALL`].
///
/// Written out one entry per line rather than derived, because the pairing is
/// the whole design: `Moss`'s dark face belongs with `Moss`'s light one, and a
/// table built from two independent lists is one edit away from pairing
/// lavender's page with rose's accent.
const DARK: [Palette; 5] = [
    lavender::DARK.sheet(false),
    moss::DARK.sheet(false),
    slate::DARK.sheet(false),
    leather::DARK.sheet(false),
    rose::DARK.sheet(false),
];

const LIGHT: [Palette; 5] = [
    lavender::LIGHT.sheet(false),
    moss::LIGHT.sheet(false),
    slate::LIGHT.sheet(false),
    leather::LIGHT.sheet(false),
    rose::LIGHT.sheet(false),
];

/// The dark faces with their fills withdrawn, for `跟随终端`. Only the flag
/// differs: the ink, the accent ladder and the wordmark ramp are the dark
/// sheet's, because a terminal a launcher runs on is dark far more often than
/// not.
const TERMINAL: [Palette; 5] = [
    lavender::DARK.sheet(true),
    moss::DARK.sheet(true),
    slate::DARK.sheet(true),
    leather::DARK.sheet(true),
    rose::DARK.sheet(true),
];

pub fn palette(theme: ShelfTheme, mode: ThemeMode) -> &'static Palette {
    let index = theme_index(theme);
    match mode {
        ThemeMode::Light => &LIGHT[index],
        // A terminal has no single background to derive a sheet from, but it
        // does have one thing to give: its own paint.
        ThemeMode::Dark => &DARK[index],
        ThemeMode::Terminal => &TERMINAL[index],
    }
}

/// The light sheet of `theme` with its fills withdrawn.
pub fn palette_on_light_terminal(theme: ShelfTheme) -> &'static Palette {
    &TERMINAL_LIGHT[theme_index(theme)]
}

fn theme_index(theme: ShelfTheme) -> usize {
    ShelfTheme::ALL
        .iter()
        .position(|candidate| *candidate == theme)
        .unwrap_or(0)
}

/// The light faces with their fills withdrawn, for `跟随终端` on a light
/// terminal: the same answer the dark terminal gets, mirrored.
const TERMINAL_LIGHT: [Palette; 5] = [
    lavender::LIGHT.sheet(true),
    moss::LIGHT.sheet(true),
    slate::LIGHT.sheet(true),
    leather::LIGHT.sheet(true),
    rose::LIGHT.sheet(true),
];

impl Sheet for Palette {
    fn fills(&self) -> [Option<Color>; 4] {
        [Some(self.bg), Some(self.surface), None, None]
    }
}

/// The two ends of the wordmark ramp, per sheet.
pub fn wordmark_ramp(pal: &Palette) -> (Color, Color) {
    (pal.wordmark_light, pal.wordmark_deep)
}

/// A [`Theme`] carrying the shelf colours, for the shared overlay painters.
///
/// Help and the detail popup draw through the classic chrome and need only the
/// semantic slots. Derived from the palette being painted rather than from
/// `(theme, mode)`, because `跟随终端` on a light terminal swaps in
/// [`TERMINAL_LIGHT`], which the mode alone cannot name.
pub fn theme_of(pal: &Palette) -> Theme {
    Theme {
        bg: pal.bg,
        fg: pal.text,
        dim: pal.dim,
        accent: pal.accent,
        label: pal.accent,
        key: pal.accent,
        selection: pal.accent,
        success: pal.ok,
        error: pal.error,
        field_bg: pal.surface,
    }
}

#[cfg(test)]
mod tests {
    use super::{Palette, palette, theme_of, wordmark_ramp};
    use crate::core::config::models::{ShelfTheme, ThemeMode};

    const MODES: [(ThemeMode, &str); 2] = [(ThemeMode::Dark, "dark"), (ThemeMode::Light, "light")];

    fn luminance(color: ratatui::style::Color) -> f32 {
        let ratatui::style::Color::Rgb(red, green, blue) = color else {
            panic!("shelf palette must be RGB: {color:?}");
        };
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue)
    }

    fn contrast(foreground: ratatui::style::Color, background: ratatui::style::Color) -> f32 {
        let (a, b) = (luminance(foreground), luminance(background));
        let (high, low) = if a > b { (a, b) } else { (b, a) };
        (high + 0.05) / (low + 0.05)
    }

    /// Every face is held to the same floors: a sheet that borrows another's
    /// accent drop stops being readable, and the scheme is only a colour choice
    /// until it fails one of these.
    #[test]
    fn every_scheme_keeps_its_text_and_accent_legible() {
        for theme in ShelfTheme::ALL {
            for (mode, label) in MODES {
                let pal = palette(theme, mode);
                let name = format!("{} {label}", theme.code());
                assert!(
                    contrast(pal.text, pal.bg) >= 12.0,
                    "{name} text: {}",
                    contrast(pal.text, pal.bg)
                );
                assert!(
                    contrast(pal.dim, pal.bg) >= 4.5,
                    "{name} dim: {}",
                    contrast(pal.dim, pal.bg)
                );
                assert!(
                    contrast(pal.accent, pal.bg) >= 4.5,
                    "{name} accent: {}",
                    contrast(pal.accent, pal.bg)
                );
                assert!(
                    contrast(pal.on_accent, pal.accent) >= 4.5,
                    "{name} on accent: {}",
                    contrast(pal.on_accent, pal.accent)
                );
                // Cards and cover panels put the same ink on `surface`, which
                // sits between the page and the text; a sheet tuned only
                // against `bg` can still drown there.
                assert!(
                    contrast(pal.text, pal.surface) >= 12.0,
                    "{name} text on surface: {}",
                    contrast(pal.text, pal.surface)
                );
                assert!(
                    contrast(pal.dim, pal.surface) >= 4.5,
                    "{name} dim on surface: {}",
                    contrast(pal.dim, pal.surface)
                );
                // `faint` draws the rules and the cover blocks, so it only has
                // to be visible, not readable.
                assert!(
                    contrast(pal.faint, pal.bg) >= 1.8,
                    "{name} faint: {}",
                    contrast(pal.faint, pal.bg)
                );
            }
        }
    }

    #[test]
    fn state_colours_stay_legible_on_every_sheet() {
        for theme in ShelfTheme::ALL {
            for (mode, label) in MODES {
                let pal = palette(theme, mode);
                for (name, colour) in [("ok", pal.ok), ("warn", pal.warn), ("error", pal.error)] {
                    assert!(
                        contrast(colour, pal.bg) >= 3.0,
                        "{} {label} {name}: {}",
                        theme.code(),
                        contrast(colour, pal.bg)
                    );
                }
            }
        }
    }

    fn ramp_is_visible(pal: &Palette, name: &str) {
        let (top, bottom) = wordmark_ramp(pal);
        assert!(
            contrast(top, pal.bg) >= 3.0,
            "{name}: the light end fades into the page: {}",
            contrast(top, pal.bg)
        );
        assert!(
            contrast(bottom, pal.bg) >= 3.0,
            "{name}: the deep end fades into the page: {}",
            contrast(bottom, pal.bg)
        );
        assert!(
            contrast(top, bottom) >= 1.6,
            "{name}: the two ends are too close to read as a ramp: {}",
            contrast(top, bottom)
        );
        assert!(
            luminance(top) > luminance(bottom),
            "{name}: the ramp has to run lighter at the top"
        );
    }

    #[test]
    fn the_wordmark_ramp_works_on_every_sheet() {
        for theme in ShelfTheme::ALL {
            for (mode, label) in MODES {
                ramp_is_visible(palette(theme, mode), &format!("{} {label}", theme.code()));
            }
        }
    }

    #[test]
    fn the_mode_picks_the_face_and_terminal_withdraws_the_fills() {
        for theme in ShelfTheme::ALL {
            let dark = palette(theme, ThemeMode::Dark);
            let light = palette(theme, ThemeMode::Light);
            let terminal = palette(theme, ThemeMode::Terminal);

            assert_ne!(dark, light, "{}: one sheet cannot serve both", theme.code());
            assert!(
                terminal.transparent,
                "{}: terminal must give its fills up",
                theme.code()
            );
            assert_eq!(
                terminal.bg,
                dark.bg,
                "{}: terminal must keep the dark ink",
                theme.code()
            );
            assert_eq!(terminal.accent, dark.accent, "{}", theme.code());
            assert!(!dark.transparent && !light.transparent);
        }
    }

    /// The five schemes have to differ in the two slots a glance reads first.
    /// A copy-paste that leaves `Moss` pointing at lavender's page would pass
    /// every contrast floor above.
    #[test]
    fn every_scheme_paints_its_own_page() {
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            for (index, theme) in ShelfTheme::ALL.iter().enumerate() {
                for other in &ShelfTheme::ALL[index + 1..] {
                    let (a, b) = (palette(*theme, mode), palette(*other, mode));
                    assert_ne!(
                        a.bg,
                        b.bg,
                        "{} and {} share a page",
                        theme.code(),
                        other.code()
                    );
                    assert_ne!(
                        a.accent,
                        b.accent,
                        "{} and {} share an accent",
                        theme.code(),
                        other.code()
                    );
                }
            }
        }
    }

    /// The shipped sheet is the one the default config selects. This is the
    /// only test that pins a literal colour, and it exists so a reordering of
    /// the tables above cannot silently repaint every existing install.
    #[test]
    fn lavender_is_still_the_sheet_it_shipped_with() {
        let pal = palette(ShelfTheme::default(), ThemeMode::Dark);
        assert_eq!(pal.bg, ratatui::style::Color::Rgb(0x12, 0x10, 0x1C));
        assert_eq!(pal.accent, ratatui::style::Color::Rgb(0xA8, 0x96, 0xFF));
        assert_eq!(ShelfTheme::default(), ShelfTheme::Lavender);
    }

    #[test]
    fn the_shared_theme_maps_every_slot_to_the_palette() {
        for theme in ShelfTheme::ALL {
            for (mode, _) in MODES {
                let pal = palette(theme, mode);
                let mapped = theme_of(pal);
                assert_eq!(mapped.bg, pal.bg);
                assert_eq!(mapped.fg, pal.text);
                assert_eq!(mapped.dim, pal.dim);
                assert_eq!(mapped.accent, pal.accent);
                assert_eq!(mapped.error, pal.error);
                assert_eq!(mapped.field_bg, pal.surface);
            }
        }
    }
}
