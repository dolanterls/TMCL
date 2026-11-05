//! Checking for a newer release, and taking it.
//!
//! The release endpoint is the GitHub "latest release" API. Two fields matter:
//! the tag (to compare versions) and the assets (to find the binary for this
//! machine). The previous version of this module only kept the tag and the
//! release page URL, which is enough to announce a version and not enough to
//! install one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};
use crate::core::version::http::HttpClient;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateAsset {
    pub name: String,
    pub url: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    /// The release page, for someone who wants to read the notes first.
    pub page_url: String,
    pub assets: Vec<UpdateAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<AssetResponse>,
}

#[derive(Debug, Deserialize)]
struct AssetResponse {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
}

pub async fn check_latest(client: &HttpClient, endpoint: &str) -> Result<UpdateInfo> {
    let url = reqwest::Url::parse(endpoint)
        .map_err(|_| Error::Version("update endpoint is not a valid URL".into()))?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err(Error::Version("update endpoint must use HTTP(S)".into()));
    }
    let release: ReleaseResponse = client.get_json(endpoint).await?;
    Ok(UpdateInfo {
        version: release.tag_name,
        page_url: release.html_url,
        assets: release
            .assets
            .into_iter()
            .map(|asset| UpdateAsset {
                name: asset.name,
                url: asset.browser_download_url,
                size: asset.size,
            })
            .collect(),
    })
}

/// The asset built for this machine, if the release ships one.
///
/// Matching is by substring because that is what release pipelines actually
/// produce (`tmcl-x86_64-pc-windows-msvc.exe`, `tmcl-windows-amd64.zip`, …).
/// A plain binary wins over an archive: unpacking is a step this command cannot
/// take back, so it is left to the user.
pub fn platform_asset(assets: &[UpdateAsset]) -> Option<&UpdateAsset> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let os_tokens: &[&str] = match os {
        "windows" => &["windows", "win"],
        "macos" => &["macos", "darwin", "osx"],
        _ => &["linux"],
    };
    let arch_tokens: &[&str] = match arch {
        "x86_64" => &["x86_64", "amd64", "x64"],
        "aarch64" => &["aarch64", "arm64"],
        "x86" => &["i686", "i386", "x86"],
        _ => &[arch],
    };
    let matches = |asset: &&UpdateAsset| {
        let name = asset.name.to_ascii_lowercase();
        os_tokens.iter().any(|token| name.contains(token))
            && arch_tokens.iter().any(|token| name.contains(token))
    };
    let candidates = assets.iter().filter(matches).collect::<Vec<_>>();
    candidates
        .iter()
        .find(|asset| !is_archive(&asset.name))
        .or_else(|| candidates.first())
        .copied()
}

pub fn is_archive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".zip") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz")
}

/// Puts `staged` in place of `current`, keeping the old file beside it.
///
/// Windows refuses to overwrite a running executable but happily renames it, so
/// the sequence is rename-then-write rather than write-then-delete. The old file
/// is returned so the caller can tell the user where it went; a failed copy puts
/// the original back instead of leaving no launcher at all.
pub fn replace_executable(current: &Path, staged: &Path) -> Result<PathBuf> {
    let backup = match current.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => current.with_extension(format!("{extension}.old")),
        None => current.with_extension("old"),
    };
    if backup.exists() {
        std::fs::remove_file(&backup)?;
    }
    std::fs::rename(current, &backup)?;
    match std::fs::copy(staged, current) {
        Ok(_) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(current, std::fs::Permissions::from_mode(0o755));
            }
            let _ = std::fs::remove_file(staged);
            Ok(backup)
        }
        Err(error) => {
            // Put the working launcher back before reporting: a half-copied
            // executable is worse than the old version.
            let _ = std::fs::rename(&backup, current);
            Err(error.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UpdateAsset, check_latest, is_archive, platform_asset, replace_executable};
    use std::path::PathBuf;

    fn asset(name: &str) -> UpdateAsset {
        UpdateAsset {
            name: name.into(),
            url: format!("https://example/{name}"),
            size: Some(1024),
        }
    }

    #[test]
    fn the_asset_for_this_machine_is_chosen_by_name() {
        let assets = [
            asset("tmcl-x86_64-unknown-linux-gnu.tar.gz"),
            asset("tmcl-x86_64-pc-windows-msvc.exe"),
            asset("tmcl-aarch64-apple-darwin"),
        ];
        let chosen = platform_asset(&assets).expect("a matching asset");
        let expected = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => "tmcl-x86_64-pc-windows-msvc.exe",
            ("linux", "x86_64") => "tmcl-x86_64-unknown-linux-gnu.tar.gz",
            ("macos", "aarch64") => "tmcl-aarch64-apple-darwin",
            _ => chosen.name.as_str(),
        };
        assert_eq!(chosen.name, expected);
    }

    #[test]
    fn a_release_without_assets_has_no_platform_asset() {
        assert!(platform_asset(&[]).is_none());
        assert!(platform_asset(&[asset("checksums.txt")]).is_none());
    }

    #[test]
    fn archives_are_recognised_so_a_plain_binary_can_win() {
        assert!(is_archive("tmcl.zip"));
        assert!(is_archive("tmcl.tar.gz"));
        assert!(!is_archive("tmcl.exe"));
        assert!(!is_archive("tmcl"));
    }

    #[test]
    fn replacing_keeps_the_old_executable_beside_the_new_one() {
        let root = std::env::temp_dir().join(format!("tmcl-update-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("root");
        let current: PathBuf = root.join("tmcl.exe");
        let staged: PathBuf = root.join("tmcl.new");
        std::fs::write(&current, b"old").expect("current");
        std::fs::write(&staged, b"new").expect("staged");

        let backup = replace_executable(&current, &staged).expect("replace");

        assert_eq!(std::fs::read(&current).expect("new"), b"new");
        assert_eq!(std::fs::read(&backup).expect("backup"), b"old");
        assert!(!staged.exists(), "the staged copy is consumed");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[tokio::test]
    async fn rejects_invalid_update_endpoint_before_network_access() {
        let client = crate::core::version::http::HttpClient::new().expect("HTTP client");
        assert!(check_latest(&client, "file:///release").await.is_err());
    }
}
