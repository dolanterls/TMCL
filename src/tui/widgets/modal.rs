use ratatui::{
    style::{Modifier, Style},
    widgets::Block,
};

use crate::tui::theme::Theme;

pub fn panel(title: impl Into<String>, theme: Theme) -> Block<'static> {
    crate::tui::chrome::panel(&title.into(), true, theme).style(theme.base())
}

pub fn danger(theme: Theme) -> Style {
    theme.base().fg(theme.error).add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::{danger, panel};
    use crate::tui::theme::Theme;

    #[test]
    fn modal_helpers_use_the_shared_theme() {
        let theme = Theme::default();
        assert_eq!(danger(theme).fg, Some(Color::Red));
        let _ = panel("Confirm", theme);
    }
}
