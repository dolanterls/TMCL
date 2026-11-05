//! Read Minecraft version and loader metadata from Modrinth/CurseForge modpacks.

use std::io::Read;
use std::path::Path;

use futures_util::StreamExt;
use serde_json::Value;

use crate::core::error::{Error, Result};
use crate::core::resource::provider::ResourceProvider;

/// A file a CurseForge manifest refers to by id instead of URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackFileRef {
    pub project_id: String,
    pub file_id: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackFile {
    /// Destination inside the instance. `None` until a reference is resolved.
    pub path: Option<String>,
    pub urls: Vec<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    /// Set by CurseForge manifests, which list `projectID`/`fileID` pairs.
    pub reference: Option<ModpackFileRef>,
}

/// Result of turning a manifest's entries into downloadable files.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModpackResolution {
    pub files: Vec<ModpackFile>,
    /// Human-readable reasons for entries that could not be resolved.
    pub unresolved: Vec<String>,
}

/// Resolves CurseForge references into concrete downloads.
///
/// Modrinth packs already carry URLs and pass through untouched. One bad
/// reference must not sink the whole pack, so failures are collected; only a
/// missing/unavailable provider is fatal, because then nothing can be resolved.
pub async fn resolve_files(
    files: Vec<ModpackFile>,
    provider: Option<&dyn ResourceProvider>,
) -> Result<ModpackResolution> {
    let needs_provider = files.iter().any(|file| file.reference.is_some());
    if needs_provider {
        let Some(provider) = provider else {
            return Err(Error::Download(
                "this modpack lists CurseForge files, but the CurseForge provider is not available"
                    .into(),
            ));
        };
        let capabilities = provider.capabilities();
        if !capabilities.available {
            return Err(Error::Download(format!(
                "this modpack lists CurseForge files, but CurseForge is unavailable: {}",
                capabilities
                    .reason
                    .unwrap_or_else(|| "API key is not configured".into())
            )));
        }
    }

    let mut resolution = ModpackResolution::default();
    let mut pending = Vec::new();
    for file in files {
        match (&file.reference, &file.path) {
            (None, Some(_)) => resolution.files.push(file),
            (Some(_), _) => pending.push(file),
            (None, None) => resolution
                .unresolved
                .push("a modpack entry has neither a path nor a reference".into()),
        }
    }
    if pending.is_empty() {
        return Ok(resolution);
    }
    let provider = provider.expect("provider checked above");
    let mut stream = futures_util::stream::iter(
        pending
            .into_iter()
            .map(|file| resolve_reference(file, provider)),
    )
    .buffer_unordered(4);
    while let Some(result) = stream.next().await {
        match result {
            Ok(file) => resolution.files.push(file),
            Err(reason) => resolution.unresolved.push(reason),
        }
    }
    Ok(resolution)
}

async fn resolve_reference(
    mut file: ModpackFile,
    provider: &dyn ResourceProvider,
) -> std::result::Result<ModpackFile, String> {
    let Some(reference) = file.reference.clone() else {
        return Err("a modpack entry has no reference".into());
    };
    let label = format!("{} / {}", reference.project_id, reference.file_id);
    let resolved = provider
        .file(reference.project_id.clone(), reference.file_id.clone())
        .await
        .map_err(|error| format!("{label}: {error}"))?;
    let name = resolved.name.trim();
    if name.is_empty() || Path::new(name).components().count() != 1 || name == "." || name == ".." {
        return Err(format!("{label}: unusable file name {name:?}"));
    }
    file.path = Some(format!("mods/{name}"));
    file.urls = vec![resolved.download_url];
    file.sha1 = resolved.sha1;
    file.size = resolved.size;
    file.reference = None;
    Ok(file)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackMetadata {
    pub minecraft_version: String,
    pub loader: String,
    pub loader_version: String,
    pub files: Vec<ModpackFile>,
}

impl ModpackMetadata {
    fn vanilla(minecraft_version: String) -> Self {
        Self {
            minecraft_version,
            loader: "vanilla".into(),
            loader_version: String::new(),
            files: Vec::new(),
        }
    }
}

/// Read metadata from a downloaded `.mrpack` or CurseForge modpack ZIP.
pub fn read_modpack_metadata(path: &Path) -> Result<ModpackMetadata> {
    let mut archive = crate::core::resource::archive::open(path)?;
    let mut modrinth = None;
    let mut curseforge = None;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| Error::Download(format!("read modpack entry: {error}")))?;
        let name = entry.name().to_owned();
        if name.ends_with("modrinth.index.json") {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            modrinth = Some(text);
        } else if name.ends_with("manifest.json") {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            curseforge = Some(text);
        }
    }
    if let Some(text) = modrinth {
        return parse_modrinth_index(&text);
    }
    if let Some(text) = curseforge {
        return parse_curseforge_manifest(&text);
    }
    Err(Error::Download(
        "modpack archive has no modrinth.index.json or manifest.json".into(),
    ))
}

