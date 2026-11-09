//! `paper` logs: level spelled as a letter, body wrapped to the page.
//!
//! Colour restates the level rather than carrying it: every entry starts with
//! `I`, `W` or `E`, and an error also carries the margin bar. A log line wraps
//! rather than being cut at the page edge, because half a stack trace is a
//! caption, not a log; continuation rows start at the left edge.

use super::*;
use crate::tui::palette;

/// Rows the head takes: title, rule, search, soft rule.
const HEAD_ROWS: u16 = 4;

impl App {
    /// The log body, shared by the painter and `log_max_scroll_offset`.
    pub(in crate::tui::app) fn paper_log_area(&self, main: Rect) -> Rect {
        Rect::new(
            main.x,
            main.y.saturating_add(HEAD_ROWS),
            main.width,
            main.height.saturating_sub(HEAD_ROWS),
        )
    }

    pub(in crate::tui::app) fn draw_paper_logs(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
    ) {
        if area.width < 24 || area.height < 6 {
            return;
        }
        self.hitmap
            .scroll(area, crate::tui::action::ScrollTarget::LogLines);
        let instance = self
            .instances
            .current_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
            .map_or_else(|| self.i18n.text("common.none"), |item| item.name.clone());
        let follow = if self.logs.follow() {
            format!("● {}", self.i18n.text("logs.live"))
        } else {
            format!("○ {}", self.i18n.text("logs.paused"))
        };
        let title = self.i18n.text("page.logs");
        let counter = format!("{instance} · {follow}");
        kit::section(frame, area, pal, &title, Some(&counter));

        let query = self.log_filter.as_deref().unwrap_or("");
        let body = self.paper_log_area(area);
        let window = self.logs.window(query, body.width, body.height, false);
        // Directly under the section rule, like the downloads page: a blank
        // row above a control reads as a hole.
        self.paper_log_search(
            frame,
            Rect::new(area.x, area.y.saturating_add(2), area.width, 1),
            pal,
            &window,
            body.height,
        );
        kit::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(3), area.width, 1),
            pal,
            pal.rule_soft,
        );
        if body.height == 0 {
            return;
        }
        if window.total_rows == 0 {
            self.paper_message_at(frame, body, pal, "logs.empty");
            return;
        }

        let rendered = window
            .lines
            .iter()
            .map(|line| self.paper_log_line(line, pal))
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(rendered)
                .wrap(Wrap { trim: false })
                .scroll((u16::try_from(window.skip_rows).unwrap_or(u16::MAX), 0)),
            body,
        );
    }

    fn paper_log_search(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
        window: &crate::tui::pages::log::LogWindow,
        height: u16,
    ) {
        let focused = matches!(
            self.search.as_ref(),
            Some(state) if state.target == crate::tui::input::SearchTarget::Logs
        );
        let filter = self.log_filter.as_deref().unwrap_or("");
        let label = self.i18n.text("common.search");
        // The last row on the page, 1-based: following the tail reads
        // `3 / 3`, and scrolling up counts down.
        let position = if window.total_rows == 0 {
            String::new()
        } else {
            let last = (window.from_row + usize::from(height)).min(window.total_rows);
            format!("{last} / {}", window.total_rows)
        };
        let position_width = palette::width(&position) as u16;
        let budget = area
            .width
            .saturating_sub(palette::width(&label) as u16 + 2 + position_width + 2);
        let mut spans = vec![Span::styled(format!("{label} "), ink(pal, pal.dim))];
        let style = if focused {
            ink(pal, pal.ink).add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
        } else if filter.is_empty() {
            ink(pal, pal.faint)
        } else {
            ink(pal, pal.soft)
        };
        let shown = if filter.is_empty() {
            "…".to_owned()
        } else {
            palette::truncate(filter, usize::from(budget.saturating_sub(1)))
        };
        spans.push(Span::styled(shown, style));
        // The caret blinks on the shared clock: a frozen bar reads as a glyph
        // that happens to sit after the query, not as a cursor.
        if focused && crate::tui::text::blink_on(self.anim_time()) {
            spans.push(Span::styled("▏", ink(pal, pal.accent)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, area.y, area.width.saturating_sub(position_width), 1),
        );
        if !position.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(position, ink(pal, pal.faint))))
                    .alignment(Alignment::Right),
                area,
            );
        }
        self.hitmap
            .add(area, Action::search(crate::tui::input::SearchTarget::Logs));
    }

    /// `▌E boot failed`: the letter is the level, the rest is the line.
    ///
    /// The body is handed over whole so `Paragraph` wraps it, keeping a mod path
    /// or a stack frame to the end instead of cutting at an ellipsis.
    fn paper_log_line(&self, line: &str, pal: &PaperPalette) -> Line<'static> {
        let (level, body) = split_log_level(line);
        let mut spans = Vec::with_capacity(3);
        if level == LogLevel::Error {
            spans.push(Span::styled("▌", ink(pal, pal.accent)));
        }
        spans.push(Span::styled(
            format!("{} ", level.letter()),
            ink(pal, level.color_in(pal.dim, pal.warn, pal.accent)).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            palette::one_line(body, usize::MAX),
            if level == LogLevel::Error {
                ink(pal, pal.ink)
            } else {
                ink(pal, pal.dim)
            },
        ));
        Line::from(spans)
    }
}
