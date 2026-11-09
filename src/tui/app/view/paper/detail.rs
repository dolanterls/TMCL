//! `paper` detail: the printed record of the selected row.
//!
//! The record is a dialog like the keyboard reference and the instance form:
//! the shared popup rectangle over the dimmed page, with the same head and the
//! same sheet. The rectangle is computed here for all three callers (painter,
//! scroll maximum, overlay blocker); deriving it twice is how a panel ends up
//! unscrollable or closes when clicked from the inside.

use super::*;
use ratatui::widgets::Clear;

impl App {
    /// Where the detail slip goes: the same popup every dialog uses.
    ///
    /// `chrome::popup_rect` keeps the card centred, no wider than 76 cells and
    /// no taller than sixteen rows, so the record sits where the help card and
    /// the instance form sit.
    pub(in crate::tui::app) fn paper_detail_rect(&self, area: Rect) -> Rect {
        // Measured from the record itself, the way the help card is: a fixed
        // sixteen-row box left a three-line task record sitting in a mostly
        // empty page, which is not how the instance form looks.
        let pal = self.paper_palette();
        let body_width = area.width.saturating_sub(4).clamp(20, 76).saturating_sub(5);
        let rows = Paragraph::new(self.paper_detail_lines(&pal))
            .wrap(Wrap { trim: false })
            .line_count(body_width.max(1));
        let height = u16::try_from(rows)
            .unwrap_or(u16::MAX)
            .saturating_add(4)
            .min(16);
        crate::tui::chrome::popup_rect_sized(area, height)
    }

    /// Rows inside the slip: one of air, the title, the hairline, then the
    /// body; the last row belongs to the bottom hairline.
    pub(in crate::tui::app) fn paper_detail_body(rect: Rect) -> Rect {
        Rect::new(
            rect.x.saturating_add(2),
            rect.y.saturating_add(3),
            rect.width.saturating_sub(5),
            rect.height.saturating_sub(4),
        )
    }

    /// The record itself, styled for the slip.
    ///
    /// Shared with `detail_max_scroll`, so the measured height and the painted
    /// height cannot drift: both wrap the same lines at the same width.
    pub(in crate::tui::app) fn paper_detail_lines(&self, pal: &PaperPalette) -> Vec<Line<'static>> {
        self.paper_record_lines(pal, true)
    }

    /// The same record, printed on the page instead of on a slip.
    ///
    /// Same `detail_lines()` source and the same label/value typesetting; only
    /// the background differs. `on_sheet` picks a slip one step brighter than
    /// the page, or the page itself, so the column matches what `classic` and
    /// `nightlight` print without a second copy of the field list.
    pub(in crate::tui::app) fn paper_record_lines(
        &self,
        pal: &PaperPalette,
        on_sheet: bool,
    ) -> Vec<Line<'static>> {
        let tint = |color: Color| {
            if on_sheet {
                sheet_ink(pal, color)
            } else {
                ink(pal, color)
            }
        };
        let raw = self
            .detail_lines()
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        let label_width = raw
            .iter()
            .filter_map(|line| split_detail_label(line).map(|(label, _)| label))
            .map(palette::width)
            .max()
            .unwrap_or(0);
        raw.into_iter()
            .map(|text| {
                if let Some(paragraph) = detail_paragraph(&text) {
                    Line::from(Span::styled(paragraph.to_owned(), tint(pal.soft)))
                } else if let Some((label, value)) = split_detail_label(&text) {
                    let padding = " ".repeat(label_width.saturating_sub(palette::width(label)));
                    Line::from(vec![
                        Span::styled(label.to_owned(), tint(pal.dim)),
                        Span::styled(padding, tint(pal.sheet)),
                        Span::styled("  ", tint(pal.sheet)),
                        Span::styled(value.to_owned(), tint(pal.ink)),
                    ])
                } else if text.starts_with('▸') {
                    // The action line the record closes with. It is the one
                    // sentence on the page that asks for a key, so it is
                    // printed in the accent rather than in body ink.
                    Line::from(Span::styled(
                        text,
                        tint(pal.accent).add_modifier(Modifier::BOLD),
                    ))
                } else if text.is_empty() {
                    Line::default()
                } else {
                    Line::from(Span::styled(text, tint(pal.ink)))
                }
            })
            .collect()
    }

    pub(in crate::tui::app) fn draw_paper_detail(&self, frame: &mut Frame<'_>, pal: &PaperPalette) {
        let rect = self.paper_detail_rect(frame.area());
        if rect.width < 12 || rect.height < 6 {
            return;
        }
        // `Block::style` recolours cells but keeps their glyphs, so the page's
        // rules and row text would print straight through the slip.
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Block::default().style(Style::default().fg(pal.ink).bg(pal.sheet)),
            rect,
        );
        // The dialog head, exactly as `modal/paper.rs` prints it: one row of
        // air, the title, a hairline.
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(
                    &self.i18n.text("common.details"),
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
        kit::rule_on(
            frame,
            Rect::new(rect.x, rect.bottom().saturating_sub(1), rect.width, 1),
            pal.rule,
            pal.sheet,
        );

        let body = Self::paper_detail_body(rect);
        if body.is_empty() {
            return;
        }
        self.hitmap.scroll_on(
            Layer::Detail,
            rect,
            crate::tui::action::ScrollTarget::DetailBody,
        );
        let lines = self.paper_detail_lines(pal);
        let total = Paragraph::new(lines.clone())
            .wrap(Wrap { trim: false })
            .line_count(body.width.max(1));
        let max_scroll = total.saturating_sub(usize::from(body.height));
        let scroll = self.detail_scroll.min(max_scroll);
        frame.render_widget(
            Paragraph::new(lines)
                .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
                .wrap(Wrap { trim: false }),
            body,
        );
        self.paper_scrollbar(frame, body, total, scroll, pal, pal.sheet);
    }
}
