//! Preset-agnostic state and behaviour.
//!
//! Nothing here draws a frame. Page actions live here too, because the key map
//! is the same for every preset, so "what a key does" cannot belong to one look.
//! Modules that draw live in [`crate::tui::app::view`].

use super::*;

pub(crate) mod apply;
pub(crate) mod commands;
pub(crate) mod covers;
pub(crate) mod data;
pub(crate) mod data_format;
pub(crate) mod diagnosis;
pub(crate) mod dispatch;
pub(crate) mod downloads;
pub(crate) mod downloads_categories;
pub(crate) mod downloads_resources;
pub(crate) mod forms;
pub(crate) mod forms_submit;
pub(crate) mod instances;
pub(crate) mod launch;
pub(crate) mod mouse;
pub(crate) mod nav;
pub(crate) mod settings;
pub(crate) mod settings_forms;
pub(crate) mod settings_rows;
pub(crate) mod status;
pub(crate) mod tasks;
pub(crate) mod toast;
pub(crate) mod update;
pub(crate) mod workspace;
pub(crate) mod workspace_install;
pub(crate) mod workspace_world;
pub(crate) mod worlds;

pub(in crate::tui::app) use data::*;
pub(in crate::tui::app) use data_format::*;
pub(in crate::tui::app) use nav::{PAGES, tab_index};
