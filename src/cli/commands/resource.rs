//! The download centre: search, look at a project, install it.
//!
//! Every command here is three steps: build the service, ask `core` what would
//! happen, print it. The decisions — which provider, which file, which
//! destination — all live in `core::resource`, so a `--dry-run` and the real run
//! cannot disagree.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde::Serialize;
use uuid::Uuid;

use crate::cli::args::{
    InstallArgs, ModpackCommand, ModpackInstallArgs, ModpackUpdateArgs, ProjectArgs, SearchArgs,
};
use crate::cli::output::OutputOptions;
use crate::core::config::models::Config;
use crate::core::config::paths::DataPaths;
use crate::core::error::{Error, Result};
use crate::core::resource::compatibility::content_root;
use crate::core::resource::model::{
    ResourceFile, ResourceKind, ResourceProject, ResourceQuery, ResourceSource,
};
use crate::core::resource::modpack_install;
use crate::core::resource::plan::{self, InstallPlan};
use crate::core::resource::select::choose_file;
use crate::core::resource::service::ResourceService;
use crate::core::task::{ModpackFilesSpec, ResourceInstallSpec, TaskOperation};

use super::common;

/// What every download command needs: where the data lives, the settings, and a
/// service wired to both providers.
struct Context {
    paths: DataPaths,
    config: Config,
    service: ResourceService,
}

fn context() -> Result<Context> {
    let paths = DataPaths::discover()?;
    paths.ensure_dirs()?;
    let config = common::load_config(&paths)?;
    let client = Arc::new(crate::core::version::http::HttpClient::with_timeout(
        config.downloads.timeout_secs.max(1),
    )?);
    Ok(Context {
        paths,
        config,
        service: ResourceService::new(crate::core::resource::registry(client)),
    })
}

fn source_from(value: &str) -> Result<ResourceSource> {
    match value {
        "modrinth" => Ok(ResourceSource::Modrinth),
        "curseforge" => Ok(ResourceSource::CurseForge),
        other => Err(Error::Config(format!("unknown source: {other}"))),
    }
}

fn kind_from(value: &str) -> Result<ResourceKind> {
    match value {
        "mod" => Ok(ResourceKind::Mod),
        "modpack" => Ok(ResourceKind::Modpack),
        "pack" => Ok(ResourceKind::ResourcePack),
        "shader" => Ok(ResourceKind::Shader),
        "world" => Ok(ResourceKind::World),
        other => Err(Error::Config(format!("unknown kind: {other}"))),
    }
}

#[derive(Debug, Serialize)]
struct SearchRow {
    id: String,
    name: String,
    author: String,
    downloads: Option<u64>,
    updated_at: Option<String>,
    /// A few supported versions, so a row is not just a name.
    versions: Vec<String>,
}

impl SearchRow {
    fn new(project: &ResourceProject) -> Self {
        Self {
            id: project.project_id.clone(),
            name: project.title.clone(),
            author: project.author.clone(),
            downloads: project.downloads,
            updated_at: project.updated_at.clone(),
            versions: project.game_versions.iter().take(3).cloned().collect(),
        }
    }
}

