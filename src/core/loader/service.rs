use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

use futures_util::{StreamExt, stream::FuturesUnordered};
use serde_json::Value;

use crate::core::download::{
    CachePolicy, DownloadRequest, DownloadSource, DownloadSourceSnapshot, IntegrityKind, Resource,
    download_request, resolve_candidates, verify_sha1,
};
use crate::core::error::{Error, Result};
use crate::core::instance::assets::{AssetIndex, asset_requirements};
use crate::core::launch::command::launch_features;
use crate::core::launch::validate::FileArtifact;
pub use crate::core::loader::LoaderKind;
use crate::core::loader::archive::{extract_prefix, read_entry};
use crate::core::loader::client_json::{
    ClientJson, parse as parse_client_json, safe_version_id, version_json_path,
};
use crate::core::loader::forge::{ForgeProfile, parse_profile};
use crate::core::task::{DownloadTaskControl, ScopedDownloadTaskControl, wait_if_paused};
use crate::core::version::http::HttpClient;
use crate::core::version::models::{VersionManifest, VersionSummary};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallLoader {
    Vanilla,
    Fabric { version: String },
    Quilt { version: String },
    Forge { version: String },
    NeoForge { version: String },
}

#[derive(Debug, Clone)]
pub struct InstallRequest {
    pub game_directory: PathBuf,
    pub instance_directory: PathBuf,
    pub instance_name: String,
    pub minecraft_version: String,
    pub loader: InstallLoader,
    pub download_snapshot: DownloadSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallResult {
    pub client_json: PathBuf,
    pub client_jar: PathBuf,
    pub main_class: String,
    pub downloaded_files: usize,
}

#[derive(Debug, Clone)]
struct InstallArtifact {
    file: FileArtifact,
    reason: String,
}

pub fn installer_name(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Vanilla => "vanilla",
        LoaderKind::Fabric => "fabric",
        LoaderKind::Quilt => "quilt",
        LoaderKind::Forge => "forge",
        LoaderKind::NeoForge => "neoforge",
    }
}

/// Resolve current Mojang version metadata, construct loader-specific metadata,
/// download and verify the complete client payload, then commit the instance JSON.
pub async fn install<F>(
    client: &HttpClient,
    request: &InstallRequest,
    java_executable: Option<&Path>,
    cancelled: &AtomicBool,
    progress: F,
) -> Result<InstallResult>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    install_with_control(client, request, java_executable, cancelled, None, progress).await
}

pub async fn install_with_control<F>(
    client: &HttpClient,
    request: &InstallRequest,
    java_executable: Option<&Path>,
    cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    mut progress: F,
) -> Result<InstallResult>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    validate_request(request)?;
    wait_if_paused(control, cancelled).await?;
    let manifest = fetch_version_manifest(client, &request.download_snapshot).await?;
    let version = manifest
        .versions
        .iter()
        .find(|version| version.id == request.minecraft_version)
        .ok_or_else(|| {
            Error::Version(format!(
                "Minecraft version not found in manifest: {}",
                request.minecraft_version
            ))
        })?;
    let base_bytes = fetch_version_json(
        client,
        version,
        &request.minecraft_version,
        &request.download_snapshot,
    )
    .await?;
    install_from_metadata(
        client,
        request,
        &base_bytes,
        java_executable,
        cancelled,
        control,
        &mut progress,
    )
    .await
}

