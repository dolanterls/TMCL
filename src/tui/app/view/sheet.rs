//! The paint answers `跟随终端` needs from a preset: which colours are container
//! fills.
//!
//! A preset whose fills are derived blends cannot list them as constants, so
//! each preset answers here in its own `tone.rs` rather than through a `match`
//! in `dispatch.rs`.

use ratatui::style::Color;

pub(in crate::tui::app) trait Sheet {
    /// Container paint, lowest first. `punch_through_terminal` hands every
    /// cell whose background equals one of these back to the terminal.
    fn fills(&self) -> [Option<Color>; 4];
}
