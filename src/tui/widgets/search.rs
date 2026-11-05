use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use ratatui::style::Style;

use crate::tui::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, query: &str, focused: bool, theme: Theme) {
    render_with_label(frame, area, "Search", query, focused, theme);
}

pub fn render_with_label(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    query: &str,
    focused: bool,
    theme: Theme,
) {
    let cursor = if focused { "_" } else { "" };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{label}: "), theme.dim()),
            Span::styled(
                format!("{query}{cursor}"),
                theme.base().add_modifier(Modifier::BOLD),
            ),
        ]))
        .style(theme.base())
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(if focused {
                    Style::default().fg(theme.accent)
                } else {
                    Style::default().fg(theme.dim)
                }),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::render;

    use crate::tui::theme::Theme;

    #[test]
    fn search_bar_renders_query_and_focus_cursor() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, frame.area(), "sodium", true, Theme::default()))
            .expect("draw search");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Search: sodium_"));
    }
}
