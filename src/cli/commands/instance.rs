use std::path::PathBuf;

use uuid::Uuid;

use crate::cli::args::{InstanceCommand, InstanceLaunchArgs};
use crate::cli::consent::{Decision, require};
use crate::cli::output::{self, OutputOptions};
use crate::core::account::models::AccountKind;
use crate::core::config::models::Config;
use crate::core::config::paths::DataPaths;
use crate::core::config::storage::{read_json, write_json_atomic};
use crate::core::error::{Error, Result};
use crate::core::instance::delete::{delete_instance, delete_metadata};
use crate::core::instance::models::InstanceLoader;
use crate::core::instance::path::{game_directory_id, normalize_game_directory};
use crate::core::instance::resolve::{RefSource, ResolvedInstance, list_all_in, resolve};
use crate::core::instance::service::{create_instance, import_instance};
use crate::core::java::scan::discover_default;
use crate::core::java::{minimum_java_major, select_java};
use crate::core::launch::plan as launch_plan;
use crate::core::launch::preflight;
use crate::core::launch::process;
use crate::core::launch::session::{self, LaunchRequest};
use crate::core::launch::validate::{self, ClientManifest, VerificationMode};
use crate::core::task::{InstanceInstallSpec, InstanceRepairSpec, TaskOperation};

use super::common::{self, load_config};

pub(super) async fn run(command: InstanceCommand, output: OutputOptions) -> Result<()> {
    match command {
        InstanceCommand::List(args) => list(args, output),
        InstanceCommand::Show(args) => show(args.reference.as_deref(), output),
        InstanceCommand::Create(args) => create(args, output).await,
        InstanceCommand::Import(args) => import(args, output),
        InstanceCommand::Rename(args) => rename(args, output),
        InstanceCommand::Repair(args) => repair(args.reference.as_deref(), output).await,
        InstanceCommand::Export(args) => export(args, output),
        InstanceCommand::Verify(args) => verify(args, output),
        InstanceCommand::Launch(args) => launch(args, output).await,
        InstanceCommand::Unregister(args) => unregister(args, output),
        InstanceCommand::Delete(args) => delete(args, output),
    }
}

/// `None` means the current instance; every instance command shares this rule.
fn current(reference: Option<&str>, config: &Config) -> Result<ResolvedInstance> {
    resolve(reference.unwrap_or("@last"), config)
}

/// Whether the instance has everything its launch needs.
///
/// There is no "missing metadata" case: a directory without `tmcl.json` gets one
/// written the moment it is scanned, so a discovered instance always has
/// metadata. What can be missing is everything the version JSON names.
fn readiness(instance: &ResolvedInstance) -> (bool, Option<usize>) {
    let Ok(client) = session::load_client(instance, None) else {
        return (false, None);
    };
    let plan = launch_plan::plan(&instance.config.path, &instance.game_directory, &client);
    let missing = preflight::inspect_plan(&plan);
    (missing.is_empty(), Some(missing.len()))
}

fn list(args: crate::cli::args::InstanceListArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = load_config(&paths)?;
    let instances = list_all_in(&config, args.game_dir.as_deref())?;

    let mut rows = Vec::new();
    // The number is the position in the whole list, not in the filtered output:
    // `3` has to mean the same instance whether or not `--status` is used.
    for (position, instance) in instances.into_iter().enumerate() {
        let (ready, missing) = readiness(&instance);
        let status = if ready { "ready" } else { "incomplete" };
        if args.status != "all" && args.status != status {
            continue;
        }
        let mut row = InstanceRow::new(&instance, status, missing);
        row.index = Some(position + 1);
        rows.push(row);
    }
    output.print(&rows, || format_rows(&rows))
}

fn show(reference: Option<&str>, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = load_config(&paths)?;
    let instance = current(reference, &config)?;
    let (ready, missing) = readiness(&instance);
    let row = InstanceRow::new(
        &instance,
        if ready { "ready" } else { "incomplete" },
        missing,
    );

    output.print(&row, || {
        format!(
            "id: {}\nname: {}\nversion: {}\nloader: {}\npath: {}\ngame directory: {}\nstatus: {}\nlaunch count: {}\nplay time: {}\nmissing files: {}",
            row.id,
            row.name,
            row.minecraft_version,
            row.loader,
            row.path,
            instance.game_directory.display(),
            row.status,
            row.launch_count,
            seconds(row.play_time_seconds),
            row.missing_files.map_or(
                "unknown (no readable version JSON)".to_owned(),
                |count| count.to_string()
            ),
        )
    })
}

