use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::{StreamExt, stream::FuturesUnordered};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::download::{
    CachePolicy, DownloadRequest, DownloadSourceSnapshot, IntegrityKind, download_request,
    resolve_candidates, verify_download,
};
use crate::core::error::{Error, Result};
use crate::core::task::{DownloadTaskControl, ScopedDownloadTaskControl, wait_if_paused};
use crate::core::version::http::HttpClient;
use std::sync::atomic::AtomicBool;

pub const RUNTIME_CATALOG_URL: &str = "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeManifest {
    pub files: BTreeMap<String, RuntimeFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeFile {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub downloads: RuntimeDownloads,
    #[serde(default)]
    pub executable: bool,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RuntimeDownloads {
    #[serde(default)]
    pub raw: Option<RuntimeArtifact>,
    #[serde(default)]
    pub lzma: Option<RuntimeArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RuntimeArtifact {
    pub url: String,
    pub sha1: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDownload {
    pub path: PathBuf,
    pub raw: Option<RuntimeArtifact>,
    pub compressed: Option<RuntimeArtifact>,
    pub executable: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeCatalog {
    #[serde(flatten)]
    pub platforms: HashMap<String, HashMap<String, Vec<RuntimeCatalogEntry>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeCatalogEntry {
    pub manifest: RuntimeArtifact,
    #[serde(default)]
    pub version: RuntimeVersion,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RuntimeVersion {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInstallResult {
    pub executable: PathBuf,
    pub files: usize,
    pub downloaded: usize,
}

pub fn runtime_root(data_root: &Path, major: u32) -> PathBuf {
    data_root.join(format!("runtime/java-{major}"))
}

pub fn runtime_component(major: u32) -> &'static str {
    match major {
        0..=8 => "jre-legacy",
        9..=16 => "java-runtime-alpha",
        17..=20 => "java-runtime-beta",
        21..=24 => "java-runtime-delta",
        _ => "java-runtime-epsilon",
    }
}

pub fn runtime_platform() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("windows-x64"),
        ("windows", "x86") => Ok("windows-x86"),
        ("windows", "aarch64") => Ok("windows-arm64"),
        ("macos", "x86_64") => Ok("mac-os"),
        ("macos", "aarch64") => Ok("mac-os-arm64"),
        ("linux", "x86_64") => Ok("linux"),
        ("linux", "x86") => Ok("linux-i386"),
        ("linux", "aarch64") => Ok("linux"),
        (os, arch) => Err(Error::Java(format!(
            "Mojang Java runtimes do not support {os}/{arch}"
        ))),
    }
}

pub fn catalog_manifest_url(
    catalog: &RuntimeCatalog,
    major: u32,
    platform: &str,
) -> Result<String> {
    catalog_entry(catalog, major, platform).map(|entry| entry.manifest.url.clone())
}

fn catalog_entry<'a>(
    catalog: &'a RuntimeCatalog,
    major: u32,
    platform: &str,
) -> Result<&'a RuntimeCatalogEntry> {
    let component = runtime_component(major);
    catalog
        .platforms
        .get(platform)
        .and_then(|components| components.get(component))
        .and_then(|versions| {
            versions
                .iter()
                .find(|entry| {
                    runtime_version_major(&entry.version.name).is_some_and(|value| value >= major)
                })
                .or_else(|| versions.first())
        })
        .ok_or_else(|| Error::Java(format!("Mojang has no {component} runtime for {platform}")))
}

fn runtime_version_major(value: &str) -> Option<u32> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse().ok())
}

pub fn download_plan(
    data_root: &Path,
    major: u32,
    manifest: &RuntimeManifest,
) -> Result<Vec<RuntimeDownload>> {
    let root = runtime_root(data_root, major);
    manifest
        .files
        .iter()
        .filter(|(_, file)| file.kind == "file")
        .map(|(relative, file)| {
            let relative_path = safe_relative_path(relative)?;
            if file.downloads.raw.is_none() && file.downloads.lzma.is_none() {
                return Err(Error::Java(format!(
                    "runtime file has no raw or lzma download: {relative}"
                )));
            }
            if let Some(artifact) = file.downloads.raw.as_ref() {
                validate_runtime_url(&artifact.url)?;
            }
            if let Some(artifact) = file.downloads.lzma.as_ref() {
                validate_runtime_url(&artifact.url)?;
            }
            Ok(RuntimeDownload {
                path: root.join(relative_path),
                raw: file.downloads.raw.clone(),
                compressed: file.downloads.lzma.clone(),
                executable: file.executable,
            })
        })
        .collect()
}

