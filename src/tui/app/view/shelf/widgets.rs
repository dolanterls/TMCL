//! Small drawing pieces that only the shelf preset uses.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};

use super::tone::Palette;
use crate::core::cover::Cover;
use crate::tui::shell::LayoutMode;

/// A deterministic stand-in for an instance without any cover file.
///
/// Blocky cells from a splitmix hash, with a darker centre so the picture
/// reads as a case with a label rather than as noise. It is obviously an
/// illustration, never a fake screenshot.
pub(in crate::tui::app) fn placeholder(seed: u64, side: u32, pal: &Palette) -> Cover {
    let side = side.clamp(4, 96);
    let mut rgba = vec![0; (side * side * 4) as usize];
    let surface = rgb(pal.surface);
    let faint = rgb(pal.faint);
    let bg = rgb(pal.bg);
    let centre_start = side / 3;
    let centre_end = side - side / 3;
    // The pattern's cell scales with the picture: four-pixel blocks read as
    // noise on a 34-pixel cover and as one flat field on a 20-pixel badge.
    // Bands are three pixels tall and therefore odd, so the two pixels a
    // half-block cell paints as foreground and background land in different
    // bands — an even band made every cell a single colour and the picture
    // printed as a flat field of `▀`.
    let cell = (side / 8).max(2);
    let band = 3u32;
    for y in 0..side {
        for x in 0..side {
            let in_label =
                (centre_start..centre_end).contains(&x) && (centre_start..centre_end).contains(&y);
            let colour = if in_label {
                bg
            } else {
                match hash(seed, x / cell, y / band) % 3 {
                    0 => surface,
                    1 => faint,
                    _ => surface,
                }
            };
            let index = ((y * side + x) * 4) as usize;
            rgba[index..index + 4].copy_from_slice(&[colour[0], colour[1], colour[2], 255]);
        }
    }
    Cover::from_rgba(side, side, rgba).expect("placeholder dimensions")
}

/// A bordered card used for the cover column. Returns its inner area.
pub(in crate::tui::app) fn panel(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    pal: &Palette,
) -> Rect {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(pal.faint).bg(pal.bg))
        .title(Line::from(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(pal.dim)
                .bg(pal.bg)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// One hairline across `area`, in the page's faint ink.
pub(in crate::tui::app) fn rule(frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = usize::from(area.width);
    let line = Line::from(Span::styled(
        "─".repeat(width),
        Style::default().fg(pal.faint).bg(pal.bg),
    ));
    frame.render_widget(
        ratatui::widgets::Paragraph::new(line).style(Style::default().bg(pal.bg)),
        Rect::new(area.x, area.y, area.width, 1),
    );
}

/// The selected row of a one-line list: a surface band with an accent bar.
///
/// A filled accent row would swallow the status colours this list depends on —
/// `ERROR` on `ACCENT` is about 1.6:1 — so the band marks the row without
/// taking the colour away from the compatibility column.
pub(in crate::tui::app) fn selection_band(frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(pal.surface)),
        area,
    );
    for row in 0..area.height {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Span::styled(
                "▌",
                Style::default().fg(pal.accent).bg(pal.surface),
            )),
            Rect::new(area.x, area.y.saturating_add(row), 1, 1),
        );
    }
}

/// One rectangle per label that fits inside `[area.x, right_limit)`.
///
/// When the whole row does not fit, the window starts at `current` and grows
/// leftwards: dropping the tail is fine for page tabs (`Tab` reaches them), but
/// on the category row it would hide exactly the category the list belongs to.
/// The returned index is the label's position in `labels`, so painting and hit
/// testing cannot disagree.
pub(in crate::tui::app) fn fit_labels(
    area: Rect,
    labels: &[String],
    current: usize,
    right_limit: u16,
) -> Vec<(usize, Rect)> {
    const GAP: u16 = 3;
    if labels.is_empty() {
        return Vec::new();
    }
    let widths = labels
        .iter()
        .map(|label| Line::from(label.as_str()).width() as u16)
        .collect::<Vec<_>>();
    let span = |start: usize, end: usize| {
        widths[start..end]
            .iter()
            .copied()
            .sum::<u16>()
            .saturating_add(GAP.saturating_mul((end - start).saturating_sub(1) as u16))
    };
    let available = right_limit.saturating_sub(area.x);
    let current = current.min(labels.len() - 1);

    let (mut start, mut end) = if span(0, labels.len()) <= available {
        (0, labels.len())
    } else {
        let mut start = current;
        let mut end = current + 1;
        while start > 0 && span(start - 1, end) <= available {
            start -= 1;
        }
        while end < labels.len() && span(start, end + 1) <= available {
            end += 1;
        }
        (start, end)
    };
    if start >= end {
        start = current;
        end = current + 1;
    }

    let mut x = area.x;
    let mut rects = Vec::with_capacity(end - start);
    for (offset, width) in widths[start..end].iter().enumerate() {
        let index = start + offset;
        let clipped = (*width).min(right_limit.saturating_sub(x));
        if clipped == 0 {
            break;
        }
        rects.push((index, Rect::new(x, area.y, clipped, area.height.min(1))));
        x = x.saturating_add(*width).saturating_add(GAP);
    }
    rects
}

