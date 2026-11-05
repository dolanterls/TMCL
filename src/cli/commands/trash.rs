//! `trash`: what the instance trash holds, and how to empty it.
//!
//! Deleting content never destroys it; a file lands under
//! `<instance>/.tmcl-trash/<stamp>/` and comes back with `trash restore`. These
//! commands are the only way back once the workspace is closed.

use serde::Serialize;

use crate::cli::args::{TrashCommand, TrashListArgs, TrashPurgeArgs, TrashRestoreArgs};
use crate::cli::consent::{Consent, Decision, require};
use crate::cli::output::OutputOptions;
use crate::core::config::paths::DataPaths;
use crate::core::content::trash;
use crate::core::error::{Error, Result};
use crate::core::instance::resolve::ResolvedInstance;

use super::common;

pub(super) fn run(command: TrashCommand, output: OutputOptions) -> Result<()> {
    match command {
        TrashCommand::List(args) => list(args, output),
        TrashCommand::Restore(args) => restore(args, output),
        TrashCommand::Purge(args) => purge(args, output),
    }
}

#[derive(Debug, Serialize)]
struct TrashRow {
    stamp: String,
    name: String,
    original: String,
    stored: String,
    size: u64,
}

fn rows(instance: &ResolvedInstance) -> Vec<TrashRow> {
    trash::list(&instance.config.path)
        .into_iter()
        .map(|entry| {
            let size = if entry.stored.is_dir() {
                crate::core::content::scan::tree_size(&entry.stored)
            } else {
                std::fs::metadata(&entry.stored)
                    .map(|meta| meta.len())
                    .unwrap_or(0)
            };
            TrashRow {
                stamp: entry.stamp,
                name: entry
                    .original
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_owned(),
                original: entry.original.display().to_string(),
                stored: entry.stored.display().to_string(),
                size,
            }
        })
        .collect()
}

fn list(args: TrashListArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(args.reference.as_deref(), &config)?;
    let rows = rows(&instance);

    output.print(&rows, || {
        if rows.is_empty() {
            return "The trash is empty.".to_owned();
        }
        rows.iter()
            .map(|row| {
                format!(
                    "{:<16}  {:<32}  {:>9}",
                    row.stamp,
                    row.name,
                    common::human_size(row.size)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn restore(args: TrashRestoreArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(args.reference.as_deref(), &config)?;
    let (_, name) = common::ref_and_name(
        args.reference,
        args.name,
        "name the trashed entry to restore: `tmcl trash restore [REF] <NAME>`",
    )?;
    let root = instance.config.path.clone();
    let entries = trash::list(&root);
    let matches = entries
        .iter()
        .filter(|entry| {
            entry
                .original
                .file_name()
                .and_then(|file| file.to_str())
                .is_some_and(|file| file.eq_ignore_ascii_case(&name))
                || entry
                    .original
                    .strip_prefix(&root)
                    .ok()
                    .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                    .is_some_and(|relative| relative.eq_ignore_ascii_case(&name))
        })
        .collect::<Vec<_>>();
    let entry = match matches.len() {
        0 => {
            return Err(Error::Config(format!(
                "nothing named {name} in the trash; `tmcl trash list` shows what is there"
            )));
        }
        1 => matches[0],
        _ => {
            return Err(Error::Config(format!(
                "{name} is in the trash {} times ({}); restore it by path instead",
                matches.len(),
                matches
                    .iter()
                    .map(|entry| entry.stamp.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    };
    let restored = trash::restore(&root, entry)?;

    output.print(
        &serde_json::json!({"name": name, "restored": restored}),
        || format!("Restored {}", restored.display()),
    )
}

fn purge(args: TrashPurgeArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(args.reference.as_deref(), &config)?;
    let root = instance.config.path.clone();
    let rows = rows(&instance);
    let total: u64 = rows.iter().map(|row| row.size).sum();
    let plan = format!(
        "Will delete {} trashed entries ({}) from {}.",
        rows.len(),
        common::human_size(total),
        root.join(trash::TRASH).display()
    );
    let consent = Consent::new(args.yes, &output);
    if require(&consent, "empty the trash", || plan.clone())? == Decision::Dry {
        return output.print(
            &serde_json::json!({"plan": plan, "applied": false, "entries": rows.len()}),
            || plan.clone(),
        );
    }
    trash::purge(&root)?;

    output.print(
        &serde_json::json!({"purged": rows.len(), "freed": total}),
        || format!("Emptied the trash ({} freed)", common::human_size(total)),
    )
}