pub async fn install_runtime<F>(
    client: &HttpClient,
    data_root: &Path,
    major: u32,
    cancelled: &AtomicBool,
    progress: F,
) -> Result<RuntimeInstallResult>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    install_runtime_with_control(client, data_root, major, cancelled, None, progress).await
}

pub async fn install_runtime_with_control<F>(
    client: &HttpClient,
    data_root: &Path,
    major: u32,
    cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    progress: F,
) -> Result<RuntimeInstallResult>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    install_runtime_with_snapshot(
        client,
        data_root,
        major,
        &DownloadSourceSnapshot::default(),
        cancelled,
        control,
        progress,
    )
    .await
}

pub async fn install_runtime_with_snapshot<F>(
    client: &HttpClient,
    data_root: &Path,
    major: u32,
    snapshot: &DownloadSourceSnapshot,
    cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    progress: F,
) -> Result<RuntimeInstallResult>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    wait_if_paused(control, cancelled).await?;
    let platform = runtime_platform()?;
    let cache_root = crate::core::config::DataPaths::discover()
        .map(|paths| paths.cache_dir)
        .unwrap_or_else(|_| data_root.join(".tmcl-cache"));
    let catalog: RuntimeCatalog = fetch_json_candidates(
        client,
        RUNTIME_CATALOG_URL,
        snapshot,
        &cache_root,
        "Java runtime catalog",
    )
    .await?;
    let manifest_url = catalog_manifest_url(&catalog, major, platform)?;
    validate_runtime_url(&manifest_url)?;
    let manifest_entry = catalog_entry(&catalog, major, platform)?;
    let manifest: RuntimeManifest = fetch_json_candidates_with_integrity(
        client,
        &manifest_url,
        snapshot,
        &cache_root,
        "Java runtime manifest",
        Some(&manifest_entry.manifest.sha1),
        manifest_entry.manifest.size,
    )
    .await?;
    install_runtime_from_manifest(
        client,
        data_root,
        major,
        snapshot,
        &manifest,
        &cache_root,
        cancelled,
        control,
        progress,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn install_runtime_from_manifest<F>(
    client: &HttpClient,
    data_root: &Path,
    major: u32,
    snapshot: &DownloadSourceSnapshot,
    manifest: &RuntimeManifest,
    cache_root: &Path,
    cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    mut progress: F,
) -> Result<RuntimeInstallResult>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    wait_if_paused(control, cancelled).await?;
    let root = runtime_root(data_root, major);
    let plan = download_plan(data_root, major, manifest)?;
    let scoped_control =
        control.map(|control| ScopedDownloadTaskControl::new(control, root.clone()));
    let total = plan
        .iter()
        .filter_map(runtime_download_size)
        .try_fold(0_u64, |sum, size| Some(sum.saturating_add(size)));
    std::fs::create_dir_all(&root)?;
    for (relative, file) in &manifest.files {
        let relative = safe_relative_path(relative)?;
        let path = root.join(relative);
        match file.kind.as_str() {
            "directory" => std::fs::create_dir_all(path)?,
            "file" => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            "link" => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            other => return Err(Error::Java(format!("unknown runtime entry type {other:?}"))),
        }
    }
    let completed = Arc::new(AtomicU64::new(0));
    let downloaded = Arc::new(AtomicU64::new(0));
    let progress_callback = Arc::new(Mutex::new(&mut progress));
    let mut in_flight = FuturesUnordered::new();
    for file in plan.iter().cloned() {
        wait_if_paused(control, cancelled).await?;
        if cancelled.load(Ordering::Relaxed) {
            return Err(Error::Download(
                "Java runtime installation cancelled".into(),
            ));
        }
        if let Some(raw) = file.raw.as_ref()
            && file.path.is_file()
            && verify_download(
                &file.path,
                Some(&raw.sha1),
                raw.size,
                IntegrityKind::JavaRaw,
            )
            .is_ok()
        {
            if let Some(control) = scoped_control.as_ref() {
                control.record_file_reference(&file.path)?;
            }
            completed.fetch_add(runtime_download_size(&file).unwrap_or(0), Ordering::Relaxed);
            continue;
        }
        let client = client.clone();
        let snapshot = snapshot.clone();
        let control = scoped_control
            .as_ref()
            .map(|value| value as &dyn DownloadTaskControl);
        let completed = Arc::clone(&completed);
        let downloaded = Arc::clone(&downloaded);
        let progress_callback = Arc::clone(&progress_callback);
        in_flight.push(async move {
            let compressed_path = if let Some(compressed) = &file.compressed {
                let path = std::env::temp_dir().join(format!("tmcl-java-{}.lzma", Uuid::new_v4()));
                let request = DownloadRequest::new(
                    resolve_candidates(
                        &compressed.url,
                        snapshot.file_download_source,
                        snapshot.region,
                    )?,
                    &path,
                    Some(compressed.sha1.clone()),
                    compressed.size,
                    format!("Java compressed {}", file.path.display()),
                )
                .with_integrity(IntegrityKind::JavaCompressed)
                .with_cache_policy(CachePolicy::ContentAddressed)
                .with_policy(snapshot.clone());
                download_request(
                    &client,
                    &request,
                    cache_root,
                    cancelled,
                    control,
                    |current, _| {
                        report_runtime_progress(&completed, &progress_callback, current, total)
                    },
                )
                .await?;
                Some(path)
            } else if let Some(raw) = &file.raw {
                let request = DownloadRequest::new(
                    resolve_candidates(&raw.url, snapshot.file_download_source, snapshot.region)?,
                    &file.path,
                    Some(raw.sha1.clone()),
                    raw.size,
                    format!("Java {}", file.path.display()),
                )
                .with_integrity(IntegrityKind::JavaRaw)
                .with_cache_policy(CachePolicy::ContentAddressed)
                .with_policy(snapshot.clone());
                download_request(
                    &client,
                    &request,
                    cache_root,
                    cancelled,
                    control,
                    |current, _| {
                        report_runtime_progress(&completed, &progress_callback, current, total)
                    },
                )
                .await?;
                None
            } else {
                return Err(Error::Java(format!(
                    "runtime file has no usable download: {}",
                    file.path.display()
                )));
            };
            if let Some(compressed_path) = compressed_path {
                decompress_lzma(&compressed_path, &file.path)?;
                std::fs::remove_file(&compressed_path)?;
                if let Some(raw) = &file.raw {
                    verify_download(
                        &file.path,
                        Some(&raw.sha1),
                        raw.size,
                        IntegrityKind::JavaRaw,
                    )?;
                }
            }
            if file.executable {
                set_executable(&file.path)?;
            }
            downloaded.fetch_add(1, Ordering::Relaxed);
            Ok::<(), Error>(())
        });
    }
    while let Some(result) = in_flight.next().await {
        result?;
    }
    drop(in_flight);
    drop(progress_callback);
    let completed_value = completed.load(Ordering::Relaxed);
    progress(completed_value, total)?;
    for (relative, file) in &manifest.files {
        if file.kind != "link" {
            continue;
        }
        let link_path = root.join(safe_relative_path(relative)?);
        let target = root.join(safe_relative_path(file.target.as_deref().ok_or_else(
            || Error::Java(format!("runtime link {relative:?} has no target")),
        )?)?);
        if !target.is_file() {
            return Err(Error::Java(format!(
                "runtime link target is missing: {}",
                target.display()
            )));
        }
        #[cfg(windows)]
        std::fs::copy(target, link_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            if link_path.exists() {
                std::fs::remove_file(&link_path)?;
            }
            symlink(target, link_path)?;
        }
    }
    let java = root
        .join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" });
    if !java.is_file() {
        return Err(Error::Java(format!(
            "downloaded runtime has no Java executable at {}",
            java.display()
        )));
    }
    Ok(RuntimeInstallResult {
        executable: java,
        files: plan.len(),
        downloaded: downloaded.load(Ordering::Relaxed) as usize,
    })
}

