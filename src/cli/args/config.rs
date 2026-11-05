use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the launcher data directory
    Path,
    /// Print every setting as `key = value`
    List,
    /// Print one setting by its dotted key
    Get(ConfigGetArgs),
    /// Change one setting by its dotted key
    Set(ConfigSetArgs),
    /// Manage game directories
    GameDir {
        #[command(subcommand)]
        command: GameDirCommand,
    },
    /// Choose the Java runtime used for launches
    Java {
        #[command(subcommand)]
        command: JavaSelectCommand,
    },
}

#[derive(Debug, Args)]
pub struct ConfigGetArgs {
    #[arg(value_name = "KEY")]
    pub key: String,
}

#[derive(Debug, Args)]
pub struct ConfigSetArgs {
    #[arg(value_name = "KEY")]
    pub key: String,
    #[arg(value_name = "VALUE")]
    pub value: String,
}

#[derive(Debug, Subcommand)]
pub enum GameDirCommand {
    /// List configured game directories
    List,
    /// Register a game directory
    Add(GameDirAddArgs),
    /// Forget a game directory
    Remove(GameDirRemoveArgs),
}

#[derive(Debug, Args)]
pub struct GameDirAddArgs {
    #[arg(value_name = "PATH")]
    pub path: PathBuf,
}

#[derive(Debug, Args)]
pub struct GameDirRemoveArgs {
    #[arg(value_name = "PATH")]
    pub path: PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum JavaSelectCommand {
    /// Use the best runtime found on this machine
    Auto,
    /// Always use this runtime
    Path(JavaSelectArgs),
}

#[derive(Debug, Args)]
pub struct JavaSelectArgs {
    #[arg(value_name = "PATH")]
    pub path: PathBuf,
}