/// Columns the cover occupies at this size. `0` means "do not draw one".
pub(in crate::tui::app) fn cover_side(mode: LayoutMode, available: u16) -> u32 {
    match mode {
        LayoutMode::Full if available >= 60 => 48,
        LayoutMode::Full => 34,
        // Compact panels are narrow and often short: on a 30-row terminal a
        // 34×17 cover crowds the six facts out, so it shrinks to 24×12 and the
        // facts keep their room.
        LayoutMode::Compact => 24,
        LayoutMode::Narrow | LayoutMode::TooSmall => 0,
    }
}

/// One rectangle per tab label that fits, left to right, three cells apart.
///
/// Drawing and hit-testing read the same list, so a tab cannot be painted in
/// one place and clicked in another. Labels that do not fit are dropped rather
/// than clipped; `Tab` still reaches every page.
pub(in crate::tui::app) fn tab_rects(area: Rect, labels: &[String]) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(labels.len());
    let mut x = area.x;
    for label in labels {
        let width = Line::from(label.as_str()).width() as u16;
        if width == 0 || x.saturating_add(width) > area.right() {
            break;
        }
        rects.push(Rect::new(x, area.y, width, area.height.min(1)));
        x = x.saturating_add(width).saturating_add(3);
    }
    rects
}

fn hash(seed: u64, x: u32, y: u32) -> u64 {
    let mut value = seed ^ (u64::from(x) << 32) ^ u64::from(y) ^ 0x9E37_79B9_7F4A_7C15;
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn rgb(color: ratatui::style::Color) -> [u8; 3] {
    match color {
        ratatui::style::Color::Rgb(red, green, blue) => [red, green, blue],
        _ => [0, 0, 0],
    }
}

#[cfg(test)]
mod tests {
    use super::{cover_side, hash, placeholder};
    use crate::tui::shell::LayoutMode;

    #[test]
    fn the_placeholder_is_deterministic() {
        let pal = crate::tui::app::view::shelf::tone::palette(
            crate::core::config::models::ShelfTheme::Lavender,
            crate::core::config::models::ThemeMode::Dark,
        );
        let first = placeholder(42, 32, pal);
        let second = placeholder(42, 32, pal);
        assert_eq!(first.rgba(), second.rgba());
        let different = placeholder(43, 32, pal);
        assert_ne!(first.rgba(), different.rgba());
    }

    #[test]
    fn the_placeholder_has_a_darker_label_centre() {
        let pal = crate::tui::app::view::shelf::tone::palette(
            crate::core::config::models::ShelfTheme::Lavender,
            crate::core::config::models::ThemeMode::Dark,
        );
        let cover = placeholder(7, 32, pal);
        assert_eq!(cover.pixel(16, 16), [0x12, 0x10, 0x1C, 255]);
    }

    #[test]
    fn cover_sizes_follow_the_layout_mode() {
        assert_eq!(cover_side(LayoutMode::Full, 80), 48);
        assert_eq!(cover_side(LayoutMode::Full, 40), 34);
        assert_eq!(cover_side(LayoutMode::Compact, 40), 24);
        assert_eq!(cover_side(LayoutMode::Narrow, 80), 0);
    }

    #[test]
    fn the_hash_is_stable_across_calls() {
        assert_eq!(hash(1, 2, 3), hash(1, 2, 3));
        assert_ne!(hash(1, 2, 3), hash(1, 2, 4));
    }

    #[test]
    fn tab_rects_follow_the_labels_and_drop_the_ones_that_do_not_fit() {
        let area = ratatui::layout::Rect::new(0, 2, 20, 1);
        let labels = vec!["Overview".to_owned(), "Instances".to_owned()];
        let rects = super::tab_rects(area, &labels);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0], ratatui::layout::Rect::new(0, 2, 8, 1));
        assert_eq!(rects[1], ratatui::layout::Rect::new(11, 2, 9, 1));

        let narrow = super::tab_rects(area, &["Overview".to_owned(), "Instances".to_owned()]);
        assert_eq!(narrow[1].right(), 20);
        let clipped = super::tab_rects(
            ratatui::layout::Rect::new(0, 2, 12, 1),
            &["Overview".to_owned(), "Instances".to_owned()],
        );
        assert_eq!(clipped.len(), 1, "the second label cannot fit");
    }
}
