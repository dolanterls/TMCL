//! Sidebar pieces for the `nightlight` preset: brand, numbered navigation and
//! the environment summary pinned to the bottom.
//!
//! The sidebar is not a card. It sits directly on the page colour, which is why
//! the selected row is a full-width accent bar rather than a tinted panel. The
//! navigation is the one place where this preset leaves the shared geometry
//! contract, so the drawn rows and the clickable rows come from the same
//! [`NavLayout`].
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use tui_big_text::{BigTextBuilder, PixelSize};

use super::tone::Palette;

/// Rows the bottom summary can occupy: phase·time, running tasks, version·theme
/// and the session uptime. Accounts and instances deliberately never appear
/// here — they are entities, and belong in content cards.
pub const SUMMARY_ROWS: u16 = 4;
/// Blank row between the navigation and the summary.
const SUMMARY_GAP: u16 = 1;
/// Below this width the navigation drops its labels and keeps the icons.
const COMPACT_WIDTH: u16 = 12;

/// Rows between the sidebar's top edge and the brand.
pub const BRAND_TOP: u16 = 1;
/// Rows the pixel wordmark needs (`Quadrant`: four rows of block glyphs).
const BIG_BRAND_ROWS: u16 = 4;
/// Columns the wordmark needs before it is worth drawing: sixteen of glyphs
/// plus two of air on each side.
const BIG_BRAND_COLS: u16 = 20;

/// Rows the brand band occupies: four for the pixel wordmark, one for the
/// plain line a narrow column falls back to.
pub fn brand_rows(area: Rect) -> u16 {
    if area.width >= BIG_BRAND_COLS {
        BIG_BRAND_ROWS
    } else {
        1
    }
}

/// Rows between the sidebar's top edge and the first navigation entry: one row
/// of margin, the brand band, one blank row.
pub fn nav_top(area: Rect) -> u16 {
    BRAND_TOP.saturating_add(brand_rows(area)).saturating_add(1)
}

/// Where the navigation lives inside the sidebar column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavLayout {
    /// Row carrying the brand.
    pub brand_y: u16,
    /// First row a navigation entry may use.
    pub start_y: u16,
    /// Exclusive bottom limit: the summary below it is never overwritten.
    pub end_y: u16,
    /// True when the sidebar is too narrow for labels.
    pub compact: bool,
    /// Rows between two entries: 2 when the column has room for the air, 1 on
    /// a short terminal where the list has to stay complete.
    pub stride: u16,
}

/// Blank rows between two navigation entries. One row of air is what keeps the
/// list from reading as a solid block; the reference has the same gap.
pub const NAV_STRIDE: u16 = 2;

/// Derive the navigation box from the sidebar rectangle.
pub fn nav_layout(area: Rect, count: usize) -> NavLayout {
    // The brand sits one row below the sidebar's top edge, not on it: flush
    // against the corner the wordmark reads as a window title rather than as
    // part of the column. Navigation starts one blank row below the brand.
    let brand_y = area.y.saturating_add(BRAND_TOP);
    let start_y = area.y.saturating_add(nav_top(area));
    let reserved = SUMMARY_ROWS + SUMMARY_GAP;
    let bottom = area.bottom().saturating_sub(reserved).max(start_y);
    let needed = (count as u16).saturating_mul(NAV_STRIDE);
    let stride = if bottom.saturating_sub(start_y) >= needed {
        NAV_STRIDE
    } else {
        1
    };
    let end_y = bottom.min(start_y.saturating_add((count as u16).saturating_mul(stride)));
    NavLayout {
        brand_y,
        start_y,
        end_y,
        compact: area.width < COMPACT_WIDTH,
        stride,
    }
}

/// Clickable rectangle of every navigation row.
///
/// The gap between entries belongs to no entry: a click in it does nothing,
/// which is why the stride is used here and in `nav_index_at` alike.
pub fn nav_rects(area: Rect, count: usize) -> Vec<Rect> {
    let layout = nav_layout(area, count);
    (0..count)
        .map(|index| {
            Rect::new(
                area.x,
                layout
                    .start_y
                    .saturating_add((index as u16).saturating_mul(layout.stride)),
                area.width,
                1,
            )
        })
        .take_while(|rect| rect.y < layout.end_y)
        .collect()
}

