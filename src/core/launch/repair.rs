//! Turns a list of missing files into downloads.
//!
//! Only files whose URL can be proven from the merged version metadata are
//! queued; anything else is counted as skipped so the caller can report it
//! instead of guessing a URL.

use std::path::{Path, PathBuf};

use crate::core::launch::plan::FileRole;
use crate::core::launch::preflight::MissingFile;
use crate::core::launch::validate::{DownloadRequirement, FileArtifact};
use crate::core::loader::client_json::ClientJson;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepairPlan {
    pub downloads: Vec<DownloadRequirement>,
    pub skipped: usize,
    /// Paths of the skipped entries, so the failure message can name them
    /// instead of only reporting a count.
    pub skipped_paths: Vec<PathBuf>,
}

pub fn requirements(missing: &[MissingFile], client: &ClientJson) -> RepairPlan {
    let mut plan = RepairPlan::default();
    for file in missing {
        match file.role {
            FileRole::Library => match library_artifact(client, file.relative.as_deref()) {
                Some((url, sha1, size)) => plan.downloads.push(DownloadRequirement {
                    artifact: FileArtifact {
                        path: file.path.clone(),
                        url,
                        sha1,
                        size,
                    },
                    reason: format!("library {}", file.path.display()),
                }),
                None => {
                    plan.skipped += 1;
                    plan.skipped_paths.push(skipped_path(file));
                }
            },
            FileRole::ClientJar => match client.downloads.client.as_ref() {
                Some(artifact) => plan.downloads.push(DownloadRequirement {
                    artifact: FileArtifact {
                        path: file.path.clone(),
                        url: artifact.url.clone(),
                        sha1: Some(artifact.sha1.clone()),
                        size: artifact.size,
                    },
                    reason: "client jar".into(),
                }),
                None => {
                    plan.skipped += 1;
                    plan.skipped_paths.push(skipped_path(file));
                }
            },
            FileRole::Asset | FileRole::Native | FileRole::Java => {
                plan.skipped += 1;
                plan.skipped_paths.push(skipped_path(file));
            }
        }
    }
    plan
}

/// Prefer the metadata-relative path: it is what the user sees in the JSON and
/// what the error message can name without a long absolute prefix.
fn skipped_path(file: &MissingFile) -> PathBuf {
    file.relative.clone().unwrap_or_else(|| file.path.clone())
}

fn library_artifact(
    client: &ClientJson,
    relative: Option<&Path>,
) -> Option<(String, Option<String>, Option<u64>)> {
    let relative = relative?.to_string_lossy().replace('\\', "/");
    let relative = relative.strip_prefix("libraries/").unwrap_or(&relative);
    for library in &client.libraries {
        if let Some(artifact) = library.downloads.artifact.as_ref()
            && normalize(&artifact.path) == normalize(relative)
        {
            return Some((artifact.url.clone(), artifact.sha1.clone(), artifact.size));
        }
        for artifact in library.downloads.classifiers.values() {
            if normalize(&artifact.path) == normalize(relative) {
                return Some((artifact.url.clone(), artifact.sha1.clone(), artifact.size));
            }
        }
    }
    None
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::requirements;
    use crate::core::launch::plan::FileRole;
    use crate::core::launch::preflight::MissingFile;
    use crate::core::loader::client_json::parse;

    fn client() -> crate::core::loader::client_json::ClientJson {
        parse(
            br#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main","downloads":{"client":{"url":"https://example.invalid/client.jar","sha1":"abc123","size":42}},"libraries":[{"name":"org.example:library:1.0","downloads":{"artifact":{"path":"org/example/library/1.0/library-1.0.jar","url":"https://example.invalid/library.jar","sha1":"def456"}}}]}"#,
        )
        .expect("client JSON")
    }

    #[test]
    fn builds_download_requirement_for_missing_library_with_sha1() {
        let missing = vec![MissingFile {
            path: PathBuf::from("C:/Games/libraries/org/example/library/1.0/library-1.0.jar"),
            relative: Some(PathBuf::from(
                "libraries/org/example/library/1.0/library-1.0.jar",
            )),
            role: FileRole::Library,
        }];

        let plan = requirements(&missing, &client());

        assert_eq!(plan.downloads.len(), 1);
        let artifact = &plan.downloads[0].artifact;
        assert_eq!(artifact.url, "https://example.invalid/library.jar");
        assert_eq!(artifact.sha1.as_deref(), Some("def456"));
        assert!(artifact.path.ends_with("library-1.0.jar"));
    }

    #[test]
    fn missing_client_jar_uses_the_downloads_client_url() {
        let missing = vec![MissingFile {
            path: PathBuf::from("C:/Games/versions/pack/pack.jar"),
            relative: None,
            role: FileRole::ClientJar,
        }];

        let plan = requirements(&missing, &client());

        assert_eq!(plan.downloads.len(), 1);
        assert_eq!(
            plan.downloads[0].artifact.url,
            "https://example.invalid/client.jar"
        );
        assert_eq!(plan.downloads[0].artifact.sha1.as_deref(), Some("abc123"));
    }

    #[test]
    fn unknown_entries_are_skipped_not_failed() {
        let missing = vec![
            MissingFile {
                path: PathBuf::from("C:/Games/libraries/unknown.jar"),
                relative: Some(PathBuf::from("libraries/unknown.jar")),
                role: FileRole::Library,
            },
            MissingFile {
                path: PathBuf::from("C:/Games/assets/objects/ab/cdef"),
                relative: None,
                role: FileRole::Asset,
            },
        ];

        let plan = requirements(&missing, &client());

        assert!(plan.downloads.is_empty());
        assert_eq!(plan.skipped, 2);
        assert_eq!(
            plan.skipped_paths,
            vec![
                PathBuf::from("libraries/unknown.jar"),
                PathBuf::from("C:/Games/assets/objects/ab/cdef"),
            ],
            "the relative path is preferred for the user-facing message"
        );
    }
}
