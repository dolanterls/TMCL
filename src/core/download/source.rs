use std::{future::Future, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum DownloadSourceMode {
    #[default]
    Default,
    Official,
    Mirror,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Region {
    ChinaMainland,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadSourceSnapshot {
    pub version_list_source: DownloadSourceMode,
    pub file_download_source: DownloadSourceMode,
    pub region: Region,
    #[serde(default)]
    pub cache_dir: Option<PathBuf>,
    #[serde(default = "default_retry_count")]
    pub retry_count: u8,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for DownloadSourceSnapshot {
    fn default() -> Self {
        Self {
            version_list_source: DownloadSourceMode::Default,
            file_download_source: DownloadSourceMode::Default,
            region: default_region(),
            cache_dir: None,
            retry_count: default_retry_count(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

impl DownloadSourceSnapshot {
    pub fn new(
        version_list_source: DownloadSourceMode,
        file_download_source: DownloadSourceMode,
    ) -> Self {
        Self {
            version_list_source,
            file_download_source,
            region: default_region(),
            ..Self::default()
        }
    }
}

const fn default_retry_count() -> u8 {
    5
}

const fn default_timeout_secs() -> u64 {
    60
}

pub fn default_region() -> Region {
    let configured = std::env::var("TMCL_REGION")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if configured.contains("global") || configured.contains("en_us") || configured.contains("en-us")
    {
        Region::Other
    } else {
        Region::ChinaMainland
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadSource {
    Official,
    Bmclapi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resource {
    VersionManifest,
    VersionDetails { id: String },
    FabricLoader { minecraft_version: String },
    QuiltLoader { minecraft_version: String },
    ForgeMetadata,
    NeoForgeMetadata { minecraft_version: String },
}

impl DownloadSource {
    pub fn url(self, resource: &Resource) -> String {
        match (self, resource) {
            (Self::Official, Resource::VersionManifest) => {
                "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json".into()
            }
            (Self::Bmclapi, Resource::VersionManifest) => {
                "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json".into()
            }
            (Self::Official, Resource::VersionDetails { id }) => {
                format!("https://piston-meta.mojang.com/v1/packages/{id}.json")
            }
            (Self::Bmclapi, Resource::VersionDetails { id }) => {
                format!("https://bmclapi2.bangbang93.com/version/{id}/json")
            }
            (Self::Official, Resource::FabricLoader { minecraft_version }) => {
                format!("https://meta.fabricmc.net/v2/versions/loader/{minecraft_version}")
            }
            (Self::Bmclapi, Resource::FabricLoader { minecraft_version }) => {
                format!("https://meta.fabricmc.net/v2/versions/loader/{minecraft_version}")
            }
            // Quilt has no mirror entry in BMCLAPI; both sources point at the
            // upstream meta rather than pretending a mirror exists.
            (_, Resource::QuiltLoader { minecraft_version }) => {
                format!("https://meta.quiltmc.org/v3/versions/loader/{minecraft_version}")
            }
            (Self::Official, Resource::ForgeMetadata) => {
                "https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json"
                    .into()
            }
            (Self::Bmclapi, Resource::ForgeMetadata) => {
                "https://bmclapi2.bangbang93.com/forge/minecraft".into()
            }
            (Self::Official, Resource::NeoForgeMetadata { minecraft_version }) => {
                let artifact = if minecraft_version == "1.20.1" {
                    "forge"
                } else {
                    "neoforge"
                };
                format!(
                    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/{artifact}/"
                )
            }
            (Self::Bmclapi, Resource::NeoForgeMetadata { minecraft_version }) => {
                format!("https://bmclapi2.bangbang93.com/neoforge/list/{minecraft_version}")
            }
        }
    }
}

pub async fn fetch_with_fallback<T, F, Fut>(sources: &[DownloadSource], mut fetch: F) -> Result<T>
where
    F: FnMut(DownloadSource) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut errors = Vec::new();
    for source in sources {
        match fetch(*source).await {
            Ok(value) => return Ok(value),
            Err(error) => errors.push(format!("{source:?}: {error}")),
        }
    }
    Err(Error::Network(format!(
        "all download sources failed: {}",
        errors.join("; ")
    )))
}

#[cfg(test)]
mod tests {
    use super::{DownloadSource, Resource, fetch_with_fallback};
    use crate::core::error::Error;

    #[tokio::test]
    async fn falls_back_to_the_next_source() {
        let value = fetch_with_fallback(
            &[DownloadSource::Official, DownloadSource::Bmclapi],
            |source| async move {
                match source {
                    DownloadSource::Official => Err(Error::Network("official failed".into())),
                    DownloadSource::Bmclapi => Ok("cached response"),
                }
            },
        )
        .await
        .expect("fallback response");

        assert_eq!(value, "cached response");
    }

    #[tokio::test]
    async fn returns_network_error_when_all_sources_fail() {
        let error = fetch_with_fallback(
            &[DownloadSource::Official, DownloadSource::Bmclapi],
            |_| async { Err::<(), _>(Error::Network("offline".into())) },
        )
        .await
        .expect_err("all sources should fail");

        assert!(
            matches!(error, Error::Network(message) if message.contains("all download sources failed"))
        );
    }

    #[test]
    fn neoforge_metadata_urls_match_the_game_version_and_source() {
        let modern = Resource::NeoForgeMetadata {
            minecraft_version: "1.21.1".into(),
        };
        assert_eq!(
            DownloadSource::Official.url(&modern),
            "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge/"
        );
        assert_eq!(
            DownloadSource::Bmclapi.url(&modern),
            "https://bmclapi2.bangbang93.com/neoforge/list/1.21.1"
        );

        let legacy = Resource::NeoForgeMetadata {
            minecraft_version: "1.20.1".into(),
        };
        assert_eq!(
            DownloadSource::Official.url(&legacy),
            "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/forge/"
        );
        assert_eq!(
            DownloadSource::Bmclapi.url(&legacy),
            "https://bmclapi2.bangbang93.com/neoforge/list/1.20.1"
        );
    }
}
