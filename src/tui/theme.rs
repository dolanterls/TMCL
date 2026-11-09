use ratatui::style::{Color, Modifier, Style};

use crate::core::config::models::{AccentColor, ThemeMode, UiConfig};

/// Semantic colors for the binsider-style shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub accent: Color,
    pub label: Color,
    pub key: Color,
    pub selection: Color,
    pub success: Color,
    pub error: Color,
    /// Fill behind an editable field, so inputs stand out from the panel.
    pub field_bg: Color,
}

const DIM: Color = Color::Rgb(100, 100, 100);
const BINSIDER_ACCENT: Color = Color::Rgb(248, 190, 117);

/// The `深色` page: warm near-black, and the inks that sit on it.
///
/// `深色` used to paint nothing and keep `fg = Reset`, so it was really "assume
/// a dark terminal". A page of its own is what lets the amber accent and the
/// key caps keep their colours whatever the window looks like — `跟随终端` is
/// the mode that hands the fill back, not this one.
const DARK_BG: Color = Color::Rgb(26, 24, 21);
const DARK_FG: Color = Color::Rgb(233, 228, 219);
const DARK_DIM: Color = Color::Rgb(139, 134, 124);
/// A step above [`DARK_BG`], so a field reads as raised off the page.
const DARK_FIELD: Color = Color::Rgb(38, 35, 32);
/// Cyan, the shell's field-label colour since the first build.
const DARK_LABEL: Color = Color::Rgb(111, 211, 223);
/// Amber for key caps: the same family as the accent, a shade deeper so a cap
/// under a highlighted border still reads as a cap.
const DARK_KEY: Color = Color::Rgb(227, 178, 94);
const DARK_SELECTION: Color = Color::Rgb(123, 203, 123);
const DARK_ERROR: Color = Color::Rgb(240, 122, 110);

/// The `浅色` page: warm off-white, and inks that read on it.
///
/// `浅色` used to paint no background at all — only the inks changed, and they
/// were picked for a light *terminal*. On a dark window that put near-navy and
/// near-purple text on a near-black page. The mode is now what its name says:
/// it lays down its own page, so the palette works whatever the terminal is.
const LIGHT_BG: Color = Color::Rgb(247, 245, 240);
const LIGHT_FG: Color = Color::Rgb(32, 30, 27);
const LIGHT_DIM: Color = Color::Rgb(107, 104, 98);
/// A step below [`LIGHT_BG`], so a field reads as pressed into the page.
const LIGHT_FIELD: Color = Color::Rgb(232, 228, 220);
/// Deep teal. Carries field labels; separate from the amber accent so a column
/// of names does not read as one more highlight.
const LIGHT_LABEL: Color = Color::Rgb(18, 95, 102);
/// Ochre. Carries key caps — the one slot that has to sit next to the amber
/// accent without being mistaken for it.
const LIGHT_KEY: Color = Color::Rgb(156, 74, 23);
const LIGHT_SELECTION: Color = Color::Rgb(47, 107, 47);
const LIGHT_ERROR: Color = Color::Rgb(176, 42, 30);

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::Reset,
            dim: DIM,
            accent: BINSIDER_ACCENT,
            label: Color::Cyan,
            key: Color::Yellow,
            selection: Color::Green,
            success: Color::Green,
            error: Color::Red,
            field_bg: Color::Rgb(38, 38, 38),
        }
    }
}

impl Theme {
    pub fn from_ui(config: &UiConfig) -> Self {
        let mut theme = match config.theme_mode {
            ThemeMode::Dark => Self {
                bg: DARK_BG,
                fg: DARK_FG,
                dim: DARK_DIM,
                accent: accent_on_dark(config.accent_color),
                label: DARK_LABEL,
                key: DARK_KEY,
                selection: DARK_SELECTION,
                success: DARK_SELECTION,
                error: DARK_ERROR,
                field_bg: DARK_FIELD,
            },
            ThemeMode::Light => Self {
                bg: LIGHT_BG,
                fg: LIGHT_FG,
                dim: LIGHT_DIM,
                accent: accent_on_light(config.accent_color),
                label: LIGHT_LABEL,
                key: LIGHT_KEY,
                selection: LIGHT_SELECTION,
                success: LIGHT_SELECTION,
                error: LIGHT_ERROR,
                field_bg: LIGHT_FIELD,
            },
            ThemeMode::Terminal => Self {
                dim: Color::DarkGray,
                accent: Color::Yellow,
                field_bg: Color::Reset,
                ..Self::default()
            },
        };
        // The accent follows `accent_color`, but each page needs its own half of
        // that hue: `RGB(248, 190, 117)` on `LIGHT_BG` is 1.6:1, which reads as
        // a stripe of nothing rather than as a border, and the named ANSI
        // colours are the terminal's to define — fine for `跟随终端`, not for a
        // page this build paints itself.
        theme.accent = match config.theme_mode {
            ThemeMode::Dark => accent_on_dark(config.accent_color),
            ThemeMode::Light => accent_on_light(config.accent_color),
            ThemeMode::Terminal => accent_color(config.accent_color),
        };
        if config.high_contrast {
            // `Gray` is lighter than the light page's own dim, so the switch
            // would lower the contrast exactly where it promises to raise it.
            theme.dim = match config.theme_mode {
                ThemeMode::Dark => Color::Rgb(185, 179, 168),
                ThemeMode::Light => Color::Rgb(70, 68, 64),
                ThemeMode::Terminal => Color::Gray,
            };
        }
        theme
    }

