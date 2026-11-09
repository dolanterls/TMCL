use std::path::PathBuf;

use clap::Args;

/// `tmcl diagnose`: explain the last run without opening the UI.
///
/// The terminal launcher can do something a windowed one cannot: print the
/// verdict where the player can pipe it, paste it, or attach it. The input is a
/// log, so the command also works on an archive someone else sent.
#[derive(Debug, Args)]
pub struct DiagnoseArgs {
    #[arg(
        value_name = "REF",
        help = "Instance name, path or index; defaults to the current instance"
    )]
    pub reference: Option<String>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Read this log instead of the newest one under logs/game"
    )]
    pub log: Option<PathBuf>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Also write the diagnosis bundle (zip) to this path"
    )]
    pub export: Option<PathBuf>,
}
