//! `content`, `trash` and `mod`: everything that lives inside one instance.
//!
//! All three share the same addressing rule as the instance commands: an
//! optional `[REF]` first, and when only one positional is given it names the
//! target, not the instance.

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum ContentCommand {
    /// List mods, resource packs or shaders
    List(ContentListArgs),
    /// Switch one entry on
    Enable(ContentToggleArgs),
    /// Switch one entry off
    Disable(ContentToggleArgs),
    /// Move an entry to the instance trash
    Remove(ContentRemoveArgs),
    /// Copy or download a file into the instance
    Add(ContentAddArgs),
    /// Move a resource pack up or down the stack
    Order(ContentOrderArgs),
}

#[derive(Debug, Args)]
pub struct ContentListArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(
        long,
        value_name = "KIND",
        value_parser = ["mod", "pack", "shader"],
        default_value = "mod",
        help = "Which content category to list"
    )]
    pub kind: String,
    #[arg(
        long,
        value_name = "ORDER",
        value_parser = ["name", "size", "status"],
        default_value = "name",
        help = "Sort order; resource packs and shaders always keep their saved order"
    )]
    pub sort: String,
    #[arg(
        long,
        value_name = "TEXT",
        help = "Only entries whose name contains this"
    )]
    pub filter: Option<String>,
}

#[derive(Debug, Args)]
pub struct ContentToggleArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
    #[arg(
        long,
        value_name = "KIND",
        value_parser = ["mod", "pack", "shader"],
        required = true
    )]
    pub kind: String,
}

#[derive(Debug, Args)]
pub struct ContentRemoveArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
    #[arg(
        long,
        value_name = "KIND",
        value_parser = ["mod", "pack", "shader"],
        required = true
    )]
    pub kind: String,
    #[arg(long, help = "Delete the file instead of moving it to the trash")]
    pub purge: bool,
    #[arg(long, help = "Skip the confirmation prompt")]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct ContentAddArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "FILE_OR_URL")]
    pub source: Option<String>,
    #[arg(
        long,
        value_name = "KIND",
        value_parser = ["mod", "pack", "shader"],
        help = "Target category; inferred from the file extension when omitted"
    )]
    pub kind: Option<String>,
    #[arg(long, help = "Skip the confirmation prompt")]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct ContentOrderArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
    #[arg(
        long,
        value_name = "KIND",
        value_parser = ["pack"],
        default_value = "pack",
        help = "Only resource packs have an order"
    )]
    pub kind: String,
    #[arg(long, conflicts_with = "down", help = "Move the entry one step up")]
    pub up: bool,
    #[arg(long, conflicts_with = "up", help = "Move the entry one step down")]
    pub down: bool,
}

#[derive(Debug, Subcommand)]
pub enum TrashCommand {
    /// List what the instance trash holds
    List(TrashListArgs),
    /// Put one trashed entry back
    Restore(TrashRestoreArgs),
    /// Empty the instance trash
    Purge(TrashPurgeArgs),
}

#[derive(Debug, Args)]
pub struct TrashListArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
}

#[derive(Debug, Args)]
pub struct TrashRestoreArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct TrashPurgeArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(long, help = "Skip the confirmation prompt")]
    pub yes: bool,
}

#[derive(Debug, Subcommand)]
pub enum ModCommand {
    /// Ask Modrinth for newer versions of every installed jar
    CheckUpdates(ModCheckArgs),
    /// Replace one outdated mod with the version Modrinth reports
    Update(ModUpdateArgs),
}

#[derive(Debug, Args)]
pub struct ModCheckArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
}

#[derive(Debug, Args)]
pub struct ModUpdateArgs {
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
    #[arg(long, help = "Skip the confirmation prompt")]
    pub yes: bool,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::args::{Cli, Command};

    #[test]
    fn content_kind_is_validated_before_the_command_runs() {
        assert!(Cli::try_parse_from(["tmcl", "content", "list", "--kind", "shaders"]).is_err());
        assert!(Cli::try_parse_from(["tmcl", "content", "list", "--kind", "shader"]).is_ok());
    }

    #[test]
    fn order_needs_a_direction() {
        // Parsing succeeds; the command rejects "neither --up nor --down" so the
        // message can point at both flags.
        let parsed = Cli::try_parse_from(["tmcl", "content", "order", "x", "--kind", "pack"])
            .expect("parse");
        assert!(matches!(
            parsed.command,
            Some(Command::Content {
                command: super::ContentCommand::Order(_)
            })
        ));
        assert!(
            Cli::try_parse_from(["tmcl", "content", "order", "x", "--up", "--down"]).is_err(),
            "two directions at once are a mistake, not a tie-break"
        );
    }

    #[test]
    fn mod_update_takes_a_name_and_an_optional_reference() {
        let short = Cli::try_parse_from(["tmcl", "mod", "update", "sodium.jar"]).expect("parse");
        let long =
            Cli::try_parse_from(["tmcl", "mod", "update", "1.21", "sodium.jar"]).expect("parse");
        assert!(matches!(short.command, Some(Command::Mod { .. })));
        assert!(matches!(long.command, Some(Command::Mod { .. })));
    }
}
