//! Logs page: one full-width card with the follow marker, the optional filter
//! and the tail of the buffer.
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};

use crate::tui::action::ScrollTarget;
use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{self, card, card_inner};
use crate::tui::palette::truncate;

use super::App;

pub(in crate::tui::app) fn layout(area: Rect) -> Rect {
    area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    })
}

impl App {
    /// Rectangle the log lines actually occupy.
    ///
    /// The wheel clamp in `App::log_max_scroll_offset` needs the same numbers
    /// the painter uses, or the last rows stay unreachable: the shared clamp is
    /// derived from `chrome::panel(main)`, which is four rows taller than this
    /// card's body (page margin 2, title padding 2, header 2).
    pub(in crate::tui::app) fn nightlight_log_body(area: Rect) -> Rect {
        let inner = card_inner(layout(area));
        Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2),
        )
    }

    pub(in crate::tui::app) fn draw_nightlight_logs(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        let main = area;
        let area = layout(area);
        self.hitmap.scroll(area, ScrollTarget::LogLines);
        // The card says whose log this is. Without it the page cannot be told
        // apart when several instances exist (design §6.1 / §7), and the other
        // two presets both show the current instance.
        let title = match self
            .instances
            .current_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
        {
            Some(instance) => format!("{} · {}", self.i18n.text("page.logs"), instance.name),
            // No instance yet: the title stays clean rather than saying "none".
            None => self.i18n.text("page.logs"),
        };
        card(frame, area, &title, pal);
        let inner = card_inner(area);
        if inner.is_empty() {
            return;
        }

        // Header: follow marker on the left, filter on the right.
        let follow = if self.logs.follow() {
            (self.i18n.text("logs.live"), pal.ok)
        } else {
            (self.i18n.text("logs.paused"), pal.muted)
        };
        let mut header = vec![widgets::pill(&follow.0, follow.1, pal)];
        if let Some(filter) = self.log_filter.as_deref() {
            let text = format!("  {}: {filter}", self.i18n.text("common.search"));
            header.push(Span::styled(
                truncate(&text, usize::from(inner.width).saturating_sub(12)),
                Style::default().fg(pal.faint).bg(pal.surface),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(header)).style(Style::default().bg(pal.surface)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        let body = Self::nightlight_log_body(main);
        if body.is_empty() {
            return;
        }
        let query = self.log_filter.as_deref().unwrap_or("");
        let lines: Vec<Line<'static>> = self
            .logs
            .matching_lines(query)
            .into_iter()
            .map(|line| highlight(&line, query, pal))
            .collect();
        if lines.is_empty() {
            frame.render_widget(
                Paragraph::new(self.i18n.text("logs.empty"))
                    .style(Style::default().fg(pal.muted).bg(pal.surface)),
                body,
            );
            return;
        }
        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(pal.surface));
        let total_rows = paragraph.line_count(body.width);
        let max_offset = total_rows.saturating_sub(usize::from(body.height));
        let bottom = self.logs.scroll_offset().min(max_offset);
        let from_top = max_offset.saturating_sub(bottom);
        frame.render_widget(
            paragraph.scroll((u16::try_from(from_top).unwrap_or(u16::MAX), 0)),
            body,
        );
        if total_rows > usize::from(body.height) {
            let mut state = ScrollbarState::new(total_rows)
                .position(from_top)
                .viewport_content_length(usize::from(body.height));
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("↑"))
                    .end_symbol(Some("↓")),
                Rect::new(body.x, body.y, body.width + 1, body.height),
                &mut state,
            );
        }
    }
}

fn highlight(line: &str, query: &str, pal: &Palette) -> Line<'static> {
    let base = if line.starts_with("[stderr]") {
        Style::default().fg(pal.err).bg(pal.surface)
    } else if line.starts_with("[stdout]") {
        Style::default().fg(pal.text).bg(pal.surface)
    } else {
        Style::default().fg(pal.muted).bg(pal.surface)
    };
    if query.is_empty() {
        return Line::styled(line.to_owned(), base);
    }
    let mut spans = Vec::new();
    let mut rest = line;
    while let Some(index) = rest.to_lowercase().find(&query.to_lowercase()) {
        let (before, matched) = rest.split_at(index);
        if !before.is_empty() {
            spans.push(Span::styled(before.to_owned(), base));
        }
        let end = index + query.len();
        let (hit, tail) = matched.split_at(end.min(matched.len()));
        spans.push(Span::styled(
            hit.to_owned(),
            Style::default()
                .fg(pal.bg)
                .bg(pal.accent)
                .add_modifier(Modifier::BOLD),
        ));
        rest = tail;
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_owned(), base));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::layout;

    #[test]
    fn the_logs_page_keeps_a_one_cell_margin() {
        let area = layout(Rect::new(27, 0, 93, 38));
        assert_eq!(area.x, 28);
        assert_eq!(area.width, 91);
        assert_eq!(area.bottom(), 37);
    }
}
