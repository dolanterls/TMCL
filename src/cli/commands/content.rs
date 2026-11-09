//! `content`, `trash` and `mod`.
//!
//! Every command here resolves the instance, asks `core` for the list or the
//! action, and renders. Nothing decides by itself which file to touch: that is
//! what keeps the CLI and the workspace showing the same rows.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use crate::cli::args::{
    ContentAddArgs, ContentCommand, ContentListArgs, ContentOrderArgs, ContentRemoveArgs,
    ContentToggleArgs,
};
use crate::cli::consent::{Consent, Decision, require};
use crate::cli::output::OutputOptions;
use crate::core::config::models::Config;
use crate::core::config::paths::DataPaths;
use crate::core::content::{
    ContentEntry, ContentKind, content_directory, import, load, order, shader, toggle, trash,
};
use crate::core::error::{Error, Result};

use super::common;

pub(super) async fn run(command: ContentCommand, output: OutputOptions) -> Result<()> {
    match command {
        ContentCommand::List(args) => list(args, output),
        ContentCommand::Enable(args) => set_state(args, true, output),
        ContentCommand::Disable(args) => set_state(args, false, output),
        ContentCommand::Remove(args) => remove(args, output),
        ContentCommand::Add(args) => add(args, output).await,
        ContentCommand::Order(args) => reorder(args, output),
    }
}

pub(super) fn kind_from(value: &str) -> Result<ContentKind> {
    match value {
        "mod" => Ok(ContentKind::Mod),
        "pack" => Ok(ContentKind::ResourcePack),
        "shader" => Ok(ContentKind::Shader),
        "datapack" => Ok(ContentKind::Datapack),
        other => Err(Error::Config(format!("unknown content kind: {other}"))),
    }
}

pub(super) fn kind_name(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Mod => "mod",
        ContentKind::ResourcePack => "pack",
        ContentKind::Shader => "shader",
        ContentKind::World => "world",
        ContentKind::Datapack => "datapack",
    }
}

pub(super) fn sort_from(value: &str) -> order::ContentSort {
    match value {
        "size" => order::ContentSort::Size,
        "status" => order::ContentSort::Status,
        _ => order::ContentSort::Name,
    }
}

#[derive(Debug, Serialize)]
pub(super) struct ContentRow {
    name: String,
    file: String,
    kind: &'static str,
    version: String,
    size: u64,
    enabled: bool,
    order: Option<usize>,
    warning: Option<String>,
}

impl ContentRow {
    pub(super) fn new(entry: &ContentEntry, kind: ContentKind) -> Self {
        Self {
            name: entry.name.clone(),
            file: entry.display_file_name(),
            kind: kind_name(kind),
            version: entry.version.clone(),
            size: entry.size,
            enabled: entry.enabled,
            order: entry.order,
            warning: entry.warning.clone(),
        }
    }

    pub(super) fn status(&self) -> String {
        let state = if self.enabled { "enabled" } else { "disabled" };
        match &self.warning {
            Some(warning) => format!("{state}, duplicate of {warning}"),
            None => state.to_owned(),
        }
    }
}

fn list(args: ContentListArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(args.reference.as_deref(), &config)?;
    let kind = kind_from(&args.kind)?;
    let entries = load::entries(&instance.config.path, kind, None);
    let needle = args.filter.map(|value| value.to_lowercase());
    let filtered = entries
        .iter()
        .filter(|entry| match &needle {
            Some(needle) => {
                entry.name.to_lowercase().contains(needle)
                    || entry.display_file_name().to_lowercase().contains(needle)
            }
            None => true,
        })
        .collect::<Vec<_>>();
    let ordered = order::sorted(filtered, kind, sort_from(&args.sort));
    let rows = ordered
        .iter()
        .map(|entry| ContentRow::new(entry, kind))
        .collect::<Vec<_>>();

    output.print(&rows, || format_rows(&rows))
}

