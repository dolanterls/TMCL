//! Repair an instance by downloading the files its launch plan requires.
//!
//! The repair only ever adds files: it never deletes or rewrites instance
//! metadata. Downloads land where the launch expects them and are verified
//! against the SHA-1 from the merged version JSON, so a second run is a no-op.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::core::download::{DownloadRequest, download_request};
use crate::core::error::{Error, Result};
use crate::core::launch::failure::OFFLINE_MARKER;
use crate::core::launch::repair;
use crate::core::loader::client_json::parse;
use crate::core::task::{DownloadTaskControl, InstanceRepairSpec, wait_if_paused};
use crate::core::version::http::HttpClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RepairSummary {
    pub downloaded: usize,
    pub skipped: usize,
}

/// A single repair task stops before this budget and asks the user to verify the
/// instance instead: a broken install can otherwise queue thousands of files.
const MAX_REPAIR_FILES: usize = 800;
const MAX_REPAIR_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Downloads everything `spec.missing` names that has a proven URL.
pub async fn repair<F>(
    spec: &InstanceRepairSpec,
    control: Option<&dyn DownloadTaskControl>,
    cancelled: &AtomicBool,
    mut progress: F,
) -> Result<RepairSummary>
where
    F: FnMut(u64, Option<u64>) -> Result<()>,
{
    let client = HttpClient::with_timeout(spec.download_snapshot.timeout_secs)?;
    let metadata_path = client_json_path(&spec.instance_directory, &spec.instance_name);
    let metadata = std::fs::read(&metadata_path).map_err(|error| {
        Error::Launch(format!(
            "read instance JSON {}: {error}",
            metadata_path.display()
        ))
    })?;
    let client_json = parse(&metadata)?.resolve_inheritance(&spec.game_directory)?;
    let plan = repair::requirements(&spec.missing, &client_json);
    if plan.downloads.is_empty() {
        if plan.skipped > 0 {
            let sample = plan
                .skipped_paths
                .iter()
                .take(3)
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let more = if plan.skipped > 3 { " …" } else { "" };
            return Err(Error::Launch(format!(
                "{} file(s) cannot be downloaded automatically ({sample}{more}); verify the instance",
                plan.skipped
            )));
        }
        progress(1, Some(1))?;
        return Ok(RepairSummary::default());
    }
    if plan.downloads.len() > MAX_REPAIR_FILES {
        return Err(Error::Launch(format!(
            "repair would download {} files (limit {MAX_REPAIR_FILES}); verify the instance instead",
            plan.downloads.len()
        )));
    }
    let known_bytes = plan
        .downloads
        .iter()
        .filter_map(|requirement| requirement.artifact.size)
        .sum::<u64>();
    if known_bytes > MAX_REPAIR_BYTES {
        return Err(Error::Launch(format!(
            "repair would download {known_bytes} bytes (limit {MAX_REPAIR_BYTES}); verify the instance instead"
        )));
    }

    let total = u64::try_from(plan.downloads.len()).unwrap_or(u64::MAX);
    let mut summary = RepairSummary {
        skipped: plan.skipped,
        ..RepairSummary::default()
    };
    for (index, requirement) in plan.downloads.iter().enumerate() {
        wait_if_paused(control, cancelled).await?;
        let target = &requirement.artifact.path;
        // Repair only adds files: an existing jar is left alone, which makes a
        // second run cheap and keeps user-modified files untouched.
        if target.is_file() {
            summary.skipped += 1;
            progress(u64::try_from(index).unwrap_or(0) + 1, Some(total))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let request = DownloadRequest::new(
            vec![requirement.artifact.url.clone()],
            target.clone(),
            requirement.artifact.sha1.clone(),
            requirement.artifact.size,
            requirement.reason.clone(),
        )
        .with_policy(spec.download_snapshot.clone());
        match download_request(
            &client,
            &request,
            &spec.cache_root,
            cancelled,
            control,
            |_, _| Ok(()),
        )
        .await
        {
            Ok(_) => {}
            Err(error)
                if error.is_retryable()
                    && matches!(error, Error::Network(_) | Error::HttpStatus { .. }) =>
            {
                // Distinguish a connectivity problem from broken metadata: the
                // user can fix this by connecting once, not by verifying files.
                return Err(Error::Launch(format!(
                    "{OFFLINE_MARKER} cannot download {}: {error}; connect once to complete the instance",
                    target.display()
                )));
            }
            Err(error) => return Err(error),
        }
        summary.downloaded += 1;
        progress(u64::try_from(index).unwrap_or(0) + 1, Some(total))?;
    }
    Ok(summary)
}

/// `<versions>/<name>/<name>.json`, the layout every launcher writes.
fn client_json_path(instance_directory: &Path, instance_name: &str) -> PathBuf {
    instance_directory.join(format!("{instance_name}.json"))
}
