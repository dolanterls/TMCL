//! Palette derivation for the `nightlight` preset.
//!
//! One accent seed plus an optional second hue and an optional design base
//! derive the whole ladder: a single hue family, no per-page colours. Ratios are
//! fixed, so a new theme is three optional colours in [`THEMES`].
//!
//! Clean-room notice: the ratios below were chosen by looking at how the
//! reference implementation behaves, then re-derived and verified here. No
//! source code or constant table is copied from `nightlightd` (GPL-3.0).

use ratatui::style::Color;

use crate::core::config::models::{AccentColor, NightlightTheme, ThemeMode};
use crate::tui::app::view::sheet::Sheet;

/// A colour before it becomes a ratatui [`Color`].
pub type Rgb = (u8, u8, u8);

pub const BLACK: Rgb = (0, 0, 0);
pub const WHITE: Rgb = (255, 255, 255);

/// Status colours. The dark ladder uses the raw values; the light ladder pushes
/// them towards black so they stay legible on a near-white surface.
const OK_RGB: Rgb = (0x5A, 0xDC, 0x78);
const ERR_RGB: Rgb = (0xF0, 0x5A, 0x5A);

/// Seed used when a theme has no fixed accent and the caller passes none, as in
/// `live` with the clock unavailable. Matches the `accent_color: orange` row.
const FALLBACK_ACCENT: Rgb = (0xFF, 0xAA, 0x5A);

/// Linear interpolation with rounding, the only blending primitive used here.
pub fn mix(from: Rgb, to: Rgb, ratio: f64) -> Rgb {
    let channel = |a: u8, b: u8| -> u8 {
        let value = f64::from(a) + (f64::from(b) - f64::from(a)) * ratio;
        value.round().clamp(0.0, 255.0) as u8
    };
    (
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
    )
}

pub const fn to_color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

pub const fn rgb_of(color: Color) -> Option<Rgb> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        _ => None,
    }
}