pub(super) async fn search(args: SearchArgs, output: OutputOptions) -> Result<()> {
    let context = context()?;
    let source = source_from(&args.source)?;
    let kind = kind_from(&args.kind)?;
    let mut query = ResourceQuery::new(source, kind);
    query.search = args.query.clone();
    // A caller asking for 5 results does not want a page of 20 printed.
    query.page_size = args.limit.clamp(1, 50);
    let page = context
        .service
        .search(context.service.begin(source), query)
        .await?;
    let rows = page.hits.iter().map(SearchRow::new).collect::<Vec<_>>();

    output.print(&rows, || {
        if rows.is_empty() {
            return format!("No results for \"{}\" on {}.", args.query, source.label());
        }
        rows.iter()
            .map(|row| {
                format!(
                    "{:<14}  {:>10}  {}  (by {}, {})",
                    row.id,
                    row.downloads
                        .map(|count| count.to_string())
                        .unwrap_or_else(|| "-".into()),
                    row.name,
                    row.author,
                    if row.versions.is_empty() {
                        "unknown versions".to_owned()
                    } else {
                        row.versions.join(", ")
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

#[derive(Debug, Serialize)]
struct FileRow {
    id: String,
    name: String,
    size: Option<u64>,
    channel: String,
    versions: Vec<String>,
    loaders: Vec<String>,
}

impl FileRow {
    fn new(file: &ResourceFile) -> Self {
        Self {
            id: file.file_id.clone(),
            name: file.name.clone(),
            size: file.size,
            channel: match file.channel {
                crate::core::resource::model::ReleaseChannel::Release => "release",
                crate::core::resource::model::ReleaseChannel::Beta => "beta",
                crate::core::resource::model::ReleaseChannel::Alpha => "alpha",
                crate::core::resource::model::ReleaseChannel::Unknown => "unknown",
            }
            .to_owned(),
            versions: file.game_versions.iter().take(4).cloned().collect(),
            loaders: file.loaders.clone(),
        }
    }
}

pub(super) async fn project(args: ProjectArgs, output: OutputOptions) -> Result<()> {
    let context = context()?;
    let source = source_from(&args.source)?;
    let kind = kind_from(&args.kind)?;
    let request = context.service.begin(source);
    let project = context
        .service
        .project(request, kind, args.id.clone())
        .await?;
    let files = context
        .service
        .files(request, kind, args.id.clone())
        .await?;
    let rows = files.iter().map(FileRow::new).collect::<Vec<_>>();

    output.print(
        &serde_json::json!({"project": project, "files": rows}),
        || {
            let mut text = format!(
                "{}\nby {}\n{}\n\n{} file(s):\n",
                project.title,
                project.author,
                project.description,
                rows.len()
            );
            if rows.is_empty() {
                text.push_str("  (none)");
            } else {
                text.push_str(
                    &rows
                        .iter()
                        .map(|row| {
                            format!(
                                "  {:<14}  {:<10}  {:>9}  {}",
                                row.id,
                                row.channel,
                                row.size
                                    .map(common::human_size)
                                    .unwrap_or_else(|| "-".to_owned()),
                                row.name
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }
            text
        },
    )
}

#[derive(Debug, Serialize)]
struct PlannedRow {
    name: String,
    path: String,
    dependency: bool,
    size: Option<u64>,
}

/// Everything a plan would write, in one shape for both the dry run and the
/// summary after a real install.
fn plan_rows(plan: &InstallPlan) -> Vec<PlannedRow> {
    plan.files
        .iter()
        .map(|planned| PlannedRow {
            name: planned.file.name.clone(),
            path: planned.destination.display().to_string(),
            dependency: planned.dependency,
            size: planned.file.size,
        })
        .collect()
}

pub(super) async fn install(args: InstallArgs, output: OutputOptions) -> Result<()> {
    let context = context()?;
    let source = source_from(&args.source)?;
    let kind = kind_from(&args.kind)?;
    // Checked before resolving the instance: a wrong kind is a mistake no
    // instance can fix, and the message should not depend on having one.
    if kind.install_directory().is_none() {
        return Err(Error::Config(format!(
            "{} is installed as an instance, not into one; use `tmcl modpack install`",
            kind.label()
        )));
    }
    let instance = common::instance(args.instance.as_deref(), &context.config)?;
    let plan = plan::plan(
        &context.service,
        context.service.begin(source),
        kind,
        &args.project,
        &instance.config,
        args.file.as_deref(),
        args.with_deps,
    )
    .await?;

    // Refuse before downloading rather than after: the task engine would
    // happily overwrite a working mod, and the player would never know it was
    // replaced.
    for planned in &plan.files {
        crate::core::resource::conflict::check_available(&planned.destination)
            .map_err(Error::Download)?;
    }

    let rows = plan_rows(&plan);
    if output.dry_run {
        return output.print(
            &serde_json::json!({
                "plan": {
                    "project": plan.project.project_id,
                    "title": plan.project.title,
                    "files": rows,
                    "unresolved": plan.unresolved,
                },
                "applied": false,
            }),
            || {
                format!(
                    "Would install {} into {}:\n{}\n{}",
                    plan.project.title,
                    instance.config.name,
                    rows.iter()
                        .map(|row| format!(
                            "  {} -> {}{}",
                            row.name,
                            row.path,
                            if row.dependency { "  (dependency)" } else { "" }
                        ))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    unresolved_note(&plan.unresolved)
                )
            },
        );
    }

    let operations = plan
        .files
        .iter()
        .map(|planned| {
            TaskOperation::ResourceInstall(ResourceInstallSpec {
                instance_id: instance.config.id,
                instance_root: content_root(&instance.config),
                destination: planned.destination.clone(),
                cache_root: context.paths.cache_dir.clone(),
                source: planned.file.source,
                kind,
                project_id: planned.file.project_id.clone(),
                file_id: planned.file.file_id.clone(),
                file_name: planned.file.name.clone(),
                candidate_urls: vec![planned.file.download_url.clone()],
                expected_sha1: planned.file.sha1.clone(),
                expected_size: planned.file.size,
                download_snapshot: context.config.download_snapshot(&context.paths),
            })
        })
        .collect::<Vec<_>>();
    for outcome in common::run_tasks(&context.paths, operations, &output).await? {
        if let crate::core::task::TaskOutcome::Failed { message, .. }
        | crate::core::task::TaskOutcome::Cancelled { message, .. } = outcome
        {
            return Err(Error::Download(message));
        }
    }

    output.print(
        &serde_json::json!({
            "project": plan.project.project_id,
            "installed": rows,
            "unresolved": plan.unresolved,
        }),
        || {
            format!(
                "Installed {} file(s) for {} into {}{}",
                rows.len(),
                plan.project.title,
                instance.config.name,
                unresolved_note(&plan.unresolved)
            )
        },
    )
}

fn unresolved_note(unresolved: &[String]) -> String {
    if unresolved.is_empty() {
        String::new()
    } else {
        format!(
            "\n{} dependency(ies) could not be resolved: {}",
            unresolved.len(),
            unresolved
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}

pub(super) async fn run_modpack(command: ModpackCommand, output: OutputOptions) -> Result<()> {
    match command {
        ModpackCommand::Install(args) => modpack_install_command(args, output).await,
        ModpackCommand::Update(args) => modpack_update_command(args, output).await,
    }
}

async fn modpack_install_command(args: ModpackInstallArgs, output: OutputOptions) -> Result<()> {
    let context = context()?;
    let source = source_from(&args.source)?;
    let archive = stage_archive(&context, &args.project, source, &output).await?;
    let provider = curseforge_provider(&context);

    if let Some(reference) = args.instance.as_deref() {
        let instance = common::instance(Some(reference), &context.config)?;
        return apply_pack(&context, &instance, &archive, provider, &output).await;
    }

    let game_directory = args
        .game_dir
        .clone()
        .or_else(|| context.config.game_directories.first().cloned())
        .ok_or_else(|| {
            Error::Config("no game directory configured; pass --game-dir <PATH>".into())
        })?;
    let game_directory = crate::core::instance::path::normalize_game_directory(&game_directory)?;
    let name = args.name.clone().unwrap_or_else(|| pack_name(&archive));

    if output.dry_run {
        let plan = modpack_install::plan(&archive, provider.as_deref()).await?;
        let _ = std::fs::remove_file(&archive);
        return output.print(
            &serde_json::json!({
                "plan": {
                    "name": name,
                    "game_directory": game_directory,
                    "minecraft_version": plan.metadata.minecraft_version,
                    "loader": plan.metadata.loader,
                    "loader_version": plan.metadata.loader_version,
                    "files": plan.files.len(),
                    "unresolved": plan.unresolved,
                },
                "applied": false,
            }),
            || {
                format!(
                    "Would create {} at {} (Minecraft {}, {}{}) and download {} file(s).{}",
                    name,
                    game_directory.join("versions").join(&name).display(),
                    plan.metadata.minecraft_version,
                    plan.metadata.loader,
                    if plan.metadata.loader_version.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", plan.metadata.loader_version)
                    },
                    plan.files.len(),
                    unresolved_note(&plan.unresolved)
                )
            },
        );
    }

    let prepared = modpack_install::prepare(
        modpack_install::InstallRequest {
            game_directory: game_directory.clone(),
            name: name.clone(),
            archive,
            // A CLI install has no project page behind it, so there is no
            // artwork to fetch; the instance keeps whatever the pack shipped.
            icon_url: None,
        },
        provider.as_deref(),
    )
    .await?;
    let installed =
        prepare_files_task(&context, &prepared.instance, &prepared.files, &output).await?;

    output.print(
        &serde_json::json!({
            "name": prepared.instance.name,
            "path": prepared.instance.path,
            "minecraft_version": prepared.metadata.minecraft_version,
            "loader": prepared.metadata.loader,
            "loader_version": prepared.metadata.loader_version,
            "files": installed,
            "unresolved": prepared.unresolved,
        }),
        || {
            format!(
                "Installed {} (Minecraft {}, {}) with {} file(s){}\nStart it with: tmcl instance launch {}",
                prepared.instance.name,
                prepared.metadata.minecraft_version,
                prepared.metadata.loader,
                installed,
                unresolved_note(&prepared.unresolved),
                prepared.instance.name
            )
        },
    )
}

async fn modpack_update_command(args: ModpackUpdateArgs, output: OutputOptions) -> Result<()> {
    let context = context()?;
    let instance = common::instance(args.reference.as_deref(), &context.config)?;
    if crate::core::resource::modpack_update::read_manifest(&instance.config.path).is_none() {
        return Err(Error::Config(format!(
            "{} was not installed from a modpack; there is no pack manifest to update",
            instance.config.name
        )));
    }
    let source = source_from(&args.source)?;
    let archive = stage_archive(&context, &args.from, source, &output).await?;
    let provider = curseforge_provider(&context);
    apply_pack(&context, &instance, &archive, provider, &output).await
}

/// Puts the pack archive into the cache so the pipeline can consume it.
///
/// A local archive is copied first: `modpack_install::prepare` deletes its
/// staging file once extraction succeeded, and deleting the player's download
/// would be a surprise.
async fn stage_archive(
    context: &Context,
    argument: &str,
    source: ResourceSource,
    output: &OutputOptions,
) -> Result<PathBuf> {
    let directory = context.paths.cache_dir.join("modpacks");
    std::fs::create_dir_all(&directory)?;
    let local = Path::new(argument);
    if local.is_file() {
        let name = local
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("pack.mrpack");
        let staging = directory.join(format!("{}-{name}", Uuid::new_v4()));
        std::fs::copy(local, &staging)?;
        return Ok(staging);
    }

    // Not a file: treat it as a project id and fetch the newest archive.
    let request = context.service.begin(source);
    let files = context
        .service
        .files(request, ResourceKind::Modpack, argument.to_owned())
        .await?;
    let file = choose_file(&files, None)
        .ok_or_else(|| Error::Download(format!("{argument} has no file to download")))?;
    let staging = directory.join(format!("{}-{}", Uuid::new_v4(), file.name));
    download(context, file, &staging, output).await?;
    Ok(staging)
}

async fn download(
    context: &Context,
    file: &ResourceFile,
    destination: &Path,
    output: &OutputOptions,
) -> Result<()> {
    use crate::core::download::{CachePolicy, DownloadRequest, download_request};

    let request = DownloadRequest::new(
        vec![file.download_url.clone()],
        destination,
        file.sha1.clone(),
        file.size,
        file.name.clone(),
    )
    .with_cache_policy(CachePolicy::None)
    .without_replacing();
    let client = crate::core::version::http::HttpClient::with_timeout(
        context.config.downloads.timeout_secs.max(1),
    )?;
    let cancelled = AtomicBool::new(false);
    let mut progress = common::DownloadProgress::new(output);
    let result = download_request(
        &client,
        &request,
        &context.paths.cache_dir,
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

/// Applies a pack to an instance that already has one: the update path.
async fn apply_pack(
    context: &Context,
    instance: &crate::core::instance::resolve::ResolvedInstance,
    archive: &Path,
    provider: Option<Arc<dyn crate::core::resource::ResourceProvider>>,
    output: &OutputOptions,
) -> Result<()> {
    let prepared = modpack_install::prepare_update(
        modpack_install::UpdateRequest {
            instance: instance.config.clone(),
            archive: archive.to_path_buf(),
        },
        provider.as_deref(),
    )
    .await?;

    if output.dry_run {
        let _ = std::fs::remove_file(archive);
        return output.print(
            &serde_json::json!({
                "plan": {
                    "instance": instance.config.name,
                    "version": prepared.version,
                    "files": prepared.files.len(),
                    "unresolved": prepared.unresolved,
                },
                "applied": false,
            }),
            || {
                format!(
                    "Would apply {} to {} and download {} file(s).{}",
                    prepared.version,
                    instance.config.name,
                    prepared.files.len(),
                    unresolved_note(&prepared.unresolved)
                )
            },
        );
    }

    let staging = crate::core::resource::modpack_update::staging_dir(
        &context.paths.cache_dir,
        instance.config.id,
    );
    let root = instance.config.path.clone();
    let archive = archive.to_path_buf();
    let version = prepared.version.clone();
    let files = prepared.files.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        crate::core::resource::modpack_update::apply(&root, &archive, &staging, &version, files)
    })
    .await
    .map_err(|error| Error::Download(format!("modpack update task failed: {error}")))??;

    let installed = prepare_files_task(context, &instance.config, &outcome.files, output).await?;

    output.print(
        &serde_json::json!({
            "instance": instance.config.name,
            "version": prepared.version,
            "overwritten": outcome.overwritten,
            "removed": outcome.removed,
            "files": installed,
            "unresolved": prepared.unresolved,
        }),
        || {
            format!(
                "Applied {} to {}: {} file(s) updated, {} removed, {} downloaded{}",
                prepared.version,
                instance.config.name,
                outcome.overwritten,
                outcome.removed,
                installed,
                unresolved_note(&prepared.unresolved)
            )
        },
    )
}

async fn prepare_files_task(
    context: &Context,
    instance: &crate::core::instance::models::InstanceConfig,
    files: &[crate::core::task::ModpackFileSpec],
    output: &OutputOptions,
) -> Result<usize> {
    if files.is_empty() {
        return Ok(0);
    }
    let operation = TaskOperation::ModpackFiles(ModpackFilesSpec {
        instance_id: instance.id,
        instance_root: content_root(instance),
        cache_root: context.paths.cache_dir.clone(),
        files: files.to_vec(),
        download_snapshot: context.config.download_snapshot(&context.paths),
    });
    match common::run_task(&context.paths, operation, output).await? {
        crate::core::task::TaskOutcome::Failed { message, .. }
        | crate::core::task::TaskOutcome::Cancelled { message, .. } => {
            Err(Error::Download(message))
        }
        _ => Ok(files.len()),
    }
}

/// Instance name from an archive's file name: `fabulously-1.2.mrpack` to
/// `fabulously-1.2`.
fn pack_name(archive: &Path) -> String {
    archive
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("modpack")
        .trim_end_matches(".mrpack")
        .trim_end_matches(".zip")
        .to_owned()
}

fn curseforge_provider(
    context: &Context,
) -> Option<Arc<dyn crate::core::resource::ResourceProvider>> {
    context
        .service
        .provider(ResourceSource::CurseForge, ResourceKind::Modpack)
        .ok()
}
