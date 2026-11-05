use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

use crate::tui::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, title: &str, message: &str, theme: Theme) {
    frame.render_widget(
        Paragraph::new(vec![Line::from(message)])
            .style(theme.base().fg(theme.dim))
            .alignment(ratatui::layout::Alignment::Center)
            .block(crate::tui::chrome::panel(title, false, theme))
            .wrap(ratatui::widgets::Wrap { trim: false }),
        area,
    );
}

pub fn render_loading(frame: &mut Frame<'_>, area: Rect, message: &str, theme: Theme) {
    render_loading_with_title(frame, area, "Loading", message, theme);
}

pub fn render_loading_with_title(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    message: &str,
    theme: Theme,
) {
    render(frame, area, title, &format!("… {message}"), theme);
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::{render, render_loading};
    use crate::tui::theme::Theme;

    #[test]
    fn empty_and_loading_states_are_explicit() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| {
                render(
                    frame,
                    frame.area(),
                    "Instances",
                    "No instances",
                    Theme::default(),
                )
            })
            .expect("draw empty");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("No instances"));
        terminal
            .draw(|frame| render_loading(frame, frame.area(), "loading versions", Theme::default()))
            .expect("draw loading");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("loading versions"));
    }
}
