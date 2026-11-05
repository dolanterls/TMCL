//! Sidebar pieces: block logo, vertical navigation and its hit-test geometry.
//!
//! The navigation is the only place where the preset leaves the shared
//! `ChromeRegions` contract, so the drawn rows and the clickable rows come from
//! the same [`NavLayout`].

use crate::tui::app::view::sheet::Sheet;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::tone::Palette;
use crate::tui::palette;

/// A creeper face drawn with half blocks: 8 cells wide, 8 rows tall.
const FACE: [&str; 8] = [
    "########", "########", "##.##.##", "##.##.##", "########", "###..###", "##....##", "########",
];

/// Rows reserved for the logo block plus the wordmark beneath it.
const LOGO_ROWS: u16 = 6;
/// Rows reserved for the runtime strip at the bottom of the sidebar.
const INFO_ROWS: u16 = 3;

/// Where the navigation lives inside a sidebar card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavLayout {
    pub inner: Rect,
    pub compact: bool,
    pub start_y: u16,
    pub end_y: u16,
}

/// Derive the navigation box from the sidebar card rectangle.
pub fn nav_layout(card_area: Rect) -> NavLayout {
    let inner = Rect {
        x: card_area.x.saturating_add(1),
        y: card_area.y.saturating_add(1),
        width: card_area.width.saturating_sub(2),
        height: card_area.height.saturating_sub(2),
    };
    let compact = inner.width < 12;
    let show_logo = !compact && inner.height >= 20;
    let start_y = if show_logo {
        inner.y.saturating_add(LOGO_ROWS)
    } else {
        inner.y
    };
    let info_rows = if inner.height >= 14 { INFO_ROWS } else { 0 };
    let end_y = inner.bottom().saturating_sub(info_rows).max(start_y);
    NavLayout {
        inner,
        compact,
        start_y,
        end_y,
    }
}

/// Rows between two navigation entries. The sidebar is tall enough for a
/// blank line on normal terminals; short ones keep the compact single-line
/// rhythm so all seven pages still fit.
fn nav_stride(layout: &NavLayout, count: usize) -> u16 {
    let available = layout.end_y.saturating_sub(layout.start_y);
    if available >= (count as u16).saturating_mul(2) {
        2
    } else {
        1
    }
}

/// Clickable rectangle of every navigation row.
pub fn nav_rects(layout: &NavLayout, count: usize) -> Vec<Rect> {
    let stride = nav_stride(layout, count);
    let available = layout.end_y.saturating_sub(layout.start_y);
    (0..count)
        .filter(|index| (*index as u16).saturating_mul(stride) < available)
        .map(|index| {
            Rect::new(
                layout.inner.x,
                layout
                    .start_y
                    .saturating_add((index as u16).saturating_mul(stride)),
                layout.inner.width,
                1,
            )
        })
        .collect()
}

/// Four lines of `▀▄█` that read as a blocky face, centred in `width` cells.
pub fn logo_lines(pal: &Palette, t: f64, width: u16) -> Vec<Line<'static>> {
    let glow = 0.55 + palette::pulse(t, 1.25) * 0.45;
    let pad = usize::from(width.saturating_sub(8)) / 2;
    let mut lines = Vec::with_capacity(FACE.len() / 2);
    for pair in FACE.chunks(2) {
        let top = pair[0].as_bytes();
        let bottom = pair[1].as_bytes();
        let mut spans = Vec::with_capacity(9 + pad);
        spans.push(Span::styled(
            " ".repeat(pad),
            Style::default().bg(pal.surface),
        ));
        for column in 0..8usize {
            let top_lit = top.get(column) == Some(&b'#');
            let bottom_lit = bottom.get(column) == Some(&b'#');
            let shade = 0.42 + column as f64 / 16.0;
            let color = palette::mix(pal.green_dark, pal.lime, (shade * glow).min(1.0));
            let (glyph, fg) = match (top_lit, bottom_lit) {
                (true, true) => ("█", color),
                (true, false) => ("▀", color),
                (false, true) => ("▄", color),
                (false, false) => (" ", pal.surface),
            };
            spans.push(Span::styled(glyph, Style::default().fg(fg).bg(pal.surface)));
        }
        lines.push(Line::from(spans).alignment(Alignment::Left));
    }
    lines
}

