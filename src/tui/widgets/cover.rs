//! Half-block rendering for a decoded cover.
//!
//! A terminal cell is about twice as tall as it is wide, so `▀` gives two
//! square pixels per cell: foreground for the upper pixel, background for the
//! lower one. A 34×34 cover therefore occupies 34 columns × 17 rows.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::core::cover::Cover;

/// Paints `cover` into the largest centred square that fits `area`.
///
/// Every cell outside the square is filled with `background` first: a cover may
/// be smaller than its slot, but a stale glyph from the previous frame may not.
/// Returns the rectangle painted, so a caller can register a click target without
/// recomputing the geometry.
pub fn render(buffer: &mut Buffer, area: Rect, cover: &Cover, background: Color) -> Rect {
    if area.is_empty() {
        return Rect::default();
    }
    let background = rgb(background);
    fill(buffer, area, background);

    let pixels = area.width.min(area.height.saturating_mul(2));
    if pixels == 0 || cover.width() == 0 || cover.height() == 0 {
        return Rect::default();
    }
    let rows = (pixels / 2).max(1).min(area.height);
    let columns = rows.saturating_mul(2).min(area.width);
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(columns) / 2);
    let y = area.y.saturating_add(area.height.saturating_sub(rows) / 2);
    let scaled = cover
        .square()
        .resized(u32::from(columns), u32::from(rows) * 2);

    for row in 0..rows {
        for column in 0..columns {
            let top = blend(
                scaled.pixel(u32::from(column), u32::from(row) * 2),
                background,
            );
            let bottom = blend(
                scaled.pixel(u32::from(column), u32::from(row) * 2 + 1),
                background,
            );
            let Some(cell) = buffer.cell_mut((x + column, y + row)) else {
                continue;
            };
            cell.set_char('▀');
            cell.fg = Color::Rgb(top[0], top[1], top[2]);
            cell.bg = Color::Rgb(bottom[0], bottom[1], bottom[2]);
        }
    }
    Rect::new(x, y, columns, rows)
}

fn fill(buffer: &mut Buffer, area: Rect, background: [u8; 3]) {
    let color = Color::Rgb(background[0], background[1], background[2]);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_char(' ');
                cell.fg = color;
                cell.bg = color;
            }
        }
    }
}

fn rgb(color: Color) -> [u8; 3] {
    match color {
        Color::Rgb(red, green, blue) => [red, green, blue],
        _ => [0, 0, 0],
    }
}

fn blend(pixel: [u8; 4], background: [u8; 3]) -> [u8; 3] {
    let alpha = u16::from(pixel[3]);
    let mix = |foreground: u8, background: u8| -> u8 {
        ((u16::from(foreground) * alpha + u16::from(background) * (255 - alpha) + 127) / 255) as u8
    };
    [
        mix(pixel[0], background[0]),
        mix(pixel[1], background[1]),
        mix(pixel[2], background[2]),
    ]
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;

    use super::render;
    use crate::core::cover::Cover;

    fn cover(width: u32, height: u32, pixels: &[[u8; 4]]) -> Cover {
        let mut rgba = Vec::with_capacity(pixels.len() * 4);
        for pixel in pixels {
            rgba.extend_from_slice(pixel);
        }
        Cover::from_rgba(width, height, rgba).expect("cover")
    }

    #[test]
    fn a_two_by_two_cover_becomes_two_half_block_cells() {
        let cover = cover(
            2,
            2,
            &[
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 0, 255, 255],
                [255, 255, 255, 255],
            ],
        );
        let area = Rect::new(0, 0, 2, 1);
        let mut buffer = Buffer::empty(area);
        let painted = render(&mut buffer, area, &cover, Color::Rgb(0, 0, 0));
        assert_eq!(painted, area);
        assert_eq!(buffer[(0, 0)].symbol(), "▀");
        assert_eq!(buffer[(0, 0)].fg, Color::Rgb(255, 0, 0));
        assert_eq!(buffer[(0, 0)].bg, Color::Rgb(0, 0, 255));
        assert_eq!(buffer[(1, 0)].fg, Color::Rgb(0, 255, 0));
        assert_eq!(buffer[(1, 0)].bg, Color::Rgb(255, 255, 255));
    }

    #[test]
    fn a_single_pixel_still_gets_a_square_picture() {
        let cover = cover(1, 1, &[[10, 20, 30, 255]]);
        let area = Rect::new(0, 0, 6, 2);
        let mut buffer = Buffer::empty(area);
        let painted = render(&mut buffer, area, &cover, Color::Rgb(0, 0, 0));
        assert_eq!(painted, Rect::new(1, 0, 4, 2));
        assert_eq!(buffer[(0, 0)].bg, Color::Rgb(0, 0, 0));
        assert_eq!(buffer[(1, 0)].fg, Color::Rgb(10, 20, 30));
    }

    #[test]
    fn transparent_pixels_blend_into_the_page_background() {
        let cover = cover(
            2,
            2,
            &[[255, 0, 0, 255], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]],
        );
        let area = Rect::new(0, 0, 2, 1);
        let mut buffer = Buffer::empty(area);
        render(&mut buffer, area, &cover, Color::Rgb(0x12, 0x10, 0x1C));
        assert_eq!(buffer[(0, 0)].fg, Color::Rgb(255, 0, 0));
        assert_eq!(buffer[(0, 0)].bg, Color::Rgb(0x12, 0x10, 0x1C));
        assert_eq!(buffer[(1, 0)].fg, Color::Rgb(0x12, 0x10, 0x1C));
    }

    #[test]
    fn an_empty_area_is_a_no_op() {
        let cover = cover(1, 1, &[[1, 2, 3, 255]]);
        let area = Rect::new(0, 0, 0, 0);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        assert_eq!(
            render(&mut buffer, area, &cover, Color::Rgb(0, 0, 0)),
            Rect::default()
        );
    }
}