fn parse_modrinth_index(text: &str) -> Result<ModpackMetadata> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| Error::Download(format!("parse modrinth.index.json: {error}")))?;
    let dependencies = value
        .get("dependencies")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Download("modpack dependencies are missing".into()))?;
    let files = modrinth_files(value.get("files"));
    let minecraft_version = dependencies
        .get("minecraft")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .ok_or_else(|| Error::Download("modpack Minecraft version is missing".into()))?
        .to_owned();
    for (key, loader) in [
        ("fabric-loader", "fabric"),
        ("quilt-loader", "quilt"),
        ("forge", "forge"),
        ("neoforge", "neoforge"),
    ] {
        if let Some(version) = dependencies.get(key) {
            let version = version
                .as_str()
                .map(str::trim)
                .filter(|version| !version.is_empty())
                .ok_or_else(|| Error::Download(format!("modpack {key} version is missing")))?;
            return Ok(ModpackMetadata {
                minecraft_version,
                loader: loader.into(),
                loader_version: version.to_owned(),
                files,
            });
        }
    }
    Ok(ModpackMetadata {
        minecraft_version,
        loader: "vanilla".into(),
        loader_version: String::new(),
        files,
    })
}

fn modrinth_files(value: Option<&Value>) -> Vec<ModpackFile> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let client = entry
                .pointer("/env/client")
                .and_then(Value::as_str)
                .unwrap_or("required");
            if client == "unsupported" {
                return None;
            }
            let path = entry.get("path").and_then(Value::as_str)?.to_owned();
            let urls = entry
                .get("downloads")
                .and_then(Value::as_array)?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if urls.is_empty() {
                return None;
            }
            Some(ModpackFile {
                path: Some(path),
                urls,
                sha1: entry
                    .pointer("/hashes/sha1")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                size: entry.get("fileSize").and_then(Value::as_u64),
                reference: None,
            })
        })
        .collect()
}

fn parse_curseforge_manifest(text: &str) -> Result<ModpackMetadata> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| Error::Download(format!("parse manifest.json: {error}")))?;
    let minecraft_version = value
        .pointer("/minecraft/version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .ok_or_else(|| Error::Download("modpack Minecraft version is missing".into()))?
        .to_owned();
    let loaders = value
        .pointer("/minecraft/modLoaders")
        .and_then(Value::as_array);
    let Some(loader) = loaders.and_then(|loaders| {
        loaders
            .iter()
            .find(|loader| loader.get("primary").and_then(Value::as_bool) == Some(true))
            .or_else(|| loaders.first())
    }) else {
        return Ok(ModpackMetadata::vanilla(minecraft_version));
    };
    let id = loader
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Download("modpack loader id is missing".into()))?;
    let (kind, version) = id
        .split_once('-')
        .ok_or_else(|| Error::Download(format!("invalid modpack loader id {id:?}")))?;
    let kind = match kind.to_ascii_lowercase().as_str() {
        "fabric" => "fabric",
        "quilt" => "quilt",
        "forge" => "forge",
        "neoforge" => "neoforge",
        other => {
            return Err(Error::Download(format!(
                "unsupported modpack loader {other:?}"
            )));
        }
    };
    let files = curseforge_files(value.get("files"));
    Ok(ModpackMetadata {
        minecraft_version,
        loader: kind.into(),
        loader_version: version.to_owned(),
        files,
    })
}

