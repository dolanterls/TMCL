//! Card-and-pill primitives: framed cards, status pills, key caps, progress
//! bars and the decorated row list.
//!
//! Every preset draws with these pieces, so this lives in the shared `widgets`
//! module instead of inside one preset's folder.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::tui::palette;
use crate::tui::widgets::status::StatusKind;

/// Attach an `n/m` counter to the bottom-right border of a card. Painting it on
/// the border keeps the content rows intact, so row-to-click mapping is
/// unaffected.
pub fn with_counter(block: Block<'static>, position: usize, total: usize) -> Block<'static> {
    if total == 0 {
        return block;
    }
    block.title_bottom(
        Line::from(Span::styled(
            format!(" {position}/{total} "),
            Style::default()
                .fg(palette::TEXT_FAINT)
                .bg(palette::SURFACE),
        ))
        .alignment(Alignment::Right),
    )
}

/// Rounded card with an accented `─ TITLE ` heading.
///
/// The focus ring breathes between `border` and `accent`, which gives the
/// layout a heartbeat without moving any glyphs.
pub fn card(title: &str, accent: Color, focused: bool, t: f64, bg: Color) -> Block<'static> {
    let title = title.to_uppercase();
    let border = if focused {
        palette::mix(
            accent,
            palette::BORDER_HI,
            0.25 + palette::pulse(t, 2.2) * 0.35,
        )
    } else {
        palette::BORDER
    };
    let title_color = if focused {
        palette::mix(accent, palette::TEXT, 0.2)
    } else {
        palette::TEXT_DIM
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border).bg(bg))
        .title(Line::from(vec![
            Span::styled(
                format!("─ {title} "),
                Style::default()
                    .fg(title_color)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "─",
                Style::default()
                    .fg(palette::dim(palette::BORDER, 0.25))
                    .bg(bg),
            ),
        ]))
        .style(Style::default().fg(palette::TEXT).bg(bg))
}

/// Card without a heading, used for purely decorative frames.
pub fn frame_card(accent: Color, focused: bool, t: f64, bg: Color) -> Block<'static> {
    let border = if focused {
        palette::mix(
            accent,
            palette::BORDER_HI,
            0.25 + palette::pulse(t, 2.2) * 0.35,
        )
    } else {
        palette::BORDER
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border).bg(bg))
        .style(Style::default().fg(palette::TEXT).bg(bg))
}

/// Card with a steady accent border: the page colour at rest, without the
/// focus pulse. Used by the side detail pane so it reads as part of the same
/// page as the focused list instead of falling back to the neutral border.
pub fn accent_card(title: &str, accent: Color, bg: Color) -> Block<'static> {
    let title = title.to_uppercase();
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(
            Style::default()
                .fg(palette::mix(accent, palette::BORDER_HI, 0.35))
                .bg(bg),
        )
        .title(Line::from(vec![
            Span::styled(
                format!("─ {title} "),
                Style::default()
                    .fg(palette::mix(accent, palette::TEXT, 0.25))
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "─",
                Style::default()
                    .fg(palette::dim(palette::BORDER, 0.25))
                    .bg(bg),
            ),
        ]))
        .style(Style::default().fg(palette::TEXT).bg(bg))
}

/// Coloured ` label ` pill; the background is a light tint of the same hue so
/// the chip reads as a solid badge instead of coloured text.
pub fn pill(label: impl Into<String>, color: Color, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {} ", label.into()),
        Style::default()
            .fg(palette::mix(color, palette::TEXT, 0.12))
            .bg(palette::mix(bg, color, 0.22))
            .add_modifier(Modifier::BOLD),
    )
}

/// Muted pill on the surface colour, for secondary metadata.
pub fn soft_pill(label: impl Into<String>, color: Color, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {} ", label.into()),
        Style::default()
            .fg(palette::mix(color, palette::TEXT, 0.15))
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )
}

/// Terminal width of one pill chip: ` label ` including both pads.
pub fn pill_width(label: &str) -> usize {
    palette::width(label) + 2
}

/// A row of pills separated by one blank column.
///
/// Drawing and hit testing both go through [`pill_width`], so a click always
/// lands on the chip painted under the cursor. A custom row measured separately
/// from `TabBar`'s geometry drifts a little further with every label.
pub fn pill_row(
    labels: &[String],
    selected: Option<usize>,
    accent: Color,
    bg: Color,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (index, label) in labels.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" ", Style::default().bg(bg)));
        }
        if selected == Some(index) {
            spans.push(pill(
                label.clone(),
                palette::mix(accent, palette::TEXT, 0.1),
                bg,
            ));
        } else {
            spans.push(Span::styled(
                format!(" {label} "),
                Style::default().fg(palette::TEXT_FAINT).bg(bg),
            ));
        }
    }
    spans
}