#[cfg(test)]
fn channel_luminance(value: u8) -> f64 {
    let value = f64::from(value) / 255.0;
    if value <= 0.039_28 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance. Non-RGB colours have no defined luminance here, so
/// they report zero; the preset only ever builds RGB palettes.
#[cfg(test)]
pub fn luminance(color: Color) -> f64 {
    match rgb_of(color) {
        Some((r, g, b)) => {
            0.2126 * channel_luminance(r)
                + 0.7152 * channel_luminance(g)
                + 0.0722 * channel_luminance(b)
        }
        None => 0.0,
    }
}

/// WCAG contrast ratio between two colours, in `1.0..=21.0`.
#[cfg(test)]
pub fn contrast(a: Color, b: Color) -> f64 {
    let (left, right) = (luminance(a), luminance(b));
    let (lighter, darker) = if left >= right {
        (left, right)
    } else {
        (right, left)
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// The resolved colours for one theme in one mode. Drawing code reads these
/// fields and never has to know whether the ladder is dark or light:
/// [`Palette::accent`] is already adjusted for the mode.
///
/// Three surface steps, not four: `bg` (page) → `surface` (cards) → `raised`
/// (dialogs, toasts). A fourth "hero" step was designed but never painted, and
/// the ladder only guarantees `muted`/`faint` against `surface`: on a lighter
/// panel `muted` drops to 2.1–3.3:1, under its own 2.8 floor. It was removed
/// rather than left as a field nothing could use with the current foregrounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub raised: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub accent: Color,
    pub accent2: Color,
    pub ok: Color,
    pub err: Color,
    pub selection: Color,
    /// `跟随终端`: the fills are withdrawn so the terminal's own background
    /// shows through. The ink stays this ladder's.
    pub transparent: bool,
}

/// The three inputs a theme contributes. Everything else is derived.
#[derive(Debug, Clone, Copy)]
pub struct ThemeSpec {
    pub theme: NightlightTheme,
    /// Fixed accent for themes that do not follow the clock or the user's
    /// accent colour. `live` leaves this empty and receives a seed instead.
    pub accent: Option<Rgb>,
    pub secondary: Option<Rgb>,
    /// Design base for the dark ladder; `None` derives it from the seed.
    pub dark_base: Option<Rgb>,
    /// Design base for the light ladder; `None` derives a tinted paper white.
    pub light_base: Option<Rgb>,
}

pub const THEMES: [ThemeSpec; 8] = [
    ThemeSpec {
        theme: NightlightTheme::Live,
        accent: None,
        secondary: Some((0x82, 0xAA, 0xBE)),
        dark_base: None,
        light_base: None,
    },
    ThemeSpec {
        theme: NightlightTheme::Tokyo,
        accent: Some((0x7A, 0xA2, 0xF7)),
        secondary: Some((0xBB, 0x9A, 0xF7)),
        dark_base: Some((0x1A, 0x1B, 0x26)),
        light_base: Some((0xE6, 0xE7, 0xED)),
    },
    ThemeSpec {
        theme: NightlightTheme::Mocha,
        accent: Some((0xCB, 0xA6, 0xF7)),
        secondary: Some((0x94, 0xE2, 0xD5)),
        dark_base: Some((0x1E, 0x1E, 0x2E)),
        light_base: Some((0xEF, 0xF1, 0xF5)),
    },
    ThemeSpec {
        theme: NightlightTheme::Nord,
        accent: Some((0x88, 0xC0, 0xD0)),
        secondary: Some((0xEB, 0xCB, 0x8B)),
        dark_base: Some((0x2E, 0x34, 0x40)),
        light_base: Some((0xEC, 0xEF, 0xF4)),
    },
    ThemeSpec {
        theme: NightlightTheme::Gruvbox,
        accent: Some((0xFA, 0xBD, 0x2F)),
        secondary: Some((0x8E, 0xC0, 0x7C)),
        dark_base: Some((0x28, 0x28, 0x28)),
        light_base: Some((0xFB, 0xF1, 0xC7)),
    },
    ThemeSpec {
        theme: NightlightTheme::Synth,
        accent: Some((0xFF, 0x6E, 0xC7)),
        secondary: Some((0x64, 0xDC, 0xFF)),
        dark_base: Some((0x24, 0x17, 0x36)),
        light_base: Some((0xF5, 0xED, 0xF7)),
    },
    ThemeSpec {
        theme: NightlightTheme::Ember,
        accent: Some((0xFF, 0xAA, 0x5A)),
        secondary: Some((0x6C, 0xBE, 0xB4)),
        dark_base: None,
        light_base: None,
    },
    ThemeSpec {
        theme: NightlightTheme::Phosphor,
        accent: Some((0x33, 0xFF, 0x66)),
        secondary: None,
        dark_base: None,
        light_base: None,
    },
];

/// The six accent colours the settings page offers, as seeds for `live` when
/// the clock-driven accent is switched off. Each value is one already used by
/// the themes above, so no new hues enter the system.
pub const fn accent_seed(color: AccentColor) -> Rgb {
    match color {
        AccentColor::Orange => (0xFF, 0xAA, 0x5A),
        AccentColor::Cyan => (0x88, 0xC0, 0xD0),
        AccentColor::Blue => (0x7A, 0xA2, 0xF7),
        AccentColor::Green => (0x8E, 0xC0, 0x7C),
        AccentColor::Yellow => (0xFA, 0xBD, 0x2F),
        AccentColor::Magenta => (0xFF, 0x6E, 0xC7),
    }
}

/// Derive the ladder for `theme`.
///
/// `seed` replaces the accent only for themes that leave `accent` empty: `live`
/// follows the clock or the user's accent colour that way, while fixed designs
/// stay fixed. `ThemeMode::Terminal` is treated as dark, because a terminal
/// palette has no single accent to derive from.
pub fn palette(
    theme: NightlightTheme,
    mode: ThemeMode,
    seed: Option<Color>,
    high_contrast: bool,
) -> Palette {
    let spec = THEMES[theme_index(theme)];
    // The table is indexed, not searched, so this is the only thing tying an
    // entry to the theme it describes. Debug builds catch a mis-sorted edit.
    debug_assert_eq!(THEMES[theme_index(theme)].theme, theme);
    let seed = match spec.accent {
        Some(fixed) => fixed,
        None => seed.and_then(rgb_of).unwrap_or(FALLBACK_ACCENT),
    };
    let mut pal = match mode {
        ThemeMode::Light => light_ladder(spec, seed, high_contrast),
        ThemeMode::Dark | ThemeMode::Terminal => dark_ladder(spec, seed, high_contrast),
    };
    // `Terminal` keeps the dark ladder and gives up every fill. The terminal a
    // launcher runs on is dark far more often than not, and an inverted sheet
    // would need a way to ask the terminal for its background (OSC 11) that the
    // rest of the app does not have.
    pal.transparent = mode == ThemeMode::Terminal;
    pal
}

impl Sheet for Palette {
    fn fills(&self) -> [Option<Color>; 4] {
        [
            Some(self.bg),
            Some(self.surface),
            Some(self.raised),
            Some(self.selection),
        ]
    }
}

fn theme_index(theme: NightlightTheme) -> usize {
    NightlightTheme::ALL
        .iter()
        .position(|candidate| *candidate == theme)
        .unwrap_or(0)
}

fn dark_ladder(spec: ThemeSpec, seed: Rgb, high_contrast: bool) -> Palette {
    let (bg, surface, raised, muted, faint) = match spec.dark_base {
        Some(base) => (
            base,
            mix(base, WHITE, 0.06),
            mix(base, WHITE, 0.14),
            mix(base, seed, 0.62),
            mix(base, seed, 0.34),
        ),
        None => (
            mix(BLACK, seed, 0.10),
            mix(BLACK, seed, 0.15),
            mix(BLACK, seed, 0.21),
            mix(BLACK, seed, 0.62),
            mix(BLACK, seed, 0.32),
        ),
    };
    let text = mix(WHITE, seed, 0.16);
    // `faint` keeps the original muted tone, so the two never collapse into
    // each other.
    let (muted, faint) = if high_contrast {
        (mix(muted, text, 0.35), muted)
    } else {
        (muted, faint)
    };
    let accent2 = spec.secondary.unwrap_or_else(|| mix(WHITE, seed, 0.55));
    Palette {
        bg: to_color(bg),
        surface: to_color(surface),
        raised: to_color(raised),
        text: to_color(text),
        muted: to_color(muted),
        faint: to_color(faint),
        accent: to_color(seed),
        accent2: to_color(accent2),
        ok: to_color(OK_RGB),
        err: to_color(ERR_RGB),
        selection: to_color(mix(surface, seed, 0.28)),
        transparent: false,
    }
}

fn light_ladder(spec: ThemeSpec, seed: Rgb, high_contrast: bool) -> Palette {
    // The seed has to be pushed towards black before it can carry text: a
    // bright accent on a near-white surface is unreadable.
    let accent_ink = mix(seed, BLACK, 0.55);
    let accent2_ink = match spec.secondary {
        Some(secondary) => mix(secondary, BLACK, 0.50),
        // Phosphor is the only theme without a second hue. The fallback is
        // pushed towards the ink rather than white, because white would land at
        // 3.20:1 on its paper, below the text floor.
        None => mix(accent_ink, WHITE, 0.12),
    };
    let (bg, surface, raised) = match spec.light_base {
        Some(base) => (base, mix(base, WHITE, 0.60), mix(base, WHITE, 0.85)),
        None => (mix(WHITE, seed, 0.10), mix(WHITE, seed, 0.02), WHITE),
    };
    let text = mix(BLACK, seed, 0.28);
    let (muted, faint) = if high_contrast {
        (mix(WHITE, text, 0.85), mix(WHITE, text, 0.68))
    } else {
        (mix(WHITE, text, 0.72), mix(WHITE, text, 0.54))
    };
    Palette {
        bg: to_color(bg),
        surface: to_color(surface),
        raised: to_color(raised),
        text: to_color(text),
        muted: to_color(muted),
        faint: to_color(faint),
        accent: to_color(accent_ink),
        accent2: to_color(accent2_ink),
        // .45, not .35: at .35 the success green only reached 3.97:1 on the
        // lightest paper, weaker than the error red it sits next to.
        ok: to_color(mix(OK_RGB, BLACK, 0.45)),
        err: to_color(mix(ERR_RGB, BLACK, 0.30)),
        selection: to_color(mix(WHITE, seed, 0.22)),
        transparent: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Field order used by the expectation tables: the three ladder steps plus
    /// the five foreground tones. There is no fourth step: see `Palette`.
    fn ladder(palette: Palette) -> [Rgb; 8] {
        [
            rgb_of(palette.accent).unwrap(),
            rgb_of(palette.accent2).unwrap(),
            rgb_of(palette.bg).unwrap(),
            rgb_of(palette.surface).unwrap(),
            rgb_of(palette.raised).unwrap(),
            rgb_of(palette.muted).unwrap(),
            rgb_of(palette.faint).unwrap(),
            rgb_of(palette.text).unwrap(),
        ]
    }

    /// Dark ladder, straight from design §6.2.
    const DARK: [[u32; 8]; 8] = [
        [
            0xFFAA5A, 0x82AABE, 0x1A1109, 0x261A0E, 0x362413, 0x9E6938, 0x52361D, 0xFFF1E5,
        ],
        [
            0x7AA2F7, 0xBB9AF7, 0x1A1B26, 0x282933, 0x3A3B44, 0x566FA8, 0x3B496D, 0xEAF0FE,
        ],
        [
            0xCBA6F7, 0x94E2D5, 0x1E1E2E, 0x2C2C3B, 0x3E3E4B, 0x8972AB, 0x594C72, 0xF7F1FE,
        ],
        [
            0x88C0D0, 0xEBCB8B, 0x2E3440, 0x3B404B, 0x4B505B, 0x668B99, 0x4D6471, 0xECF5F7,
        ],
        [
            0xFABD2F, 0x8EC07C, 0x282828, 0x353535, 0x464646, 0xAA842C, 0x6F5B2A, 0xFEF4DE,
        ],
        [
            0xFF6EC7, 0x64DCFF, 0x241736, 0x312542, 0x433752, 0xAC4D90, 0x6E3567, 0xFFE8F6,
        ],
        [
            0xFFAA5A, 0x6CBEB4, 0x1A1109, 0x261A0E, 0x362413, 0x9E6938, 0x52361D, 0xFFF1E5,
        ],
        [
            0x33FF66, 0x8FFFAB, 0x051A0A, 0x08260F, 0x0B3615, 0x209E3F, 0x105221, 0xDEFFE7,
        ],
    ];

    /// Light ladder, straight from design §6.2.
    const LIGHT: [[u32; 8]; 8] = [
        [
            0x734C28, 0x41555F, 0xFFF7EF, 0xFFFDFC, 0xFFFFFF, 0x7B6A59, 0x9C8F83, 0x473019,
        ],
        [
            0x37496F, 0x5E4D7C, 0xE6E7ED, 0xF5F5F8, 0xFBFBFC, 0x606879, 0x888E9B, 0x222D45,
        ],
        [
            0x5B4B6F, 0x4A716B, 0xEFF1F5, 0xF9F9FB, 0xFDFDFE, 0x706979, 0x948E9B, 0x392E45,
        ],
        [
            0x3D565E, 0x766646, 0xECEFF4, 0xF7F9FB, 0xFCFDFD, 0x636E71, 0x8A9295, 0x26363A,
        ],
        [
            0x715515, 0x47603E, 0xFBF1C7, 0xFDF9E9, 0xFEFDF7, 0x7A6E51, 0x9B927C, 0x46350D,
        ],
        [
            0x73315A, 0x326E80, 0xF5EDF7, 0xFBF8FC, 0xFEFCFE, 0x7B5E70, 0x9C8694, 0x471F38,
        ],
        [
            0x734C28, 0x365F5A, 0xFFF7EF, 0xFFFDFC, 0xFFFFFF, 0x7B6A59, 0x9C8F83, 0x473019,
        ],
        [
            0x17732E, 0x338447, 0xEBFFF0, 0xFBFFFC, 0xFFFFFF, 0x517B5C, 0x7D9C85, 0x0E471D,
        ],
    ];

    fn expected(table: &[[u32; 8]; 8], index: usize) -> [Rgb; 8] {
        table[index].map(|value| {
            (
                ((value >> 16) & 0xFF) as u8,
                ((value >> 8) & 0xFF) as u8,
                (value & 0xFF) as u8,
            )
        })
    }

    #[test]
    fn dark_and_light_ladders_match_the_design_tables() {
        for (index, theme) in NightlightTheme::ALL.into_iter().enumerate() {
            let dark = palette(theme, ThemeMode::Dark, None, false);
            assert_eq!(ladder(dark), expected(&DARK, index), "dark {theme:?}");
            let light = palette(theme, ThemeMode::Light, None, false);
            assert_eq!(ladder(light), expected(&LIGHT, index), "light {theme:?}");
        }
    }

    /// `跟随终端` reuses the dark ladder colour for colour and only withdraws
    /// the fills: the terminal a launcher runs on is dark far more often than
    /// not, and inverting it would need a way to ask the terminal for its
    /// background.
    #[test]
    fn terminal_mode_reuses_the_dark_ladder_and_withdraws_the_fills() {
        for theme in NightlightTheme::ALL {
            let mut expected = palette(theme, ThemeMode::Dark, None, false);
            expected.transparent = true;
            assert_eq!(
                palette(theme, ThemeMode::Terminal, None, false),
                expected,
                "terminal should differ from dark only in its fills for {theme:?}"
            );
        }
    }

    #[test]
    fn dark_ladder_stays_monotonic() {
        for theme in NightlightTheme::ALL {
            let p = palette(theme, ThemeMode::Dark, None, false);
            let (bg, surface, raised) =
                (luminance(p.bg), luminance(p.surface), luminance(p.raised));
            assert!(bg < surface, "{theme:?}: bg must be darker than surface");
            assert!(surface < raised, "{theme:?}: surface must precede raised");
            let (faint, muted, text) = (
                contrast(p.faint, p.surface),
                contrast(p.muted, p.surface),
                contrast(p.text, p.surface),
            );
            assert!(
                faint < muted && muted < text,
                "{theme:?}: faint < muted < text by contrast"
            );
        }
    }

    #[test]
    fn light_ladder_stays_monotonic() {
        for theme in NightlightTheme::ALL {
            let p = palette(theme, ThemeMode::Light, None, false);
            let (bg, surface, raised) =
                (luminance(p.bg), luminance(p.surface), luminance(p.raised));
            assert!(
                raised >= surface,
                "{theme:?}: raised must float above surface"
            );
            assert!(
                surface >= bg,
                "{theme:?}: surface must be at least as light as bg"
            );
            let (faint, muted, text) = (
                contrast(p.faint, p.surface),
                contrast(p.muted, p.surface),
                contrast(p.text, p.surface),
            );
            assert!(
                faint < muted && muted < text,
                "{theme:?}: faint < muted < text by contrast"
            );
        }
    }

    #[test]
    fn both_ladders_meet_their_contrast_floors() {
        for theme in NightlightTheme::ALL {
            let dark = palette(theme, ThemeMode::Dark, None, false);
            assert!(
                contrast(dark.text, dark.surface) >= 9.3,
                "{theme:?}: dark text"
            );
            assert!(
                contrast(dark.accent, dark.surface) >= 5.1,
                "{theme:?}: dark accent"
            );
            assert!(
                contrast(dark.muted, dark.surface) >= 2.8,
                "{theme:?}: dark muted"
            );
            assert!(
                contrast(dark.faint, dark.surface) >= 1.5,
                "{theme:?}: dark faint"
            );
            // `accent2` and `ok` carry real information (data values, the
            // success state); they get the same floor as `accent`.
            assert!(
                contrast(dark.accent2, dark.surface) >= 5.0,
                "{theme:?}: dark accent2"
            );
            assert!(contrast(dark.ok, dark.surface) >= 5.0, "{theme:?}: dark ok");

            let light = palette(theme, ThemeMode::Light, None, false);
            assert!(
                contrast(light.text, light.surface) >= 10.7,
                "{theme:?}: light text"
            );
            assert!(
                contrast(light.accent, light.surface) >= 5.85,
                "{theme:?}: light accent"
            );
            assert!(
                contrast(light.muted, light.surface) >= 4.5,
                "{theme:?}: light muted"
            );
            assert!(
                contrast(light.faint, light.surface) >= 2.8,
                "{theme:?}: light faint"
            );
            assert!(
                contrast(light.accent2, light.surface) >= 4.5,
                "{theme:?}: light accent2"
            );
            assert!(
                contrast(light.ok, light.surface) >= 4.5,
                "{theme:?}: light ok"
            );
        }
    }

    #[test]
    fn high_contrast_only_lifts_the_two_dim_tones() {
        for theme in NightlightTheme::ALL {
            let dark = palette(theme, ThemeMode::Dark, None, false);
            let lifted = palette(theme, ThemeMode::Dark, None, true);
            assert_eq!(lifted.bg, dark.bg);
            assert_eq!(lifted.surface, dark.surface);
            assert_eq!(lifted.raised, dark.raised);
            assert_eq!(lifted.text, dark.text);
            assert_eq!(lifted.accent, dark.accent);
            assert_eq!(
                lifted.faint, dark.muted,
                "{theme:?}: faint takes the old muted"
            );
            assert!(
                contrast(lifted.muted, lifted.surface) >= 4.5,
                "{theme:?}: lifted muted"
            );

            let light = palette(theme, ThemeMode::Light, None, false);
            let lifted = palette(theme, ThemeMode::Light, None, true);
            assert_eq!(lifted.bg, light.bg);
            assert_eq!(lifted.text, light.text);
            assert_eq!(lifted.accent, light.accent);
            assert!(
                contrast(lifted.muted, lifted.surface) >= 5.5,
                "{theme:?}: light lifted muted"
            );
            assert!(
                contrast(lifted.faint, lifted.surface) >= 4.0,
                "{theme:?}: light lifted faint"
            );
            assert!(
                luminance(lifted.muted) < luminance(light.muted),
                "{theme:?}: darker muted"
            );
        }
    }

    #[test]
    fn a_seed_override_repaints_the_live_theme() {
        let default = palette(NightlightTheme::Live, ThemeMode::Dark, None, false);
        assert_eq!(rgb_of(default.accent).unwrap(), FALLBACK_ACCENT);

        let cool = palette(
            NightlightTheme::Live,
            ThemeMode::Dark,
            Some(Color::Rgb(0xFF, 0xD5, 0xB3)),
            false,
        );
        assert_eq!(rgb_of(cool.accent).unwrap(), (0xFF, 0xD5, 0xB3));
        assert_ne!(cool.bg, default.bg, "the ladder follows the seed");
        // Themes with a fixed accent ignore the override.
        let tokyo = palette(
            NightlightTheme::Tokyo,
            ThemeMode::Dark,
            Some(Color::Rgb(0xFF, 0xD5, 0xB3)),
            false,
        );
        assert_eq!(rgb_of(tokyo.accent).unwrap(), (0x7A, 0xA2, 0xF7));
    }

    #[test]
    fn accent_seeds_cover_all_six_settings_values() {
        let seeds: Vec<Rgb> = [
            AccentColor::Orange,
            AccentColor::Cyan,
            AccentColor::Blue,
            AccentColor::Green,
            AccentColor::Yellow,
            AccentColor::Magenta,
        ]
        .into_iter()
        .map(accent_seed)
        .collect();
        assert_eq!(seeds[0], FALLBACK_ACCENT);
        assert_eq!(seeds.len(), 6);
        for (index, seed) in seeds.iter().enumerate() {
            assert!(
                !seeds[..index].contains(seed),
                "seed {index} duplicates an earlier one"
            );
        }
    }
}
