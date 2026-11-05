use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum VersionCommand {
    /// List Minecraft versions
    List(VersionListArgs),
}

#[derive(Debug, clap::Args)]
pub struct VersionListArgs {
    /// Filter by release channel
    #[arg(
        long = "type",
        value_name = "TYPE",
        value_parser = ["release", "snapshot", "old_beta", "old_alpha", "all"],
        default_value = "all"
    )]
    pub kind: String,
}
