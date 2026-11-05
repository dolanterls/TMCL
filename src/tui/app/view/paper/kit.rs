//! Typesetting primitives shared by every `paper` page.
//!
//! A page is only allowed to place text and ask for these six things: a sheet,
//! a hairline, a section head, a margin bar, a row of right-anchored columns and
//! a footnote. Keeping the arithmetic here is what stops each page from growing
//! its own idea of where a rule sits or how wide the status column is — the
//! failure mode the earlier presets already paid for.

use super::*;

/// Paints the page under everything else.
pub(in crate::tui::app) fn sheet(frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
    frame.render_widget(
        Block::default().style(Style::default().fg(pal.ink).bg(pal.bg)),
        area,
    );
}

/// A hairline drawn as text, so it keeps the terminal's own character rhythm.
pub(in crate::tui::app) fn rule(
    frame: &mut Frame<'_>,
    area: Rect,
    pal: &PaperPalette,
    color: Color,
) {
    rule_on(frame, area, color, pal.bg);
}

/// A hairline on an explicit surface.
///
/// The page is not the only paper: a dialog slip and the detail/help sheets are
/// one step brighter, and a rule printed with the page's background punches a
/// darker gap through them. Callers that sit on a slip pass its colour.
pub(crate) fn rule_on(frame: &mut Frame<'_>, area: Rect, color: Color, bg: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(usize::from(area.width)),
            Style::default().fg(color).bg(bg),
        ))),
        area,
    );
}

/// A section head: title, optional counter, rule, one blank row, content.
///
/// The blank row is part of the head rather than the caller's business: it is
/// what keeps a list from reading as a table, and every page needs the same
/// amount of it.
pub(in crate::tui::app) fn section(
    frame: &mut Frame<'_>,
    area: Rect,
    pal: &PaperPalette,
    title: &str,
    counter: Option<&str>,
) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect::new(area.x, area.y, 0, 0);
    }
    let counter_width = counter
        .filter(|text| !text.is_empty())
        .map_or(0, |text| palette::width(text) as u16 + 1);
    let title_width = area.width.saturating_sub(counter_width);
    if title_width > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(title, usize::from(title_width)),
                ink(pal, pal.dim).add_modifier(Modifier::BOLD),
            ))),
            Rect::new(area.x, area.y, title_width, 1),
        );
    }
    if let Some(counter) = counter.filter(|text| !text.is_empty() && area.width > counter_width) {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(counter, usize::from(area.width)),
                ink(pal, pal.faint),
            )))
            .alignment(Alignment::Right),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }
    if area.height < 2 {
        return Rect::new(area.x, area.bottom(), area.width, 0);
    }
    rule(
        frame,
        Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
        pal,
        pal.rule,
    );
    Rect::new(
        area.x,
        area.y.saturating_add(3).min(area.bottom()),
        area.width,
        area.height.saturating_sub(3),
    )
}

/// The margin bar that marks the selected row.
pub(in crate::tui::app) fn selection(frame: &mut Frame<'_>, row: Rect, pal: &PaperPalette) {
    if row.width == 0 || row.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("▌", ink(pal, pal.accent)))),
        Rect::new(row.x, row.y, 1, 1),
    );
}

/// Right-anchored columns, returned in the order the widths were given.
///
/// `widths` is read from the right edge inward and `gap` cells separate
/// neighbours. A column that no longer fits comes back zero-width instead of
/// being dropped, so the caller's indices keep pointing at the same column.
///
/// The rects span `area`'s height. A caller drawing one line per row must
/// therefore rebuild each rect at that row's `y`: handing the tall rect to a
/// paragraph paints every row's values into the first row, which is exactly how
/// the version and file tables once lost their columns.
pub(in crate::tui::app) fn columns(area: Rect, widths: &[u16], gap: u16) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(widths.len());
    let mut right = area.right();
    let mut previous = 0u16;
    for width in widths {
        // A zero-width column occupies nothing, so it neither takes a gutter
        // nor moves the anchor: it is simply reported at the current edge.
        if *width == 0 {
            rects.push(Rect::new(right, area.y, 0, area.height));
            continue;
        }
        if previous > 0 {
            right = right.saturating_sub(gap).max(area.x);
        }
        let width = (*width).min(right.saturating_sub(area.x));
        rects.push(Rect::new(
            right.saturating_sub(width),
            area.y,
            width,
            area.height,
        ));
        right = right.saturating_sub(width);
        previous = width;
    }
    rects
}

/// A footnote: the one line at the bottom of a list that says where you are.
pub(in crate::tui::app) fn note(frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette, text: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            palette::truncate(&format!("※ {text}"), usize::from(area.width)),
            ink(pal, pal.faint),
        ))),
        Rect::new(area.x, area.y, area.width, 1),
    );
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::core::config::models::{PaperTheme, ThemeMode};

    fn palette() -> PaperPalette {
        PaperTheme::Ivory.palette(ThemeMode::Light)
    }

    #[test]
    fn columns_anchor_to_the_right_and_keep_their_gutters() {
        let rects = columns(Rect::new(0, 0, 40, 1), &[10, 7, 14], 2);

        assert_eq!(rects[0], Rect::new(30, 0, 10, 1));
        assert_eq!(rects[1], Rect::new(21, 0, 7, 1));
        assert_eq!(rects[2], Rect::new(5, 0, 14, 1));
        // The gutter belongs to neither neighbour.
        assert_eq!(rects[0].x - rects[1].right(), 2);
        assert_eq!(rects[1].x - rects[2].right(), 2);
    }

    #[test]
    fn columns_collapse_to_zero_width_instead_of_shifting_the_order() {
        let rects = columns(Rect::new(0, 0, 8, 1), &[6, 6, 6], 2);

        assert_eq!(rects.len(), 3, "the caller keeps its column indices");
        assert_eq!(rects[0], Rect::new(2, 0, 6, 1));
        assert_eq!(rects[1], Rect::new(0, 0, 0, 1));
        assert_eq!(rects[2], Rect::new(0, 0, 0, 1));
    }

    #[test]
    fn a_zero_width_column_does_not_swallow_a_gutter() {
        let rects = columns(Rect::new(0, 0, 20, 1), &[5, 0, 5], 2);

        // Right to left: 5, nothing, then 5 with only one real gutter.
        assert_eq!(rects[0], Rect::new(15, 0, 5, 1));
        assert_eq!(rects[1], Rect::new(15, 0, 0, 1));
        assert_eq!(rects[2], Rect::new(8, 0, 5, 1));
    }

    #[test]
    fn a_section_head_leaves_a_title_a_rule_and_a_blank_row() {
        let pal = palette();
        let backend = TestBackend::new(24, 8);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut content = Rect::default();
        terminal
            .draw(|frame| {
                content = section(
                    frame,
                    Rect::new(0, 0, 24, 8),
                    &pal,
                    "Instances",
                    Some("14 / 19"),
                );
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();

        assert_eq!(content, Rect::new(0, 3, 24, 5));
        assert_eq!(buffer[(0, 0)].symbol(), "I", "the title starts at the left");
        assert_eq!(buffer[(0, 1)].symbol(), "─", "a rule closes the head");
        assert_eq!(buffer[(0, 2)].symbol(), " ", "one blank row follows");
        assert_eq!(content.y, 3, "content starts under the blank row");
        // The counter is right-aligned on the title row.
        let row = (0..24).map(|x| buffer[(x, 0)].symbol()).collect::<String>();
        assert!(row.trim_end().ends_with("14 / 19"), "{row:?}");
    }
}
