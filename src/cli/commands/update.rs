//! `update check` and `update apply`.
//!
//! The endpoint is not compiled in: it has to come from `--url` or
//! `TMCL_UPDATE_URL`, the same rule `update-check` has always had. A launcher
//! that phones home without being told where is a launcher that surprises
//! people.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::cli::args::{UpdateApplyArgs, UpdateCheckArgs, UpdateCommand};
use crate::cli::consent::{Consent, Decision, require};
use crate::cli::output::OutputOptions;
use crate::core::error::{Error, Result};
use crate::core::version::http::HttpClient;
use crate::core::version::update::{self, platform_asset};

use super::common;

pub(super) async fn run(command: UpdateCommand, output: OutputOptions) -> Result<()> {
    match command {
        UpdateCommand::Check(args) => check(args, output).await,
        UpdateCommand::Apply(args) => apply(args, output).await,
    }
}

/// `update-check` still works; it says so once and behaves like `update check`.
pub(super) async fn run_deprecated(args: UpdateCheckArgs, output: OutputOptions) -> Result<()> {
    eprintln!("warning: `update-check` is now `update check`; the old name will be removed");
    check(args, output).await
}

fn endpoint(url: Option<String>) -> Result<String> {
    url.or_else(|| std::env::var("TMCL_UPDATE_URL").ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::Version("this command needs --url or TMCL_UPDATE_URL".into()))
}

async fn check(args: UpdateCheckArgs, output: OutputOptions) -> Result<()> {
    let endpoint = endpoint(args.url)?;
    let info = update::check_latest(&HttpClient::new()?, &endpoint).await?;
    output.print(&info, || {
        format!(
            "Latest TMCL {}: {} ({} asset(s))",
            info.version,
            info.page_url,
            info.assets.len()
        )
    })
}

async fn apply(args: UpdateApplyArgs, output: OutputOptions) -> Result<()> {
    let endpoint = endpoint(args.url)?;
    let info = update::check_latest(&HttpClient::new()?, &endpoint).await?;
    let current = std::env::current_exe()
        .map_err(|error| Error::Version(format!("cannot locate this executable: {error}")))?;
    let installed = env!("CARGO_PKG_VERSION");
    let latest = info.version.trim_start_matches('v');

    if latest == installed {
        return output.print(
            &serde_json::json!({
                "current": installed,
                "latest": info.version,
                "updated": false,
            }),
            || format!("Already up to date ({installed})."),
        );
    }

    let asset = platform_asset(&info.assets).ok_or_else(|| {
        Error::Version(format!(
            "release {} ships no build for {} {}; assets: {}",
            info.version,
            std::env::consts::OS,
            std::env::consts::ARCH,
            if info.assets.is_empty() {
                "(none)".to_owned()
            } else {
                info.assets
                    .iter()
                    .map(|asset| asset.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ))
    })?;
    if update::is_archive(&asset.name) {
        return Err(Error::Version(format!(
            "release {} ships {} — an archive; download it from {} and unpack it yourself",
            info.version, asset.name, info.page_url
        )));
    }

    let plan = format!(
        "Will download {} ({}) and replace {}.",
        asset.name,
        asset
            .size
            .map(common::human_size)
            .unwrap_or_else(|| "unknown size".to_owned()),
        current.display()
    );
    if output.dry_run {
        return output.print(
            &serde_json::json!({
                "plan": plan,
                "applied": false,
                "current": installed,
                "latest": info.version,
                "asset": asset.name,
                "target": current,
            }),
            || plan.clone(),
        );
    }

    let consent = Consent::new(args.yes, &output);
    if require(&consent, "replace this executable", || plan.clone())? == Decision::Dry {
        return output.print(
            &serde_json::json!({"plan": plan, "applied": false, "target": current}),
            || plan.clone(),
        );
    }

    // Staged beside the executable: a rename on the same volume is the only
    // swap Windows allows on a running image.
    let staged = current.with_extension("new");
    if staged.exists() {
        std::fs::remove_file(&staged)?;
    }
    download(asset, &staged, &output).await?;
    let backup = update::replace_executable(&current, &staged)?;

    output.print(
        &serde_json::json!({
            "current": installed,
            "latest": info.version,
            "updated": true,
            "backup": backup,
        }),
        || {
            format!(
                "Updated to {}; restart to use it. The previous executable is at {}",
                info.version,
                backup.display()
            )
        },
    )
}

async fn download(
    asset: &update::UpdateAsset,
    destination: &Path,
    output: &OutputOptions,
) -> Result<()> {
    use crate::core::download::{CachePolicy, DownloadRequest, download_request};

    let request = DownloadRequest::new(
        vec![asset.url.clone()],
        destination,
        None,
        asset.size,
        asset.name.clone(),
    )
    // A release binary is not worth keeping a second copy of.
    .with_cache_policy(CachePolicy::None)
    .without_replacing();
    let client = HttpClient::new()?;
    let cancelled = AtomicBool::new(false);
    let mut progress = common::DownloadProgress::new(output);
    let result = download_request(
        &client,
        &request,
        &std::env::temp_dir(),
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
