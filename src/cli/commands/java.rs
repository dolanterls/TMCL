use std::path::PathBuf;

use uuid::Uuid;

use crate::cli::args::JavaCommand;
use crate::cli::output::OutputOptions;
use crate::core::config::paths::DataPaths;
use crate::core::config::storage::write_json_atomic;
use crate::core::download::{CachePolicy, DownloadRequest, download_request, resolve_candidates};
use crate::core::error::{Error, Result};
use crate::core::java::scan::discover_default;
use crate::core::version::http::HttpClient;

use super::common::{load_config, probe_java};

pub(super) async fn run(command: JavaCommand, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = load_config(&paths)?;
    let installations = discover_default(
        &paths.runtime_dir,
        config.game_directories.clone(),
        config
            .java
            .selected_path
            .clone()
            .into_iter()
            .collect::<Vec<_>>(),
    );
    match command {
        JavaCommand::List => output.print(&installations, || format_installations(&installations)),
        JavaCommand::Scan => {
            // `scan` exists to answer "what did the launcher find", which is a
            // different question from "which runtime will a launch pick".
            let result = JavaScan {
                count: installations.len(),
                runtimes: installations
                    .iter()
                    .map(|java| JavaScanEntry {
                        major: java.major,
                        version: java.version.clone(),
                        path: crate::core::instance::path::display_path(&java.executable),
                    })
                    .collect(),
            };
            output.print(&result, || {
                format!(
                    "Found {} Java runtimes.\n{}",
                    result.count,
                    format_installations(&installations)
                )
            })
        }
        JavaCommand::Download(args) => {
            let snapshot = config.download_snapshot(&paths);
            let client = HttpClient::with_timeout(snapshot.timeout_secs)?;
            let cancelled = std::sync::atomic::AtomicBool::new(false);
            let (root, executable, files, downloaded) =
                if let Some(manifest_url) = args.manifest_url {
                    let cache_root = paths.cache_dir.clone();
                    let manifest_temp =
                        std::env::temp_dir().join(format!("tmcl-java-manifest-{}", Uuid::new_v4()));
                    let manifest_request = DownloadRequest::new(
                        resolve_candidates(
                            &manifest_url,
                            config.downloads.file_download_source,
                            snapshot.region,
                        )?,
                        &manifest_temp,
                        None,
                        None,
                        "Java runtime manifest",
                    )
                    .with_cache_policy(CachePolicy::Etag);
                    download_request(
                        &client,
                        &manifest_request,
                        &cache_root,
                        &cancelled,
                        None,
                        |_, _| Ok(()),
                    )
                    .await?;
                    let manifest: crate::core::java::runtime::RuntimeManifest =
                        serde_json::from_slice(&std::fs::read(&manifest_temp)?)?;
                    let _ = std::fs::remove_file(&manifest_temp);
                    let installed = crate::core::java::runtime::install_runtime_from_manifest(
                        &client,
                        &paths.root,
                        args.major,
                        &snapshot,
                        &manifest,
                        &cache_root,
                        &cancelled,
                        None,
                        |_, _| Ok(()),
                    )
                    .await?;
                    (
                        crate::core::java::runtime::runtime_root(&paths.root, args.major),
                        installed.executable,
                        installed.files,
                        installed.downloaded,
                    )
                } else {
                    let installed = crate::core::java::runtime::install_runtime_with_snapshot(
                        &client,
                        &paths.root,
                        args.major,
                        &snapshot,
                        &cancelled,
                        None,
                        |_, _| Ok(()),
                    )
                    .await?;
                    (
                        crate::core::java::runtime::runtime_root(&paths.root, args.major),
                        installed.executable,
                        installed.files,
                        installed.downloaded,
                    )
                };
            let java = probe_java(&executable)?;
            if java.major != args.major {
                return Err(Error::Java(format!(
                    "downloaded runtime reports Java {}, requested {}",
                    java.major, args.major
                )));
            }
            let mut config = config;
            config.java.selected_path = Some(java.executable.clone());
            write_json_atomic(&paths.config_file, &config)?;
            let result = JavaDownloadResult {
                major: args.major,
                files,
                downloaded,
                root,
            };
            output.print(&result, || {
                format!(
                    "Java {} runtime ready at {} ({} downloaded, {} total)",
                    result.major,
                    result.root.display(),
                    result.downloaded,
                    result.files
                )
            })
        }
    }
}

fn format_installations(installations: &[crate::core::java::JavaInstallation]) -> String {
    if installations.is_empty() {
        return "No Java runtimes found.".to_owned();
    }
    installations
        .iter()
        .map(|java| {
            format!(
                "Java {}  {}",
                java.major,
                crate::core::instance::path::display_path(&java.executable)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, serde::Serialize)]
struct JavaScan {
    count: usize,
    runtimes: Vec<JavaScanEntry>,
}

#[derive(Debug, serde::Serialize)]
struct JavaScanEntry {
    major: u32,
    version: String,
    path: String,
}

#[derive(Debug, serde::Serialize)]
struct JavaDownloadResult {
    major: u32,
    files: usize,
    downloaded: usize,
    root: PathBuf,
}