async fn fetch_version_json(
    client: &HttpClient,
    version: &VersionSummary,
    version_id: &str,
    snapshot: &DownloadSourceSnapshot,
) -> Result<Vec<u8>> {
    let urls = resolve_candidates(&version.url, snapshot.version_list_source, snapshot.region)?;
    let mut errors = Vec::new();
    for url in urls {
        match fetch_candidate_bytes(client, &url, None, None, "Minecraft version JSON", snapshot)
            .await
        {
            Ok(bytes) => match serde_json::from_slice::<ClientJson>(&bytes) {
                Ok(_) => return Ok(bytes),
                Err(error) => errors.push(format!("{url}: parse error: {error}")),
            },
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }
    Err(Error::Network(format!(
        "unable to fetch Minecraft {version_id} metadata: {}",
        errors.join("; ")
    )))
}

async fn fetch_version_manifest(
    client: &HttpClient,
    snapshot: &DownloadSourceSnapshot,
) -> Result<VersionManifest> {
    let official = DownloadSource::Official.url(&Resource::VersionManifest);
    let urls = resolve_candidates(&official, snapshot.version_list_source, snapshot.region)?;
    let mut errors = Vec::new();
    for url in urls {
        match fetch_candidate_bytes(
            client,
            &url,
            None,
            None,
            "Minecraft version manifest",
            snapshot,
        )
        .await
        {
            Ok(bytes) => match serde_json::from_slice::<VersionManifest>(&bytes) {
                Ok(manifest) => return Ok(manifest),
                Err(error) => errors.push(format!("{url}: parse error: {error}")),
            },
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }
    Err(Error::Network(format!(
        "unable to fetch Minecraft version manifest: {}",
        errors.join("; ")
    )))
}

async fn install_from_metadata<F>(
    client: &HttpClient,
    request: &InstallRequest,
    base_bytes: &[u8],
    java_executable: Option<&Path>,
    cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    progress: &mut F,
) -> Result<InstallResult>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    let mut base = parse_client_json(base_bytes)?;
    base.normalize_maven_libraries();
    let game_root = &request.game_directory;
    let instance_dir = &request.instance_directory;
    std::fs::create_dir_all(game_root.join("versions"))?;
    std::fs::create_dir_all(instance_dir)?;

    let base_version_dir = game_root.join("versions").join(&request.minecraft_version);
    std::fs::create_dir_all(&base_version_dir)?;
    let base_json_path = version_json_path(game_root, &request.minecraft_version);
    write_json_bytes(&base_json_path, base_bytes)?;

    let mut completed_bytes = 0_u64;
    let mut downloaded_files = 0;
    let mut installer_profile = None;
    let mut installer_data_dir = None;
    let mut loader_bytes = match &request.loader {
        InstallLoader::Vanilla => base_bytes.to_vec(),
        InstallLoader::Fabric { version } => {
            validate_loader_version(version)?;
            let url = format!(
                "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
                request.minecraft_version, version
            );
            fetch_bytes(client, &url, &request.download_snapshot)
                .await
                .map_err(|error| {
                    error.with_context(format!("loader error: fetch Fabric {version} profile"))
                })?
        }
        InstallLoader::Quilt { version } => {
            validate_loader_version(version)?;
            let url = format!(
                "https://meta.quiltmc.org/v3/versions/loader/{}/{}/profile/json",
                request.minecraft_version, version
            );
            fetch_bytes(client, &url, &request.download_snapshot)
                .await
                .map_err(|error| {
                    error.with_context(format!("loader error: fetch Quilt {version} profile"))
                })?
        }
        InstallLoader::Forge { version } => {
            validate_loader_version(version)?;
            let (coordinate, url) = forge_installer(&request.minecraft_version, version)?;
            let installer = maven_relative_path(&coordinate)?;
            let installer_path = game_root.join("libraries").join(&installer);
            let artifact = InstallArtifact {
                file: FileArtifact {
                    path: PathBuf::from("libraries").join(&installer),
                    url,
                    sha1: None,
                    size: None,
                },
                reason: "Forge installer".into(),
            };
            progress(completed_bytes, None)?;
            downloaded_files += download_artifacts(
                client,
                game_root,
                &[artifact],
                &request.download_snapshot,
                cancelled,
                control,
                ArtifactProgress {
                    callback: progress,
                    completed: &mut completed_bytes,
                    total: ArtifactTotal::unreported(),
                    cleanup_root: instance_dir,
                },
            )
            .await?;
            let profile_bytes = read_entry(&installer_path, "install_profile.json")?;
            let profile = parse_profile(&profile_bytes)?;
            let version_bytes = read_installer_version(&installer_path, &profile)?;
            installer_data_dir = Some(instance_dir.join(".tmcl-installer-data"));
            extract_prefix(&installer_path, "maven", &game_root.join("libraries"))?;
            extract_prefix(
                &installer_path,
                "data",
                installer_data_dir
                    .as_deref()
                    .expect("data directory assigned"),
            )?;
            installer_profile = Some((profile, installer_path));
            version_bytes
        }
        InstallLoader::NeoForge { version } => {
            validate_loader_version(version)?;
            let (coordinate, url) = neoforge_installer(version)?;
            let installer = maven_relative_path(&coordinate)?;
            let installer_path = game_root.join("libraries").join(&installer);
            let artifact = InstallArtifact {
                file: FileArtifact {
                    path: PathBuf::from("libraries").join(&installer),
                    url,
                    sha1: None,
                    size: None,
                },
                reason: "NeoForge installer".into(),
            };
            progress(completed_bytes, None)?;
            downloaded_files += download_artifacts(
                client,
                game_root,
                &[artifact],
                &request.download_snapshot,
                cancelled,
                control,
                ArtifactProgress {
                    callback: progress,
                    completed: &mut completed_bytes,
                    total: ArtifactTotal::unreported(),
                    cleanup_root: instance_dir,
                },
            )
            .await?;
            let profile_bytes = read_entry(&installer_path, "install_profile.json")?;
            let mut profile = parse_profile(&profile_bytes)?;
            rewrite_neoforge_profile_repositories(&mut profile);
            let version_bytes = read_installer_version(&installer_path, &profile)?;
            installer_data_dir = Some(instance_dir.join(".tmcl-installer-data"));
            extract_prefix(&installer_path, "maven", &game_root.join("libraries"))?;
            extract_prefix(
                &installer_path,
                "data",
                installer_data_dir
                    .as_deref()
                    .expect("data directory assigned"),
            )?;
            installer_profile = Some((profile, installer_path));
            version_bytes
        }
    };

    loader_bytes = ensure_parent_inheritance(loader_bytes, &request.minecraft_version)?;
    let mut loader = parse_client_json(&loader_bytes)?;
    loader.normalize_maven_libraries();
    let mut resolved = loader.clone().inherit(&base);
    resolved.normalize_maven_libraries();

    let mut artifacts = client_artifacts(&resolved, &request.minecraft_version)?;
    if let Some((profile, installer_path)) = &installer_profile {
        artifacts.extend(forge_profile_artifacts(profile)?);
        artifacts.extend(processor_artifacts(profile, &game_root.join("libraries"))?);
        artifacts.extend(data_artifacts(
            profile,
            &base,
            &game_root.join("libraries"),
        )?);
        if installer_path.is_file() {
            // The installer is an input to processors and remains in the shared cache.
        }
    }
    let (asset_bytes, asset_index) =
        fetch_asset_index(client, &resolved, &request.download_snapshot).await?;
    if let Some(asset_ref) = &resolved.asset_index {
        let index_path = game_root
            .join("assets")
            .join("indexes")
            .join(format!("{}.json", asset_ref.id));
        let artifact = InstallArtifact {
            file: FileArtifact {
                path: index_path
                    .strip_prefix(game_root)
                    .unwrap_or(&index_path)
                    .to_path_buf(),
                url: asset_ref.url.clone(),
                sha1: asset_ref.sha1.clone(),
                size: Some(asset_bytes.len() as u64),
            },
            reason: "Minecraft asset index".into(),
        };
        artifacts.push(artifact);
    }
    if let Some(index) = &asset_index {
        validate_asset_hashes(index)?;
        artifacts.extend(asset_requirements(index).into_iter().map(|requirement| {
            InstallArtifact {
                file: requirement.artifact,
                reason: requirement.reason,
            }
        }));
    }

    let total = ArtifactTotal::reported(known_total(&artifacts, completed_bytes));
    progress(completed_bytes, total.report())?;
    let newly_downloaded = download_artifacts(
        client,
        game_root,
        &artifacts,
        &request.download_snapshot,
        cancelled,
        control,
        ArtifactProgress {
            callback: progress,
            completed: &mut completed_bytes,
            total: total.clone(),
            cleanup_root: instance_dir,
        },
    )
    .await?;
    downloaded_files += newly_downloaded;

    let base_jar = base_version_dir.join(format!("{}.jar", request.minecraft_version));
    let instance_jar = instance_dir.join(format!("{}.jar", request.instance_name));
    if !base_jar.is_file() {
        return Err(Error::Loader(format!(
            "Minecraft client JAR was not downloaded: {}",
            base_jar.display()
        )));
    }
    if base_jar != instance_jar {
        std::fs::copy(&base_jar, &instance_jar)?;
    }

    if let Some(index) = &asset_index {
        materialize_virtual_assets(game_root, index)?;
    }

    if let Some((profile, installer_path)) = &installer_profile {
        let java = java_executable.ok_or_else(|| {
            Error::Java("Forge/NeoForge installation processors require a Java runtime".into())
        })?;
        execute_processors(ProcessorContext {
            client,
            java,
            game_root,
            instance_dir,
            instance_name: &request.instance_name,
            base: &base,
            profile,
            installer: installer_path,
            data_dir: installer_data_dir.as_deref(),
            cancelled,
            snapshot: &request.download_snapshot,
        })
        .await?;
    }

    let instance_json = instance_dir.join(format!("{}.json", request.instance_name));
    write_json_bytes(&instance_json, &loader_bytes)?;
    progress(completed_bytes, total.report())?;
    Ok(InstallResult {
        client_json: instance_json,
        client_jar: instance_jar,
        main_class: resolved.main_class,
        downloaded_files,
    })
}

fn validate_request(request: &InstallRequest) -> Result<()> {
    crate::core::instance::service::validate_name(&request.instance_name)?;
    if !safe_version_id(&request.minecraft_version) {
        return Err(Error::Version("unsafe Minecraft version id".into()));
    }
    if !request.game_directory.is_dir() {
        return Err(Error::Config(format!(
            "game directory does not exist: {}",
            request.game_directory.display()
        )));
    }
    if request.instance_directory.parent().and_then(Path::parent)
        != Some(request.game_directory.as_path())
    {
        let normalized_root = request.game_directory.canonicalize()?;
        let normalized_instance = request.instance_directory.canonicalize().or_else(|_| {
            request
                .instance_directory
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| std::io::Error::other("instance must be under versions"))?
                .canonicalize()
        })?;
        if normalized_root != normalized_instance {
            return Err(Error::Config(
                "instance directory must be under the selected game directory".into(),
            ));
        }
    }
    Ok(())
}