fn curseforge_files(value: Option<&Value>) -> Vec<ModpackFile> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let project_id = entry.get("projectID").and_then(Value::as_u64)?;
            let file_id = entry.get("fileID").and_then(Value::as_u64)?;
            Some(ModpackFile {
                path: None,
                urls: Vec::new(),
                sha1: None,
                size: None,
                reference: Some(ModpackFileRef {
                    project_id: project_id.to_string(),
                    file_id: file_id.to_string(),
                    required: entry
                        .get("required")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                }),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_curseforge_manifest, parse_modrinth_index};

    #[test]
    fn modrinth_metadata_uses_minecraft_and_loader_dependencies() {
        let metadata = parse_modrinth_index(
            r#"{"dependencies":{"minecraft":"1.20.1","fabric-loader":"0.15.11"}}"#,
        )
        .expect("metadata");
        assert_eq!(metadata.minecraft_version, "1.20.1");
        assert_eq!(metadata.loader, "fabric");
        assert_eq!(metadata.loader_version, "0.15.11");
    }

    #[test]
    fn modrinth_metadata_falls_back_to_vanilla_without_a_loader() {
        let metadata =
            parse_modrinth_index(r#"{"dependencies":{"minecraft":"1.21.1"}}"#).expect("metadata");
        assert_eq!(metadata.loader, "vanilla");
        assert!(metadata.loader_version.is_empty());
    }

    #[test]
    fn curseforge_metadata_uses_the_primary_loader() {
        let metadata = parse_curseforge_manifest(
            r#"{"minecraft":{"version":"1.12.2","modLoaders":[{"id":"forge-14.23.5.2860","primary":true}]}}"#,
        )
        .expect("metadata");
        assert_eq!(metadata.minecraft_version, "1.12.2");
        assert_eq!(metadata.loader, "forge");
        assert_eq!(metadata.loader_version, "14.23.5.2860");
    }

    #[test]
    fn quilt_modpacks_name_their_loader() {
        let metadata = parse_modrinth_index(
            r#"{"dependencies":{"minecraft":"1.20.1","quilt-loader":"0.24.0"}}"#,
        )
        .expect("quilt metadata");
        assert_eq!(metadata.loader, "quilt");
        assert_eq!(metadata.loader_version, "0.24.0");
    }
}

#[cfg(test)]
mod file_tests {
    use super::{
        ModpackFile, ModpackFileRef, parse_curseforge_manifest, parse_modrinth_index, resolve_files,
    };

    #[test]
    fn modrinth_files_are_kept_for_client_side() {
        let metadata = parse_modrinth_index(
            r#"{
                "dependencies":{"minecraft":"1.20.1","forge":"47.2.0"},
                "files":[
                    {"path":"mods/a.jar","hashes":{"sha1":"abc"},"fileSize":12,"downloads":["https://cdn.example/a.jar"],"env":{"client":"required"}},
                    {"path":"mods/server.jar","hashes":{"sha1":"def"},"downloads":["https://cdn.example/server.jar"],"env":{"client":"unsupported"}}
                ]
            }"#,
        )
        .expect("metadata");
        assert_eq!(metadata.files.len(), 1);
        assert_eq!(metadata.files[0].path.as_deref(), Some("mods/a.jar"));
        assert_eq!(metadata.files[0].urls, ["https://cdn.example/a.jar"]);
        assert_eq!(metadata.files[0].sha1.as_deref(), Some("abc"));
        assert_eq!(metadata.files[0].size, Some(12));
    }

    #[test]
    fn curseforge_manifest_keeps_file_references() {
        let metadata = parse_curseforge_manifest(
            r#"{
                "minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.2.0","primary":true}]},
                "files":[
                    {"projectID":238222,"fileID":1234,"required":true},
                    {"projectID":306770,"fileID":5678,"required":false}
                ]
            }"#,
        )
        .expect("manifest");
        assert_eq!(metadata.files.len(), 2);
        let first = metadata.files[0].reference.as_ref().expect("reference");
        assert_eq!(first.project_id, "238222");
        assert_eq!(first.file_id, "1234");
        assert!(first.required);
        assert!(
            !metadata.files[1]
                .reference
                .as_ref()
                .expect("reference")
                .required
        );
        assert!(
            metadata.files.iter().all(|file| file.path.is_none()),
            "a reference has no destination until it is resolved"
        );
    }

    fn reference_file(project_id: &str, file_id: &str) -> ModpackFile {
        ModpackFile {
            path: None,
            urls: Vec::new(),
            sha1: None,
            size: None,
            reference: Some(ModpackFileRef {
                project_id: project_id.into(),
                file_id: file_id.into(),
                required: true,
            }),
        }
    }

    struct FakeProvider {
        files:
            std::collections::HashMap<(String, String), crate::core::resource::model::ResourceFile>,
        available: bool,
    }

    impl crate::core::resource::provider::ResourceProvider for FakeProvider {
        fn capabilities(&self) -> crate::core::resource::provider::ProviderCapabilities {
            crate::core::resource::provider::ProviderCapabilities {
                source: crate::core::resource::model::ResourceSource::CurseForge,
                available: self.available,
                kinds: vec![crate::core::resource::model::ResourceKind::Mod],
                reason: (!self.available).then(|| "API key is not configured".into()),
            }
        }

        fn search(
            &self,
            _query: crate::core::resource::model::ResourceQuery,
        ) -> crate::core::resource::provider::ProviderFuture<
            crate::core::resource::model::ResourceSearchPage,
        > {
            Box::pin(async { Err(crate::core::error::Error::Download("not used".into())) })
        }

        fn categories(
            &self,
            _kind: crate::core::resource::model::ResourceKind,
        ) -> crate::core::resource::provider::ProviderFuture<
            Vec<crate::core::resource::model::ResourceCategory>,
        > {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn project(
            &self,
            _project_id: String,
        ) -> crate::core::resource::provider::ProviderFuture<
            crate::core::resource::model::ResourceProject,
        > {
            Box::pin(async { Err(crate::core::error::Error::Download("not used".into())) })
        }

        fn files(
            &self,
            _project_id: String,
        ) -> crate::core::resource::provider::ProviderFuture<
            Vec<crate::core::resource::model::ResourceFile>,
        > {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn file(
            &self,
            project_id: String,
            file_id: String,
        ) -> crate::core::resource::provider::ProviderFuture<
            crate::core::resource::model::ResourceFile,
        > {
            let value = self.files.get(&(project_id, file_id)).cloned();
            Box::pin(async move {
                value.ok_or_else(|| crate::core::error::Error::Download("file is gone".into()))
            })
        }
    }

    fn resource_file(name: &str, url: &str) -> crate::core::resource::model::ResourceFile {
        crate::core::resource::model::ResourceFile {
            source: crate::core::resource::model::ResourceSource::CurseForge,
            project_id: "p".into(),
            file_id: "f".into(),
            name: name.into(),
            download_url: url.into(),
            sha1: Some("abc".into()),
            size: Some(12),
            game_versions: Vec::new(),
            loaders: Vec::new(),
            channel: crate::core::resource::model::ReleaseChannel::Release,
            dependencies: Vec::new(),
        }
    }

    #[tokio::test]
    async fn references_resolve_into_the_mods_directory() {
        let mut files = std::collections::HashMap::new();
        files.insert(
            ("238222".to_string(), "1234".to_string()),
            resource_file("sodium.jar", "https://cdn.example/sodium.jar"),
        );
        let provider = FakeProvider {
            files,
            available: true,
        };
        let resolution = resolve_files(vec![reference_file("238222", "1234")], Some(&provider))
            .await
            .expect("resolve");
        assert!(
            resolution.unresolved.is_empty(),
            "{:?}",
            resolution.unresolved
        );
        assert_eq!(resolution.files.len(), 1);
        assert_eq!(resolution.files[0].path.as_deref(), Some("mods/sodium.jar"));
        assert_eq!(resolution.files[0].urls, ["https://cdn.example/sodium.jar"]);
        assert_eq!(resolution.files[0].sha1.as_deref(), Some("abc"));
        assert_eq!(resolution.files[0].size, Some(12));
        assert!(resolution.files[0].reference.is_none());
    }

    #[tokio::test]
    async fn one_missing_reference_does_not_sink_the_pack() {
        let mut files = std::collections::HashMap::new();
        files.insert(
            ("1".to_string(), "2".to_string()),
            resource_file("ok.jar", "https://cdn.example/ok.jar"),
        );
        let provider = FakeProvider {
            files,
            available: true,
        };
        let resolution = resolve_files(
            vec![reference_file("1", "2"), reference_file("9", "9")],
            Some(&provider),
        )
        .await
        .expect("resolve");
        assert_eq!(resolution.files.len(), 1);
        assert_eq!(resolution.unresolved.len(), 1);
        assert!(
            resolution.unresolved[0].contains("9 / 9"),
            "{:?}",
            resolution.unresolved
        );
    }

    #[tokio::test]
    async fn a_reference_without_a_provider_fails_before_installing() {
        let error = resolve_files(vec![reference_file("1", "2")], None)
            .await
            .expect_err("no provider");
        assert!(error.to_string().contains("CurseForge"), "{error}");
    }

    #[tokio::test]
    async fn an_unavailable_provider_reports_its_reason() {
        let provider = FakeProvider {
            files: std::collections::HashMap::new(),
            available: false,
        };
        let error = resolve_files(vec![reference_file("1", "2")], Some(&provider))
            .await
            .expect_err("unavailable");
        assert!(error.to_string().contains("API key"), "{error}");
    }
}