/// Rectangle of every pill drawn by [`pill_row`], in draw order.
///
/// Registration and drawing both go through [`pill_width`], so a click always
/// lands on the chip painted under the cursor.
pub fn pill_rects(labels: &[String], area: Rect) -> Vec<Rect> {
    let mut cursor = usize::from(area.x);
    labels
        .iter()
        .map(|label| {
            let width = pill_width(label);
            let rect = Rect::new(
                u16::try_from(cursor).unwrap_or(u16::MAX),
                area.y,
                u16::try_from(width).unwrap_or(u16::MAX),
                1,
            );
            cursor = cursor.saturating_add(width + 1);
            rect
        })
        .collect()
}

/// Key cap used in the footer and in modal buttons.
pub fn key_cap(key: &str) -> Span<'static> {
    Span::styled(
        format!(" {key} "),
        Style::default()
            .fg(palette::BG)
            .bg(palette::mix(palette::TEXT_DIM, palette::TEXT, 0.18))
            .add_modifier(Modifier::BOLD),
    )
}

/// Inline hint: `[Enter] 启动`.
pub fn hint(key: &str, label: &str, bg: Color) -> Vec<Span<'static>> {
    vec![
        key_cap(key),
        Span::styled(
            format!(" {label} "),
            Style::default().fg(palette::TEXT_FAINT).bg(bg),
        ),
    ]
}

pub fn status_color(kind: StatusKind) -> Color {
    match kind {
        StatusKind::Ready | StatusKind::Completed => palette::GREEN,
        StatusKind::Current | StatusKind::Running => palette::LIME,
        StatusKind::Queued => palette::GOLD,
        StatusKind::Warning => palette::GOLD,
        StatusKind::Failed => palette::RED,
        StatusKind::Missing => palette::TEXT_FAINT,
        StatusKind::Info => palette::CYAN,
    }
}

pub fn status_glyph(kind: StatusKind) -> &'static str {
    match kind {
        StatusKind::Ready => "●",
        StatusKind::Current => "◆",
        StatusKind::Running => "▶",
        StatusKind::Completed => "✓",
        StatusKind::Queued => "◷",
        StatusKind::Warning => "!",
        StatusKind::Failed => "✕",
        StatusKind::Missing => "—",
        StatusKind::Info => "·",
    }
}

/// Status pill with glyph, label and tinted background.
pub fn status_pill(kind: StatusKind, label: impl Into<String>, bg: Color) -> Span<'static> {
    let color = status_color(kind);
    Span::styled(
        format!(" {} {} ", status_glyph(kind), label.into()),
        Style::default()
            .fg(palette::mix(color, palette::TEXT, 0.15))
            .bg(palette::mix(bg, color, 0.2))
            .add_modifier(Modifier::BOLD),
    )
}

/// Block-character progress bar. `None` animates a travelling highlight for
/// work whose total size is unknown.
pub fn progress_spans(
    ratio: Option<f64>,
    width: usize,
    from: Color,
    to: Color,
    t: f64,
    bg: Color,
) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }
    const PARTIALS: [&str; 8] = ["▏", "▎", "▍", "▌", "▋", "▊", "▉", ""];
    let mut spans = Vec::with_capacity(width);
    match ratio {
        Some(ratio) => {
            let filled = ratio.clamp(0.0, 1.0) * width as f64;
            let full = filled.floor() as usize;
            let fraction = filled - full as f64;
            for index in 0..width {
                let shade = index as f64 / width.max(1) as f64;
                let color = palette::mix(from, to, shade);
                if index < full {
                    spans.push(Span::styled("█", Style::default().fg(color).bg(bg)));
                } else if index == full && fraction > 0.02 {
                    let partial = PARTIALS[((fraction * 8.0).floor() as usize) % 8];
                    spans.push(Span::styled(
                        partial,
                        Style::default().fg(palette::brighten(color, 0.25)).bg(bg),
                    ));
                } else {
                    spans.push(Span::styled(
                        "░",
                        Style::default()
                            .fg(palette::dim(palette::BORDER, 0.35))
                            .bg(bg),
                    ));
                }
            }
        }
        None => {
            let head = ((t * 0.38) % 1.3 - 0.15) * width as f64;
            for index in 0..width {
                let distance = (index as f64 - head).abs();
                let color = if distance < 3.0 {
                    1.0 - distance / 3.0
                } else {
                    0.0
                };
                spans.push(Span::styled(
                    if distance < 3.0 { "█" } else { "░" },
                    Style::default()
                        .fg(palette::mix(palette::BORDER, to, 0.35 + color * 0.65))
                        .bg(bg),
                ));
            }
        }
    }
    spans
}

