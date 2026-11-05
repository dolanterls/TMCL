//! `classic` preset: the framed shell, tables and lists.
//!
//! Only this preset's pages live here. Layout geometry and the per-page
//! dispatch sit in `app::view::dispatch`; anything two presets share lives one
//! level up in `app::view` or in `crate::tui`.

use super::*;

mod accounts;
mod detail;
mod downloads;
mod frame;
mod instances;
mod logs;
mod overview;
mod settings;
mod tasks;
