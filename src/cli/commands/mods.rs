//! `mod`: the two commands that talk to Modrinth about installed jars.
//!
//! Kept apart from `content` because they are the only ones that need a
//! network: a check is a hash lookup, and an update is that lookup plus a
//! download task.

use serde::Serialize;

use crate::cli::args::{ModCheckArgs, ModCommand, ModUpdateArgs};
use crate::cli::consent::{Consent, Decision, decide, require};
use crate::cli::output::OutputOptions;
use crate::core::config::paths::DataPaths;
use crate::core::content::trash;
use crate::core::content::update::ModUpdate;
use crate::core::error::{Error, Result};
use crate::core::instance::resolve::ResolvedInstance;
use crate::core::resource::model::ResourceKind;

use super::common;

pub(super) async fn run(command: ModCommand, output: OutputOptions) -> Result<()> {
    match command {
        ModCommand::CheckUpdates(args) => check_updates(args, output).await,
        ModCommand::Update(args) => update(args, output).await,
    }
}

#[derive(Debug, Serialize)]
struct UpdateRow {
    name: String,
    current: String,
    latest: String,
    sha1: Option<String>,
}

impl UpdateRow {
    fn new(update: &ModUpdate) -> Self {
        Self {
            name: update.file_name.clone(),
            current: update.current_version.clone(),
            latest: update.new_version.clone(),
            sha1: update.file.sha1.clone(),
        }
    }
}

async fn check_updates(args: ModCheckArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(args.reference.as_deref(), &config)?;
    let report = mod_report(&instance).await?;
    let rows = report
        .updates
        .iter()
        .map(UpdateRow::new)
        .collect::<Vec<_>>();

    output.print(&rows, || {
        if rows.is_empty() {
            return format!(
                "No updates ({} checked, {} unknown to Modrinth).",
                report.checked, report.unknown
            );
        }
        let mut text = rows
            .iter()
            .map(|row| {
                format!(
                    "{:<36}  {} -> {}",
                    row.name,
                    if row.current.is_empty() {
                        "?"
                    } else {
                        row.current.as_str()
                    },
                    row.latest
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        text.push_str(&format!(
            "\n\n{} checked, {} unknown to Modrinth, {} update(s).",
            report.checked,
            report.unknown,
            rows.len()
        ));
        text
    })
}

async fn mod_report(
    instance: &ResolvedInstance,
) -> Result<crate::core::content::update::UpdateReport> {
    let client = crate::core::version::http::HttpClient::with_timeout(60)?;
    let loader = crate::core::resource::compatibility::loader_filter(&instance.config.loader);
    crate::core::content::update::check_modrinth(
        &client,
        &instance.config.path.join("mods"),
        &instance.config.minecraft_version,
        loader,
    )
    .await
}

async fn update(args: ModUpdateArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    paths.ensure_dirs()?;
    let config = common::load_config(&paths)?;
    let instance = common::instance(args.reference.as_deref(), &config)?;
    let (_, name) = common::ref_and_name(
        args.reference,
        args.name,
        "name the mod to update: `tmcl mod update [REF] <NAME>`",
    )?;

    if output.dry_run {
        let plan = format!(
            "Would replace {name} with the newest version Modrinth reports for {} ({}).",
            instance.config.minecraft_version,
            instance.config.loader.key()
        );
        return output.print(
            &serde_json::json!({"plan": plan, "applied": false, "name": name}),
            || plan.clone(),
        );
    }

    let consent = Consent::new(args.yes, &output);
    let action = format!("update {name}");
    // Refuse before the lookup when nobody can be asked: which version is
    // available does not change whether the command may proceed, and a hash
    // lookup is still a network request.
    if !consent.interactive && !consent.yes {
        decide(&consent, &action, None)?;
    }

    let report = mod_report(&instance).await?;
    let found = report
        .updates
        .iter()
        .find(|update| update.file_name.eq_ignore_ascii_case(&name))
        .ok_or_else(|| {
            Error::Download(format!(
                "no update for {name} ({} checked, {} unknown to Modrinth)",
                report.checked, report.unknown
            ))
        })?;

    let plan = format!(
        "Will replace {} ({}) with {} ({}); the old jar goes to the trash.",
        found.file_name,
        if found.current_version.is_empty() {
            "unknown version"
        } else {
            found.current_version.as_str()
        },
        found.new_version,
        found.file.name
    );
    if require(&consent, &action, || plan.clone())? == Decision::Dry {
        return output.print(
            &serde_json::json!({"plan": plan, "applied": false, "name": name}),
            || plan.clone(),
        );
    }

    // Old jar to the trash first: if the download fails the previous version is
    // one `trash restore` away, which is how the workspace does it too.
    let root = instance.config.path.clone();
    trash::move_to_trash(&root, &found.path)?;
    // The new file keeps the name the destination gives it (colour codes in a
    // publisher's file name are dropped there), so record that name.
    let file_name = crate::core::resource::conflict::sanitize_file_name(&found.file.name);
    let destination = crate::core::resource::compatibility::resource_destination(
        &instance.config,
        "mods",
        &file_name,
    )
    .map_err(Error::Download)?;
    let operation =
        crate::core::task::TaskOperation::ResourceInstall(crate::core::task::ResourceInstallSpec {
            instance_id: instance.config.id,
            instance_root: crate::core::resource::compatibility::content_root(&instance.config),
            destination: destination.clone(),
            cache_root: paths.cache_dir.clone(),
            source: found.file.source,
            kind: ResourceKind::Mod,
            project_id: found.file.project_id.clone(),
            file_id: found.file.file_id.clone(),
            file_name,
            candidate_urls: vec![found.file.download_url.clone()],
            expected_sha1: found.file.sha1.clone(),
            expected_size: found.file.size,
            download_snapshot: config.download_snapshot(&paths),
        });
    let outcome = common::run_task(&paths, operation, &output).await?;
    if let crate::core::task::TaskOutcome::Failed { message, .. }
    | crate::core::task::TaskOutcome::Cancelled { message, .. } = outcome
    {
        return Err(Error::Download(message));
    }

    output.print(
        &serde_json::json!({
            "name": found.file_name,
            "previous": found.current_version,
            "installed": found.new_version,
            "path": destination,
        }),
        || format!("Updated {} to {}", found.file_name, found.new_version),
    )
}