/// A stat tile: big value, small caption. Rendered as its own rounded box.
pub fn stat_tile(
    frame: &mut Frame<'_>,
    area: Rect,
    value: &str,
    caption: &str,
    color: Color,
    t: f64,
) {
    if area.width < 6 || area.height < 3 {
        return;
    }
    let block = frame_card(color, true, t, palette::SURFACE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let mut lines = if inner.height >= 2 {
        vec![
            Line::from(Span::styled(
                palette::truncate(value, usize::from(inner.width)),
                Style::default()
                    .fg(palette::mix(color, palette::TEXT, 0.25))
                    .bg(palette::SURFACE)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                palette::truncate(caption, usize::from(inner.width)),
                Style::default()
                    .fg(palette::TEXT_FAINT)
                    .bg(palette::SURFACE),
            )),
        ]
    } else {
        vec![Line::from(vec![
            Span::styled(
                palette::truncate(value, usize::from(inner.width)),
                Style::default()
                    .fg(palette::mix(color, palette::TEXT, 0.25))
                    .bg(palette::SURFACE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " {}",
                    palette::truncate(caption, usize::from(inner.width.saturating_sub(3)))
                ),
                Style::default()
                    .fg(palette::TEXT_FAINT)
                    .bg(palette::SURFACE),
            ),
        ])]
    };
    lines.truncate(usize::from(inner.height));
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(Style::default().bg(palette::SURFACE)),
        inner,
    );
}

/// Row payload for [`render_rows`]: text cells plus per-cell highlight colours.
#[derive(Debug, Clone, Default)]
pub struct RowData {
    pub cells: Vec<String>,
    highlights: Vec<(usize, Color)>,
}

impl RowData {
    pub fn new<I, S>(cells: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            cells: cells.into_iter().map(Into::into).collect(),
            highlights: Vec::new(),
        }
    }

    /// Colour one cell (0-based) with `color`.
    pub fn mark(mut self, index: usize, color: Color) -> Self {
        self.highlights.push((index, color));
        self
    }

    fn color_of(&self, index: usize) -> Option<Color> {
        self.highlights
            .iter()
            .rev()
            .find(|(cell, _)| *cell == index)
            .map(|(_, color)| *color)
    }
}

/// Split `total` cells across `constraints`, leaving `spacing` between columns.
fn column_widths(constraints: &[Constraint], total: u16, spacing: u16) -> Vec<u16> {
    let count = constraints.len();
    if count == 0 {
        return Vec::new();
    }
    let spacing_total = spacing.saturating_mul(count.saturating_sub(1) as u16);
    let available = total.saturating_sub(spacing_total);
    let mut widths = vec![0u16; count];
    let mut fixed = 0u16;
    for (index, constraint) in constraints.iter().enumerate() {
        if let Constraint::Length(value) = constraint {
            widths[index] = *value;
            fixed = fixed.saturating_add(*value);
        }
    }
    let remaining = available.saturating_sub(fixed);
    let flexible = constraints
        .iter()
        .filter(|constraint| !matches!(constraint, Constraint::Length(_)))
        .count();
    if flexible > 0 {
        let mut left = remaining;
        let mut flexible_left = flexible;
        for (index, constraint) in constraints.iter().enumerate() {
            if matches!(constraint, Constraint::Length(_)) {
                continue;
            }
            let share = match constraint {
                Constraint::Percentage(percent) => {
                    (u32::from(remaining) * u32::from(*percent) / 100) as u16
                }
                _ => u16::try_from(u32::from(left) / flexible_left.max(1) as u32).unwrap_or(left),
            };
            let share = share.min(left);
            widths[index] = share;
            left = left.saturating_sub(share);
            flexible_left = flexible_left.saturating_sub(1);
        }
    }
    widths
}

