//! `shelf` logs: level spelled as a letter, body wrapped to the page.
//!
//! The page keeps the launcher's filter field and follow state. Long lines wrap
//! rather than being cut at the edge: a mod path or a stack frame that stops
//! mid-word tells the reader nothing.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::tone::Palette;
use super::widgets;
use super::*;
use crate::tui::action::{Action, ScrollTarget};
use crate::tui::input::SearchTarget;
use crate::tui::palette;

/// Rows the head takes: instance/follow, rule, search, rule.
const HEAD_ROWS: u16 = 4;

impl App {
    /// The log body, shared by the painter and `log_max_scroll_offset`.
    pub(in crate::tui::app) fn shelf_log_area(&self, main: Rect) -> Rect {
        let y = main.y.saturating_add(HEAD_ROWS);
        Rect::new(main.x, y, main.width, main.bottom().saturating_sub(y))
    }

    pub(in crate::tui::app) fn draw_shelf_logs(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        if area.width < 24 || area.height < 6 {
            return;
        }
        self.hitmap.scroll(area, ScrollTarget::LogLines);

        let instance = self
            .instances
            .current_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
            .map_or_else(|| self.i18n.text("common.none"), |item| item.name.clone());
        let follow = if self.logs.follow() {
            self.i18n.text("logs.live")
        } else {
            self.i18n.text("logs.paused")
        };
        let follow_colour = if self.logs.follow() { pal.ok } else { pal.warn };
        let meta = Rect::new(area.x, area.y, area.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("● ", Style::default().fg(follow_colour).bg(pal.bg)),
                Span::styled(
                    palette::truncate(&follow, usize::from(area.width / 3)),
                    Style::default().fg(follow_colour).bg(pal.bg),
                ),
                Span::styled("   ", Style::default().bg(pal.bg)),
                Span::styled(
                    palette::truncate(&instance, usize::from(area.width / 2)),
                    Style::default().fg(pal.dim).bg(pal.bg),
                ),
            ])),
            meta,
        );
        widgets::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
            pal,
        );
        let body = self.shelf_log_area(area);
        let query = self.log_filter.as_deref().unwrap_or("");
        // One column is kept for the rail; a log line that ends under the thumb
        // looks truncated by the scrollbar rather than by the width.
        let text_width = body.width.saturating_sub(1);
        let window = self.logs.window(query, text_width, body.height, false);
        self.shelf_log_search(
            frame,
            Rect::new(area.x, area.y.saturating_add(2), area.width, 1),
            pal,
            &window,
            body.height,
        );
        widgets::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(3), area.width, 1),
            pal,
        );

        if body.height == 0 || body.width < 4 {
            return;
        }
        if window.total_rows == 0 {
            let text = self.i18n.text("logs.empty");
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&text, usize::from(body.width)),
                    Style::default().fg(pal.faint).bg(pal.bg),
                )),
                Rect::new(body.x, body.y, body.width, 1),
            );
            return;
        }

        let rows = window
            .lines
            .iter()
            .map(|line| self.shelf_log_line(line, pal, query))
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(rows)
                .wrap(Wrap { trim: false })
                .scroll((u16::try_from(window.skip_rows).unwrap_or(u16::MAX), 0)),
            Rect::new(body.x, body.y, text_width, body.height),
        );
        crate::tui::app::view::margin_scrollbar(
            frame,
            body,
            window.total_rows,
            window.from_row,
            pal.faint,
            pal.accent,
            pal.bg,
        );
    }

    /// `搜索 …` with the position of the reader on the right.
    fn shelf_log_search(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
        window: &crate::tui::pages::log::LogWindow,
        viewport: u16,
    ) {
        let target = SearchTarget::Logs;
        let focused = matches!(self.search.as_ref(), Some(state) if state.target == target);
        let filter = self.log_filter.as_deref().unwrap_or("");
        let label = self.i18n.text("common.search");
        let position = if window.total_rows == 0 {
            String::new()
        } else {
            let last = (window.from_row + usize::from(viewport)).min(window.total_rows);
            format!("{last} / {}", window.total_rows)
        };
        let position_width = palette::width(&position) as u16;
        let budget = area
            .width
            .saturating_sub(palette::width(&label) as u16 + 2)
            .saturating_sub(position_width.saturating_add(2));
        let style = if focused {
            Style::default()
                .fg(pal.text)
                .bg(pal.bg)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else if filter.is_empty() {
            Style::default().fg(pal.faint).bg(pal.bg)
        } else {
            Style::default().fg(pal.text).bg(pal.bg)
        };
        let shown = if filter.is_empty() {
            "…".to_owned()
        } else {
            palette::truncate(filter, usize::from(budget.saturating_sub(1)))
        };
        let mut spans = vec![
            Span::styled(format!("{label} "), Style::default().fg(pal.dim).bg(pal.bg)),
            Span::styled(shown, style),
        ];
        if focused && crate::tui::text::blink_on(self.anim_time()) {
            spans.push(Span::styled(
                "▏",
                Style::default().fg(pal.accent).bg(pal.bg),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, area.y, area.width.saturating_sub(position_width), 1),
        );
        if !position.is_empty() {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    position,
                    Style::default().fg(pal.faint).bg(pal.bg),
                ))
                .alignment(Alignment::Right),
                area,
            );
        }
        self.hitmap.add(area, Action::search(target));
    }

    /// `▌E boot failed`: the letter is the level, the rest is the line.
    fn shelf_log_line(&self, line: &str, pal: &Palette, query: &str) -> Line<'static> {
        let (level, body) = split_log_level(line);
        let mut spans = Vec::with_capacity(4);
        if level == LogLevel::Error {
            spans.push(Span::styled("▌", Style::default().fg(pal.error).bg(pal.bg)));
        }
        spans.push(Span::styled(
            format!("{} ", level.letter()),
            Style::default()
                .fg(level.color_in(pal.dim, pal.warn, pal.error))
                .bg(pal.bg)
                .add_modifier(Modifier::BOLD),
        ));
        spans.extend(self.shelf_log_matches(
            body,
            query,
            if level == LogLevel::Error {
                pal.text
            } else {
                pal.dim
            },
            pal,
        ));
        Line::from(spans)
    }

    /// The body, with every occurrence of the filter underlined.
    ///
    /// Underline rather than colour: the accent is already used for the cursor
    /// and the error bar, and a second accent inside a line reads as another
    /// kind of message.
    fn shelf_log_matches(
        &self,
        text: &str,
        query: &str,
        base: ratatui::style::Color,
        pal: &Palette,
    ) -> Vec<Span<'static>> {
        let normal = Style::default().fg(base).bg(pal.bg);
        if query.is_empty() {
            return vec![Span::styled(text.to_owned(), normal)];
        }
        let mut spans = Vec::new();
        let mut rest = text;
        while let Some(position) = rest.find(query) {
            if position > 0 {
                spans.push(Span::styled(rest[..position].to_owned(), normal));
            }
            spans.push(Span::styled(
                query.to_owned(),
                normal.add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
            ));
            rest = &rest[position + query.len()..];
        }
        if !rest.is_empty() {
            spans.push(Span::styled(rest.to_owned(), normal));
        }
        spans
    }
}
