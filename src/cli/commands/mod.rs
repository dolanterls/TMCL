//! Command implementations, one file per family.
//!
//! Every function here does three things and nothing else: resolve what the
//! user pointed at, call one `core` entry point, render the result. Anything
//! longer than that means `core` is missing a use case, not that the command
//! needs more code.

mod account;
mod common;
mod completions;
mod config;
mod content;
mod instance;
mod java;
mod mods;
mod path;
mod resource;
mod task;
mod trash;
mod update;
mod version;
mod world;

use crate::cli::args::{Cli, Command};
use crate::cli::output::OutputOptions;
use crate::core::error::Result;

pub async fn run(cli: Cli) -> Result<()> {
    let output = OutputOptions::from_cli(&cli);
    match cli.command {
        Some(Command::Tui) | None => crate::tui::run().await,
        Some(Command::Instance { command }) => instance::run(command, output).await,
        Some(Command::Account { command }) => account::run(command, output).await,
        Some(Command::Java { command }) => java::run(command, output).await,
        Some(Command::Version { command }) => version::run(command, output).await,
        Some(Command::Task { command }) => task::run(command, output).await,
        Some(Command::Config { command }) => config::run(command, output),
        Some(Command::Content { command }) => content::run(command, output).await,
        Some(Command::Trash { command }) => trash::run(command, output),
        Some(Command::World { command }) => world::run(command, output),
        Some(Command::Mod { command }) => mods::run(command, output).await,
        Some(Command::Search(args)) => resource::search(args, output).await,
        Some(Command::Project(args)) => resource::project(args, output).await,
        Some(Command::Install(args)) => resource::install(args, output).await,
        Some(Command::Modpack { command }) => resource::run_modpack(command, output).await,
        Some(Command::Update { command }) => update::run(command, output).await,
        Some(Command::UpdateCheck(args)) => update::run_deprecated(args, output).await,
        Some(Command::Path { command }) => path::run(command, output),
        Some(Command::Completions(args)) => completions::run(args, output),
    }
}