async fn create(args: crate::cli::args::InstanceCreateArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    paths.ensure_dirs()?;
    let config = load_config(&paths)?;
    let game_directory = args
        .game_dir
        .clone()
        .or_else(|| config.game_directories.first().cloned())
        .ok_or_else(|| {
            Error::Config("no game directory configured; pass --game-dir <PATH>".into())
        })?;
    let root = normalize_game_directory(&game_directory)?;
    let id = game_directory_id(&root);
    let (instance_loader, install_loader) =
        parse_cli_loader(&args.loader, args.loader_version.as_deref())?;
    let target = root.join("versions").join(&args.name);

    if output.dry_run {
        let plan = format!(
            "Would create {} (Minecraft {}, loader {})",
            target.display(),
            args.version,
            args.loader
        );
        return output.print(
            &serde_json::json!({
                "plan": plan,
                "instance": target,
                "version": args.version,
                "loader": args.loader,
                "applied": false,
            }),
            || plan.clone(),
        );
    }

    let mut instance = create_instance(&root, id, &args.name, &args.version)?;
    instance.loader = instance_loader;
    write_json_atomic(&instance.path.join("tmcl.json"), &instance)?;

    let java = select_install_java(&config, &paths, &args.version);
    let request = crate::core::loader::service::InstallRequest {
        game_directory: root.clone(),
        instance_directory: instance.path.clone(),
        instance_name: instance.name.clone(),
        minecraft_version: instance.minecraft_version.clone(),
        loader: install_loader,
        download_snapshot: config.download_snapshot(&paths),
    };
    let operation = TaskOperation::InstanceInstall(InstanceInstallSpec {
        instance_id: instance.id,
        game_directory: request.game_directory,
        instance_directory: request.instance_directory,
        instance_name: request.instance_name,
        minecraft_version: request.minecraft_version,
        loader: crate::core::task::task_loader(&request.loader),
        java_executable: java,
        download_snapshot: request.download_snapshot.clone(),
    });
    common::run_task(&paths, operation, &output).await?;

    let row = InstanceRow::new(
        &ResolvedInstance {
            config: instance,
            game_directory: root,
            source: RefSource::Path,
        },
        "ready",
        None,
    );
    output.print(&row, || format!("Installed {} at {}", row.name, row.path))
}

fn import(args: crate::cli::args::InstanceImportArgs, output: OutputOptions) -> Result<()> {
    // The rule lives in one place (design §10.1): no caller derives the game
    // directory by walking up two levels itself.
    let root = crate::core::instance::path::game_directory_of(&args.instance_dir)?;
    let id = args
        .game_directory_id
        .unwrap_or_else(|| game_directory_id(&root));
    let mut instance = import_instance(&args.instance_dir, id, &args.version)?;
    if let Some(name) = args.name.as_deref()
        && name != instance.name
    {
        instance = crate::core::instance::rename::rename_instance(&instance.path, name)?;
    }
    let row = InstanceRow::new(
        &ResolvedInstance {
            config: instance,
            game_directory: root,
            source: RefSource::Path,
        },
        "ready",
        None,
    );
    output.print(&row, || format!("Imported {} at {}", row.name, row.path))
}

fn rename(args: crate::cli::args::InstanceRenameArgs, output: OutputOptions) -> Result<()> {
    // `rename <NEW>` and `rename <REF> <NEW>` both arrive here; the single-value
    // form names the new name, not a reference.
    let (reference, new_name) = match (args.reference, args.name) {
        (Some(reference), Some(name)) => (Some(reference), name),
        (Some(name), None) => (None, name),
        (None, _) => {
            return Err(Error::Config(
                "rename needs a new name: `tmcl instance rename [REF] <NEW_NAME>`".into(),
            ));
        }
    };
    let paths = DataPaths::discover()?;
    let config = load_config(&paths)?;
    let instance = current(reference.as_deref(), &config)?;
    let old = instance.config.name.clone();
    let renamed = crate::core::instance::rename::rename_instance(&instance.config.path, &new_name)?;

    let mut config = config;
    if config.state.current_instance_id.as_deref() == Some(&instance.config.id.to_string()) {
        config.state.current_instance_id = Some(renamed.id.to_string());
        write_json_atomic(&paths.config_file, &config)?;
    }
    output.print(
        &serde_json::json!({
            "id": renamed.id,
            "old_name": old,
            "name": renamed.name,
            "path": renamed.path,
        }),
        || format!("Renamed {old} to {}", renamed.name),
    )
}

