//! `world`: saved worlds, their backups and their data packs.
//!
//! A world is a directory, not a file, so the commands here lean on
//! `core::content::backup` and `core::content::trash` rather than on the file
//! helpers the other content kinds use.

use std::path::PathBuf;

use serde::Serialize;

use crate::cli::args::{
    DatapackCommand, DatapackListArgs, DatapackNameArgs, WorldCloneArgs, WorldCommand,
    WorldListArgs, WorldNameArgs, WorldRemoveArgs, WorldRestoreArgs,
};
use crate::cli::consent::{Consent, Decision, require};
use crate::cli::output::OutputOptions;
use crate::core::config::paths::DataPaths;
use crate::core::content::{
    ContentEntry, ContentKind, backup, content_directory, load, trash, world as level,
};
use crate::core::error::{Error, Result};
use crate::core::instance::resolve::ResolvedInstance;

use super::common;
use super::content::{ContentRow, format_rows};

pub(super) fn run(command: WorldCommand, output: OutputOptions) -> Result<()> {
    match command {
        WorldCommand::List(args) => list(args, output),
        WorldCommand::Info(args) => info(args, output),
        WorldCommand::Backup(args) => backup_world(args, output),
        WorldCommand::Backups(args) => backups(args, output),
        WorldCommand::Restore(args) => restore(args, output),
        WorldCommand::Clone(args) => clone(args, output),
        WorldCommand::Remove(args) => remove(args, output),
        WorldCommand::Datapack { command } => datapack(command, output),
    }
}