/// Which navigation entry sits under a mouse position, if any.
#[cfg(test)]
pub fn nav_index_at(area: Rect, count: usize, row: u16, column: u16) -> Option<usize> {
    if column < area.x || column >= area.right() {
        return None;
    }
    let layout = nav_layout(area, count);
    if row < layout.start_y || row >= layout.end_y {
        return None;
    }
    let offset = row - layout.start_y;
    // The blank row between two entries is a gap, not a hit target.
    if offset % layout.stride != 0 {
        return None;
    }
    let index = usize::from(offset / layout.stride);
    (index < count).then_some(index)
}

/// Draws the brand band: the same wordmark the classic overview uses, at
/// `Quadrant` size (16 columns by four rows), centred in the sidebar. A narrow
/// column keeps the one-line ` ☾ TMCL ` instead — four rows of block glyphs do
/// not fit in a six-column rail.
pub fn draw_brand(frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
    if area.width >= BIG_BRAND_COLS && area.height >= BIG_BRAND_ROWS {
        let banner = BigTextBuilder::default()
            .pixel_size(PixelSize::Quadrant)
            .style(Style::default().fg(pal.accent).bg(pal.bg))
            .lines([Line::from("TMCL")])
            .centered()
            .build();
        frame.render_widget(
            banner,
            Rect {
                height: BIG_BRAND_ROWS,
                ..area
            },
        );
        return;
    }
    let line = if area.width >= 8 {
        brand(pal)
    } else {
        brand_compact(pal)
    };
    frame.render_widget(Paragraph::new(line), area);
}

/// The brand, on the sidebar's first row: ` ☾ TMCL `. No version here — that
/// lives in the summary, exactly where the reference puts it.
pub fn brand(pal: &Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ☾ ", Style::default().fg(pal.accent).bg(pal.bg)),
        Span::styled(
            "TMCL",
            Style::default()
                .fg(pal.accent)
                .bg(pal.bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default().bg(pal.bg)),
    ])
}

/// Brand for the icon rail: at six columns there is no room for the wordmark,
/// so only the moon remains.
pub fn brand_compact(pal: &Palette) -> Line<'static> {
    Line::from(Span::styled(
        "  ☾ ",
        Style::default().fg(pal.accent).bg(pal.bg),
    ))
}

/// One navigation row: number, icon and label, with the selected entry painted
/// as a full-width accent bar.
pub fn nav_line(
    index: usize,
    label: &str,
    icon: &str,
    active: bool,
    width: u16,
    pal: &Palette,
) -> Line<'static> {
    let number = (index + 1).to_string();
    let compact = width < COMPACT_WIDTH;
    let (foreground, background) = if active {
        (pal.bg, pal.accent)
    } else {
        (pal.muted, pal.bg)
    };
    let modifier = if active {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };
    let number_style = Style::default()
        .fg(if active { pal.bg } else { pal.faint })
        .bg(background);
    let label_style = Style::default()
        .fg(foreground)
        .bg(background)
        .add_modifier(modifier);
    let icon_style = Style::default()
        .fg(if active { pal.bg } else { pal.muted })
        .bg(background)
        .add_modifier(modifier);

    let mut spans = vec![
        Span::styled("  ", Style::default().bg(background)),
        Span::styled(number, number_style),
        Span::styled(" ", Style::default().bg(background)),
    ];
    if compact {
        spans.push(Span::styled(icon.to_owned(), icon_style));
    } else {
        spans.push(Span::styled(label.to_owned(), label_style));
    }
    let used: usize = spans
        .iter()
        .map(|span| super::widgets::width(&span.content))
        .sum();
    if usize::from(width) > used {
        spans.push(Span::styled(
            " ".repeat(usize::from(width) - used),
            Style::default().bg(background),
        ));
    }
    Line::from(spans)
}