async fn repair(reference: Option<&str>, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    paths.ensure_dirs()?;
    let config = load_config(&paths)?;
    let instance = current(reference, &config)?;
    let client = session::load_client(&instance, None)?;
    let plan = launch_plan::plan(&instance.config.path, &instance.game_directory, &client);
    let missing = preflight::inspect_plan(&plan);

    if missing.is_empty() {
        return output.print(&serde_json::json!({"missing": 0, "downloaded": 0}), || {
            format!("{} is complete; nothing to repair.", instance.config.name)
        });
    }
    let files = missing
        .iter()
        .map(|file| file.path.display().to_string())
        .collect::<Vec<_>>();
    if output.dry_run {
        let text = format!(
            "Would download {} missing files for {}",
            files.len(),
            instance.config.name
        );
        return output.print(
            &serde_json::json!({
                "plan": text,
                "missing": files.len(),
                "files": files,
                "applied": false,
            }),
            || {
                format!(
                    "{text}\n  {}",
                    files
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n  ")
                )
            },
        );
    }

    let spec = InstanceRepairSpec {
        instance_id: instance.config.id,
        instance_name: instance.config.name.clone(),
        instance_directory: instance.config.path.clone(),
        game_directory: instance.game_directory.clone(),
        cache_root: config
            .downloads
            .cache_dir
            .clone()
            .unwrap_or_else(|| paths.cache_dir.clone()),
        missing,
        download_snapshot: config.download_snapshot(&paths),
    };
    let count = spec.missing.len();
    common::run_task(&paths, TaskOperation::InstanceRepair(spec), &output).await?;
    output.print(
        // The task fails if any file cannot be written, so a successful repair
        // downloaded exactly the files it was given.
        &serde_json::json!({"missing": count, "downloaded": count}),
        || format!("Repaired {count} files in {}", instance.config.name),
    )
}

fn export(args: crate::cli::args::InstanceExportArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = load_config(&paths)?;
    let instance = current(args.reference.as_deref(), &config)?;
    let target = args.output.unwrap_or_else(|| {
        PathBuf::from(crate::core::content::export::suggested_archive_name(
            &instance.config.name,
        ))
    });
    if output.dry_run {
        let plan = format!(
            "Would pack {} into {}",
            instance.config.path.display(),
            target.display()
        );
        return output.print(
            &serde_json::json!({"plan": plan, "target": target, "applied": false}),
            || plan.clone(),
        );
    }
    let written = crate::core::content::export::export_instance(
        &instance.config.path,
        &instance.config,
        &target,
    )?;
    output.print(&serde_json::json!({"path": written}), || {
        written.display().to_string()
    })
}

fn verify(args: crate::cli::args::InstanceVerifyArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = load_config(&paths)?;
    let instance = current(args.reference.as_deref(), &config)?;
    let client_path = args.client_json.unwrap_or_else(|| {
        instance
            .config
            .path
            .join(format!("{}.json", instance.config.name))
    });
    let manifest: ClientManifest = read_json(&client_path)?;
    let report = validate::validate(
        &instance.game_directory,
        &manifest,
        None,
        if args.full {
            VerificationMode::Full
        } else {
            VerificationMode::Normal
        },
    )?;
    let result = VerifyRow {
        checked: report.checked,
        missing: report
            .missing
            .into_iter()
            .map(|item| VerifyMissing {
                path: item.artifact.path.to_string_lossy().into_owned(),
                url: item.artifact.url,
                sha1: item.artifact.sha1,
                reason: item.reason,
            })
            .collect(),
    };
    output.print(&result, || {
        format!(
            "Checked {} files; {} missing or invalid.",
            result.checked,
            result.missing.len()
        )
    })
}

