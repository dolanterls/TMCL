//! Command-line surface.
//!
//! One file per command family. The types are re-exported from here so call
//! sites keep using `crate::cli::args::*` regardless of where a type lives.

mod account;
mod completions;
mod config;
mod content;
mod diagnose;
mod instance;
mod java;
mod path;
mod resource;
mod task;
mod update;
mod version;
mod world;

pub use account::{AccountCommand, AccountOfflineArgs, AccountRemoveArgs};
pub use completions::CompletionsArgs;
pub use config::{
    ConfigCommand, ConfigGetArgs, ConfigSetArgs, GameDirAddArgs, GameDirCommand, GameDirRemoveArgs,
    JavaSelectArgs, JavaSelectCommand,
};
pub use content::{
    ContentAddArgs, ContentCommand, ContentListArgs, ContentOrderArgs, ContentRemoveArgs,
    ContentToggleArgs, ModCheckArgs, ModCommand, ModUpdateArgs, TrashCommand, TrashListArgs,
    TrashPurgeArgs, TrashRestoreArgs,
};
pub use diagnose::DiagnoseArgs;
pub use instance::{
    InstanceCommand, InstanceCreateArgs, InstanceDeleteArgs, InstanceExportArgs,
    InstanceImportArgs, InstanceLaunchArgs, InstanceListArgs, InstanceRenameArgs,
    InstanceRepairArgs, InstanceShowArgs, InstanceUnregisterArgs, InstanceVerifyArgs,
};
pub use java::{JavaCommand, JavaDownloadArgs};
pub use path::PathCommand;
pub use resource::{
    InstallArgs, ModpackCommand, ModpackInstallArgs, ModpackUpdateArgs, ProjectArgs, SearchArgs,
};
pub use task::{TaskCancelArgs, TaskCommand, TaskListArgs, TaskRetryArgs, TaskWatchArgs};
pub use update::{UpdateApplyArgs, UpdateCheckArgs, UpdateCommand};
pub use version::{VersionCommand, VersionListArgs};
pub use world::{
    DatapackCommand, DatapackListArgs, DatapackNameArgs, WorldCloneArgs, WorldCommand,
    WorldListArgs, WorldNameArgs, WorldRemoveArgs, WorldRestoreArgs,
};

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "tmcl",
    about = "A terminal Minecraft Java Edition launcher",
    version = env!("CARGO_PKG_VERSION")
)]
pub struct Cli {
    #[arg(long, global = true, help = "Print machine-readable JSON")]
    pub json: bool,

    #[arg(long, global = true, help = "Disable colored output")]
    pub no_color: bool,

