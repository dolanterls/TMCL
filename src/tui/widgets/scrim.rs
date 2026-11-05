//! Darkens the frame behind a dialog.
//!
//! Terminals have no alpha channel, so an overlay is separated from the page by
//! re-colouring every cell already drawn instead: RGB values lose most of their
//! brightness, named colours fall back to the palette's darkest slots. The
//! dialog itself is drawn afterwards at full contrast.

use ratatui::Frame;
use ratatui::style::Color;

/// Share of each channel removed from the underlying frame.
const DEPTH: f64 = 0.55;

/// Dims the whole frame. Call right before drawing the topmost overlay.
pub fn dim(frame: &mut Frame<'_>) {
    let area = frame.area();
    let buffer = frame.buffer_mut();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.fg = darken_foreground(cell.fg);
                cell.bg = darken_background(cell.bg);
            }
        }
    }
}

fn scale(channel: u8) -> u8 {
    (f64::from(channel) * (1.0 - DEPTH)).round() as u8
}

fn darken_foreground(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(scale(r), scale(g), scale(b)),
        // Terminal defaults and named colours cannot be scaled; pin them to
        // dark grey so text recedes without turning invisible.
        Color::Reset | Color::Indexed(_) => Color::Indexed(8),
        _ => Color::DarkGray,
    }
}

fn darken_background(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(scale(r), scale(g), scale(b)),
        // The terminal background is already the darkest surface available.
        Color::Reset => Color::Reset,
        _ => Color::Black,
    }
}