/// Bottom summary, up to four rows: phase and clock, running tasks (skipped
/// when there are none), version and theme, then the session uptime.
///
/// Every row starts on the same column as the brand glyph and the navigation
/// numbers — the version line used to sit one cell left of the moon, which made
/// the block look ragged. The uptime lives here rather than in the footer: it
/// is session state, and the footer is the place for what just happened.
pub fn summary(
    phase: &str,
    clock: &str,
    running: Option<(usize, &str)>,
    version: &str,
    theme_name: &str,
    elapsed: &str,
    pal: &Palette,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled("  ☾ ", Style::default().fg(pal.accent).bg(pal.bg)),
        Span::styled(phase.to_owned(), Style::default().fg(pal.muted).bg(pal.bg)),
        Span::styled(" · ", Style::default().fg(pal.faint).bg(pal.bg)),
        Span::styled(clock.to_owned(), Style::default().fg(pal.faint).bg(pal.bg)),
    ])];
    if let Some((count, label)) = running.filter(|(count, _)| *count > 0) {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default().bg(pal.bg)),
            Span::styled(
                count.to_string(),
                Style::default()
                    .fg(pal.accent2)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {label}"),
                Style::default().fg(pal.muted).bg(pal.bg),
            ),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("  ", Style::default().bg(pal.bg)),
        Span::styled(
            version.to_owned(),
            Style::default().fg(pal.faint).bg(pal.bg),
        ),
        Span::styled(" · ", Style::default().fg(pal.faint).bg(pal.bg)),
        Span::styled(
            theme_name.to_owned(),
            Style::default().fg(pal.muted).bg(pal.bg),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  ", Style::default().bg(pal.bg)),
        Span::styled(
            elapsed.to_owned(),
            Style::default().fg(pal.faint).bg(pal.bg),
        ),
    ]));
    lines
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::Paragraph;

    use super::*;
    use crate::core::config::models::{NightlightTheme, ThemeMode};
    use crate::tui::app::view::nightlight::tone;
    use crate::tui::app::view::nightlight::widgets::width;

    fn palette() -> Palette {
        tone::palette(NightlightTheme::Live, ThemeMode::Dark, None, false)
    }

    #[test]
    fn navigation_rows_and_hit_tests_are_inverse() {
        let area = Rect::new(0, 0, 26, 40);
        let rects = nav_rects(area, 7);
        assert_eq!(rects.len(), 7);
        for (index, rect) in rects.iter().enumerate() {
            assert_eq!(
                nav_index_at(area, 7, rect.y, area.x),
                Some(index),
                "row {} should map back to {index}",
                rect.y
            );
        }
        // The brand row and the gap below it are not clickable.
        assert_eq!(nav_index_at(area, 7, 0, 0), None);
        assert_eq!(nav_index_at(area, 7, 1, 0), None);
        assert_eq!(nav_index_at(area, 7, 2, 0), None);
        // Neither is anything below the list or outside the column.
        assert_eq!(nav_index_at(area, 7, 30, 0), None);
        assert_eq!(nav_index_at(area, 7, 2, 26), None);
    }

    #[test]
    fn the_navigation_never_reaches_the_summary() {
        let area = Rect::new(0, 0, 26, 40);
        let layout = nav_layout(area, 7);
        assert_eq!(layout.brand_y, 1);
        // One row of margin, the four-row wordmark, one blank row.
        assert_eq!(nav_top(area), 6);
        assert_eq!(layout.start_y, nav_top(area));
        // Seven entries with one blank row between them: 6 + 14.
        assert_eq!(layout.stride, 2);
        assert_eq!(layout.end_y, 20);
        assert!(layout.end_y <= area.bottom() - SUMMARY_ROWS);
        // A short sidebar clips instead of overlapping the summary, and drops
        // the gaps first so the list stays complete.
        let short = nav_layout(Rect::new(0, 0, 26, 12), 7);
        assert_eq!(short.stride, 1);
        assert!(short.end_y <= 12 - (SUMMARY_ROWS + SUMMARY_GAP));
        assert!(short.end_y >= short.start_y);
    }

    #[test]
    fn the_selected_row_paints_the_full_width() {
        let pal = palette();
        let backend = TestBackend::new(24, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                // ASCII label: a wide glyph leaves its second cell unstyled in
                // the backend, which says nothing about the row itself.
                frame.render_widget(nav_line(0, "logs", "≡", true, 24, &pal), frame.area());
            })
            .expect("draw nav");
        let buffer = terminal.backend().buffer();
        for x in 0..24 {
            assert_eq!(
                buffer[(x, 0)].bg,
                pal.accent,
                "cell {x} is not part of the selection bar"
            );
        }
        assert_eq!(
            buffer[(4, 0)].fg,
            pal.bg,
            "selected text uses the page colour"
        );
    }

    #[test]
    fn an_unselected_row_keeps_the_page_background() {
        let pal = palette();
        let line = nav_line(1, "实例", "▤", false, 24, &pal);
        let rendered: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            rendered.starts_with("  2"),
            "number then label: {rendered:?}"
        );
        assert!(rendered.contains("实例"));
        assert!(line.spans.iter().all(|span| span.style.bg == Some(pal.bg)));
    }

    #[test]
    fn compact_rows_keep_the_number_and_icon_only() {
        let pal = palette();
        let line = nav_line(2, "下载", "↓", false, 6, &pal);
        let rendered: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(rendered.contains('↓'));
        assert!(!rendered.contains("下载"), "compact row kept its label");
        assert_eq!(width(&rendered), 6);
    }

    #[test]
    fn the_summary_stacks_time_version_and_uptime_without_entities() {
        let pal = palette();
        let idle = summary(
            "白天",
            "12:00",
            None,
            "v0.1.0",
            "nightlight",
            "4m 37s",
            &pal,
        );
        assert_eq!(idle.len(), 3);

        let busy = summary(
            "黄昏",
            "19:42",
            Some((2, "进行中")),
            "v0.1.0",
            "nightlight",
            "4m 37s",
            &pal,
        );
        assert_eq!(busy.len(), 4);
        let text: String = busy
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(text.contains("黄昏"));
        assert!(text.contains("19:42"));
        assert!(text.contains("2 进行中"));
        assert!(text.contains("v0.1.0"));
        assert!(text.contains("nightlight"));
        assert!(
            text.contains("4m 37s"),
            "the uptime belongs under the version"
        );
        assert!(
            !text.contains("Steve"),
            "entities must stay out of the sidebar"
        );

        // A zero count is skipped rather than drawn as "0".
        let zero = summary(
            "白天",
            "12:00",
            Some((0, "进行中")),
            "v0.1.0",
            "live",
            "0s",
            &pal,
        );
        assert_eq!(zero.len(), 3);
    }

    /// The version line used to start one cell left of the moon glyph, and the
    /// uptime lived in the footer instead of here. All three rows now share the
    /// moon's column.
    #[test]
    fn every_summary_row_starts_on_the_same_column() {
        let pal = palette();
        let rows = summary("夜间", "22:11", None, "v0.1.0", "live", "4m 37s", &pal)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], "  ☾ 夜间 · 22:11");
        assert_eq!(rows[1], "  v0.1.0 · live");
        assert_eq!(rows[2], "  4m 37s");

        let indent = |row: &str| row.len() - row.trim_start().len();
        assert_eq!(indent(&rows[0]), 2, "the moon sets the column");
        assert_eq!(indent(&rows[1]), indent(&rows[0]));
        assert_eq!(indent(&rows[2]), indent(&rows[0]));
    }

    #[test]
    fn the_brand_sits_on_the_first_row() {
        let pal = palette();
        let line = brand(&pal);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "  ☾ TMCL ");
        assert_eq!(line.spans[0].style.fg, Some(pal.accent));

        let backend = TestBackend::new(20, 3);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget(Paragraph::new(brand(&pal)), frame.area()))
            .expect("draw brand");
        let row: String = (0..20)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
            .collect();
        assert!(row.starts_with("  ☾ TMCL"));
    }
}
