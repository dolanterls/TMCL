use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum JavaCommand {
    /// List detected Java runtimes
    List,
    /// Rescan for Java runtimes and report what was found
    Scan,
    /// Download a Mojang Java runtime
    Download(JavaDownloadArgs),
}

#[derive(Debug, Args)]
pub struct JavaDownloadArgs {
    #[arg(value_name = "MAJOR")]
    pub major: u32,
    #[arg(long, value_name = "URL")]
    pub manifest_url: Option<String>,
}
