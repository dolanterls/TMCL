use std::path::PathBuf;

use clap::{Args, Subcommand};
use uuid::Uuid;

#[derive(Debug, Subcommand)]
pub enum InstanceCommand {
    /// List installed instances
    List(InstanceListArgs),
    /// Show one instance in detail
    Show(InstanceShowArgs),
    /// Create an instance and install its loader
    Create(InstanceCreateArgs),
    /// Adopt an existing version directory
    Import(InstanceImportArgs),
    /// Rename an instance and its version files
    Rename(InstanceRenameArgs),
    /// Re-download the files an instance is missing
    Repair(InstanceRepairArgs),
    /// Pack an instance into a zip
    Export(InstanceExportArgs),
    /// Check an instance's files against its version JSON
    Verify(InstanceVerifyArgs),
    /// Start the game
    Launch(InstanceLaunchArgs),
    /// Forget an instance, keeping its files
    Unregister(InstanceUnregisterArgs),
    /// Delete an instance and everything inside it
    Delete(InstanceDeleteArgs),
}

#[derive(Debug, Args)]
pub struct InstanceListArgs {
    #[arg(long, value_name = "PATH")]
    pub game_dir: Option<PathBuf>,
    #[arg(
        long,
        value_name = "STATUS",
        value_parser = ["ready", "incomplete", "all"],
        default_value = "all"
    )]
    pub status: String,
}

#[derive(Debug, Args)]
pub struct InstanceShowArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
}

#[derive(Debug, Args)]
pub struct InstanceCreateArgs {
    #[arg(value_name = "NAME")]
    pub name: String,
    #[arg(long, value_name = "MINECRAFT_VERSION")]
    pub version: String,
    #[arg(long, default_value = "vanilla", value_parser = ["vanilla", "fabric", "quilt", "forge", "neoforge"])]
    pub loader: String,
    #[arg(
        long,
        value_name = "VERSION",
        help = "Exact Fabric/Forge/NeoForge loader version"
    )]
    pub loader_version: Option<String>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Game directory; defaults to the first configured one"
    )]
    pub game_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct InstanceImportArgs {
    #[arg(value_name = "INSTANCE_DIR")]
    pub instance_dir: PathBuf,
    #[arg(long, value_name = "MINECRAFT_VERSION")]
    pub version: String,
    #[arg(
        long,
        value_name = "NAME",
        help = "Rename the imported instance; defaults to the directory name"
    )]
    pub name: Option<String>,
    #[arg(long, value_name = "UUID")]
    pub game_directory_id: Option<Uuid>,
}

/// `rename` accepts both `rename <REF> <NEW>` and `rename <NEW>`.
///
/// The short form means "the current instance", which is the common case. Two
/// required positionals would make the short form an error; two optional ones
/// with an explicit check give both shapes one meaning.
#[derive(Debug, Args)]
pub struct InstanceRenameArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "NEW_NAME")]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct InstanceRepairArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
}

#[derive(Debug, Args)]
pub struct InstanceExportArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(long, value_name = "PATH", help = "Defaults to <instance>.zip")]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct InstanceVerifyArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(long)]
    pub full: bool,
    #[arg(long, value_name = "CLIENT_JSON")]
    pub client_json: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct InstanceLaunchArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(long, value_name = "JAVA")]
    pub java: Option<PathBuf>,
    #[arg(long, value_name = "CLIENT_JSON")]
    pub client_json: Option<PathBuf>,
    #[arg(long, value_name = "ACCOUNT_ID")]
    pub account_id: Option<Uuid>,
    #[arg(long, value_name = "TOKEN", hide = true)]
    pub token: Option<String>,
    #[arg(long)]
    pub min_memory_mb: Option<u32>,
    #[arg(long)]
    pub max_memory_mb: Option<u32>,
    #[arg(long)]
    pub width: Option<u32>,
    #[arg(long)]
    pub height: Option<u32>,
    #[arg(long)]
    pub fullscreen: bool,
    #[arg(
        long,
        value_name = "PATH",
        help = "Write the launch command to a script (.bat/.ps1/.sh) instead of starting the game"
    )]
    pub export_script: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct InstanceUnregisterArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(long, help = "Skip the confirmation prompt")]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct InstanceDeleteArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(long, help = "Skip the confirmation prompt")]
    pub yes: bool,
}