fn validate_loader_version(version: &str) -> Result<()> {
    if version.is_empty()
        || version.len() > 100
        || version
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | '?' | '#'))
    {
        return Err(Error::Loader(format!(
            "invalid loader version: {version:?}"
        )));
    }
    Ok(())
}

fn forge_installer(minecraft_version: &str, version: &str) -> Result<(String, String)> {
    let full_version = if version.starts_with(minecraft_version) {
        version.to_owned()
    } else {
        format!("{minecraft_version}-{version}")
    };
    let coordinate = format!("net.minecraftforge:forge:{full_version}:installer");
    let path = maven_relative_path(&coordinate)?;
    Ok((
        coordinate,
        format!(
            "https://maven.minecraftforge.net/{}",
            path.to_string_lossy().replace('\\', "/")
        ),
    ))
}

fn neoforge_installer(version: &str) -> Result<(String, String)> {
    let legacy = version.starts_with("1.20.1-");
    let coordinate = if legacy {
        format!("net.neoforged:forge:{version}:installer")
    } else {
        format!("net.neoforged:neoforge:{version}:installer")
    };
    let path = maven_relative_path(&coordinate)?;
    Ok((
        coordinate,
        format!(
            "https://maven.neoforged.net/releases/{}",
            path.to_string_lossy().replace('\\', "/")
        ),
    ))
}

fn maven_relative_path(coordinate: &str) -> Result<PathBuf> {
    crate::core::loader::maven_path(coordinate)
        .ok_or_else(|| Error::Loader(format!("invalid Maven coordinate: {coordinate}")))
}

fn ensure_parent_inheritance(bytes: Vec<u8>, parent: &str) -> Result<Vec<u8>> {
    let mut json: Value = serde_json::from_slice(&bytes)?;
    if json.get("inheritsFrom").is_none() {
        json.as_object_mut()
            .ok_or_else(|| Error::Loader("loader version JSON must be an object".into()))?
            .insert("inheritsFrom".into(), Value::String(parent.into()));
    }
    serde_json::to_vec_pretty(&json).map_err(Into::into)
}

fn read_installer_version(installer: &Path, profile: &ForgeProfile) -> Result<Vec<u8>> {
    match read_entry(installer, "version.json") {
        Ok(bytes) => Ok(bytes),
        Err(_) => profile
            .version_info
            .as_ref()
            .map(serde_json::to_vec_pretty)
            .transpose()?
            .ok_or_else(|| Error::Loader("installer contains no version.json metadata".into())),
    }
}

async fn fetch_asset_index(
    client: &HttpClient,
    metadata: &ClientJson,
    snapshot: &DownloadSourceSnapshot,
) -> Result<(Vec<u8>, Option<AssetIndex>)> {
    let Some(reference) = &metadata.asset_index else {
        return Ok((Vec::new(), None));
    };
    validate_download_url(&reference.url)?;
    let urls = resolve_candidates(
        &reference.url,
        snapshot.file_download_source,
        snapshot.region,
    )?;
    let mut errors = Vec::new();
    for url in urls {
        match fetch_candidate_bytes(
            client,
            &url,
            reference.sha1.as_deref(),
            None,
            "Minecraft asset index",
            snapshot,
        )
        .await
        {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(index) => return Ok((bytes, Some(index))),
                Err(error) => errors.push(format!("{url}: parse error: {error}")),
            },
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }
    Err(Error::Network(format!(
        "unable to fetch Minecraft asset index: {}",
        errors.join("; ")
    )))
}

fn client_artifacts(metadata: &ClientJson, version: &str) -> Result<Vec<InstallArtifact>> {
    let mut artifacts = Vec::new();
    let client = metadata
        .downloads
        .client
        .as_ref()
        .ok_or_else(|| Error::Loader(format!("Minecraft {version} has no client artifact")))?;
    artifacts.push(InstallArtifact {
        file: FileArtifact {
            path: PathBuf::from("versions")
                .join(version)
                .join(format!("{version}.jar")),
            url: client.url.clone(),
            sha1: Some(client.sha1.clone()),
            size: client.size,
        },
        reason: "Minecraft client".into(),
    });
    let features = launch_features();
    for library in metadata.active_libraries(&features)? {
        if let Some(artifact) = &library.downloads.artifact {
            artifacts.push(InstallArtifact {
                file: library_artifact(artifact)?,
                reason: format!("library {}", library.name),
            });
        }
        if let Some(natives) = metadata
            .native_artifacts()
            .into_iter()
            .find(|(candidate, _)| candidate.name == library.name)
        {
            artifacts.push(InstallArtifact {
                file: library_artifact(natives.1)?,
                reason: format!("native library {}", library.name),
            });
        }
    }
    if let Some(logging) = metadata
        .logging
        .as_ref()
        .and_then(|logging| logging.client.as_ref())
    {
        let mut file = client_file_artifact(&logging.file)?;
        file.path = PathBuf::from("assets/log_configs")
            .join(logging.file.id.as_deref().unwrap_or(&logging.file.sha1));
        artifacts.push(InstallArtifact {
            file,
            reason: "Minecraft logging configuration".into(),
        });
    }
    Ok(deduplicate_artifacts(artifacts))
}

fn forge_profile_artifacts(profile: &ForgeProfile) -> Result<Vec<InstallArtifact>> {
    let mut artifacts = Vec::new();
    for library in &profile.libraries {
        let path = maven_relative_path(&library.name)?;
        let download = library.downloads.artifact.as_ref();
        let url = if let Some(url) = download.and_then(|artifact| artifact.url.as_deref()) {
            url.to_owned()
        } else {
            maven_url(&library.name, &library.url)?
        };
        validate_download_url(&url)?;
        artifacts.push(InstallArtifact {
            file: FileArtifact {
                path: PathBuf::from("libraries").join(path),
                url,
                sha1: download.and_then(|artifact| artifact.sha1.clone()),
                size: download.and_then(|artifact| artifact.size),
            },
            reason: format!("loader library {}", library.name),
        });
    }
    Ok(artifacts)
}

