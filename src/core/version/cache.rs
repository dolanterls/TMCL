use std::future::Future;
use std::path::{Path, PathBuf};

use crate::core::config::storage::{read_json, write_json_atomic};
use crate::core::download::{DownloadSource, Resource, fetch_with_fallback};
use crate::core::error::{Error, Result};
use crate::core::version::http::HttpClient;
use crate::core::version::models::{VersionManifest, VersionSummary};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionCatalog {
    pub manifest: VersionManifest,
    pub from_cache: bool,
}

#[derive(Debug, Clone)]
pub struct VersionCache {
    path: PathBuf,
}

impl VersionCache {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn save(&self, manifest: &VersionManifest) -> Result<()> {
        write_json_atomic(&self.path, manifest)
    }

    pub fn load(&self) -> Result<VersionManifest> {
        read_json(&self.path)
    }
}

pub async fn list_versions(
    client: &HttpClient,
    sources: &[DownloadSource],
    cache: &VersionCache,
) -> Result<VersionCatalog> {
    list_versions_with(sources, cache, |source| async move {
        client
            .get_json(&source.url(&Resource::VersionManifest))
            .await
    })
    .await
}

pub async fn list_versions_with<F, Fut>(
    sources: &[DownloadSource],
    cache: &VersionCache,
    fetch: F,
) -> Result<VersionCatalog>
where
    F: FnMut(DownloadSource) -> Fut,
    Fut: Future<Output = Result<VersionManifest>>,
{
    match fetch_with_fallback(sources, fetch).await {
        Ok(manifest) => {
            cache.save(&manifest)?;
            Ok(VersionCatalog {
                manifest,
                from_cache: false,
            })
        }
        Err(network_error) => match cache.load() {
            Ok(manifest) => Ok(VersionCatalog {
                manifest,
                from_cache: true,
            }),
            Err(_) => Err(Error::Version(format!(
                "unable to load Minecraft versions: {network_error}"
            ))),
        },
    }
}

pub fn get_version<'a>(catalog: &'a VersionCatalog, id: &str) -> Result<&'a VersionSummary> {
    catalog
        .manifest
        .versions
        .iter()
        .find(|version| version.id == id)
        .ok_or_else(|| Error::Version(format!("Minecraft version not found: {id}")))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{VersionCache, list_versions_with};
    use crate::core::download::DownloadSource;
    use crate::core::error::Error;
    use crate::core::version::manifest::parse_manifest;

    fn temp_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "tmcl-version-cache-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ))
    }

    fn fixture() -> crate::core::version::models::VersionManifest {
        parse_manifest(include_bytes!(
            "../../../fixtures/version_manifests/version_manifest_v2.json"
        ))
        .expect("parse fixture")
    }

    #[tokio::test]
    async fn writes_reads_and_falls_back_to_cache() {
        let root = temp_path();
        let cache = VersionCache::new(root.join("manifest.json"));
        let expected = fixture();
        cache.save(&expected).expect("write cache");
        assert_eq!(cache.load().expect("read cache"), expected);

        let catalog = list_versions_with(&[DownloadSource::Official], &cache, |_| async {
            Err::<_, _>(Error::Network("offline".into()))
        })
        .await
        .expect("cached catalog");
        assert!(catalog.from_cache);
        assert_eq!(catalog.manifest, expected);

        std::fs::remove_dir_all(root).expect("clean cache");
    }

    #[tokio::test]
    async fn reports_version_error_without_network_or_cache() {
        let root = temp_path();
        let cache = VersionCache::new(root.join("missing.json"));
        let error = list_versions_with(&[DownloadSource::Official], &cache, |_| async {
            Err::<_, _>(Error::Network("offline".into()))
        })
        .await
        .expect_err("missing cache should fail");

        assert!(matches!(error, Error::Version(message) if message.contains("unable to load")));
        let _ = std::fs::remove_dir_all(root);
    }
}
