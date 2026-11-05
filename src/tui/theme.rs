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
            ThemeMode::Dark => Self::default(),
            ThemeMode::Light => Self {
                dim: Color::Rgb(154, 154, 154),
                accent: Color::Rgb(168, 91, 0),
                label: Color::Blue,
                key: Color::Magenta,
                field_bg: Color::Rgb(228, 228, 228),
                ..Self::default()
            },
            ThemeMode::Terminal => Self {
                dim: Color::DarkGray,
                accent: Color::Yellow,
                field_bg: Color::Reset,
                ..Self::default()
            },
        };
        theme.accent = accent_color(config.accent_color);
        if config.high_contrast {
            theme.dim = Color::Gray;
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
        assert_eq!(theme.accent, Color::Green);
        assert_eq!(theme.dim, Color::Rgb(154, 154, 154));
    }

    #[test]
    fn orange_is_the_default_accent() {
        assert_eq!(AccentColor::default(), AccentColor::Orange);
        let theme = Theme::from_ui(&crate::core::config::models::UiConfig::default());
        assert_eq!(theme.accent, Color::Rgb(248, 190, 117));
    }
}