/// Four columns, padded to the widest value so a list stays readable.
pub(super) fn format_rows(rows: &[ContentRow]) -> String {
    if rows.is_empty() {
        return "No entries.".to_owned();
    }
    let name = rows
        .iter()
        .map(|row| crate::cli::output::width(&row.name))
        .max()
        .unwrap_or(4);
    let version = rows
        .iter()
        .map(|row| crate::cli::output::width(&row.version))
        .max()
        .unwrap_or(7)
        .max(7);
    rows.iter()
        .map(|row| {
            format!(
                "{}  {}  {}  {}",
                crate::cli::output::pad(&row.name, name),
                crate::cli::output::pad(
                    if row.version.is_empty() {
                        "-"
                    } else {
                        row.version.as_str()
                    },
                    version
                ),
                crate::cli::output::pad_start(&common::human_size(row.size), 9),
                row.status(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `content enable` and `content disable` share everything but the flag.
///
/// The three stores differ (a file name, a line in `options.txt`, the shader
/// loader's own config), so which one to touch is decided once, here, from the kind
/// the user named.
fn set_state(args: ContentToggleArgs, enabled: bool, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(args.reference.as_deref(), &config)?;
    let kind = kind_from(&args.kind)?;
    let (_, name) = common::ref_and_name(
        args.reference,
        args.name,
        "name the entry to switch: `tmcl content enable [REF] <NAME> --kind <KIND>`",
    )?;
    let root = instance.config.path.clone();
    let entries = load::entries(&root, kind, None);
    let entry = common::find_content(&entries, &name, kind)?;
    let file = entry.display_file_name();
    let path = entry.path.clone();
    let already = entry.enabled == enabled;

    if kind == ContentKind::Shader {
        let loader = shader::detect(&root.join("mods"));
        if enabled && loader.is_none() && !already {
            return Err(Error::Config(
                "no shader loader installed; add Iris or OptiFine before enabling a shader".into(),
            ));
        }
        shader::set_selected(&root, loader, if enabled { &file } else { "" })?;
    } else if kind == ContentKind::ResourcePack {
        toggle::set_pack_enabled(&root, &file, enabled)?;
    } else {
        toggle::set_enabled(&path, enabled)?;
    }

    let state = if enabled { "enabled" } else { "disabled" };
    output.print(
        &serde_json::json!({
            "name": file,
            "kind": kind_name(kind),
            "enabled": enabled,
            "changed": !already,
        }),
        || {
            if already {
                format!("{file} was already {state}")
            } else {
                format!("{state} {file}")
            }
        },
    )
}

fn remove(args: ContentRemoveArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(args.reference.as_deref(), &config)?;
    let kind = kind_from(&args.kind)?;
    let (_, name) = common::ref_and_name(
        args.reference,
        args.name,
        "name the entry to remove: `tmcl content remove [REF] <NAME> --kind <KIND>`",
    )?;
    let root = instance.config.path.clone();
    let entries = load::entries(&root, kind, None);
    let entry = common::find_content(&entries, &name, kind)?;
    let file = entry.display_file_name();
    let path = entry.path.clone();
    let size = common::human_size(entry.size);

    let plan = if args.purge {
        format!("Will delete {file} ({size}) permanently.")
    } else {
        format!("Will move {file} ({size}) to the instance trash.")
    };

    let mut stamp: Option<String> = None;
    if args.purge {
        let consent = Consent::new(args.yes, &output);
        let action = format!("delete {file}");
        if require(&consent, &action, || plan.clone())? == Decision::Dry {
            return output.print(&serde_json::json!({"plan": plan, "applied": false}), || {
                plan.clone()
            });
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    } else {
        if output.dry_run {
            return output.print(&serde_json::json!({"plan": plan, "applied": false}), || {
                plan.clone()
            });
        }
        // Recoverable, so no prompt: `trash restore` is one command away.
        stamp = Some(trash::move_to_trash(&root, &path)?.stamp);
    }
    // The config that names the file outlives it; leaving the entry behind
    // makes the game read a pack that is no longer there.
    toggle::forget(&root, kind, &file);

    output.print(
        &serde_json::json!({
            "name": file,
            "kind": kind_name(kind),
            "purged": args.purge,
            "trash": stamp,
        }),
        || {
            if args.purge {
                format!("Deleted {file}")
            } else {
                format!("Moved {file} to the trash")
            }
        },
    )
}

async fn add(args: ContentAddArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    paths.ensure_dirs()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(args.reference.as_deref(), &config)?;
    let (_, source) = common::ref_and_name(
        args.reference,
        args.source,
        "name the file or URL to add: `tmcl content add [REF] <FILE|URL>`",
    )?;
    let kind = match args.kind.as_deref() {
        Some(kind) => kind_from(kind)?,
        None => infer_kind(&source)?,
    };
    let directory = content_directory(&instance.config.path, kind, None);
    let file_name = if is_url(&source) {
        url_file_name(&source)?
    } else {
        Path::new(&source)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::Config(format!("invalid file name: {source}")))?
            .to_owned()
    };
    let target = directory.join(&file_name);
    if target.exists() {
        return Err(Error::Config(format!(
            "{} already exists; remove or rename it first",
            target.display()
        )));
    }

    if output.dry_run {
        let plan = format!("Would add {file_name} to {}", directory.display());
        return output.print(
            &serde_json::json!({"plan": plan, "applied": false, "target": target}),
            || plan.clone(),
        );
    }

    if is_url(&source) {
        if !kind.accepts(&target) {
            return Err(Error::Config(format!(
                "{file_name} is not a {} file",
                kind.label()
            )));
        }
        download_into(&paths, &config, &source, &target, &file_name, &output).await?;
    } else {
        // The copy rules — name check, kind check, collision — live in `core`
        // so the TUI's import refuses exactly the same files.
        import::copy_in(&directory, kind, Path::new(&source))?;
    }

    output.print(
        &serde_json::json!({"name": file_name, "kind": kind_name(kind), "path": target}),
        || format!("Added {file_name} to {}", directory.display()),
    )
}

async fn download_into(
    paths: &DataPaths,
    config: &Config,
    url: &str,
    target: &Path,
    file_name: &str,
    output: &OutputOptions,
) -> Result<()> {
    use crate::core::download::{CachePolicy, DownloadRequest, download_request};

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let request = DownloadRequest::new(
        vec![url.to_owned()],
        target,
        None,
        None,
        format!("content {file_name}"),
    )
    // Nothing addresses the file yet, so caching it would only add a copy.
    .with_cache_policy(CachePolicy::None)
    .without_replacing();
    let client =
        crate::core::version::http::HttpClient::with_timeout(config.downloads.timeout_secs.max(1))?;
    let cancelled = AtomicBool::new(false);
    let mut progress = common::DownloadProgress::new(output);
    let result = download_request(
        &client,
        &request,
        &paths.cache_dir,
        &cancelled,
        None,
        |done, total| {
            progress.line(done, total);
            Ok(())
        },
    )
    .await;
    progress.finish();
    result.map(|_| ())
}

/// Guesses the category from the file name; ambiguous suffixes need `--kind`.
fn infer_kind(source: &str) -> Result<ContentKind> {
    let name = if is_url(source) {
        url_file_name(source)?
    } else {
        source.to_owned()
    };
    let lower = name.to_lowercase();
    if lower.ends_with(".jar") {
        Ok(ContentKind::Mod)
    } else if lower.ends_with(".zip") {
        // A zip is either a resource pack or a shader; the pack is the common
        // case, and the help says so.
        Ok(ContentKind::ResourcePack)
    } else {
        Err(Error::Config(format!(
            "cannot tell what {name} is; pass --kind mod|pack|shader"
        )))
    }
}

fn is_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

/// Last path segment of a URL, without query or fragment.
fn url_file_name(url: &str) -> Result<String> {
    let trimmed = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .trim_end_matches('/');
    let name = trimmed.rsplit('/').next().unwrap_or_default();
    if name.is_empty() {
        return Err(Error::Config(format!(
            "cannot read a file name out of {url}; download it and use `content add <FILE>`"
        )));
    }
    crate::core::resource::conflict::validate_file_name(name)
        .map_err(|reason| Error::Config(format!("cannot use {name}: {reason}")))?;
    Ok(name.to_owned())
}

/// `content order`: only resource packs have a stack, and only enabled packs
/// are in it.
fn reorder(args: ContentOrderArgs, output: OutputOptions) -> Result<()> {
    if !args.up && !args.down {
        return Err(Error::Config(
            "say which way: `tmcl content order [REF] <NAME> --up|--down`".into(),
        ));
    }
    let paths = DataPaths::discover()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(args.reference.as_deref(), &config)?;
    let kind = kind_from(&args.kind)?;
    let (_, name) = common::ref_and_name(
        args.reference,
        args.name,
        "name the pack to move: `tmcl content order [REF] <NAME> --up|--down`",
    )?;
    let root = instance.config.path.clone();
    let entries = load::entries(&root, kind, None);
    let entry = common::find_content(&entries, &name, kind)?;
    if !entry.enabled {
        return Err(Error::Config(format!(
            "{} is not enabled, so it has no place in the stack",
            entry.display_file_name()
        )));
    }
    let moving = entry.display_file_name();

    // Enabled packs in the order the game reads them.
    let stack = order::sorted(
        entries.iter().filter(|item| item.enabled).collect(),
        kind,
        order::ContentSort::Name,
    );
    let position = stack
        .iter()
        .position(|item| item.display_file_name() == moving)
        .ok_or_else(|| Error::Config(format!("{moving} is no longer in the stack")))?;
    let target = if args.down {
        position + 1
    } else {
        position
            .checked_sub(1)
            .ok_or_else(|| Error::Config(format!("{moving} is already at the top")))?
    };
    let neighbour = stack
        .get(target)
        .ok_or_else(|| Error::Config(format!("{moving} is already at the bottom")))?;
    let other = neighbour.display_file_name();

    toggle::swap_packs(&root, &moving, &other)?;

    output.print(
        &serde_json::json!({
            "name": moving,
            "swapped_with": other,
            "position": target,
        }),
        || format!("Moved {moving} past {other}"),
    )
}

#[cfg(test)]
mod tests {
    use super::{ContentRow, format_rows};

    fn row(name: &str) -> ContentRow {
        ContentRow {
            name: name.to_owned(),
            file: format!("{name}.zip"),
            kind: "pack",
            version: String::new(),
            size: 1,
            enabled: false,
            order: None,
            warning: None,
        }
    }

    /// Column of the status cell, in terminal cells.
    fn status_column(line: &str) -> usize {
        let byte = line.find("disabled").expect("status cell");
        crate::cli::output::width(&line[..byte])
    }

    #[test]
    fn a_cjk_name_does_not_shift_the_columns() {
        let text = format_rows(&[row("abc"), row("中文包")]);
        let columns = text.lines().map(status_column).collect::<Vec<_>>();
        assert_eq!(columns[0], columns[1], "{text}");
    }
}