fn processor_artifacts(
    profile: &ForgeProfile,
    libraries_root: &Path,
) -> Result<Vec<InstallArtifact>> {
    let mut artifacts = Vec::new();
    for processor in &profile.processors {
        if processor
            .sides
            .as_ref()
            .is_some_and(|sides| !sides.iter().any(|side| side == "client"))
        {
            continue;
        }
        for coordinate in std::iter::once(&processor.jar).chain(processor.classpath.iter()) {
            let path = maven_relative_path(coordinate)?;
            let rooted_path = libraries_root.join(path);
            let relative = rooted_path
                .strip_prefix(libraries_root.parent().unwrap_or(libraries_root))
                .unwrap_or(&rooted_path)
                .to_path_buf();
            artifacts.push(InstallArtifact {
                file: FileArtifact {
                    path: relative,
                    url: maven_url(coordinate, "")?,
                    sha1: None,
                    size: None,
                },
                reason: format!("installer processor dependency {coordinate}"),
            });
        }
    }
    Ok(artifacts)
}

fn data_artifacts(
    profile: &ForgeProfile,
    base: &ClientJson,
    libraries_root: &Path,
) -> Result<Vec<InstallArtifact>> {
    let mut artifacts = Vec::new();
    for (name, value) in &profile.data {
        let Some(value) = value.get("client").and_then(Value::as_str) else {
            continue;
        };
        if !is_coordinate_reference(value) {
            continue;
        }
        let coordinate = unbracket_coordinate(value)?;
        let path = maven_relative_path(coordinate)?;
        let rooted_path = libraries_root.join(path);
        let relative = rooted_path
            .strip_prefix(libraries_root.parent().unwrap_or(libraries_root))
            .unwrap_or(&rooted_path)
            .to_path_buf();
        if name.eq_ignore_ascii_case("mojmaps") {
            let mappings = base.downloads.client_mappings.as_ref().ok_or_else(|| {
                Error::Loader(
                    "installer requires Mojang client mappings but metadata has none".into(),
                )
            })?;
            artifacts.push(InstallArtifact {
                file: FileArtifact {
                    path: relative,
                    url: mappings.url.clone(),
                    sha1: Some(mappings.sha1.clone()),
                    size: mappings.size,
                },
                reason: "Minecraft client mappings".into(),
            });
        }
    }
    Ok(artifacts)
}

fn maven_url(coordinate: &str, supplied_base: &str) -> Result<String> {
    let path = maven_relative_path(coordinate)?;
    let base = if !supplied_base.trim().is_empty() {
        supplied_base.trim_end_matches('/')
    } else if coordinate.starts_with("net.minecraftforge:") {
        "https://maven.minecraftforge.net"
    } else if is_neoforged_coordinate(coordinate) {
        "https://maven.neoforged.net/releases"
    } else if coordinate.starts_with("net.fabricmc:") {
        "https://maven.fabricmc.net"
    } else {
        "https://libraries.minecraft.net"
    };
    Ok(format!(
        "{base}/{}",
        path.to_string_lossy().replace('\\', "/")
    ))
}

fn is_neoforged_coordinate(coordinate: &str) -> bool {
    let Some((group, _)) = coordinate.split_once(':') else {
        return false;
    };
    group == "net.neoforged"
        || group
            .strip_prefix("net.neoforged.")
            .is_some_and(|subgroup| !subgroup.is_empty())
}

fn rewrite_neoforge_profile_repositories(profile: &mut ForgeProfile) {
    for library in &mut profile.libraries {
        if library.url == "https://maven.minecraftforge.net"
            && is_neoforged_coordinate(&library.name)
        {
            library.url = "https://maven.neoforged.net/releases".into();
        }
    }
}

fn library_artifact(
    artifact: &crate::core::loader::client_json::ClientLibraryArtifact,
) -> Result<FileArtifact> {
    let path = safe_relative_path(&artifact.path)?;
    validate_download_url(&artifact.url)?;
    Ok(FileArtifact {
        path: PathBuf::from("libraries").join(path),
        url: artifact.url.clone(),
        sha1: artifact.sha1.clone(),
        size: artifact.size,
    })
}

fn client_file_artifact(
    artifact: &crate::core::loader::client_json::ClientArtifact,
) -> Result<FileArtifact> {
    validate_download_url(&artifact.url)?;
    Ok(FileArtifact {
        path: PathBuf::new(),
        url: artifact.url.clone(),
        sha1: Some(artifact.sha1.clone()),
        size: artifact.size,
    })
}

fn deduplicate_artifacts(artifacts: Vec<InstallArtifact>) -> Vec<InstallArtifact> {
    let mut seen = HashSet::new();
    artifacts
        .into_iter()
        .filter(|artifact| {
            let key = if cfg!(windows) {
                artifact.file.path.to_string_lossy().to_lowercase()
            } else {
                artifact.file.path.to_string_lossy().into_owned()
            };
            seen.insert(key)
        })
        .collect()
}

fn known_total(artifacts: &[InstallArtifact], already_completed: u64) -> u64 {
    artifacts.iter().fold(already_completed, |total, item| {
        total.saturating_add(item.file.size.unwrap_or(0))
    })
}

/// The byte total a running install reports, which can still grow.
///
/// Fabric and NeoForge library metadata often carries no size, so a plan-time
/// sum is short by however many of those files exist - and an `Option` that
/// goes `None` the moment one size is missing turned the whole install into
/// an indeterminate bar. Instead the sum is the known bytes, and an artifact
/// without a declared size adds its real length to the total when it lands.
#[derive(Clone)]
struct ArtifactTotal {
    bytes: Arc<AtomicU64>,
    reported: bool,
}

impl ArtifactTotal {
    fn reported(bytes: u64) -> Self {
        Self {
            bytes: Arc::new(AtomicU64::new(bytes)),
            reported: true,
        }
    }

    fn unreported() -> Self {
        Self {
            bytes: Arc::new(AtomicU64::new(0)),
            reported: false,
        }
    }

    fn report(&self) -> Option<u64> {
        self.reported.then(|| self.bytes.load(Ordering::Relaxed))
    }

    fn add(&self, bytes: u64) {
        if self.reported {
            self.bytes.fetch_add(bytes, Ordering::Relaxed);
        }
    }
}

struct ArtifactProgress<'a, F> {
    callback: &'a mut F,
    completed: &'a mut u64,
    total: ArtifactTotal,
    cleanup_root: &'a Path,
}

struct ArtifactDownloadContext<'a, F> {
    client: &'a HttpClient,
    game_directory: &'a Path,
    cancelled: &'a AtomicBool,
    control: Option<&'a dyn DownloadTaskControl>,
    progress_callback: Arc<Mutex<&'a mut F>>,
    completed_bytes: Arc<AtomicU64>,
    total: ArtifactTotal,
    cleanup_root: &'a Path,
    snapshot: &'a DownloadSourceSnapshot,
    cache_root: PathBuf,
}