async fn launch(args: InstanceLaunchArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    paths.ensure_dirs()?;
    let config = load_config(&paths)?;
    let instance = current(args.reference.as_deref(), &config)?;

    if let Some(path) = args.export_script.clone() {
        return export_launch_script(&paths, &config, &instance, &args, path, output);
    }

    let request = LaunchRequest {
        instance: instance.clone(),
        account_id: args.account_id,
        java_installations: None,
        java_override: args.java.clone(),
        allow_java_download: config.java.auto_download,
        client_json: args.client_json.clone(),
        min_memory_mb: args.min_memory_mb,
        max_memory_mb: args.max_memory_mb,
        width: args.width,
        height: args.height,
        fullscreen: args.fullscreen.then_some(true),
    };
    let mode = output.progress_mode();
    let prepared = session::prepare(request, &paths, &config, |stage| {
        if let Some(line) = crate::cli::progress::stage_line(mode, stage) {
            eprintln!("{line}");
        }
    })
    .await?;

    let mut local_authlib =
        crate::core::account::authlib::LocalAuthlibService::start(&paths.root, &prepared.account)
            .await?;
    let mut spec = prepared.spec;
    if let Some(service) = &local_authlib {
        let main_class = spec
            .args
            .iter()
            .position(|argument| argument == &prepared.client.main_class)
            .ok_or_else(|| Error::Launch("cannot locate Minecraft main class".into()))?;
        spec.args
            .splice(main_class..main_class, service.jvm_args.clone());
    }
    let (events, mut event_receiver) = tokio::sync::mpsc::unbounded_channel();
    let output_events = tokio::spawn(async move {
        while let Some(event) = event_receiver.recv().await {
            if let crate::core::launch::process::GameEvent::GameLog { stream, line } = event {
                eprintln!("[{stream}] {line}");
            }
        }
    });
    let result = process::run_with_events_and_priority_and_logging(
        &spec,
        &prepared.log_path,
        Some(events),
        config.logs.game_debug,
        config.launcher.process_priority,
    )
    .await?;
    let _ = output_events.await;
    if let Some(service) = local_authlib.take() {
        service.stop().await;
    }
    let _ = crate::core::instance::stats::record_session(
        &instance.config.path,
        result.duration.as_secs(),
    );
    let retention = std::time::Duration::from_secs(
        u64::from(config.logs.retention_days).saturating_mul(24 * 60 * 60),
    );
    let cutoff = std::time::SystemTime::now()
        .checked_sub(retention)
        .unwrap_or(std::time::UNIX_EPOCH);
    let _ = crate::core::config::log_retention::cleanup_logs(&paths.logs_dir, cutoff);
    let summary = LaunchRow {
        pid: result.pid,
        exit_code: result.exit_code,
        log_path: result.log_path,
        crash_report: result.crash_report,
    };
    output.print(&summary, || {
        format!(
            "Game exited with {:?}; log: {}",
            summary.exit_code,
            summary.log_path.display()
        )
    })
}

fn export_launch_script(
    paths: &DataPaths,
    config: &Config,
    instance: &ResolvedInstance,
    args: &InstanceLaunchArgs,
    path: PathBuf,
    output: OutputOptions,
) -> Result<()> {
    use crate::core::launch::script::{self, ScriptShell};
    use crate::core::launch::session::{
        CommandRequest, build_command, load_client, resolve_account,
    };

    let account = resolve_account(args.account_id, paths, config)?;
    let client = load_client(instance, args.client_json.as_deref())?;
    let installations = discover_default(
        &paths.runtime_dir,
        config.game_directories.clone(),
        config.java.selected_path.clone(),
    );
    // An offline session's token is the literal "0"; hiding it would replace
    // unrelated arguments that happen to be "0".
    let token = match account.kind {
        AccountKind::Microsoft => script::TOKEN_SENTINEL,
        AccountKind::Offline => "0",
    };
    let (spec, _) = build_command(CommandRequest {
        instance_directory: &instance.config.path,
        instance_name: &instance.config.name,
        minecraft_version: &instance.config.minecraft_version,
        game_directory: &instance.game_directory,
        config,
        overrides: &instance.config.overrides,
        java_installations: &installations,
        java_override: args.java.as_deref(),
        account: &account,
        token,
        client: &client,
        logs_dir: &paths.game_logs_dir,
        min_memory_mb: args.min_memory_mb,
        max_memory_mb: args.max_memory_mb,
        width: args.width,
        height: args.height,
        fullscreen: args.fullscreen.then_some(true),
    })?;
    let shell = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(ScriptShell::parse)
        .unwrap_or_else(ScriptShell::for_host);
    let secret = match account.kind {
        AccountKind::Microsoft => token,
        AccountKind::Offline => "",
    };
    let rendered = script::render(&spec, shell, secret);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &rendered.contents)?;
    for warning in &rendered.warnings {
        eprintln!("warning: {warning}");
    }
    let row = serde_json::json!({
        "path": path,
        "format": shell.extension(),
        "warnings": rendered.warnings,
    });
    output.print(&row, || path.display().to_string())
}

