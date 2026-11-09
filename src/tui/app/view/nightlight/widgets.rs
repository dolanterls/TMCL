//! Drawing primitives for the `nightlight` preset.
//!
//! Everything here is deliberately borderless: hierarchy comes from the
//! `surface` step of the palette, never from a box. The only line in the whole
//! preset is the hairline above the footer, and that is drawn by the shell.
//!
//! Clean-room notice: see `crate::tui::app::view::nightlight`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use tui_big_text::{BigText, BigTextBuilder, PixelSize};

use super::tone::{self, Palette, Rgb};

/// Card padding: two columns on the left, one on the right. The title sits one
/// row below the card's top edge and the content starts one row below the
/// title, so no label ever touches the card boundary.
pub const CARD_PAD_LEFT: u16 = 2;
pub const CARD_PAD_RIGHT: u16 = 1;
pub const CARD_TITLE_Y: u16 = 1;
pub const CARD_PAD_TOP: u16 = 2;

/// One eighth-block per step, so a bar moves smoothly at cell resolution.
const PARTIAL: [char; 9] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// Terminal width of a string, counting CJK and emoji as two cells.
pub fn width(value: &str) -> usize {
    Line::from(value).width()
}

/// Paints a borderless card and its title, and returns the content rectangle.
///
/// The title is drawn by hand rather than through `Block::title`: ratatui
/// anchors block titles to the block's *outer* top row, so a title set that way
/// always touches the card edge no matter what padding the block has. Drawing
/// it on [`CARD_TITLE_Y`] keeps the same spacing as the reference.
pub fn card(frame: &mut Frame<'_>, area: Rect, title: &str, pal: &Palette) -> Rect {
    if area.is_empty() {
        return area;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(pal.surface)),
        area,
    );
    if !title.is_empty() && area.height > CARD_TITLE_Y {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                title.to_owned(),
                Style::default()
                    .fg(pal.accent)
                    .bg(pal.surface)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect::new(
                area.x.saturating_add(CARD_PAD_LEFT),
                area.y.saturating_add(CARD_TITLE_Y),
                area.width.saturating_sub(CARD_PAD_LEFT + CARD_PAD_RIGHT),
                1,
            ),
        );
    }
    card_inner(area)
}

/// A dialog card: title on the first row, content on the second.
///
/// The shared modal layout sizes every dialog assuming the title owns a row
/// above the content. A dialog drawn with the page card (one row of air above
/// the title) is a row short, and its last line collides with the key hints.
pub fn dialog_card(frame: &mut Frame<'_>, area: Rect, title: &str, pal: &Palette) -> Rect {
    if area.is_empty() {
        return area;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(pal.surface)),
        area,
    );
    if !title.is_empty() && area.height > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                title.to_owned(),
                Style::default()
                    .fg(pal.accent)
                    .bg(pal.surface)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect::new(
                area.x.saturating_add(CARD_PAD_LEFT),
                area.y,
                area.width.saturating_sub(CARD_PAD_LEFT + CARD_PAD_RIGHT),
                1,
            ),
        );
    }
    Rect {
        x: area.x.saturating_add(CARD_PAD_LEFT),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(CARD_PAD_LEFT + CARD_PAD_RIGHT),
        height: area.height.saturating_sub(1),
    }
}

/// Content rectangle of a card built by [`card`]: below the title row, inside
/// the horizontal padding.
pub fn card_inner(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(CARD_PAD_LEFT),
        y: area.y.saturating_add(CARD_PAD_TOP),
        width: area.width.saturating_sub(CARD_PAD_LEFT + CARD_PAD_RIGHT),
        height: area.height.saturating_sub(CARD_PAD_TOP),
    }
}

/// The one large number a card is allowed. `Quadrant` is the pixel size the
/// logo already uses: common Windows terminal fonts cover it.
pub fn big_number(value: &str, pal: &Palette) -> BigText<'static> {
    BigTextBuilder::default()
        .pixel_size(PixelSize::Quadrant)
        .style(Style::default().fg(pal.accent).bg(pal.surface))
        .lines([Line::from(value.to_owned())])
        .build()
}

