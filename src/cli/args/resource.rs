//! The download centre: search a source, look at a project, install it.

use std::path::PathBuf;

use clap::{Args, Subcommand};

const KINDS: [&str; 5] = ["mod", "modpack", "pack", "shader", "world"];
const SOURCES: [&str; 2] = ["modrinth", "curseforge"];

#[derive(Debug, Args)]
pub struct SearchArgs {
    #[arg(value_name = "QUERY", help = "Words to search for")]
    pub query: String,
    #[arg(
        long,
        value_name = "SOURCE",
        value_parser = SOURCES,
        default_value = "modrinth",
        help = "Where to search"
    )]
    pub source: String,
    #[arg(
        long,
        value_name = "KIND",
        value_parser = KINDS,
        default_value = "mod",
        help = "What to search for"
    )]
    pub kind: String,
    #[arg(
        long,
        value_name = "N",
        default_value_t = 10,
        help = "How many results to ask for"
    )]
    pub limit: u32,
}

#[derive(Debug, Args)]
pub struct ProjectArgs {
    #[arg(value_name = "ID", help = "Project id, as printed by `tmcl search`")]
    pub id: String,
    #[arg(
        long,
        value_name = "SOURCE",
        value_parser = SOURCES,
        default_value = "modrinth"
    )]
    pub source: String,
    #[arg(
        long,
        value_name = "KIND",
        value_parser = KINDS,
        default_value = "mod"
    )]
    pub kind: String,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    #[arg(
        value_name = "PROJECT",
        help = "Project id, as printed by `tmcl search`"
    )]
    pub project: String,
    #[arg(long, value_name = "REF", help = "Instance to install into")]
    pub instance: Option<String>,
    #[arg(long, value_name = "ID", help = "Pin one file instead of picking")]
    pub file: Option<String>,
    #[arg(long, help = "Also install the mod's required dependencies")]
    pub with_deps: bool,
    #[arg(
        long,
        value_name = "SOURCE",
        value_parser = SOURCES,
        default_value = "modrinth"
    )]
    pub source: String,
    // `modpack` and `world` are accepted so the command can point at the right
    // one instead of clap answering "invalid value".
    #[arg(
        long,
        value_name = "KIND",
        value_parser = KINDS,
        default_value = "mod"
    )]
    pub kind: String,
}

#[derive(Debug, Subcommand)]
pub enum ModpackCommand {
    /// Install a pack: a new instance, or one named with --instance
    Install(ModpackInstallArgs),
    /// Apply a newer archive to an instance installed from a pack
    Update(ModpackUpdateArgs),
}

#[derive(Debug, Args)]
pub struct ModpackInstallArgs {
    #[arg(
        value_name = "PROJECT_OR_ARCHIVE",
        help = "A local .mrpack/.zip, or a project id to download"
    )]
    pub project: String,
    #[arg(
        long,
        value_name = "REF",
        help = "Apply the pack to this instance instead of creating one"
    )]
    pub instance: Option<String>,
    #[arg(long, value_name = "NAME", help = "Instance name for a new instance")]
    pub name: Option<String>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Game directory for a new instance; defaults to the first configured"
    )]
    pub game_dir: Option<PathBuf>,
    #[arg(
        long,
        value_name = "SOURCE",
        value_parser = SOURCES,
        default_value = "modrinth"
    )]
    pub source: String,
}

#[derive(Debug, Args)]
pub struct ModpackUpdateArgs {
    #[arg(
        value_name = "REF",
        help = "Instance installed from a pack; defaults to the current one"
    )]
    pub reference: Option<String>,
    #[arg(
        long,
        value_name = "ARCHIVE",
        help = "New pack archive to apply; a path or a project id"
    )]
    pub from: String,
    #[arg(
        long,
        value_name = "SOURCE",
        value_parser = SOURCES,
        default_value = "modrinth"
    )]
    pub source: String,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::args::{Cli, Command};

    #[test]
    fn a_source_or_kind_typo_is_rejected_before_any_request() {
        assert!(Cli::try_parse_from(["tmcl", "search", "sodium", "--source", "curse"]).is_err());
        assert!(Cli::try_parse_from(["tmcl", "search", "sodium", "--kind", "mods"]).is_err());
        assert!(Cli::try_parse_from(["tmcl", "search", "sodium", "--kind", "shader"]).is_ok());
    }

    #[test]
    fn install_takes_the_project_positionally() {
        let parsed = Cli::try_parse_from([
            "tmcl",
            "install",
            "sodium",
            "--instance",
            "demo",
            "--with-deps",
        ])
        .expect("parse install");
        match parsed.command {
            Some(Command::Install(args)) => {
                assert_eq!(args.project, "sodium");
                assert_eq!(args.instance.as_deref(), Some("demo"));
                assert!(args.with_deps);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
