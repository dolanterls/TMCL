//! Logs page: a single full-height stream card with follow state.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};

use super::tone::Palette;
use super::*;
use crate::tui::palette;
use crate::tui::widgets::kit as widgets;

impl App {
    pub(in crate::tui::app) fn draw_creeper_logs(&self, frame: &mut Frame<'_>, area: Rect, t: f64) {
        let pal = self.creeper_palette();
        if area.is_empty() {
            return;
        }
        let current = self
            .instances
            .current_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
            .map_or_else(|| self.i18n.text("common.none"), |item| item.name.clone());
        let block = widgets::card(&self.i18n.text("page.logs"), pal.gold, true, t, pal.surface);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.height < 2 || inner.width < 8 {
            return;
        }

        let header = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner);
        let mut status = Vec::new();
        if let Some(badge) = self.diagnosis_badge() {
            status.push(widgets::status_pill(StatusKind::Failed, badge, pal.surface));
            status.push(Span::styled(" ", Style::default().bg(pal.surface)));
        }
        status.extend([
            Span::styled(" ", Style::default().bg(pal.surface)),
            widgets::soft_pill(
                format!("{}: {current}", self.i18n.text("logs.instance")),
                pal.gold,
                pal.surface,
            ),
            Span::styled(" ", Style::default().bg(pal.surface)),
        ]);
        if self.logs.follow() {
            status.push(widgets::status_pill(
                StatusKind::Running,
                self.i18n.text("logs.live"),
                pal.surface,
            ));
        } else {
            status.push(widgets::status_pill(
                StatusKind::Warning,
                self.i18n.text("logs.paused"),
                pal.surface,
            ));
        }
        if let Some(filter) = self.log_filter.as_deref() {
            let focused = matches!(
                self.search.as_ref(),
                Some(state) if state.target == SearchTarget::Logs
            );
            let cursor = if focused {
                self.search
                    .as_ref()
                    .map_or(filter.chars().count(), |state| state.cursor)
            } else {
                filter.chars().count()
            };
            let label = format!(" {}: ", self.i18n.text("common.search"));
            let used = ratatui::text::Line::from(status.clone()).width();
            let budget = usize::from(header[0].width).saturating_sub(used + palette::width(&label));
            let tone = palette::mix(pal.gold, pal.text_dim, 0.4);
            status.push(Span::styled(
                label,
                Style::default().fg(tone).bg(pal.surface),
            ));
            let style = crate::tui::text::FieldStyle {
                text: tone,
                dim: pal.text_faint,
                bg: pal.surface,
                cursor_fg: pal.bg,
                cursor_bg: pal.gold,
            };
            status.extend(
                crate::tui::text::render(
                    filter,
                    cursor,
                    u16::try_from(budget).unwrap_or(u16::MAX),
                    focused,
                    crate::tui::text::blink_on(t),
                    style,
                    "",
                )
                .spans,
            );
        }
        frame.render_widget(
            Paragraph::new(Line::from(status)).style(Style::default().bg(pal.surface)),
            header[0],
        );

        let text_area = Rect {
            x: header[1].x.saturating_add(1),
            width: header[1].width.saturating_sub(2),
            ..header[1]
        };
        if text_area.is_empty() {
            return;
        }
        let query = self.log_filter.as_deref().unwrap_or("");
        // Only the visible page: measuring the whole buffer every frame made
        // this page crawl once the game was logging.
        let window = self
            .logs
            .window(query, text_area.width, text_area.height, false);
        let lines = window
            .lines
            .iter()
            .map(|line| highlight(pal, line, query))
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .style(Style::default().bg(pal.surface))
                .scroll((u16::try_from(window.skip_rows).unwrap_or(u16::MAX), 0)),
            text_area,
        );

        if window.total_rows > usize::from(text_area.height) {
            let mut state = ScrollbarState::new(window.total_rows)
                .position(window.from_row)
                .viewport_content_length(usize::from(text_area.height));
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("↑"))
                    .end_symbol(Some("↓"))
                    .style(Style::default().fg(pal.text_faint).bg(pal.surface)),
                text_area,
                &mut state,
            );
        }

        if window.total_rows == 0 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    self.i18n.text("logs.empty"),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                )))
                .alignment(Alignment::Center),
                Rect::new(text_area.x, text_area.y, text_area.width, 1),
            );
        }
    }
}

/// Colour a log line by stream and highlight the active query.
fn highlight(pal: &Palette, line: &str, query: &str) -> Line<'static> {
    let base = if line.starts_with("[stderr]") {
        palette::mix(pal.gold, pal.text, 0.35)
    } else if line.starts_with("[stdout]") {
        pal.text_dim
    } else {
        palette::mix(pal.text, pal.text_dim, 0.35)
    };
    let text = palette::one_line(line, usize::MAX);
    if query.trim().is_empty() {
        return Line::from(Span::styled(
            text,
            Style::default().fg(base).bg(pal.surface),
        ));
    }
    let mut spans = Vec::new();
    let mut rest = text.as_str();
    while let Some(position) = rest.find(query) {
        let (before, after) = rest.split_at(position);
        if !before.is_empty() {
            spans.push(Span::styled(
                before.to_owned(),
                Style::default().fg(base).bg(pal.surface),
            ));
        }
        let (matched, tail) = after.split_at(query.len());
        spans.push(Span::styled(
            matched.to_owned(),
            Style::default()
                .fg(pal.bg)
                .bg(pal.gold)
                .add_modifier(Modifier::BOLD),
        ));
        rest = tail;
    }
    if !rest.is_empty() {
        spans.push(Span::styled(
            rest.to_owned(),
            Style::default().fg(base).bg(pal.surface),
        ));
    }
    Line::from(spans)
}
