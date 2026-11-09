use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::download::verify_sha1;
use crate::core::error::{Error, Result};
use crate::core::instance::assets::{AssetIndex, asset_requirements};
use crate::core::loader::client_json::{ArgumentRule, rules_allow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationMode {
    Normal,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileArtifact {
    pub path: PathBuf,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRequirement {
    pub artifact: FileArtifact,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ValidationReport {
    pub missing: Vec<DownloadRequirement>,
    pub checked: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientManifest {
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndexReference>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexReference {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    /// Platform rules. A library that only applies to another OS must not be
    /// demanded here: `1.20.1` ships `java-objc-bridge`, which is macOS-only, and
    /// requiring it makes every instance look 88 files short on Windows.
    #[serde(default)]
    pub rules: Vec<ArgumentRule>,
    /// `{ "windows": "natives-windows-${arch}" }`, the same map the launcher
    /// reads when it extracts natives.
    #[serde(default)]
    pub natives: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub downloads: LibraryDownloads,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<FileArtifactJson>,
    #[serde(default)]
    pub classifiers: BTreeMap<String, FileArtifactJson>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileArtifactJson {
    pub path: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

pub fn validate(
    game_directory: &Path,
    manifest: &ClientManifest,
    asset_index: Option<&AssetIndex>,
    mode: VerificationMode,
) -> Result<ValidationReport> {
    if !game_directory.is_dir() {
        return Err(Error::Config(format!(
            "game directory does not exist: {}",
            game_directory.display()
        )));
    }
    let mut requirements = Vec::new();
    let features = BTreeMap::new();
    for library in &manifest.libraries {
        // Same rule evaluation the launch command uses, so a file that would
        // never be on the classpath is never reported as missing.
        if !rules_allow(&library.rules, &features)? {
            continue;
        }
        if let Some(artifact) = &library.downloads.artifact {
            requirements.push(DownloadRequirement {
                artifact: artifact.clone().into_artifact("libraries"),
                reason: format!("library {}", library.name),
            });
        }
        // `artifact.path` is relative to `libraries/`, exactly as the launch
        // path joins it; the previous code looked for it in the game root and
        // therefore found nothing.
        if let Some(template) = library
            .natives
            .as_ref()
            .and_then(|natives| natives.get(std::env::consts::OS))
        {
            let arch = if std::env::consts::ARCH.contains("64") {
                "64"
            } else {
                "32"
            };
            let classifier = template.replace("${arch}", arch);
            if let Some(artifact) = library.downloads.classifiers.get(&classifier) {
                requirements.push(DownloadRequirement {
                    artifact: artifact.clone().into_artifact("libraries"),
                    reason: format!("native {} ({classifier})", library.name),
                });
            }
        }
    }
    if let Some(index) = asset_index {
        requirements.extend(asset_requirements(index));
    }

    let mut report = ValidationReport::default();
    for requirement in requirements {
        let path = game_directory.join(&requirement.artifact.path);
        if !path.is_file() {
            report.missing.push(requirement);
            continue;
        }
        report.checked += 1;
        if mode == VerificationMode::Full
            && let Some(expected) = requirement.artifact.sha1.as_deref()
            && verify_sha1(&path, Some(expected)).is_err()
        {
            report.missing.push(requirement);
        }
    }
    Ok(report)
}

impl FileArtifactJson {
    /// `base` is the directory the manifest path is relative to: `libraries`
    /// for every artifact a version JSON names.
    fn into_artifact(self, base: &str) -> FileArtifact {
        FileArtifact {
            path: PathBuf::from(base).join(self.path),
            url: self.url,
            sha1: self.sha1,
            size: self.size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ClientManifest, VerificationMode, validate};
    use crate::core::instance::assets::AssetIndex;

    fn manifest() -> ClientManifest {
        serde_json::from_slice(include_bytes!(
            "../../../fixtures/version_manifests/client.json"
        ))
        .expect("client manifest")
    }

    fn assets() -> AssetIndex {
        serde_json::from_slice(include_bytes!(
            "../../../fixtures/version_manifests/asset_index.json"
        ))
        .expect("asset index")
    }

    #[test]
    fn normal_mode_reports_missing_libraries_and_assets_without_hashing() {
        let root = std::env::temp_dir().join(format!("tmcl-validate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("libraries/example")).expect("libraries");
        std::fs::create_dir_all(root.join("assets/objects/ab")).expect("assets");
        std::fs::write(root.join("libraries/example/example.jar"), b"wrong").expect("library");
        std::fs::write(
            root.join("libraries/example/example-natives.jar"),
            b"native",
        )
        .expect("native");
        std::fs::write(root.join("assets/objects/ab/abcdef"), b"wrong").expect("asset");

        let report = validate(
            &root,
            &manifest(),
            Some(&assets()),
            VerificationMode::Normal,
        )
        .expect("normal validation");
        assert_eq!(report.checked, 3);
        assert!(report.missing.is_empty(), "{:?}", report.missing);
        std::fs::remove_dir_all(root).expect("clean validation");
    }

    #[test]
    fn full_mode_reports_existing_files_with_wrong_hash() {
        let root = std::env::temp_dir().join(format!("tmcl-validate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("libraries/example")).expect("libraries");
        std::fs::create_dir_all(root.join("assets/objects/ab")).expect("assets");
        std::fs::write(root.join("libraries/example/example.jar"), b"wrong").expect("library");
        std::fs::write(
            root.join("libraries/example/example-natives.jar"),
            b"native",
        )
        .expect("native");
        std::fs::write(root.join("assets/objects/ab/abcdef"), b"wrong").expect("asset");

        let report = validate(&root, &manifest(), Some(&assets()), VerificationMode::Full)
            .expect("full validation");
        assert_eq!(report.missing.len(), 2);
        std::fs::remove_dir_all(root).expect("clean validation");
    }

    #[test]
    fn a_library_for_another_platform_is_not_required() {
        // `1.20.1` lists `java-objc-bridge` with `allow osx`; on Windows it is
        // never on the classpath, so demanding the file reported every instance
        // as broken.
        let root = std::env::temp_dir().join(format!("tmcl-validate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("libraries/example")).expect("libraries");
        std::fs::write(root.join("libraries/example/example.jar"), b"jar").expect("library");

        let report =
            validate(&root, &manifest(), None, VerificationMode::Normal).expect("validation");

        let reasons = report
            .missing
            .iter()
            .map(|item| item.reason.clone())
            .collect::<Vec<_>>();
        assert!(
            !reasons.iter().any(|reason| reason.contains("macos-only")),
            "{reasons:?}"
        );
        // Only the Windows natives are asked for, never the Linux classifier.
        assert!(
            !reasons
                .iter()
                .any(|reason| reason.contains("natives-linux")),
            "{reasons:?}"
        );
        std::fs::remove_dir_all(root).expect("clean validation");
    }

    #[test]
    fn a_library_artifact_lives_under_the_libraries_directory() {
        // The path in the manifest is relative to `libraries/`, which is how the
        // launch path joins it.
        let root = std::env::temp_dir().join(format!("tmcl-validate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("libraries/example")).expect("libraries");
        std::fs::write(root.join("libraries/example/example.jar"), b"jar").expect("library");

        let report =
            validate(&root, &manifest(), None, VerificationMode::Normal).expect("validation");

        // The jar that is there counts as checked; only the natives are missing.
        assert_eq!(report.checked, 1);
        let paths = report
            .missing
            .iter()
            .map(|item| item.artifact.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            paths
                .iter()
                .all(|path| path.replace('\\', "/").starts_with("libraries/")),
            "{paths:?}"
        );
        std::fs::remove_dir_all(root).expect("clean validation");
    }
}