async fn download_artifacts<F>(
    client: &HttpClient,
    game_directory: &Path,
    artifacts: &[InstallArtifact],
    snapshot: &DownloadSourceSnapshot,
    cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    progress: ArtifactProgress<'_, F>,
) -> Result<usize>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    let ArtifactProgress {
        callback: progress,
        completed,
        total,
        cleanup_root,
    } = progress;
    let mut pending = VecDeque::new();
    let mut seen_destinations = HashSet::new();
    for artifact in artifacts {
        wait_if_paused(control, cancelled).await?;
        if cancelled.load(Ordering::Relaxed) {
            return Err(Error::Download("installation cancelled".into()));
        }
        let relative = safe_relative_path(&artifact.file.path.to_string_lossy())?;
        if !seen_destinations.insert(relative.clone()) {
            continue;
        }
        let target = game_directory.join(&relative);
        let valid = if target.is_file() {
            let path = target.clone();
            let sha1 = artifact.file.sha1.clone();
            let size = artifact.file.size;
            crate::core::download::blocking(move || {
                Ok(match sha1.as_deref() {
                    Some(hash) => verify_sha1(&path, Some(hash)).is_ok(),
                    None => size.is_some_and(|size| {
                        std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() == size)
                    }),
                })
            })
            .await?
        } else {
            false
        };
        if valid {
            if let Some(control) = control {
                control.record_file_reference(&target)?;
            }
            let size = std::fs::metadata(&target)?.len();
            // A file that was already there still has to move the bar, both
            // sides of it: its bytes were not in the plan-time total when the
            // metadata carried no size.
            if artifact.file.size.is_none() {
                total.add(size);
            }
            *completed = completed.saturating_add(size);
            progress(*completed, total.report())?;
            continue;
        }
        validate_download_url(&artifact.file.url)?;
        pending.push_back(artifact.clone());
    }

    let completed_bytes = Arc::new(AtomicU64::new(*completed));
    let progress_callback = Arc::new(Mutex::new(progress));
    let download_context = ArtifactDownloadContext {
        client,
        game_directory,
        cancelled,
        control,
        progress_callback: Arc::clone(&progress_callback),
        completed_bytes: Arc::clone(&completed_bytes),
        total: total.clone(),
        cleanup_root,
        snapshot,
        cache_root: download_cache_root(game_directory),
    };
    let mut in_flight = FuturesUnordered::new();
    let concurrency = crate::core::download::DownloadExecutor::global().concurrency();
    for _ in 0..concurrency {
        let Some(artifact) = pending.pop_front() else {
            break;
        };
        in_flight.push(download_one_labelled(&download_context, artifact));
    }

    let mut downloaded = 0;
    let mut failed = Vec::new();
    while let Some((artifact, result)) = in_flight.next().await {
        match result {
            Ok(count) => downloaded += count,
            Err(error) => failed.push((artifact, error)),
        }
        if let Some(artifact) = pending.pop_front() {
            in_flight.push(download_one_labelled(&download_context, artifact));
        }
    }

    // A single transient asset failure must not abort the whole installation.
    // Give every failed artifact a second, spaced-out pass before reporting.
    if !failed.is_empty() {
        let (retryable, mut still_failed): (Vec<_>, Vec<_>) = failed
            .into_iter()
            .partition(|(_, error)| error.is_retryable());
        if !retryable.is_empty() {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        for (artifact, original_error) in retryable {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            match download_one_artifact(&download_context, artifact.clone()).await {
                Ok(count) => downloaded += count,
                Err(_) => still_failed.push((artifact, original_error)),
            }
        }
        if let Some((artifact, error)) = still_failed
            .into_iter()
            .find(|(artifact, _)| !is_optional_language_asset(&artifact.reason))
        {
            return Err(error.with_context(format!("download {}", artifact.reason)));
        }
    }

    *completed = completed_bytes.load(Ordering::Relaxed);
    (download_context
        .progress_callback
        .lock()
        .expect("progress callback lock poisoned"))(*completed, total.report())?;
    Ok(downloaded)
}

fn is_optional_language_asset(reason: &str) -> bool {
    reason
        .strip_prefix("asset minecraft/lang/")
        .is_some_and(|name| name.ends_with(".json"))
}

async fn download_one_labelled<'a, F>(
    context: &'a ArtifactDownloadContext<'a, F>,
    artifact: InstallArtifact,
) -> (InstallArtifact, Result<usize>)
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send + 'a,
{
    let result = download_one_artifact(context, artifact.clone()).await;
    (artifact, result)
}

async fn download_one_artifact<'a, F>(
    context: &'a ArtifactDownloadContext<'a, F>,
    artifact: InstallArtifact,
) -> Result<usize>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send + 'a,
{
    let relative = safe_relative_path(&artifact.file.path.to_string_lossy())?;
    let target = context.game_directory.join(relative);
    let scoped_control = context
        .control
        .map(|inner| ScopedDownloadTaskControl::new(inner, context.cleanup_root.to_path_buf()));
    let mut file_completed = 0_u64;
    // An artifact whose size the metadata never declared may not report while
    // it streams: its bytes are not in the total yet, and adding them to one
    // side only would push the bar past 100% and then pull it back down when
    // the real size finally lands. It reports once, at the end, when the size
    // is known and both sides can move together.
    let unknown_size = artifact.file.size.is_none();
    let mut report_progress = |file_bytes: u64| -> Result<()> {
        let delta = file_bytes.saturating_sub(file_completed);
        if delta == 0 {
            return Ok(());
        }
        file_completed = file_bytes;
        let mut progress_callback = context
            .progress_callback
            .lock()
            .expect("progress callback lock poisoned");
        let completed = context
            .completed_bytes
            .fetch_add(delta, Ordering::Relaxed)
            .saturating_add(delta);
        progress_callback(completed, context.total.report())
    };
    let candidates = resolve_candidates(
        &artifact.file.url,
        context.snapshot.file_download_source,
        context.snapshot.region,
    )?;
    let integrity = if artifact.reason.contains("installer")
        || artifact.reason.contains("library ")
        || artifact.reason.contains("loader library ")
    {
        IntegrityKind::ZipJar
    } else {
        IntegrityKind::File
    };
    let cache_policy = if artifact.file.sha1.is_some() {
        CachePolicy::ContentAddressed
    } else {
        CachePolicy::Etag
    };
    let request = DownloadRequest::new(
        candidates,
        &target,
        artifact.file.sha1.clone(),
        artifact.file.size,
        artifact.reason.clone(),
    )
    .with_integrity(integrity)
    .with_cache_policy(cache_policy)
    .with_policy(context.snapshot.clone());
    let file_result = download_request(
        context.client,
        &request,
        &context.cache_root,
        context.cancelled,
        scoped_control
            .as_ref()
            .map(|value| value as &dyn DownloadTaskControl),
        |completed, _| {
            if unknown_size {
                Ok(())
            } else {
                report_progress(completed)
            }
        },
    )
    .await;
    file_result.map_err(|error| error.with_context(artifact.reason))?;

    let actual_size = std::fs::metadata(&target)?.len();
    if unknown_size {
        // Grow the total first, so `completed` never runs past it.
        context.total.add(actual_size);
    }
    report_progress(actual_size)?;
    Ok(1)
}