    #[arg(
        long,
        global = true,
        help = "Hide progress output; only the final result is printed"
    )]
    pub quiet: bool,

    #[arg(
        long,
        global = true,
        help = "Print what a destructive command would do and exit without doing it"
    )]
    pub dry_run: bool,

    #[arg(
        long,
        global = true,
        help = "Write diagnostics (HTTP requests, task events) to stderr; $TMCL_LOG does the same"
    )]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open the terminal UI
    Tui,
    /// Explain why the last run of an instance ended badly
    Diagnose(DiagnoseArgs),
    /// Manage instances
    Instance {
        #[command(subcommand)]
        command: InstanceCommand,
    },
    /// Manage accounts
    Account {
        #[command(subcommand)]
        command: AccountCommand,
    },
    /// Detect and install Java runtimes
    Java {
        #[command(subcommand)]
        command: JavaCommand,
    },
    /// Browse Minecraft versions
    Version {
        #[command(subcommand)]
        command: VersionCommand,
    },
    /// Inspect and control tasks
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Read and write launcher configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Manage the mods, packs and shaders of an instance
    Content {
        #[command(subcommand)]
        command: ContentCommand,
    },
    /// Put removed content back, or empty the trash
    Trash {
        #[command(subcommand)]
        command: TrashCommand,
    },
    /// Manage the worlds of an instance
    World {
        #[command(subcommand)]
        command: WorldCommand,
    },
    /// Check installed mods against Modrinth
    Mod {
        #[command(subcommand)]
        command: ModCommand,
    },
    /// Search a download source
    Search(SearchArgs),
    /// Show one project and the files it offers
    Project(ProjectArgs),
    /// Install a project into an instance
    Install(InstallArgs),
    /// Install and update modpacks
    Modpack {
        #[command(subcommand)]
        command: ModpackCommand,
    },
    /// Check for a newer TMCL release, or install it
    Update {
        #[command(subcommand)]
        command: UpdateCommand,
    },
    /// Deprecated name for `update check`
    #[command(name = "update-check", hide = true)]
    UpdateCheck(UpdateCheckArgs),
    /// Make the tmcl command available in new shells
    Path {
        #[command(subcommand)]
        command: PathCommand,
    },
    /// Print a shell completion script
    Completions(CompletionsArgs),
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command, InstanceCommand};

    #[test]
    fn parses_the_diagnose_command_and_its_three_inputs() {
        let bare = Cli::try_parse_from(["tmcl", "diagnose"]).expect("parse bare");
        match bare.command {
            Some(Command::Diagnose(args)) => {
                assert!(args.reference.is_none());
                assert!(args.log.is_none());
                assert!(args.export.is_none());
            }
            other => panic!("unexpected command: {other:?}"),
        }

        let full = Cli::try_parse_from([
            "tmcl", "diagnose", "survival", "--log", "game.log", "--export", "out.zip", "--json",
        ])
        .expect("parse full");
        assert!(full.json);
        match full.command {
            Some(Command::Diagnose(args)) => {
                assert_eq!(args.reference.as_deref(), Some("survival"));
                assert_eq!(args.log.unwrap().to_string_lossy(), "game.log");
                assert_eq!(args.export.unwrap().to_string_lossy(), "out.zip");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_nested_commands_and_global_options_after_subcommand() {
        let cli = Cli::try_parse_from([
            "tmcl",
            "--json",
            "instance",
            "create",
            "survival",
            "--version",
            "1.21.1",
        ])
        .expect("parse instance create");

        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Some(Command::Instance {
                command: InstanceCommand::Create(_)
            })
        ));
    }

    #[test]
    fn parses_required_verify_and_launch_options() {
        let verify = Cli::try_parse_from(["tmcl", "instance", "verify", "survival", "--full"])
            .expect("parse verify");
        assert!(matches!(verify.command, Some(Command::Instance { .. })));

        let launch = Cli::try_parse_from([
            "tmcl",
            "instance",
            "launch",
            "survival",
            "--java",
            "java.exe",
            "--client-json",
            "client.json",
        ])
        .expect("parse launch");
        assert!(matches!(launch.command, Some(Command::Instance { .. })));
    }

    #[test]
    fn rejects_unknown_arguments() {
        assert!(Cli::try_parse_from(["tmcl", "instance", "list", "--unknown"]).is_err());
    }

    #[test]
    fn a_loader_needs_an_explicit_version() {
        let without = Cli::try_parse_from([
            "tmcl",
            "instance",
            "create",
            "survival",
            "--version",
            "1.21.1",
            "--loader",
            "fabric",
        ])
        .expect("parsing succeeds; the loader check happens later");

        match without.command {
            Some(Command::Instance {
                command: InstanceCommand::Create(args),
            }) => {
                assert_eq!(args.loader, "fabric");
                assert!(args.loader_version.is_none());
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn rename_accepts_one_or_two_positionals() {
        let short =
            Cli::try_parse_from(["tmcl", "instance", "rename", "newname"]).expect("rename <NEW>");
        let long = Cli::try_parse_from(["tmcl", "instance", "rename", "old", "newname"])
            .expect("rename <REF> <NEW>");
        assert!(matches!(short.command, Some(Command::Instance { .. })));
        assert!(matches!(long.command, Some(Command::Instance { .. })));
    }

    #[test]
    fn parses_quiet_and_dry_run_globally() {
        let cli = Cli::try_parse_from(["tmcl", "instance", "list", "--quiet", "--dry-run"])
            .expect("parse global flags after the subcommand");

        assert!(cli.quiet);
        assert!(cli.dry_run);
    }
}
