//! `update`: is there a newer TMCL, and can I take it.

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum UpdateCommand {
    /// Ask the release endpoint for the newest version
    Check(UpdateCheckArgs),
    /// Download the newest release and replace this executable
    Apply(UpdateApplyArgs),
}

#[derive(Debug, Args)]
pub struct UpdateCheckArgs {
    #[arg(
        long,
        value_name = "URL",
        help = "Release endpoint, or $TMCL_UPDATE_URL"
    )]
    pub url: Option<String>,
}

#[derive(Debug, Args)]
pub struct UpdateApplyArgs {
    #[arg(
        long,
        value_name = "URL",
        help = "Release endpoint, or $TMCL_UPDATE_URL"
    )]
    pub url: Option<String>,
    #[arg(long, help = "Skip the confirmation prompt")]
    pub yes: bool,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::args::{Cli, Command, UpdateCommand};

    #[test]
    fn check_and_apply_are_subcommands_of_update() {
        let check = Cli::try_parse_from(["tmcl", "update", "check"]).expect("parse");
        assert!(matches!(
            check.command,
            Some(Command::Update {
                command: UpdateCommand::Check(_)
            })
        ));
        let apply = Cli::try_parse_from(["tmcl", "update", "apply", "--yes"]).expect("parse");
        assert!(matches!(
            apply.command,
            Some(Command::Update {
                command: UpdateCommand::Apply(_)
            })
        ));
    }

    #[test]
    fn the_old_name_still_parses() {
        let old = Cli::try_parse_from(["tmcl", "update-check"]).expect("deprecated alias");
        assert!(matches!(old.command, Some(Command::UpdateCheck(_))));
    }
}
