//! The `creeper` palette: neon lime on near-black.
//!
//! `ThemeMode::Light` swaps the whole ladder; `Terminal` keeps the dark sheet
//! and hands the page colour back through `punch_through_terminal`, so
//! transparency survives while the ink stays legible. `accent_color` and
//! `high_contrast` stay out: a second accent would fight the lime.

use ratatui::style::Color;

use crate::core::config::models::ThemeMode;
use crate::tui::app::view::sheet::Sheet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_faint: Color,
    /// Between `surface` and the page: the second card tone.
    pub surface_alt: Color,
    /// Hairlines and card borders.
    pub border: Color,
    /// The lit edge of a focused card.
    pub border_hi: Color,
    pub lime: Color,
    pub green: Color,
    pub green_dark: Color,
    pub cyan: Color,
    pub gold: Color,
    pub purple: Color,
    pub blue: Color,
    pub pink: Color,
    pub red: Color,
    /// `跟随终端`: the fills are withdrawn so the terminal's own background
    /// shows through. The ink stays this sheet's, because a terminal a creeper
    /// runs on is dark far more often than not.
    pub transparent: bool,
}

/// The original sheet: `crate::tui::palette`'s constants, verbatim.
pub const DARK: Palette = Palette {
    bg: Color::Rgb(0x0A, 0x0D, 0x14),
    surface: Color::Rgb(0x13, 0x18, 0x24),
    surface_alt: Color::Rgb(0x1A, 0x21, 0x30),
    border: Color::Rgb(0x2E, 0x3A, 0x4E),
    border_hi: Color::Rgb(0x52, 0x66, 0x88),
    text: Color::Rgb(0xE6, 0xEC, 0xF6),
    text_dim: Color::Rgb(0x87, 0x95, 0xAD),
    text_faint: Color::Rgb(0x52, 0x60, 0x78),
    lime: Color::Rgb(0xBE, 0xF6, 0x80),
    green: Color::Rgb(0x78, 0xD4, 0x63),
    green_dark: Color::Rgb(0x37, 0x70, 0x36),
    cyan: Color::Rgb(0x5C, 0xCD, 0xEB),
    gold: Color::Rgb(0xF5, 0xC7, 0x5B),
    purple: Color::Rgb(0xB0, 0x91, 0xF5),
    blue: Color::Rgb(0x7A, 0xA0, 0xF5),
    pink: Color::Rgb(0xF0, 0x96, 0xC3),
    red: Color::Rgb(0xF0, 0x6E, 0x78),
    transparent: false,
};

/// The light sheet: the same ladder sunk far enough to read on paper.
///
/// The lime moves furthest: `#BEF680` is 1.3:1 on a white page. At `#3E7A1C`
/// it keeps the chartreuse hue and clears 4.5:1, which the key caps and the
/// brand mark need.
pub const LIGHT: Palette = Palette {
    bg: Color::Rgb(0xF7, 0xF5, 0xEE),
    surface: Color::Rgb(0xFF, 0xFF, 0xFF),
    surface_alt: Color::Rgb(0xF1, 0xEE, 0xE4),
    border: Color::Rgb(0xD5, 0xD0, 0xC2),
    border_hi: Color::Rgb(0x8C, 0x84, 0x72),
    text: Color::Rgb(0x14, 0x18, 0x0F),
    text_dim: Color::Rgb(0x4E, 0x55, 0x45),
    text_faint: Color::Rgb(0x7A, 0x83, 0x70),
    lime: Color::Rgb(0x3E, 0x7A, 0x1C),
    green: Color::Rgb(0x2E, 0x7D, 0x32),
    green_dark: Color::Rgb(0x1F, 0x5A, 0x22),
    cyan: Color::Rgb(0x0F, 0x6E, 0x7A),
    gold: Color::Rgb(0x8A, 0x5A, 0x00),
    purple: Color::Rgb(0x6A, 0x3F, 0xA0),
    blue: Color::Rgb(0x2C, 0x5A, 0xA0),
    pink: Color::Rgb(0xA0, 0x3A, 0x6E),
    red: Color::Rgb(0xB3, 0x26, 0x1E),
    transparent: false,
};

/// The dark sheet with its fills withdrawn, for `跟随终端`.
///
/// Derived from [`DARK`] rather than copied: the ink, the accent ladder and the
/// page tone are the dark sheet's, and a second transcription would drift the
/// first time a colour is tuned.
pub const TERMINAL: Palette = Palette {
    transparent: true,
    ..DARK
};

/// The light sheet with its fills withdrawn, for `跟随终端` on a light terminal.
pub const TERMINAL_LIGHT: Palette = Palette {
    transparent: true,
    ..LIGHT
};

impl Sheet for Palette {
    fn surface(&self) -> Color {
        self.surface
    }

    fn fills(&self) -> [Option<Color>; 4] {
        [
            Some(self.bg),
            Some(self.surface),
            Some(self.surface_alt),
            None,
        ]
    }

