//! The paint answers `跟随终端` needs from a preset: which colours are container
//! fills, and what a fill becomes once the sheet follows the terminal.
//!
//! A preset whose fills are derived blends (creeper: `mix(surface, page hue,
//! 0.18)`) cannot list them as constants, so each preset answers here in its
//! own `tone.rs` rather than through a `match` in `dispatch.rs`.

use ratatui::style::Color;

use crate::tui::palette;

pub(in crate::tui::app) trait Sheet {
    /// The tone that panels, rows and cards sit on.
    fn surface(&self) -> Color;

    /// Container paint, lowest first. `punch_through_terminal` hands every
    /// cell whose background equals one of these back to the terminal.
    fn fills(&self) -> [Option<Color>; 4];

    /// Whether the fills are withdrawn. The ink never changes: a terminal a
    /// launcher runs on is dark far more often than not.
    fn follows_terminal(&self) -> bool;

    /// The fill for a container: a list row, a selection band, a soft card.
    ///
    /// `Reset` on a following sheet. Rows are the bulk of a list page, so they
    /// are what must be withdrawn; a literal blend would leave the acrylic
    /// visible only between them, so the setting would appear to do nothing.
    /// Pills, bars, borders and key caps keep their paint because they carry
    /// information rather than containment.
    fn fill(&self, tint: Color, ratio: f64) -> Color {
        if self.follows_terminal() {
            Color::Reset
        } else {
            palette::mix(self.surface(), tint, ratio)
        }
    }
}