fn unregister(args: crate::cli::args::InstanceUnregisterArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let mut config = load_config(&paths)?;
    let instance = current(args.reference.as_deref(), &config)?;
    let path = instance.config.path.clone();
    let consent = crate::cli::consent::Consent::new(args.yes, &output);
    let action = format!("unregister {}", instance.config.name);
    let plan = format!(
        "Will remove TMCL metadata from {} and hide it from the instance list. Files are kept.",
        path.display()
    );
    if require(&consent, &action, || plan.clone())? == Decision::Dry {
        return output.print(&serde_json::json!({"plan": plan, "applied": false}), || {
            plan.clone()
        });
    }

    delete_metadata(&instance.game_directory, &path)?;
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
    if !config.excluded_instances.contains(&canonical) {
        config.excluded_instances.push(canonical);
        write_json_atomic(&paths.config_file, &config)?;
    }
    output.print(
        &serde_json::json!({"path": path, "unregistered": true}),
        || format!("Unregistered {}", path.display()),
    )
}

fn delete(args: crate::cli::args::InstanceDeleteArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let mut config = load_config(&paths)?;
    let instance = current(args.reference.as_deref(), &config)?;
    let path = instance.config.path.clone();
    let bytes = crate::core::content::scan::tree_size(&path);
    let consent = crate::cli::consent::Consent::new(args.yes, &output);
    let action = format!("delete {}", instance.config.name);
    let plan = format!(
        "Will delete {} and everything inside it ({})",
        path.display(),
        size(bytes)
    );
    if require(&consent, &action, || plan.clone())? == Decision::Dry {
        return output.print(&serde_json::json!({"plan": plan, "applied": false}), || {
            plan.clone()
        });
    }

    delete_instance(&instance.game_directory, &path)?;
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
    config
        .excluded_instances
        .retain(|entry| entry != &canonical);
    if config.state.current_instance_id.as_deref() == Some(&instance.config.id.to_string()) {
        config.state.current_instance_id = None;
    }
    write_json_atomic(&paths.config_file, &config)?;
    output.print(
        &serde_json::json!({"path": path, "deleted": true, "bytes": bytes}),
        || format!("Deleted {}", path.display()),
    )
}

fn select_install_java(config: &Config, paths: &DataPaths, version: &str) -> Option<PathBuf> {
    let candidates = discover_default(
        &paths.runtime_dir,
        config.game_directories.clone(),
        config.java.selected_path.clone(),
    );
    let required = minimum_java_major(version).ok()?;
    select_java(&candidates, required)
        .ok()
        .map(|java| java.executable.clone())
}

fn parse_cli_loader(
    loader: &str,
    version: Option<&str>,
) -> Result<(InstanceLoader, crate::core::loader::service::InstallLoader)> {
    // `--loader fabric --loader-version 0.16.5` and `--loader fabric:0.16.5` are
    // the same request; the shared parser accepts both.
    crate::core::loader::parse_install_loader(loader, version.unwrap_or_default())
}

/// Where the path column goes when the window is too narrow for it.
enum PathColumn {
    Full,
    Truncated(usize),
    Omitted,
}

fn format_rows(rows: &[InstanceRow]) -> String {
    format_rows_with_width(rows, output::table_width())
}