/// One navigation row: selection bar, icon, label, and a background pad that
/// carries the selection tint to the right edge of the sidebar.
pub fn nav_line(
    pal: &Palette,
    icon: &str,
    label: &str,
    active: bool,
    width: u16,
    color: Color,
    t: f64,
) -> Line<'static> {
    let bg = if active {
        pal.fill(color, 0.18)
    } else {
        pal.surface
    };
    let bar = if active {
        palette::mix(color, pal.text, palette::pulse(t, 2.0) * 0.3)
    } else {
        pal.surface
    };
    let mut spans = vec![
        Span::styled(
            if active { "▌" } else { " " },
            Style::default().fg(bar).bg(bg),
        ),
        Span::styled(
            format!(" {icon} "),
            Style::default()
                .fg(if active { color } else { pal.text_faint })
                .bg(bg)
                .add_modifier(if active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::styled(
            label.to_owned(),
            Style::default()
                .fg(if active { pal.text } else { pal.text_dim })
                .bg(bg)
                .add_modifier(if active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
    ];
    // Without this pad only the text itself is tinted, leaving the right half
    // of the selected row in the card colour.
    let used = 1 + 2 + palette::width(icon) + palette::width(label);
    if usize::from(width) > used {
        spans.push(Span::styled(
            " ".repeat(usize::from(width) - used),
            Style::default().bg(bg),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::view::creeper::tone;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn dark() -> &'static Palette {
        tone::palette(crate::core::config::models::ThemeMode::Dark)
    }

    #[test]
    fn logo_paints_block_glyphs() {
        let pal = dark();
        let backend = TestBackend::new(24, 6);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(logo_lines(pal, 0.0, 24)),
                    frame.area(),
                );
            })
            .expect("draw logo");
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains('█'), "{text}");
    }

    #[test]
    fn nav_layout_skips_the_logo_and_reserves_the_info_strip() {
        let layout = nav_layout(Rect::new(1, 2, 22, 34));
        assert!(!layout.compact);
        assert_eq!(layout.start_y, 2 + 1 + LOGO_ROWS);
        assert_eq!(layout.end_y, 2 + 34 - 1 - INFO_ROWS);
        let rects = nav_rects(&layout, 7);
        assert_eq!(rects.len(), 7);
        assert_eq!(rects[0].y, layout.start_y);
        assert!(rects[6].y < layout.end_y);
    }

    #[test]
    fn nav_rects_are_advertised_rows_only() {
        let layout = nav_layout(Rect::new(1, 2, 22, 34));
        let rects = nav_rects(&layout, 7);
        assert_eq!(rects.len(), 7);
        for pair in rects.windows(2) {
            // Rows between two entries are breathing room, not a second hot box.
            assert!(pair[0].bottom() < pair[1].y);
        }
        assert!(rects[0].y >= layout.start_y - 1);
        assert!(rects[6].bottom() <= layout.end_y);
    }

    #[test]
    fn nav_rows_get_a_blank_line_when_the_sidebar_is_tall_enough() {
        let tall = nav_layout(Rect::new(1, 2, 22, 34));
        let rects = nav_rects(&tall, 7);
        assert_eq!(rects[1].y - rects[0].y, 2);

        // A 60x18 terminal leaves too little room: keep every page reachable.
        let short = nav_layout(Rect::new(1, 2, 22, 12));
        let rects = nav_rects(&short, 7);
        assert_eq!(rects[1].y - rects[0].y, 1);
    }

    #[test]
    fn active_nav_row_has_a_selection_bar() {
        let pal = dark();
        let line = nav_line(pal, "◈", "Overview", true, 20, pal.lime, 0.0);
        assert!(line.to_string().starts_with('▌'));
        assert_eq!(line.width(), 20, "selection tint must reach the right edge");
        let inactive = nav_line(pal, "▤", "Instances", false, 20, pal.lime, 0.0);
        assert!(inactive.to_string().starts_with(' '));
        assert_eq!(inactive.width(), 20);
    }
    #[test]
    fn selected_row_tint_reaches_the_last_column() {
        let pal = dark();
        let backend = TestBackend::new(20, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(nav_line(
                        pal, "◫", "Settings", true, 20, pal.lime, 0.0,
                    )),
                    frame.area(),
                );
            })
            .expect("draw nav row");
        let buffer = terminal.backend().buffer();
        let expected = pal.fill(pal.lime, 0.18);
        assert_eq!(
            buffer[(19, 0)].bg,
            expected,
            "selection tint stops before the sidebar edge"
        );
    }
}