/// A metric on one line: the label first in `muted`, then the value in `color`.
/// Reading order matches speech ("instances: 3"), and the value never sits in
/// the tile's top-left corner on its own.
pub fn stat(value: &str, label: &str, color: Color, pal: &Palette) -> Paragraph<'static> {
    Paragraph::new(Line::from(vec![
        Span::styled(
            format!("{label} "),
            Style::default().fg(pal.muted).bg(pal.surface),
        ),
        Span::styled(
            value.to_owned(),
            Style::default()
                .fg(color)
                .bg(pal.surface)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
}

/// A rounded-off status chip: one space of padding on each side, background
/// tinted 18% towards the foreground colour.
pub fn pill(label: &str, fg: Color, pal: &Palette) -> Span<'static> {
    let background = blend(pal.surface, fg, 0.18);
    Span::styled(format!(" {label} "), Style::default().fg(fg).bg(background))
}

/// A progress bar of eighth blocks, coloured from `from` to `to` across its
/// width. No highlight travels along it: the preset has no animation.
pub fn bar(ratio: f64, width: u16, from: Color, to: Color) -> Line<'static> {
    let cells = usize::from(width);
    if cells == 0 {
        return Line::default();
    }
    let clamped = ratio.clamp(0.0, 1.0);
    let eighths = (clamped * cells as f64 * 8.0).round() as usize;
    let spans = (0..cells)
        .map(|index| {
            let filled = eighths.saturating_sub(index * 8).min(8);
            let glyph = PARTIAL[filled];
            let progress = if cells > 1 {
                index as f64 / (cells - 1) as f64
            } else {
                0.0
            };
            let style = if filled == 0 {
                Style::default()
            } else {
                Style::default().fg(interpolate(from, to, progress))
            };
            Span::styled(glyph.to_string(), style)
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

/// A bar whose progress is unknown: a still `░` track, no shimmer.
/// [`bar`] with a faint track: the unfilled cells stay visible, so a row at 5%
/// and a row at 95% still show the same bar footprint and the rows line up.
pub fn tracked_bar(ratio: f64, width: u16, from: Color, to: Color, pal: &Palette) -> Line<'static> {
    let track = blend(pal.surface, pal.muted, 0.55);
    let mut line = bar(ratio, width, from, to);
    for span in line.spans.iter_mut() {
        if span.content.trim().is_empty() {
            *span = Span::styled("░", Style::default().fg(track));
        }
    }
    line
}

pub fn bar_unknown(width: u16, pal: &Palette) -> Line<'static> {
    let color = blend(pal.surface, pal.muted, 0.55);
    Line::from(Span::styled(
        "░".repeat(usize::from(width)),
        Style::default().fg(color),
    ))
}

/// Table header: `faint`, bold, each label padded to its column width.
pub fn table_head(labels: &[&str], widths: &[u16], pal: &Palette) -> Line<'static> {
    let spans = labels
        .iter()
        .zip(widths)
        .map(|(label, width)| {
            Span::styled(
                pad_right(label, *width),
                Style::default()
                    .fg(pal.faint)
                    .bg(pal.surface)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

/// A selected table row: the whole width carries the selection colour, not just
/// the text.
#[cfg(test)]
pub fn selected_row(spans: Vec<Span<'static>>, width: u16, pal: &Palette) -> Line<'static> {
    let used: usize = spans.iter().map(|span| width_of(&span.content)).sum();
    let mut spans = spans
        .into_iter()
        .map(|span| {
            let style = span.style.bg(pal.selection).fg(pal.text);
            Span::styled(span.content.into_owned(), style)
        })
        .collect::<Vec<_>>();
    if usize::from(width) > used {
        spans.push(Span::styled(
            " ".repeat(usize::from(width) - used),
            Style::default().bg(pal.selection),
        ));
    }
    Line::from(spans)
}

/// The `n/m` counter drawn at the bottom-right of a table card. Alignment is
/// the caller's job: the card knows its own width.
#[cfg(test)]
pub fn counter(position: usize, total: usize, pal: &Palette) -> Line<'static> {
    Line::from(Span::styled(
        format!("{position}/{total}"),
        Style::default().fg(pal.faint).bg(pal.surface),
    ))
}

/// A wrapped block of text that scrolls, with a scrollbar when it has to.
///
/// A description column is the one place where text legitimately exceeds its
/// box, so it always wraps and always shows how much is left. Drawing it as a
/// bare `Paragraph` both truncates the sentences and hides the overflow.
///
/// The rail is drawn one column past `area`, in the card's right padding:
/// `card_inner` already stops short of the card edge, so a bar inside `area`
/// floats a cell away from the border instead of hugging it the way the
/// bordered presets' scrollbars do. Text keeps the full width.
pub fn detail_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    lines: Vec<Line<'static>>,
    scroll: usize,
    pal: &Palette,
) -> usize {
    use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};

    if area.is_empty() {
        return 0;
    }
    let total = Paragraph::new(lines.clone())
        .wrap(Wrap { trim: false })
        .line_count(area.width);
    let viewport = usize::from(area.height);
    if total <= viewport {
        frame.render_widget(
            Paragraph::new(lines)
                .style(Style::default().fg(pal.muted).bg(pal.surface))
                .wrap(Wrap { trim: false })
                .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
            area,
        );
        return 0;
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(pal.muted).bg(pal.surface))
            .wrap(Wrap { trim: false })
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        area,
    );
    let rail = Rect::new(area.right(), area.y, 1, area.height);
    let mut state = ScrollbarState::new(total.saturating_sub(viewport))
        .position(scroll.min(total.saturating_sub(viewport)))
        .viewport_content_length(viewport);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("▌")
            .style(Style::default().fg(pal.bg).bg(pal.surface))
            .track_style(Style::default().fg(pal.faint).bg(pal.surface))
            .thumb_style(Style::default().fg(pal.accent).bg(pal.surface)),
        rail,
        &mut state,
    );
    total.saturating_sub(viewport)
}

/// Truncate with an ellipsis, then pad to `width` so the next column starts at
/// a fixed offset even when the text is short.
pub fn fit_cell(text: &str, cells: usize) -> String {
    let mut cell = crate::tui::palette::truncate(text, cells);
    let used = width(&cell);
    if used < cells {
        cell.push_str(&" ".repeat(cells - used));
    }
    cell
}

/// Right-align inside `width`: sizes and counts read as a column, not prose.
pub fn right_cell(text: &str, cells: usize) -> String {
    let cell = crate::tui::palette::truncate(text, cells);
    let used = width(&cell);
    if used < cells {
        format!("{}{}", " ".repeat(cells - used), cell)
    } else {
        cell
    }
}

/// A footer key hint: accent key, muted description, three spaces of gap.
#[cfg(test)]
pub fn key_hint(key: &str, label: &str, pal: &Palette) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            key.to_owned(),
            Style::default()
                .fg(pal.accent)
                .bg(pal.bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {label}   "),
            Style::default().fg(pal.muted).bg(pal.bg),
        ),
    ]
}

