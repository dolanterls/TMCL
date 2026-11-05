//! `world`: the saves of one instance, their backups and their data packs.

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum WorldCommand {
    /// List saved worlds
    List(WorldListArgs),
    /// Show what `level.dat` says about one world
    Info(WorldNameArgs),
    /// Zip a world into the instance backup directory
    Backup(WorldNameArgs),
    /// List the backups of one world
    Backups(WorldNameArgs),
    /// Replace a world with one of its backups
    Restore(WorldRestoreArgs),
    /// Copy a world under a new name
    Clone(WorldCloneArgs),
    /// Move a world to the instance trash
    Remove(WorldRemoveArgs),
    /// Work with the data packs of one world
    Datapack {
        #[command(subcommand)]
        command: DatapackCommand,
    },
}

#[derive(Debug, Args)]
pub struct WorldListArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorldNameArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorldRestoreArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
    #[arg(
        long,
        value_name = "ARCHIVE",
        help = "Backup to restore: a path, or a stamp printed by `world backups`"
    )]
    pub from: String,
    #[arg(long, help = "Skip the confirmation prompt")]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct WorldCloneArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
    #[arg(long, value_name = "TEXT", default_value = " (copy)")]
    pub suffix: String,
}

#[derive(Debug, Args)]
pub struct WorldRemoveArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
    #[arg(long, help = "Skip the confirmation prompt")]
    pub yes: bool,
}

#[derive(Debug, Subcommand)]
pub enum DatapackCommand {
    /// List the data packs of one world
    List(DatapackListArgs),
    /// Switch one data pack on
    Enable(DatapackNameArgs),
    /// Switch one data pack off
    Disable(DatapackNameArgs),
    /// Move one data pack to the instance trash
    Remove(DatapackNameArgs),
}

#[derive(Debug, Args)]
pub struct DatapackListArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "WORLD")]
    pub world: Option<String>,
}

#[derive(Debug, Args)]
pub struct DatapackNameArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "WORLD")]
    pub world: Option<String>,
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
}
