//! Pushes the frame behind a dialog into the background.
//!
//! Terminals have no alpha channel, so an overlay is separated from the page by
//! re-colouring every cell already drawn instead. Which direction that goes
//! depends on the page: a dark page recedes when it loses brightness, a light
//! one only turns muddy — scaling cream by 0.55 gives mid grey, and the near
//! black ink on it drops to 2.6:1. So the rule is "less contrast against the
//! page", not "darker": dark pages are darkened, light pages keep their paper
//! and lose only their ink. The dialog itself is drawn afterwards, at full
//! contrast.

use ratatui::Frame;
use ratatui::style::Color;

use crate::tui::palette::relative_luminance;

/// Share of each channel removed from the underlying frame.
const DEPTH: f64 = 0.55;

/// A background this bright is paper; anything else is a dark surface.
const PAPER: f64 = 0.5;

/// Pushes the whole frame back. Call right before drawing the topmost overlay.
pub fn dim(frame: &mut Frame<'_>) {
    let area = frame.area();
    let buffer = frame.buffer_mut();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                if is_paper(cell.bg) {
                    // The page keeps its colour and the text fades into it, so
                    // the window does not flash white when a dialog opens.
                    cell.fg = fade_into(cell.fg, cell.bg);
                } else {
                    cell.fg = darken_foreground(cell.fg);
                    cell.bg = darken_background(cell.bg);
                }
            }
        }
    }
}

fn is_paper(color: Color) -> bool {
    matches!(color, Color::Rgb(..)) && relative_luminance(color) > PAPER
}

fn scale(channel: u8) -> u8 {
    (f64::from(channel) * (1.0 - DEPTH)).round() as u8
}

/// Moves `ink` most of the way to `paper`, keeping the same hue.
fn fade_into(ink: Color, paper: Color) -> Color {
    let (Color::Rgb(r, g, b), Color::Rgb(pr, pg, pb)) = (ink, paper) else {
        // Nothing to blend against, and the paper underneath stays light, so
        // the terminal's own ink is still the reader's best option.
        return ink;
    };
    let mix = |from: u8, to: u8| -> u8 {
        (f64::from(from) * (1.0 - DEPTH) + f64::from(to) * DEPTH).round() as u8
    };
    Color::Rgb(mix(r, pr), mix(g, pg), mix(b, pb))
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
