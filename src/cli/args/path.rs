//! `path`: can a new shell find `tmcl`, and can I make it so.

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum PathCommand {
    /// Show whether this executable can be found by name
    Status,
    /// Add this executable's directory to your PATH
    Add,
    /// Remove this executable's directory from your PATH
    Remove,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::args::{Cli, Command, PathCommand};

    #[test]
    fn path_has_three_leaves() {
        for word in ["status", "add", "remove"] {
            let cli = Cli::try_parse_from(["tmcl", "path", word]).expect("parse");
            let command = cli.command.expect("subcommand");
            assert!(
                matches!(command, Command::Path { .. }),
                "{word} did not land under path"
            );
        }
    }

    #[test]
    fn the_leaves_are_distinct() {
        let status = Cli::try_parse_from(["tmcl", "path", "status"]).expect("parse");
        let add = Cli::try_parse_from(["tmcl", "path", "add"]).expect("parse");
        let remove = Cli::try_parse_from(["tmcl", "path", "remove"]).expect("parse");

        let pick = |cli: Cli| match cli.command {
            Some(Command::Path { command }) => match command {
                PathCommand::Status => "status",
                PathCommand::Add => "add",
                PathCommand::Remove => "remove",
            },
            _ => "other",
        };
        assert_eq!(pick(status), "status");
        assert_eq!(pick(add), "add");
        assert_eq!(pick(remove), "remove");
    }
}
