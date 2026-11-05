use std::cmp::Ordering;
use std::future::Future;

use serde::Deserialize;
use serde_json::Value;

use crate::core::download::{DownloadSource, Resource, fetch_with_fallback};
use crate::core::error::{Error, Result};
use crate::core::version::http::HttpClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderKind {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoaderVersion {
    pub kind: LoaderKind,
    pub minecraft_version: String,
    pub version: String,
    pub stable: bool,
    pub installer_url: Option<String>,
    pub maven: Option<String>,
    pub profile: Option<Value>,
}

pub async fn list_loader_versions(
    client: &HttpClient,
    sources: &[DownloadSource],
    kind: LoaderKind,
    minecraft_version: &str,
) -> Result<Vec<LoaderVersion>> {
    let resource = match kind {
        LoaderKind::Fabric => Resource::FabricLoader {
            minecraft_version: minecraft_version.to_owned(),
        },
        LoaderKind::Quilt => Resource::QuiltLoader {
            minecraft_version: minecraft_version.to_owned(),
        },
        LoaderKind::Forge => Resource::ForgeMetadata,
        LoaderKind::NeoForge => Resource::NeoForgeMetadata {
            minecraft_version: minecraft_version.to_owned(),
        },
    };

    list_loader_versions_with(sources, kind, minecraft_version, |source| {
        let url = source.url(&resource);
        let client = client.clone();
        async move {
            let response = client.get_response(&url).await?;
            response
                .bytes()
                .await
                .map(|bytes| bytes.to_vec())
                .map_err(|error| Error::Network(format!("read {url}: {error}")))
        }
    })
    .await
}

pub async fn list_loader_versions_with<F, Fut>(
    sources: &[DownloadSource],
    kind: LoaderKind,
    minecraft_version: &str,
    mut fetch: F,
) -> Result<Vec<LoaderVersion>>
where
    F: FnMut(DownloadSource) -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
{
    let minecraft_version = minecraft_version.to_owned();
    fetch_with_fallback(sources, |source| {
        let response = fetch(source);
        let minecraft_version = minecraft_version.clone();
        async move {
            let bytes = response.await?;
            parse_loader_metadata(kind, &minecraft_version, &bytes)
        }
    })
    .await
}

#[derive(Debug, Deserialize)]
struct FabricEntry {
    loader: FabricLoader,
    #[serde(default)]
    intermediary: Option<Value>,
    #[serde(default)]
    launcher_meta: Option<Value>,
    #[serde(default)]
    minecraft_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FabricLoader {
    version: String,
    #[serde(default)]
    stable: bool,
    #[serde(default)]
    maven: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ForgeEntry {
    #[serde(alias = "minecraft_version", alias = "gameVersion")]
    mcversion: String,
    version: String,
    #[serde(default = "default_stable")]
    stable: bool,
    #[serde(default)]
    installer: Option<String>,
    #[serde(default)]
    maven: Option<String>,
    #[serde(default)]
    install_profile: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct NeoForgeEntry {
    #[serde(alias = "mcversion", alias = "minecraftVersion", alias = "gameVersion")]
    minecraft_version: String,
    version: String,
    #[serde(default = "default_stable")]
    stable: bool,
    #[serde(default)]
    installer: Option<String>,
    #[serde(default)]
    maven: Option<String>,
    #[serde(default)]
    profile: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NeoForgeMetadata {
    Entries(Vec<NeoForgeEntry>),
    VersionList(NeoForgeVersionList),
}

#[derive(Debug, Deserialize)]
struct NeoForgeVersionList {
    #[serde(rename = "isSnapshot", default)]
    is_snapshot: bool,
    versions: Vec<String>,
}

pub fn parse_loader_metadata(
    kind: LoaderKind,
    minecraft_version: &str,
    bytes: &[u8],
) -> Result<Vec<LoaderVersion>> {
    let mut versions = match kind {
        LoaderKind::Fabric => parse_fabric(minecraft_version, bytes)?,
        LoaderKind::Quilt => parse_quilt(minecraft_version, bytes)?,
        LoaderKind::Forge => serde_json::from_slice::<Vec<ForgeEntry>>(bytes)
            .map_err(|error| parse_error(kind, error))?
            .into_iter()
            .filter(|entry| entry.mcversion == minecraft_version)
            .map(|entry| LoaderVersion {
                kind,
                minecraft_version: entry.mcversion,
                version: entry.version,
                stable: entry.stable,
                installer_url: entry.installer,
                maven: entry.maven,
                profile: entry.install_profile,
            })
            .collect(),
        LoaderKind::NeoForge => parse_neoforge(minecraft_version, bytes)?,
    };

    versions.sort_by(|left, right| {
        right
            .stable
            .cmp(&left.stable)
            .then_with(|| compare_versions(&right.version, &left.version))
    });
    Ok(versions)
}

#[derive(Debug, Deserialize)]
struct QuiltEntry {
    loader: QuiltLoader,
}

#[derive(Debug, Deserialize)]
struct QuiltLoader {
    version: String,
    #[serde(default)]
    maven: Option<String>,
}

/// Quilt's meta lists no stability flag; a pre-release has a suffix
/// (`0.20.0-beta.9`), a release does not.
fn parse_quilt(minecraft_version: &str, bytes: &[u8]) -> Result<Vec<LoaderVersion>> {
    let entries = serde_json::from_slice::<Vec<QuiltEntry>>(bytes)
        .map_err(|error| parse_error(LoaderKind::Quilt, error))?;
    Ok(entries
        .into_iter()
        .map(|entry| LoaderVersion {
            kind: LoaderKind::Quilt,
            minecraft_version: minecraft_version.to_owned(),
            stable: !entry.loader.version.contains('-'),
            version: entry.loader.version,
            installer_url: None,
            maven: entry.loader.maven,
            profile: None,
        })
        .collect())
}

fn parse_neoforge(minecraft_version: &str, bytes: &[u8]) -> Result<Vec<LoaderVersion>> {
    let metadata = serde_json::from_slice::<NeoForgeMetadata>(bytes)
        .map_err(|error| parse_error(LoaderKind::NeoForge, error))?;

    Ok(match metadata {
        NeoForgeMetadata::Entries(entries) => entries
            .into_iter()
            .filter(|entry| entry.minecraft_version == minecraft_version)
            .map(|entry| {
                let version =
                    if minecraft_version == "1.20.1" && !entry.version.starts_with("1.20.1-") {
                        format!("1.20.1-{}", entry.version)
                    } else {
                        entry.version
                    };
                LoaderVersion {
                    kind: LoaderKind::NeoForge,
                    minecraft_version: minecraft_version.into(),
                    stable: entry.stable && is_stable_neoforge_version(&version),
                    version,
                    installer_url: entry.installer,
                    maven: entry.maven,
                    profile: entry.profile,
                }
            })
            .collect(),
        NeoForgeMetadata::VersionList(metadata) => metadata
            .versions
            .into_iter()
            .filter_map(|version| {
                let stable = if minecraft_version == "1.20.1" {
                    legacy_neoforge_version_matches(&version).then_some(!metadata.is_snapshot)
                } else {
                    neoforge_version_matches_game(&version, minecraft_version)
                }?;
                Some(LoaderVersion {
                    kind: LoaderKind::NeoForge,
                    minecraft_version: minecraft_version.into(),
                    version,
                    stable,
                    installer_url: None,
                    maven: None,
                    profile: None,
                })
            })
            .collect(),
    })
}

fn legacy_neoforge_version_matches(version: &str) -> bool {
    let version = version.strip_prefix("1.20.1-").unwrap_or(version);
    let parts = version.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn neoforge_version_matches_game(version: &str, minecraft_version: &str) -> Option<bool> {
    if let Some((game_version, stable)) = april_fools_neoforge_game_version(version) {
        return (game_version == minecraft_version).then_some(stable);
    }

    let (version, maven_suffix) = match version.split_once('+') {
        Some((version, suffix)) => (version, Some(suffix)),
        None => (version, None),
    };
    let (version, prerelease) = match version.split_once('-') {
        Some((version, prerelease)) if valid_neoforge_prerelease(prerelease) => {
            (version, Some(prerelease))
        }
        Some(_) => return None,
        None => (version, None),
    };
    let parts = version.split('.').collect::<Vec<_>>();
    if !(3..=4).contains(&parts.len())
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let major = parts[0].parse::<u32>().ok()?;
    let minor = parts[1].parse::<u32>().ok()?;
    let patch = parts[2].parse::<u32>().ok()?;
    let mut game_version = if minecraft_version.starts_with("1.") {
        format!("1.{major}.{minor}")
    } else if patch == 0 {
        format!("{major}.{minor}")
    } else {
        format!("{major}.{minor}.{patch}")
    };

    if let Some(suffix) = maven_suffix {
        let (kind, number) = suffix.split_once('-')?;
        if !matches!(kind, "snapshot" | "pre" | "rc")
            || number.is_empty()
            || !number.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        game_version.push_str(&format!("-{kind}-{number}"));
    }

    (game_version == minecraft_version).then_some(prerelease.is_none())
}

fn valid_neoforge_prerelease(prerelease: &str) -> bool {
    let (kind, number) = match prerelease.split_once('.') {
        Some((kind, number)) => (kind, Some(number)),
        None => (prerelease, None),
    };
    matches!(kind, "alpha" | "beta" | "rc")
        && number.is_none_or(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn april_fools_neoforge_game_version(version: &str) -> Option<(String, bool)> {
    let version = version.strip_prefix("0.")?;
    let (game_version, revision) = version.split_once('.')?;
    let digit_count = game_version.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count == 0
        || digit_count == game_version.len()
        || !game_version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return None;
    }
    let (revision, stable) = match revision.strip_suffix("-beta") {
        Some(revision) => (revision, false),
        None => (revision, true),
    };
    if revision.is_empty() || !revision.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((game_version.into(), stable))
}

fn is_stable_neoforge_version(version: &str) -> bool {
    !["-alpha", "-beta", "-rc", "+snapshot", "+pre"]
        .iter()
        .any(|suffix| version.contains(suffix))
}

fn parse_fabric(minecraft_version: &str, bytes: &[u8]) -> Result<Vec<LoaderVersion>> {
    serde_json::from_slice::<Vec<FabricEntry>>(bytes)
        .map_err(|error| parse_error(LoaderKind::Fabric, error))
        .map(|entries| {
            entries
                .into_iter()
                .filter(|entry| {
                    entry
                        .minecraft_version
                        .as_deref()
                        .is_none_or(|version| version == minecraft_version)
                })
                .map(|entry| LoaderVersion {
                    kind: LoaderKind::Fabric,
                    minecraft_version: minecraft_version.into(),
                    version: entry.loader.version,
                    stable: entry.loader.stable,
                    installer_url: None,
                    maven: entry.loader.maven,
                    profile: entry.launcher_meta.or(entry.intermediary),
                })
                .collect()
        })
}

fn default_stable() -> bool {
    true
}

fn parse_error(kind: LoaderKind, error: serde_json::Error) -> Error {
    Error::Version(format!("parse {kind:?} metadata: {error}"))
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let mut left = left.split('.').map(numeric_part);
    let mut right = right.split('.').map(numeric_part);
    loop {
        match (left.next(), right.next()) {
            (Some(left), Some(right)) => match left.cmp(&right) {
                Ordering::Equal => continue,
                ordering => return ordering,
            },
            (Some(left), None) => return left.cmp(&0),
            (None, Some(right)) => return 0.cmp(&right),
            (None, None) => return Ordering::Equal,
        }
    }
}

fn numeric_part(part: &str) -> u64 {
    part.split('-')
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{LoaderKind, list_loader_versions_with, parse_loader_metadata};
    use crate::core::download::DownloadSource;
    use crate::core::error::Error;

    #[test]
    fn quilt_entries_become_versions_with_local_prerelease_stability() {
        let versions = parse_loader_metadata(
            LoaderKind::Quilt,
            "1.20.1",
            br#"[
                {"loader":{"version":"0.27.0","maven":"org.quiltmc:quilt-loader:0.27.0"}},
                {"loader":{"version":"0.20.0-beta.9","maven":"org.quiltmc:quilt-loader:0.20.0-beta.9"}}
            ]"#,
        )
        .expect("quilt metadata");
        assert_eq!(versions.len(), 2);
        // Stable first, then newest: the list is what the version picker shows.
        assert_eq!(versions[0].version, "0.27.0");
        assert!(versions[0].stable);
        assert_eq!(
            versions[0].maven.as_deref(),
            Some("org.quiltmc:quilt-loader:0.27.0")
        );
        assert!(!versions[1].stable);
    }

    #[tokio::test]
    async fn fetches_only_loader_versions_compatible_with_the_game_version() {
        let versions = list_loader_versions_with(
            &[DownloadSource::Official],
            LoaderKind::Fabric,
            "1.21.1",
            |_| async {
                Ok(include_bytes!("../../../fixtures/loader_metadata/fabric.json").to_vec())
            },
        )
        .await
        .expect("parse Fabric metadata");

        assert_eq!(versions.len(), 2);
        assert!(
            versions
                .iter()
                .all(|version| version.minecraft_version == "1.21.1")
        );
    }

    #[tokio::test]
    async fn parse_failure_on_official_metadata_falls_back_to_bmclapi() {
        let versions = list_loader_versions_with(
            &[DownloadSource::Official, DownloadSource::Bmclapi],
            LoaderKind::Forge,
            "1.21.1",
            |source| async move {
                match source {
                    DownloadSource::Official => Ok(b"not JSON".to_vec()),
                    DownloadSource::Bmclapi => {
                        Ok(include_bytes!("../../../fixtures/loader_metadata/forge.json").to_vec())
                    }
                }
            },
        )
        .await
        .expect("BMCLAPI metadata should be used after parse failure");

        assert_eq!(versions[0].version, "52.0.1");
    }

    #[tokio::test]
    async fn neoforge_xml_official_failure_falls_back_to_version_scoped_bmclapi_data() {
        let versions = list_loader_versions_with(
            &[DownloadSource::Official, DownloadSource::Bmclapi],
            LoaderKind::NeoForge,
            "1.21.1",
            |source| async move {
                match source {
                    DownloadSource::Official => Ok(
                        br#"<?xml version="1.0"?><metadata><versioning></versioning></metadata>"#
                            .to_vec(),
                    ),
                    DownloadSource::Bmclapi => Ok(br#"[{"rawVersion":"neoforge-21.1.247","version":"21.1.247","mcversion":"1.21.1"}]"#.to_vec()),
                }
            },
        )
        .await
        .expect("use BMCLAPI metadata after the official payload fails to parse");

        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, "21.1.247");
    }

    #[tokio::test]
    async fn reports_an_error_when_every_loader_metadata_source_fails() {
        let error = list_loader_versions_with(
            &[DownloadSource::Official, DownloadSource::Bmclapi],
            LoaderKind::NeoForge,
            "1.21.1",
            |_| async { Err::<Vec<u8>, _>(Error::Network("offline".into())) },
        )
        .await
        .expect_err("all failed sources should return an error");

        assert!(
            matches!(error, Error::Network(message) if message.contains("all download sources failed"))
        );
    }

    #[test]
    fn parses_official_neoforge_version_lists_for_legacy_and_modern_versions() {
        let legacy = parse_loader_metadata(
            LoaderKind::NeoForge,
            "1.20.1",
            br#"{"isSnapshot":false,"versions":["1.20.1-47.2.0","1.20.1-47.1.0","1.20.2-48.0.1"]}"#,
        )
        .expect("parse legacy official NeoForge metadata");
        assert_eq!(
            legacy
                .iter()
                .map(|entry| entry.version.as_str())
                .collect::<Vec<_>>(),
            ["1.20.1-47.2.0", "1.20.1-47.1.0"]
        );

        let modern = parse_loader_metadata(
            LoaderKind::NeoForge,
            "1.21.1",
            br#"{"isSnapshot":false,"versions":["21.1.247","21.1.23","21.2.1","20.6.1"]}"#,
        )
        .expect("parse modern official NeoForge metadata");
        assert_eq!(
            modern
                .iter()
                .map(|entry| entry.version.as_str())
                .collect::<Vec<_>>(),
            ["21.1.247", "21.1.23"]
        );
    }

    #[test]
    fn parses_official_neoforge_snapshot_and_april_fools_versions() {
        let versions = parse_loader_metadata(
            LoaderKind::NeoForge,
            "26.3",
            br#"{"isSnapshot":false,"versions":["26.3.0.10-beta","26.3.0.8-beta","26.2.0.3","0.25w14craftmine.3-beta"]}"#,
        )
        .expect("parse 26.x official NeoForge metadata");

        assert_eq!(
            versions
                .iter()
                .map(|entry| entry.version.as_str())
                .collect::<Vec<_>>(),
            ["26.3.0.10-beta", "26.3.0.8-beta"]
        );
        assert!(versions.iter().all(|entry| !entry.stable));

        let april_fools = parse_loader_metadata(
            LoaderKind::NeoForge,
            "25w14craftmine",
            br#"{"isSnapshot":false,"versions":["0.25w14craftmine.3-beta","0.25w14craftmine.2-beta","21.1.247"]}"#,
        )
        .expect("parse April Fools official NeoForge metadata");
        assert_eq!(april_fools.len(), 2);
        assert_eq!(april_fools[0].version, "0.25w14craftmine.3-beta");
        assert!(april_fools.iter().all(|entry| !entry.stable));
    }

    #[test]
    fn parses_bmclapi_neoforge_metadata_with_mcversion_field() {
        let versions = parse_loader_metadata(
            LoaderKind::NeoForge,
            "1.21.1",
            br#"[{"rawVersion":"neoforge-21.1.247","version":"21.1.247","mcversion":"1.21.1"},{"rawVersion":"neoforge-21.2.1","version":"21.2.1","mcversion":"1.21.2"}]"#,
        )
        .expect("parse BMCLAPI NeoForge metadata");

        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, "21.1.247");
        assert_eq!(versions[0].minecraft_version, "1.21.1");

        let legacy = parse_loader_metadata(
            LoaderKind::NeoForge,
            "1.20.1",
            br#"[{"rawVersion":"1.20.1-47.1.5","version":"47.1.5","mcversion":"1.20.1"}]"#,
        )
        .expect("parse legacy BMCLAPI NeoForge metadata");
        assert_eq!(legacy[0].version, "1.20.1-47.1.5");
    }

    #[test]
    fn filters_and_sorts_fabric_versions() {
        let versions = parse_loader_metadata(
            LoaderKind::Fabric,
            "1.21.1",
            include_bytes!("../../../fixtures/loader_metadata/fabric.json"),
        )
        .expect("parse Fabric metadata");

        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, "0.16.5");
        assert_eq!(versions[1].version, "0.16.4");
        assert_eq!(
            versions[0].maven.as_deref(),
            Some("net.fabricmc:fabric-loader:0.16.5")
        );
    }

    #[test]
    fn filters_and_sorts_forge_and_neoforge_versions() {
        let forge = parse_loader_metadata(
            LoaderKind::Forge,
            "1.21.1",
            include_bytes!("../../../fixtures/loader_metadata/forge.json"),
        )
        .expect("parse Forge metadata");
        let neoforge = parse_loader_metadata(
            LoaderKind::NeoForge,
            "1.21.1",
            include_bytes!("../../../fixtures/loader_metadata/neoforge.json"),
        )
        .expect("parse NeoForge metadata");

        assert_eq!(forge[0].version, "52.0.1");
        assert_eq!(
            forge[0].installer_url.as_deref(),
            Some("https://example.test/forge.jar")
        );
        assert_eq!(neoforge[0].version, "21.1.1");
        assert!(neoforge[0].profile.is_some());
    }
}