fn runtime_download_size(file: &RuntimeDownload) -> Option<u64> {
    file.compressed
        .as_ref()
        .and_then(|artifact| artifact.size)
        .or_else(|| file.raw.as_ref().and_then(|artifact| artifact.size))
}

async fn fetch_json_candidates<T: serde::de::DeserializeOwned>(
    client: &HttpClient,
    url: &str,
    snapshot: &DownloadSourceSnapshot,
    cache_root: &Path,
    label: &str,
) -> Result<T> {
    fetch_json_candidates_with_integrity(client, url, snapshot, cache_root, label, None, None).await
}

async fn fetch_json_candidates_with_integrity<T: serde::de::DeserializeOwned>(
    client: &HttpClient,
    url: &str,
    snapshot: &DownloadSourceSnapshot,
    cache_root: &Path,
    label: &str,
    expected_sha1: Option<&String>,
    expected_size: Option<u64>,
) -> Result<T> {
    let candidates = resolve_candidates(url, snapshot.file_download_source, snapshot.region)?;
    let mut errors = Vec::new();
    for candidate in candidates {
        let temporary = std::env::temp_dir().join(format!("tmcl-metadata-{}", Uuid::new_v4()));
        let request = DownloadRequest::new(
            vec![candidate.clone()],
            &temporary,
            expected_sha1.cloned(),
            expected_size,
            label,
        )
        .with_cache_policy(if expected_sha1.is_some() {
            CachePolicy::ContentAddressed
        } else {
            CachePolicy::Etag
        })
        .with_policy(snapshot.clone());
        match download_request(
            client,
            &request,
            cache_root,
            &AtomicBool::new(false),
            None,
            |_, _| Ok(()),
        )
        .await
        {
            Ok(_) => {
                let bytes = std::fs::read(&temporary)?;
                let _ = std::fs::remove_file(&temporary);
                match serde_json::from_slice(&bytes) {
                    Ok(value) => return Ok(value),
                    Err(error) => errors.push(format!("{candidate}: parse error: {error}")),
                }
            }
            Err(error) => {
                let _ = std::fs::remove_file(&temporary);
                errors.push(format!("{candidate}: {error}"));
            }
        }
    }
    Err(Error::Network(format!(
        "{label} failed for all candidates: {}",
        errors.join("; ")
    )))
}