async fn fetch_bytes(
    client: &HttpClient,
    url: &str,
    snapshot: &DownloadSourceSnapshot,
) -> Result<Vec<u8>> {
    validate_download_url(url)?;
    let urls = resolve_candidates(url, snapshot.file_download_source, snapshot.region)?;
    let mut errors = Vec::new();
    for candidate in urls {
        match fetch_candidate_bytes(
            client,
            &candidate,
            None,
            None,
            "Minecraft loader data",
            snapshot,
        )
        .await
        {
            Ok(bytes) => return Ok(bytes),
            Err(error) => errors.push(format!("{candidate}: {error}")),
        }
    }
    Err(Error::Network(format!(
        "unable to fetch {url}: {}",
        errors.join("; ")
    )))
}

async fn fetch_candidate_bytes(
    client: &HttpClient,
    url: &str,
    expected_sha1: Option<&str>,
    expected_size: Option<u64>,
    label: &str,
    snapshot: &DownloadSourceSnapshot,
) -> Result<Vec<u8>> {
    let temporary =
        std::env::temp_dir().join(format!("tmcl-loader-metadata-{}", uuid::Uuid::new_v4()));
    let cache_root = crate::core::config::DataPaths::discover()
        .map(|paths| paths.cache_dir)
        .unwrap_or_else(|_| std::env::temp_dir().join("tmcl-cache"));
    let request = DownloadRequest::new(
        vec![url.to_owned()],
        &temporary,
        expected_sha1.map(str::to_owned),
        expected_size,
        label,
    )
    .with_cache_policy(if expected_sha1.is_some() {
        CachePolicy::ContentAddressed
    } else {
        CachePolicy::Etag
    })
    .with_policy(snapshot.clone());
    let cancelled = AtomicBool::new(false);
    let result = download_request(client, &request, &cache_root, &cancelled, None, |_, _| {
        Ok(())
    })
    .await
    .and_then(|_| std::fs::read(&temporary).map_err(Error::from));
    let _ = std::fs::remove_file(&temporary);
    result
}

fn download_cache_root(game_directory: &Path) -> PathBuf {
    crate::core::config::DataPaths::discover()
        .map(|paths| paths.cache_dir)
        .unwrap_or_else(|_| game_directory.join(".tmcl-cache"))
}

fn validate_download_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| Error::Network(format!("invalid download URL: {value}")))?;
    let secure = url.scheme() == "https";
    let loopback_http = url.scheme() == "http"
        && url
            .host_str()
            .is_some_and(|host| matches!(host, "127.0.0.1" | "localhost" | "::1"));
    if !secure && !loopback_http {
        return Err(Error::Network(format!(
            "download URL must use HTTPS (or loopback HTTP): {value}"
        )));
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::Loader(format!(
            "unsafe installation path: {value:?}"
        )));
    }
    Ok(path.to_path_buf())
}

fn verify_sha1_bytes(bytes: &[u8], expected: &str) -> Result<()> {
    use sha1::{Digest, Sha1};
    let actual = format!("{:x}", Sha1::digest(bytes));
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(Error::Download(format!(
            "SHA-1 mismatch: expected {expected}, got {actual}"
        )))
    }
}

fn write_json_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let value: Value = serde_json::from_slice(bytes)?;
    let mut file = std::fs::File::create(path)?;
    serde_json::to_writer_pretty(&mut file, &value)?;
    use std::io::Write;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn materialize_virtual_assets(game_directory: &Path, index: &AssetIndex) -> Result<()> {
    if !index.virtual_assets && !index.map_to_resources {
        return Ok(());
    }
    for (name, asset) in &index.objects {
        if asset.hash.len() != 40 || !asset.hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::Version(format!(
                "invalid Minecraft asset hash: {}",
                asset.hash
            )));
        }
        let name = safe_relative_path(name)?;
        let source = game_directory
            .join("assets/objects")
            .join(&asset.hash[..2])
            .join(&asset.hash);
        let mut roots = Vec::new();
        if index.virtual_assets {
            roots.push(game_directory.join("assets/virtual/legacy"));
        }
        if index.map_to_resources {
            roots.push(game_directory.join("resources"));
        }
        for root in roots {
            let target = root.join(&name);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if !target.is_file() {
                std::fs::copy(&source, target)?;
            }
        }
    }
    Ok(())
}

fn validate_asset_hashes(index: &AssetIndex) -> Result<()> {
    for object in index.objects.values() {
        if object.hash.len() != 40 || !object.hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::Version(format!(
                "invalid Minecraft asset hash: {}",
                object.hash
            )));
        }
    }
    Ok(())
}

struct ProcessorContext<'a> {
    client: &'a HttpClient,
    java: &'a Path,
    game_root: &'a Path,
    instance_dir: &'a Path,
    instance_name: &'a str,
    base: &'a ClientJson,
    profile: &'a ForgeProfile,
    installer: &'a Path,
    data_dir: Option<&'a Path>,
    cancelled: &'a AtomicBool,
    snapshot: &'a DownloadSourceSnapshot,
}