fn list(args: WorldListArgs, output: OutputOptions) -> Result<()> {
    let (_, instance) = open(args.reference.as_deref())?;
    let rows = world_rows(&instance);
    output.print(&rows, || {
        if rows.is_empty() {
            return "No worlds.".to_owned();
        }
        let width = rows
            .iter()
            .map(|row| row.name.chars().count())
            .max()
            .unwrap_or(5);
        rows.iter()
            .map(|row| {
                format!(
                    "{:<width$}  {:<9}  {:<16}  {:>9}",
                    row.name,
                    row.version.as_deref().unwrap_or("-"),
                    row.last_played
                        .map(format_timestamp)
                        .unwrap_or_else(|| "never".to_owned()),
                    common::human_size(row.size),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

#[derive(Debug, Serialize)]
pub(super) struct WorldRow {
    name: String,
    folder: String,
    version: Option<String>,
    /// Unix milliseconds, the unit the game writes; `null` when `level.dat`
    /// is unreadable.
    last_played: Option<i64>,
    game_mode: Option<String>,
    hardcore: bool,
    parsed: bool,
    size: u64,
    path: String,
}

fn world_rows(instance: &ResolvedInstance) -> Vec<WorldRow> {
    load::entries(&instance.config.path, ContentKind::World, None)
        .iter()
        .map(WorldRow::new)
        .collect()
}

impl WorldRow {
    fn new(entry: &ContentEntry) -> Self {
        let (version, last_played, game_mode, hardcore, parsed) = match &entry.meta {
            crate::core::content::ContentMeta::World {
                version,
                last_played,
                game_mode,
                hardcore,
                parsed,
                ..
            } => (
                version.clone(),
                *last_played,
                game_mode.clone(),
                *hardcore,
                *parsed,
            ),
            _ => (None, None, None, false, false),
        };
        Self {
            name: entry.name.clone(),
            folder: entry
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned(),
            version,
            last_played,
            game_mode,
            hardcore,
            parsed,
            size: entry.size,
            path: entry.path.display().to_string(),
        }
    }
}

fn info(args: WorldNameArgs, output: OutputOptions) -> Result<()> {
    let (_, instance) = open(args.reference.as_deref())?;
    let entries = load::entries(&instance.config.path, ContentKind::World, None);
    let entry = find_world(&entries, args)?;
    // A broken `level.dat` is a fact about the world, not a command failure:
    // the caller still wants the row and the reason the columns are empty.
    let read = level::read(&entry.path.join("level.dat"));

    output.print(
        &serde_json::json!({
            "name": entry.name,
            "folder": entry.path.file_name().and_then(|name| name.to_str()),
            "path": entry.path,
            "size": entry.size,
            "parsed": read.is_some(),
            "level_name": read.as_ref().map(|info| info.level_name.clone()),
            "version": read.as_ref().and_then(|info| info.version.clone()),
            "last_played": read.as_ref().and_then(|info| info.last_played),
            "game_mode": read.as_ref().and_then(|info| info.game_mode.clone()),
            "hardcore": read.as_ref().map(|info| info.hardcore),
        }),
        || {
            let Some(info) = read else {
                return format!(
                    "{} ({})\nlevel.dat could not be read; the folder name stands in for it",
                    entry.name,
                    common::human_size(entry.size)
                );
            };
            format!(
                "name: {}\nfolder: {}\nversion: {}\nlast played: {}\ngame mode: {}\nhardcore: {}\nsize: {}\npath: {}",
                info.level_name,
                entry.path.file_name().and_then(|name| name.to_str()).unwrap_or_default(),
                info.version.as_deref().unwrap_or("unknown"),
                info.last_played
                    .map(format_timestamp)
                    .unwrap_or_else(|| "never".to_owned()),
                info.game_mode.as_deref().unwrap_or("unknown"),
                if info.hardcore { "yes" } else { "no" },
                common::human_size(entry.size),
                entry.path.display(),
            )
        },
    )
}

fn backup_world(args: WorldNameArgs, output: OutputOptions) -> Result<()> {
    let (_, instance) = open(args.reference.as_deref())?;
    let entry = find_world_in(&instance, args.reference.clone(), args.name.clone())?;
    let archive = backup::backup_world(&instance.config.path, &entry.path)?;
    let size = std::fs::metadata(&archive)
        .map(|meta| meta.len())
        .unwrap_or(0);

    output.print(
        &serde_json::json!({"name": entry.name, "archive": archive, "size": size}),
        || format!("Backed up {} to {}", entry.name, archive.display()),
    )
}

#[derive(Debug, Serialize)]
struct BackupRow {
    stamp: String,
    size: u64,
    path: String,
}

fn backups(args: WorldNameArgs, output: OutputOptions) -> Result<()> {
    let (_, instance) = open(args.reference.as_deref())?;
    let entry = find_world_in(&instance, args.reference.clone(), args.name.clone())?;
    let folder = folder_of(&entry);
    let rows = backup::list_backups(&instance.config.path, &folder)
        .into_iter()
        .map(|item| BackupRow {
            stamp: item.stamp,
            size: item.size,
            path: item.path.display().to_string(),
        })
        .collect::<Vec<_>>();

    output.print(&rows, || {
        if rows.is_empty() {
            return format!("No backups of {}.", entry.name);
        }
        rows.iter()
            .map(|row| {
                format!(
                    "{:<16}  {:>9}  {}",
                    row.stamp,
                    common::human_size(row.size),
                    row.path
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn restore(args: WorldRestoreArgs, output: OutputOptions) -> Result<()> {
    let (_, instance) = open(args.reference.as_deref())?;
    let (_, name) = common::ref_and_name(
        args.reference,
        args.name,
        "name the world to restore: `tmcl world restore [REF] <NAME> --from <ARCHIVE>`",
    )?;
    let entries = load::entries(&instance.config.path, ContentKind::World, None);
    let entry = common::find_content(&entries, &name, ContentKind::World)?;
    let folder = folder_of(entry);
    let archive = resolve_archive(&instance, &folder, &args.from)?;
    let plan = format!(
        "Will replace {} with {}; the current world is backed up first.",
        entry.name,
        archive.display()
    );
    let consent = Consent::new(args.yes, &output);
    if require(&consent, &format!("restore {}", entry.name), || {
        plan.clone()
    })? == Decision::Dry
    {
        return output.print(
            &serde_json::json!({"plan": plan, "applied": false, "archive": archive}),
            || plan.clone(),
        );
    }

    let safety = backup::restore_world(&instance.config.path, &entry.path, &archive)?;
    output.print(
        &serde_json::json!({
            "name": entry.name,
            "restored_from": archive,
            "safety_backup": safety,
        }),
        || {
            format!(
                "Restored {} from {}; the previous world is in {}",
                entry.name,
                archive.display(),
                safety.display()
            )
        },
    )
}

/// `--from` accepts a path or a stamp, because `world backups` prints stamps.
fn resolve_archive(instance: &ResolvedInstance, folder: &str, from: &str) -> Result<PathBuf> {
    let path = PathBuf::from(from);
    if path.is_file() {
        return Ok(path);
    }
    let available = backup::list_backups(&instance.config.path, folder);
    let found = available.iter().find(|item| {
        item.stamp.eq_ignore_ascii_case(from)
            || item
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(from))
    });
    match found {
        Some(item) => Ok(item.path.clone()),
        None if available.is_empty() => Err(Error::Config(format!(
            "no backups of {folder}; run `tmcl world backup {folder}` first"
        ))),
        None => Err(Error::Config(format!(
            "no backup {from} for {folder}; available: {}",
            available
                .iter()
                .map(|item| item.stamp.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn clone(args: WorldCloneArgs, output: OutputOptions) -> Result<()> {
    let (_, instance) = open(args.reference.as_deref())?;
    let entry = find_world_in(&instance, args.reference.clone(), args.name.clone())?;
    let target = backup::clone_world(&entry.path, &args.suffix)?;

    output.print(
        &serde_json::json!({"name": entry.name, "clone": target}),
        || format!("Cloned {} to {}", entry.name, target.display()),
    )
}

fn remove(args: WorldRemoveArgs, output: OutputOptions) -> Result<()> {
    let (_, instance) = open(args.reference.as_deref())?;
    let (_, name) = common::ref_and_name(
        args.reference,
        args.name,
        "name the world to remove: `tmcl world remove [REF] <NAME>`",
    )?;
    let entries = load::entries(&instance.config.path, ContentKind::World, None);
    let entry = common::find_content(&entries, &name, ContentKind::World)?;
    let plan = format!(
        "Will move world {} ({}) to the instance trash.",
        entry.name,
        common::human_size(entry.size)
    );
    let consent = Consent::new(args.yes, &output);
    if require(&consent, &format!("remove world {}", entry.name), || {
        plan.clone()
    })? == Decision::Dry
    {
        return output.print(&serde_json::json!({"plan": plan, "applied": false}), || {
            plan.clone()
        });
    }

    let stored = trash::move_to_trash(&instance.config.path, &entry.path)?;
    output.print(
        &serde_json::json!({"name": entry.name, "trash": stored.stamp, "stored": stored.stored}),
        || format!("Moved {} to the trash ({})", entry.name, stored.stamp),
    )
}

fn datapack(command: DatapackCommand, output: OutputOptions) -> Result<()> {
    match command {
        DatapackCommand::List(args) => datapack_list(args, output),
        DatapackCommand::Enable(args) => datapack_state(args, true, output),
        DatapackCommand::Disable(args) => datapack_state(args, false, output),
        DatapackCommand::Remove(args) => datapack_remove(args, output),
    }
}

fn datapack_list(args: DatapackListArgs, output: OutputOptions) -> Result<()> {
    let (_, instance) = open(args.reference.as_deref())?;
    let (_, world) = common::ref_and_name(
        args.reference,
        args.world,
        "name the world: `tmcl world datapack list [REF] <WORLD>`",
    )?;
    let entries = datapack_entries(&instance, &world)?;
    let rows = entries
        .iter()
        .map(|entry| ContentRow::new(entry, ContentKind::Datapack))
        .collect::<Vec<_>>();

    output.print(&rows, || {
        if rows.is_empty() {
            return format!("{world} has no data packs.");
        }
        format_rows(&rows)
    })
}

fn datapack_state(args: DatapackNameArgs, enabled: bool, output: OutputOptions) -> Result<()> {
    let (_, instance) = open(args.reference.as_deref())?;
    let (_, world, name) = common::ref_world_name(args.reference, args.world, args.name)?;
    let name = name.ok_or_else(|| {
        Error::Config(
            "name the data pack: `tmcl world datapack <enable|disable> [REF] <WORLD> <NAME>`"
                .into(),
        )
    })?;
    let entries = datapack_entries(&instance, &world)?;
    let entry = common::find_content(&entries, &name, ContentKind::Datapack)?;
    let file = entry.display_file_name();
    let already = entry.enabled == enabled;
    // A data pack is switched by the `.disabled` marker, exactly like a mod.
    crate::core::content::toggle::set_enabled(&entry.path, enabled)?;

    let state = if enabled { "enabled" } else { "disabled" };
    output.print(
        &serde_json::json!({
            "name": file,
            "world": world,
            "enabled": enabled,
            "changed": !already,
        }),
        || {
            if already {
                format!("{file} was already {state}")
            } else {
                format!("{state} {file} in {world}")
            }
        },
    )
}

fn datapack_remove(args: DatapackNameArgs, output: OutputOptions) -> Result<()> {
    let (_, instance) = open(args.reference.as_deref())?;
    let (_, world, name) = common::ref_world_name(args.reference, args.world, args.name)?;
    let name = name.ok_or_else(|| {
        Error::Config(
            "name the data pack: `tmcl world datapack remove [REF] <WORLD> <NAME>`".into(),
        )
    })?;
    let entries = datapack_entries(&instance, &world)?;
    let entry = common::find_content(&entries, &name, ContentKind::Datapack)?;
    let file = entry.display_file_name();
    let plan = format!(
        "Will move data pack {file} of {world} ({}) to the instance trash.",
        common::human_size(entry.size)
    );
    if output.dry_run {
        return output.print(&serde_json::json!({"plan": plan, "applied": false}), || {
            plan.clone()
        });
    }
    let stored = trash::move_to_trash(&instance.config.path, &entry.path)?;

    output.print(
        &serde_json::json!({"name": file, "world": world, "trash": stored.stamp}),
        || format!("Moved {file} to the trash ({})", stored.stamp),
    )
}

/// Data packs of one world, after checking the world exists: a typo in the world
/// name must say so instead of reporting an empty list.
fn datapack_entries(instance: &ResolvedInstance, world: &str) -> Result<Vec<ContentEntry>> {
    let directory = content_directory(&instance.config.path, ContentKind::Datapack, Some(world));
    if !directory.is_dir() {
        let available = load::entries(&instance.config.path, ContentKind::World, None)
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();
        let hint = if available.is_empty() {
            "this instance has no worlds".to_owned()
        } else {
            format!("worlds: {}", available.join(", "))
        };
        return Err(Error::Config(format!("no world named {world}; {hint}")));
    }
    // Listing a world that exists but has no packs yet is not an error.
    Ok(load::entries(
        &instance.config.path,
        ContentKind::Datapack,
        Some(world),
    ))
}

fn open(
    reference: Option<&str>,
) -> Result<(crate::core::config::models::Config, ResolvedInstance)> {
    let paths = DataPaths::discover()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(reference, &config)?;
    Ok((config, instance))
}

fn find_world(entries: &[ContentEntry], args: WorldNameArgs) -> Result<&ContentEntry> {
    let (_, name) = common::ref_and_name(
        args.reference,
        args.name,
        "name the world: `tmcl world info [REF] <NAME>`",
    )?;
    common::find_content(entries, &name, ContentKind::World)
}

/// One world by name, owned: the caller keeps it after the list is dropped.
fn find_world_in(
    instance: &ResolvedInstance,
    reference: Option<String>,
    name: Option<String>,
) -> Result<ContentEntry> {
    let entries = load::entries(&instance.config.path, ContentKind::World, None);
    let (_, name) = common::ref_and_name(
        reference,
        name,
        "name the world: `tmcl world <backup|backups|clone> [REF] <NAME>`",
    )?;
    Ok(common::find_content(&entries, &name, ContentKind::World)?.clone())
}

fn folder_of(entry: &ContentEntry) -> String {
    entry
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned()
}

/// `yyyyMMdd-HHmmss` to `yyyy-MM-dd HH:mm`, which is what a player reads.
///
/// The value comes from `level.dat`, which counts milliseconds.
fn format_timestamp(millis: i64) -> String {
    let seconds = u64::try_from(millis / 1000).unwrap_or(0);
    let stamp = trash::stamp(seconds);
    match stamp.split_once('-') {
        Some((date, time)) if date.len() == 8 && time.len() >= 4 => format!(
            "{}-{}-{} {}:{}",
            &date[0..4],
            &date[4..6],
            &date[6..8],
            &time[0..2],
            &time[2..4]
        ),
        _ => stamp,
    }
}