/// Builds one line per instance, sized to the window.
///
/// A terminal wraps anything wider than the window, and a wrapped row reads
/// like a second, empty-path instance — that is what "the list shows every
/// instance twice" was. So the path column gives way first: it shrinks, then
/// it is dropped, and only then are the name and version columns squeezed.
///
/// `terminal` is `None` when stdout is redirected, where nothing wraps and a
/// script wants the whole path.
fn format_rows_with_width(rows: &[InstanceRow], terminal: Option<usize>) -> String {
    if rows.is_empty() {
        return "No instances found.".to_owned();
    }
    let natural_name = rows
        .iter()
        .map(|row| output::width(&row.name))
        .max()
        .unwrap_or(4);
    let natural_version = rows
        .iter()
        .map(|row| output::width(&row.minecraft_version))
        .max()
        .unwrap_or(7);
    let loader = rows
        .iter()
        .map(|row| output::width(&row.loader))
        .max()
        .unwrap_or(6);
    let longest_path = rows
        .iter()
        .map(|row| output::width(&row.path))
        .max()
        .unwrap_or(0);
    // The number column: as wide as the largest number it has to hold.
    let index_cells = rows
        .iter()
        .filter_map(|row| row.index)
        .max()
        .map(|largest| largest.to_string().len())
        .unwrap_or(0);
    let index_gap = if index_cells > 0 { index_cells + 2 } else { 0 };

    let mut name_cells = natural_name;
    let mut version_cells = natural_version;
    let mut path = PathColumn::Full;
    if let Some(columns) = terminal {
        // Four separators of two spaces, plus the ten-cell status column.
        let path_cells = columns
            .saturating_sub(index_gap + natural_name + natural_version + loader + 10 + 2 * 4);
        if longest_path > path_cells {
            if path_cells >= 24 {
                path = PathColumn::Truncated(path_cells);
            } else {
                path = PathColumn::Omitted;
                // Three separators now, and the two free-text columns share
                // whatever the fixed ones leave.
                let room = columns.saturating_sub(index_gap + loader + 10 + 2 * 3);
                version_cells = natural_version.min(room / 2);
                name_cells = room.saturating_sub(version_cells).min(natural_name);
            }
        }
    }
    rows.iter()
        .map(|row| {
            let mut line = String::new();
            if let Some(index) = row.index {
                line.push_str(&output::pad_start(&index.to_string(), index_cells));
                line.push_str("  ");
            }
            line.push_str(&format!(
                "{}  {}  {}  {}",
                output::pad(&output::truncate_end(&row.name, name_cells), name_cells),
                output::pad(
                    &output::truncate_end(&row.minecraft_version, version_cells),
                    version_cells
                ),
                output::pad(&row.loader, loader),
                output::pad(row.status, 10)
            ));
            match path {
                PathColumn::Full => {
                    line.push_str("  ");
                    line.push_str(&row.path);
                }
                PathColumn::Truncated(cells) => {
                    line.push_str("  ");
                    line.push_str(&output::truncate_start(&row.path, cells));
                }
                PathColumn::Omitted => {}
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn seconds(total: u64) -> String {
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[derive(Debug, serde::Serialize)]
struct InstanceRow {
    /// Position in the unfiltered `instance list`, which is what `@n` accepts.
    /// Absent from `instance show`, where a position means nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<usize>,
    id: Uuid,
    name: String,
    minecraft_version: String,
    loader: String,
    loader_version: Option<String>,
    status: &'static str,
    path: String,
    launch_count: u64,
    play_time_seconds: u64,
    missing_files: Option<usize>,
}

impl InstanceRow {
    fn new(instance: &ResolvedInstance, status: &'static str, missing: Option<usize>) -> Self {
        Self {
            index: None,
            id: instance.config.id,
            name: instance.config.name.clone(),
            minecraft_version: instance.config.minecraft_version.clone(),
            loader: instance.config.loader.key().to_owned(),
            loader_version: instance.config.loader.version().map(str::to_owned),
            status,
            path: instance.config.path.display().to_string(),
            launch_count: instance.config.stats.launch_count,
            play_time_seconds: instance.config.stats.play_time_seconds,
            missing_files: missing,
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct VerifyRow {
    checked: usize,
    missing: Vec<VerifyMissing>,
}

#[derive(Debug, serde::Serialize)]
struct VerifyMissing {
    path: String,
    url: String,
    sha1: Option<String>,
    reason: String,
}

#[derive(Debug, serde::Serialize)]
struct LaunchRow {
    pid: u32,
    exit_code: Option<i32>,
    log_path: PathBuf,
    crash_report: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::{InstanceRow, format_rows, format_rows_with_width};

    fn row(name: &str) -> InstanceRow {
        InstanceRow {
            index: None,
            id: uuid::Uuid::nil(),
            name: name.to_owned(),
            minecraft_version: "1.21.1".into(),
            loader: "Vanilla".into(),
            loader_version: None,
            status: "ready",
            path: format!("C:/games/versions/{name}"),
            launch_count: 0,
            play_time_seconds: 0,
            missing_files: Some(0),
        }
    }

    /// Column of the status cell, in terminal cells.
    fn status_column(line: &str) -> usize {
        let byte = line.find("ready").expect("status cell");
        crate::cli::output::width(&line[..byte])
    }

    #[test]
    fn a_cjk_name_does_not_shift_the_columns() {
        let text = format_rows(&[row("demo"), row("中文实例")]);
        let columns = text.lines().map(status_column).collect::<Vec<_>>();
        assert_eq!(columns[0], columns[1], "{text}");
    }

    #[test]
    fn the_list_number_leads_every_row() {
        let mut first = row("demo");
        first.index = Some(1);
        let mut second = row("中文实例");
        second.index = Some(2);

        let text = format_rows_with_width(&[first, second], Some(90));
        let lines = text.lines().collect::<Vec<_>>();

        assert!(lines[0].starts_with("1  demo"), "{:?}", lines[0]);
        assert!(lines[1].starts_with("2  中文实例"), "{:?}", lines[1]);
        // The number is part of the width budget, so nothing may overflow.
        for line in lines {
            assert!(crate::cli::output::width(line) <= 90, "{line}");
        }
    }

    #[test]
    fn a_row_without_a_number_has_no_number_column() {
        // `instance show` prints one instance and no position.
        let text = format_rows_with_width(&[row("demo")], Some(90));

        assert!(text.starts_with("demo"), "{text}");
    }

    #[test]
    fn a_narrow_window_truncates_the_path_instead_of_wrapping() {
        let mut long = row("FarmingTales_ForgeII v1.5.2fix2");
        long.path = r"D:\game\pcl\.minecraft\versions\FarmingTales_ForgeII v1.5.2fix2".into();

        let text = format_rows_with_width(&[long], Some(90));

        let line = text.lines().next().expect("line");
        assert!(crate::cli::output::width(line) <= 90, "{line}");
        assert!(line.contains('…'), "{line}");
        assert!(
            line.ends_with("2fix2"),
            "the instance name is what has to survive: {line}"
        );
        // The head of the path is the part the reader already knows.
        assert!(!line.contains(r"D:\000"), "{line}");
    }

    #[test]
    fn a_wide_window_keeps_the_whole_path() {
        let text = format_rows_with_width(&[row("demo")], Some(200));

        assert!(text.contains("C:/games/versions/demo"), "{text}");
        assert!(!text.contains('…'), "{text}");
    }

    #[test]
    fn a_redirected_stdout_keeps_the_whole_path() {
        // No terminal means nothing wraps, and a script may need the path.
        let text = format_rows_with_width(&[row("demo")], None);

        assert!(text.contains("C:/games/versions/demo"), "{text}");
    }

    #[test]
    fn every_row_fits_a_ninety_column_window() {
        // The shape the report had: a long name, a long version string, and a
        // path that repeats the game directory.
        let long = || {
            let mut row = row("FarmingTales_ForgeII v1.5.2fix2");
            row.minecraft_version = "1.12.2cocricot&miniaturia".into();
            row.path = r"D:\game\pcl\.minecraft\versions\FarmingTales_ForgeII v1.5.2fix2".into();
            row
        };

        for columns in [70, 90, 120] {
            let text = format_rows_with_width(&[long(), row("demo")], Some(columns));
            for line in text.lines() {
                assert!(
                    crate::cli::output::width(line) <= columns,
                    "{columns} columns: {line}"
                );
            }
        }
    }
}