async fn execute_processors(context: ProcessorContext<'_>) -> Result<()> {
    let ProcessorContext {
        client,
        java,
        game_root,
        instance_dir,
        instance_name,
        base,
        profile,
        installer,
        data_dir,
        cancelled,
        snapshot,
    } = context;
    let libraries = game_root.join("libraries");
    let mut values = BTreeMap::from([
        (
            "{MINECRAFT_JAR}".to_owned(),
            instance_dir
                .join(format!("{instance_name}.jar"))
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "{INSTALLER}".into(),
            installer.to_string_lossy().into_owned(),
        ),
        ("{SIDE}".into(), "client".into()),
        ("{ROOT}".into(), game_root.to_string_lossy().into_owned()),
    ]);
    for (key, value) in &profile.data {
        let Some(raw) = value.get("client").and_then(Value::as_str) else {
            continue;
        };
        let expanded = if is_coordinate_reference(raw) {
            let coordinate = unbracket_coordinate(raw)?;
            libraries
                .join(maven_relative_path(coordinate)?)
                .to_string_lossy()
                .into_owned()
        } else if let Some(relative) = raw.strip_prefix("data/") {
            data_dir
                .ok_or_else(|| Error::Loader("installer data directory is missing".into()))?
                .join(safe_relative_path(relative)?)
                .to_string_lossy()
                .into_owned()
        } else {
            raw.to_owned()
        };
        values.insert(format!("{{{key}}}"), expanded);
    }

    // Forge installers embed the binary patch under data/client.lzma; the
    // install profile points to its normal Maven cache location.
    if let (Some(data_dir), Some(binpatch)) = (data_dir, values.get("{BINPATCH}")) {
        let embedded = data_dir.join("client.lzma");
        if embedded.is_file() && !Path::new(binpatch).exists() {
            if let Some(parent) = Path::new(binpatch).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(embedded, binpatch)?;
        }
    }
    if let Some(mojmaps) = values.get("{MOJMAPS}")
        && let Some(mapping) = &base.downloads.client_mappings
        && !Path::new(mojmaps).is_file()
    {
        validate_download_url(&mapping.url)?;
        let bytes = fetch_bytes(client, &mapping.url, snapshot).await?;
        verify_sha1_bytes(&bytes, &mapping.sha1)?;
        if let Some(parent) = Path::new(mojmaps).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(mojmaps, bytes)?;
    }

    for processor in &profile.processors {
        if processor
            .sides
            .as_ref()
            .is_some_and(|sides| !sides.iter().any(|side| side == "client"))
            || processor
                .args
                .iter()
                .any(|argument| argument == "DOWNLOAD_MOJMAPS")
        {
            continue;
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err(Error::Download("installation cancelled".into()));
        }
        let jar_path = libraries.join(maven_relative_path(&processor.jar)?);
        let manifest = read_entry(&jar_path, "META-INF/MANIFEST.MF")?;
        let main_class = String::from_utf8_lossy(&manifest)
            .lines()
            .find_map(|line| line.strip_prefix("Main-Class:"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                Error::Loader(format!(
                    "processor JAR has no Main-Class: {}",
                    jar_path.display()
                ))
            })?;
        let mut classpath = processor
            .classpath
            .iter()
            .map(|coordinate| maven_relative_path(coordinate).map(|path| libraries.join(path)))
            .collect::<Result<Vec<_>>>()?;
        classpath.push(jar_path);
        let mut args = Vec::new();
        args.push("-cp".into());
        args.push(
            classpath
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(&if cfg!(windows) { ';' } else { ':' }.to_string()),
        );
        args.push(main_class);
        for argument in &processor.args {
            let mut expanded = if is_coordinate_reference(argument) {
                libraries
                    .join(maven_relative_path(unbracket_coordinate(argument)?)?)
                    .to_string_lossy()
                    .into_owned()
            } else {
                argument.clone()
            };
            for (key, value) in &values {
                expanded = expanded.replace(key, value);
            }
            args.push(expanded);
        }
        let command = tokio::process::Command::new(java)
            .args(args)
            .current_dir(game_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output();
        let output = tokio::select! {
            output = command => output.map_err(|error| Error::Loader(format!("start installer processor: {error}")))?,
            _ = wait_for_cancellation(cancelled) => return Err(Error::Download("installation cancelled".into())),
        };
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Loader(format!(
                "installer processor failed ({:?}): {}{}",
                output.status.code(),
                truncate_diagnostic(&stdout),
                truncate_diagnostic(&stderr)
            )));
        }
    }
    Ok(())
}

async fn wait_for_cancellation(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn is_coordinate_reference(value: &str) -> bool {
    value.starts_with('[') && value.ends_with(']') && value.len() > 2
}

fn unbracket_coordinate(value: &str) -> Result<&str> {
    value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::Loader(format!("invalid Maven reference: {value}")))
}

fn truncate_diagnostic(value: &str) -> String {
    const MAX: usize = 3000;
    if value.len() <= MAX {
        value.to_owned()
    } else {
        format!("…{}", &value[value.len() - MAX..])
    }
}

#[cfg(test)]
mod tests {
    use super::is_optional_language_asset;

    #[test]
    fn language_assets_are_optional_downloads() {
        assert!(is_optional_language_asset(
            "asset minecraft/lang/vec_it.json"
        ));
        assert!(!is_optional_language_asset("asset minecraft/sounds/a.ogg"));
        assert!(!is_optional_language_asset("loader library example:1.0"));
    }

    use std::sync::atomic::AtomicBool;

    use super::LoaderKind;
    use super::{
        ArtifactProgress, ArtifactTotal, DownloadSourceSnapshot, InstallArtifact, InstallLoader,
        InstallRequest, download_artifacts, forge_installer, forge_profile_artifacts,
        install_from_metadata, installer_name, is_neoforged_coordinate, maven_url,
        neoforge_installer, rewrite_neoforge_profile_repositories,
    };
    use crate::core::launch::validate::FileArtifact;
    use crate::core::loader::forge::parse_profile;
    use crate::core::version::http::HttpClient;

    #[test]
    fn dispatches_every_supported_loader() {
        assert_eq!(installer_name(LoaderKind::Vanilla), "vanilla");
        assert_eq!(installer_name(LoaderKind::Fabric), "fabric");
        assert_eq!(installer_name(LoaderKind::Forge), "forge");
        assert_eq!(installer_name(LoaderKind::NeoForge), "neoforge");
        assert!(matches!(InstallLoader::Vanilla, InstallLoader::Vanilla));
    }

    #[test]
    fn builds_official_forge_and_neoforge_installer_coordinates() {
        assert_eq!(
            forge_installer("1.20.1", "47.2.0").unwrap(),
            (
                "net.minecraftforge:forge:1.20.1-47.2.0:installer".into(),
                "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar".into()
            )
        );
        assert_eq!(
            neoforge_installer("1.20.1-20.4.237").unwrap().0,
            "net.neoforged:forge:1.20.1-20.4.237:installer"
        );
        assert_eq!(
            neoforge_installer("21.1.247").unwrap().0,
            "net.neoforged:neoforge:21.1.247:installer"
        );
    }

    #[test]
    fn neoforge_subgroups_use_the_neoforged_maven_repository() {
        let coordinate = "net.neoforged.fancymodloader:earlydisplay:12.0.0";
        assert!(is_neoforged_coordinate(coordinate));
        assert_eq!(
            maven_url(coordinate, "").expect("NeoForge Maven URL"),
            "https://maven.neoforged.net/releases/net/neoforged/fancymodloader/earlydisplay/12.0.0/earlydisplay-12.0.0.jar"
        );
        assert!(!is_neoforged_coordinate(
            "net.neoforged-malicious:artifact:1.0"
        ));
    }

    #[test]
    fn rewrites_neoforge_subgroup_libraries_away_from_forge_maven() {
        let mut profile = parse_profile(
            br#"{"libraries":[{"name":"net.neoforged.fancymodloader:earlydisplay:12.0.0","url":"https://maven.minecraftforge.net"},{"name":"net.neoforged-malicious:artifact:1.0","url":"https://maven.minecraftforge.net"}]}"#,
        )
        .expect("NeoForge profile");

        rewrite_neoforge_profile_repositories(&mut profile);

        assert_eq!(
            profile.libraries[0].url,
            "https://maven.neoforged.net/releases"
        );
        assert_eq!(profile.libraries[1].url, "https://maven.minecraftforge.net");
    }

    #[test]
    fn forge_profile_artifacts_keep_download_integrity_metadata() {
        let profile = parse_profile(
            br#"{"libraries":[{"name":"net.neoforged.installertools:installertools:4.0.17:fatjar","downloads":{"artifact":{"url":"https://maven.neoforged.net/releases/net/neoforged/installertools/installertools/4.0.17/installertools-4.0.17-fatjar.jar","sha1":"d0e22dfef7eb7eeffa20fee4c393b3f7cad0323b","size":814132}}}]}"#,
        )
        .expect("NeoForge profile");

        let artifacts = forge_profile_artifacts(&profile).expect("profile artifacts");
        let artifact = &artifacts[0].file;
        assert_eq!(
            artifact.sha1.as_deref(),
            Some("d0e22dfef7eb7eeffa20fee4c393b3f7cad0323b")
        );
        assert_eq!(artifact.size, Some(814132));
        assert!(artifact.url.ends_with("installertools-4.0.17-fatjar.jar"));
    }