    fn follows_terminal(&self) -> bool {
        self.transparent
    }
}

/// The subset the shared `kit` primitives paint with, handed to them once per
/// frame instead of reading global constants.
pub fn skin(pal: &Palette) -> crate::tui::widgets::kit::Skin {
    crate::tui::widgets::kit::Skin {
        bg: pal.bg,
        surface: pal.surface,
        surface_alt: pal.surface_alt,
        border: pal.border,
        border_hi: pal.border_hi,
        text: pal.text,
        text_dim: pal.text_dim,
        text_faint: pal.text_faint,
        green: pal.green,
        lime: pal.lime,
        cyan: pal.cyan,
        gold: pal.gold,
        red: pal.red,
        transparent: pal.transparent,
    }
}

pub fn palette(mode: ThemeMode) -> &'static Palette {
    match mode {
        ThemeMode::Light => &LIGHT,
        // `Terminal` keeps the dark ink and gives up every fill: the page, the
        // panels and the row bands all become `Reset`, so the terminal's own
        // background shows through. `punch_through_terminal` does the handing
        // back for the fills that are painted as literal colours.
        ThemeMode::Dark => &DARK,
        ThemeMode::Terminal => &TERMINAL,
    }
}

#[cfg(test)]
mod tests {
    use super::{DARK, LIGHT, palette};
    use crate::core::config::models::ThemeMode;
    use crate::tui::app::view::sheet::Sheet;

    fn luminance(color: ratatui::style::Color) -> f32 {
        let ratatui::style::Color::Rgb(red, green, blue) = color else {
            panic!("creeper palette must be RGB: {color:?}");
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

    #[test]
    fn every_sheet_keeps_ink_and_accents_legible() {
        for (name, pal) in [("dark", &DARK), ("light", &LIGHT)] {
            assert!(
                contrast(pal.text, pal.bg) >= 12.0,
                "{name} text: {}",
                contrast(pal.text, pal.bg)
            );
            assert!(
                contrast(pal.lime, pal.bg) >= 4.5,
                "{name} lime: {}",
                contrast(pal.lime, pal.bg)
            );
            assert!(
                contrast(pal.text_dim, pal.bg) >= 3.0,
                "{name} dim: {}",
                contrast(pal.text_dim, pal.bg)
            );
        }
    }

    #[test]
    fn every_state_hue_stays_visible() {
        for (sheet, pal) in [("dark", &DARK), ("light", &LIGHT)] {
            for (name, colour) in [
                ("green", pal.green),
                ("cyan", pal.cyan),
                ("gold", pal.gold),
                ("purple", pal.purple),
                ("blue", pal.blue),
                ("pink", pal.pink),
                ("red", pal.red),
            ] {
                assert!(
                    contrast(colour, pal.bg) >= 3.0,
                    "{sheet} {name}: {}",
                    contrast(colour, pal.bg)
                );
            }
        }
    }

    #[test]
    fn the_light_sheet_is_actually_light() {
        assert!(luminance(LIGHT.bg) > 0.8, "the light page is not light");
        assert!(luminance(DARK.bg) < 0.02, "the dark page is not dark");
        assert_eq!(palette(ThemeMode::Dark), &DARK);
        assert_eq!(palette(ThemeMode::Light), &LIGHT);
    }

    /// `跟随终端` is the dark sheet with its fills withdrawn: same ink, no
    /// container paint. Only the flag differs, so the page still reads on a
    /// dark terminal while the terminal's own background shows through.
    #[test]
    fn the_terminal_sheet_withdraws_the_fills_and_keeps_the_ink() {
        let terminal = palette(ThemeMode::Terminal);
        assert!(terminal.transparent);
        for (name, colour) in [
            ("bg", terminal.bg),
            ("surface", terminal.surface),
            ("text", terminal.text),
            ("lime", terminal.lime),
        ] {
            assert_eq!(
                colour,
                match name {
                    "bg" => DARK.bg,
                    "surface" => DARK.surface,
                    "text" => DARK.text,
                    _ => DARK.lime,
                }
            );
        }
        assert_eq!(
            terminal.fill(terminal.green, 0.2),
            ratatui::style::Color::Reset
        );
        assert_eq!(
            DARK.fill(DARK.green, 0.2),
            crate::tui::palette::mix(DARK.surface, DARK.green, 0.2)
        );
    }

    /// The same slot has to play the same role on both sheets: a colour that is
    /// lighter than the page on one and darker on the other would flip the
    /// meaning of "dim".
    #[test]
    fn both_sheets_put_the_ink_on_the_right_side_of_the_page() {
        for (name, pal) in [("dark", &DARK), ("light", &LIGHT)] {
            let page = luminance(pal.bg);
            for (slot, colour) in [
                ("text", pal.text),
                ("dim", pal.text_dim),
                ("lime", pal.lime),
            ] {
                assert!(
                    (luminance(colour) - page).abs() > 0.05,
                    "{name} {slot} blends into the page"
                );
            }
        }
    }
}