/// Decorated row list inside a card: dim bold header, hover bar on the selected
/// row, optional per-cell colours and an `n/m` counter on the bottom border.
///
/// Geometry matches `RowLayout::table`: one border row plus one header row, so
/// mouse hit-testing keeps working with the shared layout helper. The counter is
/// painted onto the bottom border instead of a content row, so it cannot shift
/// the rows that clicks are mapped against.
#[allow(clippy::too_many_arguments)]
pub fn render_rows(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    accent: Color,
    headers: &[String],
    rows: &[RowData],
    selected: Option<usize>,
    widths: &[Constraint],
    t: f64,
    offset: usize,
    counter: Option<(usize, usize)>,
) {
    if area.width < 8 || area.height < 3 {
        return;
    }
    let mut block = card(title, accent, true, t, palette::SURFACE);
    if let Some((position, total)) = counter {
        block = with_counter(block, position, total);
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let indent = 2u16;
    let content_width = inner.width.saturating_sub(indent);
    let columns = column_widths(widths, content_width, 1);

    let mut header_spans = vec![Span::styled(" ".repeat(indent as usize), Style::default())];
    for (index, header) in headers.iter().enumerate() {
        let width = columns.get(index).copied().unwrap_or(0) as usize;
        if width == 0 {
            continue;
        }
        header_spans.push(Span::styled(
            palette::fit(header, width),
            Style::default()
                .fg(palette::TEXT_FAINT)
                .bg(palette::SURFACE)
                .add_modifier(Modifier::BOLD),
        ));
        if index + 1 < headers.len() {
            header_spans.push(Span::styled(" ", Style::default().bg(palette::SURFACE)));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(header_spans)).style(Style::default().bg(palette::SURFACE)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let body = Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(1),
    );
    for (visual, row) in rows
        .iter()
        .skip(offset)
        .take(usize::from(body.height))
        .enumerate()
    {
        let y = body.y + visual as u16;
        let index = offset + visual;
        let active = selected == Some(index);
        // A row is the bulk of a list page: leaving the band painted would
        // show the terminal's own background only in the gaps between rows.
        let bg = if active {
            palette::mix(palette::SURFACE, accent, 0.2)
        } else if visual % 2 == 1 {
            palette::mix(palette::SURFACE, palette::SURFACE_ALT, 0.55)
        } else {
            palette::SURFACE
        };
        frame.render_widget(
            Block::default().style(Style::default().bg(bg)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        let mut spans = vec![Span::styled(
            if active { "▌ " } else { "  " }.to_owned(),
            Style::default()
                .fg(palette::mix(accent, palette::TEXT, 0.15))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )];
        for (column, constraint_width) in columns.iter().enumerate() {
            let width = usize::from(*constraint_width);
            if width == 0 {
                continue;
            }
            let text = row.cells.get(column).cloned().unwrap_or_default();
            let style = match row.color_of(column) {
                Some(color) => Style::default()
                    .fg(color)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
                None if active => Style::default().fg(palette::TEXT).bg(bg),
                None => Style::default()
                    .fg(palette::mix(palette::TEXT, palette::TEXT_DIM, 0.35))
                    .bg(bg),
            };
            spans.push(Span::styled(palette::fit(&text, width), style));
            if column + 1 < columns.len() {
                spans.push(Span::styled(" ", Style::default().bg(bg)));
            }
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
            Rect::new(inner.x, y, inner.width, 1),
        );
    }
}

/// Centred empty / error state inside a card.
pub fn empty_card(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    message: &str,
    accent: Color,
    t: f64,
) {
    empty_card_with_hint(frame, area, title, message, "", accent, t);
}

/// Empty card with a second, dimmer line for the action that fills it.
pub fn empty_card_with_hint(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    message: &str,
    hint: &str,
    accent: Color,
    t: f64,
) {
    if area.width < 8 || area.height < 3 {
        return;
    }
    let block = card(title, accent, false, t, palette::SURFACE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let mut lines = Vec::new();
    if inner.height >= 3 {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![Span::styled(
        "◆",
        Style::default()
            .fg(palette::mix(
                accent,
                palette::TEXT,
                palette::pulse(t, 1.5) * 0.25,
            ))
            .bg(palette::SURFACE),
    )]));
    lines.push(Line::from(Span::styled(
        palette::truncate(message, usize::from(inner.width)),
        Style::default().fg(palette::TEXT_DIM).bg(palette::SURFACE),
    )));
    if !hint.is_empty() {
        lines.push(Line::from(Span::styled(
            palette::truncate(hint, usize::from(inner.width)),
            Style::default()
                .fg(palette::TEXT_FAINT)
                .bg(palette::SURFACE),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(Style::default().bg(palette::SURFACE)),
        inner,
    );
}

/// Horizontal gradient rule with a travelling highlight.
pub fn rule_spans(width: u16, t: f64) -> Vec<Span<'static>> {
    let total = usize::from(width);
    let mut spans = Vec::with_capacity(total);
    for index in 0..total {
        let position = index as f64 / total.max(1) as f64;
        let base = if position < 0.34 {
            palette::mix(palette::GREEN, palette::CYAN, position / 0.34)
        } else if position < 0.67 {
            palette::mix(palette::CYAN, palette::PURPLE, (position - 0.34) / 0.33)
        } else {
            palette::mix(palette::PURPLE, palette::GOLD, (position - 0.67) / 0.33)
        };
        let shimmer = ((position * 9.0 - t * 1.4).sin() * 0.5 + 0.5).powf(9.0);
        spans.push(Span::styled(
            "─",
            Style::default()
                .fg(palette::mix(palette::BG, base, 0.28 + shimmer * 0.5))
                .bg(palette::BG),
        ));
    }
    spans
}

/// Uppercase section label with a trailing dim rule, e.g. `ACTIVITY ────────`.
pub fn section_label(text: &str, width: usize, accent: Color, bg: Color) -> Line<'static> {
    let label = format!(" {} ", text.to_uppercase());
    let used = palette::width(&label) + 2;
    let rule = "─".repeat(width.saturating_sub(used));
    Line::from(vec![
        Span::styled(
            "▎",
            Style::default()
                .fg(accent)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            label,
            Style::default()
                .fg(palette::mix(accent, palette::TEXT, 0.2))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            rule,
            Style::default()
                .fg(palette::dim(palette::BORDER, 0.35))
                .bg(bg),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn column_widths_split_lengths_and_flexible_columns() {
        let widths = column_widths(
            &[
                Constraint::Length(8),
                Constraint::Min(2),
                Constraint::Percentage(50),
            ],
            40,
            1,
        );
        assert_eq!(widths[0], 8);
        assert!(widths[1] > 0 && widths[2] > 0);
        assert!(widths.iter().sum::<u16>() <= 40);
    }

    #[test]
    fn card_titles_render_inside_the_border() {
        let backend = TestBackend::new(30, 5);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    card("INSTANCE", palette::GREEN, true, 0.0, palette::SURFACE),
                    frame.area(),
                );
            })
            .expect("draw card");
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("INSTANCE"), "{text}");
    }

    #[test]
    fn rows_render_values_and_highlight_the_selection() {
        let backend = TestBackend::new(40, 6);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let rows = vec![
            RowData::new(["alpha", "1"]).mark(1, palette::GREEN),
            RowData::new(["beta", "2"]),
        ];
        terminal
            .draw(|frame| {
                render_rows(
                    frame,
                    frame.area(),
                    "ITEMS",
                    palette::GREEN,
                    &["NAME".into(), "N".into()],
                    &rows,
                    Some(0),
                    &[Constraint::Min(6), Constraint::Length(4)],
                    0.0,
                    0,
                    Some((1, 2)),
                );
            })
            .expect("draw rows");
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("alpha"), "{text}");
        assert!(text.contains("beta"), "{text}");
        let painted = (0..40)
            .any(|x| buffer[(x, 2)].bg == palette::mix(palette::SURFACE, palette::GREEN, 0.2));
        assert!(painted, "selected row should be tinted");
        assert!(
            text.contains("1/2"),
            "counter missing from the border: {text}"
        );
    }

    #[test]
    fn progress_bars_fill_and_leave_a_track() {
        let full = progress_spans(
            Some(1.0),
            10,
            palette::GREEN,
            palette::CYAN,
            0.0,
            palette::SURFACE,
        );
        let half = progress_spans(
            Some(0.5),
            10,
            palette::GREEN,
            palette::CYAN,
            0.0,
            palette::SURFACE,
        );
        assert_eq!(full.len(), 10);
        assert_eq!(half.len(), 10);
        assert_eq!(full.iter().filter(|span| span.content == "█").count(), 10);
        assert!(half.iter().any(|span| span.content == "░"));
    }
}
