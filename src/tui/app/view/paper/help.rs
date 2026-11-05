//! `paper` help: the keyboard reference printed on a slip.
//!
//! The reference is a dialog in every preset, and paper keeps that: the same
//! card geometry as `classic` and `nightlight` (width follows the longest row,
//! height stops at `HELP_VIEWPORT_ROWS`, the page behind it dimmed), so `?`
//! never swallows the running head and the key band that get you out again.
//! Only the surface is paper's: no border, a sheet one step brighter than the
//! page, the title, a hairline, then sections in the accent colour with keys in
//! one column and their meaning in the other.
//!
//! The rectangle and the body come from `help_layout`, the same call
//! `help_max_scroll` and `help_page_rows` make; a card whose scroll limit came
//! from a different geometry would page past its own end.

use super::*;
use ratatui::widgets::Clear;

impl App {
    pub(in crate::tui::app) fn draw_paper_help(&self, frame: &mut Frame<'_>, pal: &PaperPalette) {
        let lines = self.help_lines();
        let layout = self.help_layout(frame.area(), &lines);
        let rect = layout.rect;
        let body = layout.body;
        if rect.width < 12 || rect.height < 5 || body.is_empty() {
            return;
        }
        // Without the clear the page underneath prints through the sheet.
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Block::default().style(Style::default().fg(pal.ink).bg(pal.sheet)),
            rect,
        );
        // The dialog head, exactly as `modal/paper.rs` prints it: one row of
        // air, the title in the accent, a hairline. The body starts three rows
        // down (`help_body`), so the head costs the reference nothing.
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(
                    &self.i18n.text("help.keyboard"),
                    usize::from(rect.width.saturating_sub(4)),
                ),
                sheet_ink(pal, pal.accent).add_modifier(Modifier::BOLD),
            ))),
            Rect::new(
                rect.x.saturating_add(2),
                rect.y.saturating_add(1),
                rect.width.saturating_sub(4),
                1,
            ),
        );
        kit::rule_on(
            frame,
            Rect::new(rect.x, rect.y.saturating_add(2), rect.width, 1),
            pal.rule,
            pal.sheet,
        );

        let room = usize::from(body.width).saturating_sub(HELP_KEY_COLUMN);
        let rendered: Vec<Line<'static>> = lines
            .into_iter()
            .map(|line| match line {
                HelpLine::Header(title) => Line::from(Span::styled(
                    title,
                    sheet_ink(pal, pal.accent).add_modifier(Modifier::BOLD),
                )),
                HelpLine::Row { key, action } => Line::from(vec![
                    Span::styled(palette::pad(&key, HELP_KEY_COLUMN), sheet_ink(pal, pal.ink)),
                    Span::styled(palette::truncate(&action, room), sheet_ink(pal, pal.dim)),
                ]),
                HelpLine::Blank => Line::default(),
            })
            .collect();
        frame.render_widget(
            Paragraph::new(rendered).scroll((u16::try_from(layout.scroll).unwrap_or(u16::MAX), 0)),
            body,
        );
        self.paper_scrollbar(frame, body, layout.total, layout.scroll, pal, pal.sheet);
    }
}
