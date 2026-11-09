//! `path`: report and edit the PATH entry for this executable.
//!
//! This is the command a user reaches for when `tmcl` answers "not recognized".
//! It edits the user PATH: the machine one needs elevation, and a launcher that
//! runs unelevated should not pop UAC to make its own name work.

use crate::cli::args::PathCommand;
use crate::cli::output::OutputOptions;
use crate::core::error::Result;
use crate::core::shell::path::{self, PathChange, Presence};

pub(super) fn run(command: PathCommand, output: OutputOptions) -> Result<()> {
    match command {
        PathCommand::Status => status(output),
        PathCommand::Add => edit(output, true),
        PathCommand::Remove => edit(output, false),
    }
}

fn status(output: OutputOptions) -> Result<()> {
    let status = path::status()?;
    let directory = status.directory.display().to_string();
    let broken = match &status.presence {
        Presence::Broken(entry) => Some(entry.clone()),
        _ => None,
    };
    output.print(
        &serde_json::json!({
            "directory": directory,
            "present": status.presence == Presence::Present,
            "broken_entry": broken,
        }),
        || match &status.presence {
            Presence::Present => format!("{directory} is on PATH"),
            Presence::Broken(entry) => format!(
                "{directory} is on PATH only as {entry:?}, which does not resolve; \
                 run `tmcl path add` to repair it"
            ),
            Presence::Missing => {
                format!("{directory} is not on PATH; run `tmcl path add` to add it")
            }
        },
    )
}

fn edit(output: OutputOptions, add: bool) -> Result<()> {
    if output.dry_run {
        let status = path::status()?;
        let directory = status.directory.display().to_string();
        let plan = match (&status.presence, add) {
            (Presence::Present, true) => format!("{directory} is already on PATH; nothing to do"),
            (Presence::Missing, false) => {
                format!("{directory} is not on PATH; nothing to remove")
            }
            (Presence::Broken(_), true) => {
                format!("Repair the damaged PATH entry for {directory}")
            }
            (_, true) => format!("Add {directory} to PATH"),
            (_, false) => format!("Remove {directory} from PATH"),
        };
        return output.print(&serde_json::json!({"plan": plan, "applied": false}), || {
            plan
        });
    }

    let change = if add { path::add()? } else { path::remove()? };
    report(output, &change, add)
}

fn report(output: OutputOptions, change: &PathChange, add: bool) -> Result<()> {
    let directory = change.directory.display().to_string();
    output.print(
        &serde_json::json!({
            "changed": change.changed,
            "directory": directory,
            "before": change.before,
            "after": change.after,
            "note": "a new terminal is needed for this to take effect",
        }),
        || {
            if !change.changed {
                return if add {
                    format!("{directory} is already on PATH")
                } else {
                    format!("{directory} is not on PATH")
                };
            }
            let verb = if add { "Added" } else { "Removed" };
            format!("{verb} {directory} on PATH\nA new terminal is needed for this to take effect.")
        },
    )
}