#[cfg(test)]
fn width_of(value: &str) -> usize {
    width(value)
}

/// Blend `tint` into `base`. Non-RGB colours are returned untouched, which only
/// happens in tests.
pub fn blend(base: Color, tint: Color, ratio: f64) -> Color {
    match (tone::rgb_of(base), tone::rgb_of(tint)) {
        (Some(base), Some(tint)) => mix_color(base, tint, ratio),
        _ => base,
    }
}

fn mix_color(from: Rgb, to: Rgb, ratio: f64) -> Color {
    tone::to_color(tone::mix(from, to, ratio))
}

fn interpolate(from: Color, to: Color, progress: f64) -> Color {
    match (tone::rgb_of(from), tone::rgb_of(to)) {
        (Some(from), Some(to)) => mix_color(from, to, progress),
        _ => from,
    }
}

/// Pad a label to exactly `width` cells, truncating with an ellipsis when it
/// does not fit.
pub fn pad_right(value: &str, width: u16) -> String {
    let target = usize::from(width);
    let mut text = value.to_owned();
    let mut used = self::width(&text);
    if used > target {
        while used > target.saturating_sub(1) {
            if text.pop().is_none() {
                break;
            }
            used = self::width(&text);
        }
        text.push('…');
        used = self::width(&text);
    }
    if used < target {
        text.push_str(&" ".repeat(target - used));
    }
    text
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    use super::*;
    use crate::core::config::models::{NightlightTheme, ThemeMode};

    fn palette() -> Palette {
        tone::palette(NightlightTheme::Live, ThemeMode::Dark, None, false)
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn a_card_draws_its_title_without_any_border() {
        let pal = palette();
        let backend = TestBackend::new(24, 6);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                let area = frame.area();
                let inner = card(frame, area, "now", &pal);
                frame.render_widget(
                    Paragraph::new("content").style(Style::default().bg(pal.surface)),
                    inner,
                );
            })
            .expect("draw card");
        let rows = buffer_text(&terminal);
        // One row of breathing space above the title, then the title itself,
        // lower-case exactly as passed in.
        assert_eq!(rows[0].trim(), "", "title must not touch the card edge");
        assert!(rows[1].contains("  now"), "title missing: {:?}", rows[1]);
        assert!(!rows[1].contains("NOW"), "titles must not be upper-cased");
        // Content starts on the row below the title, two columns in.
        assert!(
            rows[2].contains("  content"),
            "content offset: {:?}",
            rows[2]
        );
        assert!(!rows[2].contains("now"), "title must not repeat");
        // No border glyphs anywhere.
        let all = rows.join("");
        assert!(
            !all.contains('│') && !all.contains('─') && !all.contains('┌'),
            "card drew a border: {all:?}"
        );
    }

    #[test]
    fn card_inner_stays_inside_the_card() {
        let area = Rect::new(10, 5, 30, 8);
        let inner = card_inner(area);
        assert_eq!(inner.x, area.x + 2);
        assert_eq!(inner.y, area.y + 2);
        assert_eq!(inner.width, area.width - 3);
        assert_eq!(inner.height, area.height - 2);
        assert!(inner.right() <= area.right() && inner.bottom() <= area.bottom());
        // Degenerate rectangles must not panic or invert.
        let tiny = card_inner(Rect::new(0, 0, 1, 1));
        assert_eq!(tiny.width, 0);
        assert_eq!(tiny.height, 0);
    }

    #[test]
    fn a_stat_reads_label_then_value() {
        let pal = palette();
        let backend = TestBackend::new(20, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(stat("12h", "playtime", pal.accent2, &pal), frame.area());
            })
            .expect("draw stat");
        let rows = buffer_text(&terminal);
        assert!(
            rows[0].starts_with("playtime 12h"),
            "label comes first: {:?}",
            rows[0]
        );
        let cell = terminal.backend().buffer()[(9, 0)].clone();
        assert_eq!(cell.fg, pal.accent2, "the value keeps its own colour");
    }

    #[test]
    fn a_pill_tints_its_background_and_keeps_a_space_on_both_sides() {
        let pal = palette();
        let chip = pill("ready", pal.ok, &pal);
        assert_eq!(chip.content, " ready ");
        let background = chip.style.bg.expect("pill background");
        assert_ne!(background, pal.surface);
        assert_eq!(background, blend(pal.surface, pal.ok, 0.18));
        assert_eq!(chip.style.fg, Some(pal.ok));
    }

    #[test]
    fn a_bar_fills_from_empty_to_full() {
        let pal = palette();
        let empty = bar(0.0, 10, pal.accent, pal.accent2);
        assert_eq!(empty.spans.len(), 10);
        assert!(empty.spans.iter().all(|span| span.content == " "));

        let half = bar(0.5, 10, pal.accent, pal.accent2);
        let full_cells = half.spans.iter().filter(|span| span.content == "█").count();
        assert_eq!(full_cells, 5);

        let full = bar(1.0, 10, pal.accent, pal.accent2);
        assert!(full.spans.iter().all(|span| span.content == "█"));
        // The colour runs from the first colour to the second.
        assert_eq!(full.spans[0].style.fg, Some(pal.accent));
        assert_eq!(full.spans[9].style.fg, Some(pal.accent2));
        // Out-of-range ratios clamp instead of panicking.
        assert_eq!(bar(-1.0, 4, pal.accent, pal.accent2).spans.len(), 4);
        assert!(
            bar(2.0, 4, pal.accent, pal.accent2)
                .spans
                .iter()
                .all(|span| span.content == "█")
        );
    }

    #[test]
    fn an_unknown_bar_is_a_still_track() {
        let pal = palette();
        let line = bar_unknown(6, &pal);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content, "░░░░░░");
    }

    #[test]
    fn a_selected_row_carries_the_tint_to_its_last_cell() {
        let pal = palette();
        let line = selected_row(
            vec![Span::styled("item", Style::default().fg(pal.muted))],
            12,
            &pal,
        );
        let backend = TestBackend::new(12, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(line.clone(), frame.area());
            })
            .expect("draw row");
        for x in 0..12 {
            assert_eq!(
                terminal.backend().buffer()[(x, 0)].bg,
                pal.selection,
                "cell {x} lost the selection tint"
            );
        }
    }

    /// The scrollbar sits in the card's right padding, one column past the text
    /// area, so it touches the card edge rather than taking a column of text.
    #[test]
    fn the_pane_scrollbar_sits_on_the_card_edge() {
        let pal = palette();
        let backend = TestBackend::new(12, 4);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let lines = (0..5).map(|_| Line::from("0123456789")).collect::<Vec<_>>();
        let mut max = 0;
        terminal
            .draw(|frame| {
                max = detail_pane(frame, Rect::new(0, 0, 10, 4), lines.clone(), 0, &pal);
            })
            .expect("draw pane");
        // Five ten-cell rows in a four-row window: one row of travel.
        assert_eq!(max, 1, "the pane miscounted its scrollable rows");

        let buffer = terminal.backend().buffer();
        let column = (0..12)
            .find(|x| (0..4).any(|y| matches!(buffer[(*x, y)].symbol(), "▌" | "│")))
            .expect("a scrollbar");
        assert_eq!(column, 10, "the rail must sit one column past the text");
        assert_eq!(
            (0..10).map(|x| buffer[(x, 0)].symbol()).collect::<String>(),
            "0123456789",
            "the text lost a column to the rail"
        );
    }

    #[test]
    fn a_table_head_pads_each_label_to_its_column() {
        let pal = palette();
        let line = table_head(&["name", "size"], &[10, 6], &pal);
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content, "name      ");
        assert_eq!(line.spans[1].content, "size  ");
        assert_eq!(line.spans[0].style.fg, Some(pal.faint));
    }

    #[test]
    fn counters_and_key_hints_render_their_text() {
        let pal = palette();
        let count = counter(3, 7, &pal);
        assert_eq!(count.spans[0].content, "3/7");

        let hint = key_hint("␣", "launch", &pal);
        assert_eq!(hint.len(), 2);
        assert_eq!(hint[0].content, "␣");
        assert_eq!(hint[1].content, " launch   ");
        assert_eq!(hint[0].style.fg, Some(pal.accent));
        assert_eq!(hint[1].style.fg, Some(pal.muted));
    }

    #[test]
    fn big_numbers_produce_block_glyphs() {
        let pal = palette();
        let backend = TestBackend::new(30, 8);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(big_number("12h", &pal), frame.area());
            })
            .expect("draw big number");
        let all = buffer_text(&terminal).join("");
        assert!(
            all.chars()
                .any(|character| matches!(character, '▘' | '▝' | '▖' | '▗' | '▀' | '▄' | '█')),
            "no block glyphs: {all:?}"
        );
    }

    #[test]
    fn pad_right_handles_wide_and_overlong_labels() {
        assert_eq!(pad_right("ab", 4), "ab  ");
        assert_eq!(pad_right("概览", 6), "概览  ");
        assert_eq!(pad_right("toolong", 4), "too…");
        assert_eq!(pad_right("", 2), "  ");
    }
}