    #[tokio::test]
    async fn downloads_independent_artifacts_concurrently_with_a_bounded_pool() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use std::time::Duration;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        const ARTIFACT_COUNT: usize = 4;
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind local server");
        let address = listener.local_addr().expect("local address");
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let server_active = Arc::clone(&active);
        let server_max_active = Arc::clone(&max_active);
        let server = tokio::spawn(async move {
            let mut handlers = Vec::new();
            for _ in 0..ARTIFACT_COUNT {
                let (mut stream, _) = listener.accept().await.expect("accept request");
                let active = Arc::clone(&server_active);
                let max_active = Arc::clone(&server_max_active);
                handlers.push(tokio::spawn(async move {
                    let mut request = [0; 1024];
                    let _ = stream.read(&mut request).await.expect("read request");
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(current, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\ndata",
                        )
                        .await
                        .expect("write response");
                    active.fetch_sub(1, Ordering::SeqCst);
                }));
            }
            for handler in handlers {
                handler.await.expect("serve artifact");
            }
        });

        let root = std::env::temp_dir().join(format!(
            "tmcl-concurrent-artifacts-{}",
            uuid::Uuid::new_v4()
        ));
        let artifacts = (0..ARTIFACT_COUNT)
            .map(|index| InstallArtifact {
                file: FileArtifact {
                    path: format!("libraries/parallel-{index}.jar").into(),
                    url: format!("http://{address}/artifact-{index}"),
                    sha1: None,
                    size: Some(4),
                },
                reason: format!("test artifact {index}"),
            })
            .collect::<Vec<_>>();
        let http = HttpClient::new().expect("HTTP client");
        let cancelled = AtomicBool::new(false);
        let mut completed = 0;
        let mut progress_updates = Vec::new();
        let mut callback = |completed, total| {
            progress_updates.push((completed, total));
            Ok(())
        };
        let downloaded = download_artifacts(
            &http,
            &root,
            &artifacts,
            &DownloadSourceSnapshot::default(),
            &cancelled,
            None,
            ArtifactProgress {
                callback: &mut callback,
                completed: &mut completed,
                total: ArtifactTotal::reported((ARTIFACT_COUNT * 4) as u64),
                cleanup_root: &root,
            },
        )
        .await
        .expect("download artifacts");
        server.await.expect("local fixture server");

        assert_eq!(downloaded, ARTIFACT_COUNT);
        assert!(
            max_active.load(Ordering::SeqCst) > 1,
            "independent artifacts should overlap instead of waiting for each request in turn"
        );
        assert_eq!(completed, (ARTIFACT_COUNT * 4) as u64);
        assert_eq!(
            progress_updates.last(),
            Some(&((ARTIFACT_COUNT * 4) as u64, Some(16)))
        );
        assert!(
            progress_updates
                .windows(2)
                .all(|window| window[0].0 <= window[1].0)
        );
        std::fs::remove_dir_all(root).expect("clean concurrent download fixture");
    }

    #[tokio::test]
    async fn artifact_http_404_is_not_classified_as_retryable() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind local server");
        let address = listener.local_addr().expect("local address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).await.expect("read request");
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .expect("write not found response");
        });
        let root = std::env::temp_dir().join(format!("tmcl-404-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create game directory");
        let artifact = InstallArtifact {
            file: FileArtifact {
                path: "libraries/net/neoforged/fancymodloader/earlydisplay/12.0.0/earlydisplay-12.0.0.jar".into(),
                url: format!("http://{address}/earlydisplay-12.0.0.jar"),
                sha1: None,
                size: None,
            },
            reason: "loader library net.neoforged.fancymodloader:earlydisplay:12.0.0".into(),
        };
        let http = HttpClient::new().expect("HTTP client");
        let cancelled = AtomicBool::new(false);
        let mut completed = 0;
        let mut callback = |_, _| Ok(());
        let error = download_artifacts(
            &http,
            &root,
            &[artifact],
            &DownloadSourceSnapshot::default(),
            &cancelled,
            None,
            ArtifactProgress {
                callback: &mut callback,
                completed: &mut completed,
                total: ArtifactTotal::unreported(),
                cleanup_root: &root,
            },
        )
        .await
        .expect_err("404 download should fail");
        server.await.expect("local fixture server");

        assert!(
            !error.is_retryable(),
            "404 should not be retryable: {error}"
        );
        assert!(
            error
                .to_string()
                .contains("loader library net.neoforged.fancymodloader")
        );
        assert!(error.to_string().contains("HTTP status 404"));
        std::fs::remove_dir_all(root).expect("clean download fixture");
    }

    #[tokio::test]
    async fn vanilla_install_downloads_verified_client_and_commits_instance_files() {
        use sha1::{Digest, Sha1};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let client_jar =
            format!("local vanilla client jar fixture {}", uuid::Uuid::new_v4()).into_bytes();
        let sha1 = format!("{:x}", Sha1::digest(&client_jar));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind local server");
        let address = listener.local_addr().expect("local address");
        let response_bytes = client_jar.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).await.expect("read request");
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response_bytes.len()
            );
            stream
                .write_all(headers.as_bytes())
                .await
                .expect("write headers");
            stream
                .write_all(&response_bytes)
                .await
                .expect("write fixture");
        });
        let root = std::env::temp_dir().join(format!("tmcl-install-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create game directory");
        let request = InstallRequest {
            game_directory: root.clone(),
            instance_directory: root.join("versions/survival"),
            instance_name: "survival".into(),
            minecraft_version: "1.21.1".into(),
            loader: InstallLoader::Vanilla,
            download_snapshot: DownloadSourceSnapshot::default(),
        };
        let metadata = serde_json::json!({
            "id":"1.21.1",
            "type":"release",
            "mainClass":"net.minecraft.client.main.Main",
            "arguments":{"game":[],"jvm":[]},
            "downloads":{"client":{"url":format!("http://{address}/client.jar"),"sha1":sha1,"size":client_jar.len()}}
        });
        let http = HttpClient::new().expect("HTTP client");
        let cancelled = AtomicBool::new(false);
        let result = install_from_metadata(
            &http,
            &request,
            &serde_json::to_vec(&metadata).expect("serialize fixture metadata"),
            None,
            &cancelled,
            None,
            &mut |_, _| Ok(()),
        )
        .await
        .expect("install vanilla client");
        server.await.expect("local fixture server");
        assert_eq!(
            std::fs::read(result.client_jar).expect("read instance jar"),
            client_jar
        );
        assert!(result.client_json.is_file());
        assert!(root.join("versions/1.21.1/1.21.1.jar").is_file());
        std::fs::remove_dir_all(root).expect("clean install fixture");
    }
}