fn report_runtime_progress<F>(
    completed: &AtomicU64,
    progress: &Mutex<&mut F>,
    current: u64,
    total: Option<u64>,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    loop {
        let previous = completed.load(Ordering::Relaxed);
        let next = previous.max(current);
        if next == previous {
            return Ok(());
        }
        if completed
            .compare_exchange(previous, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            progress
                .lock()
                .map_err(|_| Error::Download("runtime progress lock poisoned".into()))?(
                next, total,
            )?;
            return Ok(());
        }
    }
}

fn decompress_lzma(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = destination.with_file_name(format!(
        ".{}.tmp-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("java"),
        Uuid::new_v4()
    ));
    let result = (|| {
        let input = std::fs::File::open(source)?;
        let mut input = std::io::BufReader::new(input);
        let mut output = std::fs::File::create(&temporary)?;
        lzma_rs::lzma_decompress(&mut input, &mut output)
            .map_err(|error| Error::Download(format!("decompress Java runtime lzma: {error}")))?;
        use std::io::Write;
        output.flush()?;
        output.sync_all()?;
        crate::core::config::storage::replace_path(&temporary, destination)?;
        Ok::<(), Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn safe_relative_path(path: &str) -> Result<PathBuf> {
    let relative = Path::new(path);
    if relative.is_absolute()
        || path.contains('\\')
        || path.contains(':')
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(Error::Java(format!("unsafe runtime path: {path}")));
    }
    Ok(relative.to_path_buf())
}

fn validate_runtime_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| Error::Java("invalid Java runtime download URL".into()))?;
    let loopback = url.scheme() == "http"
        && url
            .host_str()
            .is_some_and(|host| matches!(host, "127.0.0.1" | "localhost" | "::1"));
    if url.scheme() != "https" && !loopback {
        return Err(Error::Java(
            "Java runtime URLs must use HTTPS (or loopback HTTP)".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        RuntimeCatalog, RuntimeManifest, catalog_manifest_url, download_plan, runtime_component,
        runtime_root,
    };

    #[test]
    fn runtime_plan_stays_under_major_specific_root() {
        let manifest: RuntimeManifest = serde_json::from_slice(
            br#"{"files":{"bin/java.exe":{"type":"file","downloads":{"raw":{"url":"https://example.test/java.exe","sha1":"abc"}}}}}"#,
        )
        .expect("runtime manifest");
        let root = std::path::Path::new("C:/TMCL");
        let plan = download_plan(root, 21, &manifest).expect("runtime plan");
        assert_eq!(plan[0].path, runtime_root(root, 21).join("bin/java.exe"));
        assert!(
            download_plan(
                root,
                21,
                &serde_json::from_str(r#"{"files":{"../escape":{"type":"file"}}}"#)
                    .expect("unsafe manifest")
            )
            .is_err()
        );
    }

    #[test]
    fn selects_the_official_runtime_component_for_each_major_band() {
        assert_eq!(runtime_component(8), "jre-legacy");
        assert_eq!(runtime_component(17), "java-runtime-beta");
        assert_eq!(runtime_component(21), "java-runtime-delta");
        assert_eq!(runtime_component(25), "java-runtime-epsilon");
        let catalog: RuntimeCatalog = serde_json::from_str(r#"{
          "windows-x64":{"java-runtime-delta":[{"manifest":{"url":"https://example.test/manifest.json","sha1":"abc"},"version":{"name":"21.0.7"}}]}
        }"#).expect("runtime catalog");
        assert_eq!(
            catalog_manifest_url(&catalog, 21, "windows-x64").unwrap(),
            "https://example.test/manifest.json"
        );
    }
}