    pub fn selection_style(self) -> Style {
        Style::default()
            .fg(self.selection)
            .add_modifier(Modifier::BOLD)
    }

    pub fn base(self) -> Style {
        Style::default().bg(self.bg).fg(self.fg)
    }

    pub fn dim(self) -> Style {
        Style::default().fg(self.dim)
    }

    /// Fill of an editable field inside a panel or dialog.
    pub fn field_style(self) -> Style {
        Style::default().bg(self.field_bg).fg(self.fg)
    }

    /// Text cursor drawn over a field.
    pub fn cursor_style(self) -> Style {
        Style::default()
            .bg(self.accent)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    }

    pub fn accent_style(self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn label_style(self) -> Style {
        Style::default().fg(self.label)
    }

    pub fn key_style(self) -> Style {
        Style::default().fg(self.key)
    }
}

fn accent_color(color: AccentColor) -> Color {
    match color {
        AccentColor::Orange => BINSIDER_ACCENT,
        AccentColor::Cyan => Color::Cyan,
        AccentColor::Blue => Color::Blue,
        AccentColor::Green => Color::Green,
        AccentColor::Yellow => Color::Yellow,
        AccentColor::Magenta => Color::Magenta,
    }
}

/// The same six accents, at a depth that holds up on [`LIGHT_BG`].
///
/// Named ANSI colours cannot be darkened — the terminal owns their values — so
/// the light page carries its own values instead of the bright half of the
/// palette. Every entry clears 4.5:1 against `LIGHT_BG`.
fn accent_on_light(color: AccentColor) -> Color {
    match color {
        AccentColor::Orange => Color::Rgb(153, 92, 0),
        AccentColor::Cyan => Color::Rgb(14, 110, 112),
        AccentColor::Blue => Color::Rgb(31, 78, 156),
        AccentColor::Green => Color::Rgb(44, 107, 47),
        AccentColor::Yellow => Color::Rgb(138, 107, 0),
        AccentColor::Magenta => Color::Rgb(138, 45, 107),
    }
}

/// The same six accents, bright enough for [`DARK_BG`].
///
/// Same reason as [`accent_on_light`]: on a page this build paints, a named
/// colour would let the terminal decide whether a border is visible.
fn accent_on_dark(color: AccentColor) -> Color {
    match color {
        AccentColor::Orange => BINSIDER_ACCENT,
        AccentColor::Cyan => Color::Rgb(95, 211, 223),
        AccentColor::Blue => Color::Rgb(127, 168, 240),
        AccentColor::Green => Color::Rgb(127, 207, 127),
        AccentColor::Yellow => Color::Rgb(240, 206, 106),
        AccentColor::Magenta => Color::Rgb(226, 143, 212),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;
    use ratatui::text::Line;

    use super::{AccentColor, Theme, ThemeMode};
    use crate::tui::widgets::status::{StatusKind, badge};

    #[test]
    fn default_theme_uses_the_binsider_palette() {
        let theme = Theme::default();
        assert_eq!(theme.dim, Color::Rgb(100, 100, 100));
        assert_eq!(theme.accent, Color::Rgb(248, 190, 117));
        assert_eq!(theme.label, Color::Cyan);
        assert_eq!(theme.key, Color::Yellow);
        assert_eq!(theme.selection, Color::Green);
    }

    #[test]
    fn status_text_remains_present_without_relying_on_color() {
        let line: Line<'static> = badge(StatusKind::Failed, "Failed");
        assert_eq!(line.to_string(), "× Failed");
    }

    #[test]
    fn ui_settings_change_theme_and_accent() {
        let config = crate::core::config::models::UiConfig {
            theme_mode: ThemeMode::Light,
            accent_color: AccentColor::Green,
            ..Default::default()
        };
        let theme = Theme::from_ui(&config);
        assert_eq!(theme.accent, Color::Rgb(44, 107, 47));
        assert_eq!(theme.dim, Color::Rgb(107, 104, 98));
    }

    /// Both painted modes lay down a page, and their inks have to work on it.
    ///
    /// Every slot clears 4.5:1 against the page it is drawn on. The old light
    /// palette had `Color::Blue` and `Color::Magenta` in it and no page under
    /// them: on a dark window they came out at about 2:1.
    #[test]
    fn every_ink_clears_the_contrast_floor_on_its_own_page() {
        fn ratio(a: Color, b: Color) -> f64 {
            let (a, b) = (
                crate::tui::palette::relative_luminance(a),
                crate::tui::palette::relative_luminance(b),
            );
            let (light, dark) = if a > b { (a, b) } else { (b, a) };
            (light + 0.05) / (dark + 0.05)
        }

        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let config = crate::core::config::models::UiConfig {
                theme_mode: mode,
                ..Default::default()
            };
            let theme = Theme::from_ui(&config);
            let page = theme.bg;
            for (slot, ink) in [
                ("fg", theme.fg),
                ("dim", theme.dim),
                ("accent", theme.accent),
                ("label", theme.label),
                ("key", theme.key),
                ("selection", theme.selection),
                ("error", theme.error),
            ] {
                let against = ratio(ink, page);
                assert!(
                    against >= 4.5,
                    "{mode:?} {slot} is {against:.2}:1 on {page:?}, needs 4.5"
                );
            }

            // And the accents are still distinguishable from each other: two
            // slots that land on the same value are one slot drawn twice.
            let accents = [
                AccentColor::Orange,
                AccentColor::Cyan,
                AccentColor::Blue,
                AccentColor::Green,
                AccentColor::Yellow,
                AccentColor::Magenta,
            ]
            .map(|accent| match mode {
                ThemeMode::Light => super::accent_on_light(accent),
                _ => super::accent_on_dark(accent),
            });
            for (index, first) in accents.iter().enumerate() {
                assert!(
                    ratio(*first, page) >= 4.5,
                    "{mode:?}: {first:?} is too close to the page"
                );
                for second in &accents[index + 1..] {
                    assert_ne!(first, second, "{mode:?}: two accents share a value");
                }
            }
        }
    }

    /// The modes are what their names say: they lay down a page.
    ///
    /// Both used to keep `bg = Color::Reset`, which made them "assume a
    /// terminal dark / light enough" rather than themes of their own. Only
    /// `跟随终端` hands the fill back now.
    #[test]
    fn dark_and_light_paint_their_own_pages() {
        let dark = Theme::from_ui(&crate::core::config::models::UiConfig {
            theme_mode: ThemeMode::Dark,
            ..Default::default()
        });
        let dark_page = crate::tui::palette::relative_luminance(dark.bg);
        assert!(
            dark_page < 0.05,
            "the dark page must be dark: {dark_page:.3}"
        );
        assert!(
            crate::tui::palette::relative_luminance(dark.fg) > 0.7,
            "the body ink must be light"
        );

        let light = Theme::from_ui(&crate::core::config::models::UiConfig {
            theme_mode: ThemeMode::Light,
            ..Default::default()
        });
        let light_page = crate::tui::palette::relative_luminance(light.bg);
        assert!(
            light_page > 0.8,
            "the light page must be light: {light_page:.2}"
        );
        assert!(
            crate::tui::palette::relative_luminance(light.fg) < 0.05,
            "the body ink must be dark"
        );

        // `跟随终端` still leaves the fill to the terminal — that is the mode
        // that exists for transparency.
        let terminal = Theme::from_ui(&crate::core::config::models::UiConfig {
            theme_mode: ThemeMode::Terminal,
            ..Default::default()
        });
        assert_eq!(terminal.bg, Color::Reset, "follow-terminal must stay clear");
        assert_eq!(
            terminal.fg,
            Color::Reset,
            "follow-terminal keeps the ink too"
        );
    }

    #[test]
    fn orange_is_the_default_accent() {
        assert_eq!(AccentColor::default(), AccentColor::Orange);
        let theme = Theme::from_ui(&crate::core::config::models::UiConfig::default());
        assert_eq!(theme.accent, Color::Rgb(248, 190, 117));
    }
}
