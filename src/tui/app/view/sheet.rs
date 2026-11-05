//! What every preset has to say about its own paint.
//!
//! `跟随终端` needs two answers from a preset: which colours are container
//! fills, and what a container fill looks like once the sheet follows the
//! terminal. Both used to live in `dispatch.rs` as a `match` over the preset,
//! so a new preset had to remember to add a branch — and a preset whose rows
//! are literal blends rather than palette constants (creeper: `mix(surface,
//! page hue, 0.18)` per page) had them missed.
//!
//! Asking the palette instead keeps the knowledge next to the colours it
//! describes: a preset implements this trait in its own `tone.rs` and inherits
//! both behaviours.

use ratatui::style::Color;

use crate::tui::palette;

pub(in crate::tui::app) trait Sheet {
    /// The tone panels, rows and cards sit on.
    fn surface(&self) -> Color;

    /// Container paint, lowest first. `punch_through_terminal` hands every
    /// cell whose background equals one of these back to the terminal.
    fn fills(&self) -> [Option<Color>; 4];

    /// Whether the sheet follows the terminal, i.e. whether its fills are
    /// withdrawn. The ink never changes: a terminal a launcher runs on is dark
    /// far more often than not.
    fn follows_terminal(&self) -> bool;

    /// A container fill — a list row, a selection band, a soft card.
    ///
    /// Rows are the bulk of a list page, so they are what has to be withdrawn:
    /// a literal blend leaves the acrylic visible only in the gaps between
    /// rows, which reads as "跟随终端 did nothing". Pills, progress bars,
    /// borders and key caps do not come through here and keep their paint —
    /// they carry information rather than containment.
    fn fill(&self, tint: Color, ratio: f64) -> Color {
        if self.follows_terminal() {
            Color::Reset
        } else {
            palette::mix(self.surface(), tint, ratio)
        }
    }
}
