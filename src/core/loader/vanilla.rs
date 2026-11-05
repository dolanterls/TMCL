use std::path::{Path, PathBuf};

use crate::core::error::Result;
use crate::core::launch::validate::{DownloadRequirement, FileArtifact};
use crate::core::loader::client_json::ClientJson;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaInstallPlan {
    pub downloads: Vec<DownloadRequirement>,
    pub client_json: PathBuf,
    pub client_jar: PathBuf,
}

pub fn install_plan(
    game_directory: &Path,
    instance_name: &str,
    client: &ClientJson,
    _client_json_url: &str,
) -> Result<VanillaInstallPlan> {
    let version_dir = game_directory.join("versions").join(instance_name);
    let client_json_path = version_dir.join(format!("{instance_name}.json"));
    let client_jar_path = version_dir.join(format!("{instance_name}.jar"));
    let mut downloads = Vec::new();
    if let Some(client) = &client.downloads.client {
        downloads.push(DownloadRequirement {
            artifact: FileArtifact {
                path: client_jar_path
                    .strip_prefix(game_directory)
                    .unwrap_or(&client_jar_path)
                    .to_path_buf(),
                url: client.url.clone(),
                sha1: Some(client.sha1.clone()),
                size: client.size,
            },
            reason: "Minecraft client".into(),
        });
    }
    for library in &client.libraries {
        if let Some(artifact) = &library.downloads.artifact {
            downloads.push(DownloadRequirement {
                artifact: FileArtifact {
                    path: PathBuf::from(&artifact.path),
                    url: artifact.url.clone(),
                    sha1: artifact.sha1.clone(),
                    size: artifact.size,
                },
                reason: format!("library {}", library.name),
            });
        }
    }
    Ok(VanillaInstallPlan {
        downloads,
        client_json: client_json_path,
        client_jar: client_jar_path,
    })
}

#[cfg(test)]
mod tests {
    use super::install_plan;
    use crate::core::loader::client_json::parse;

    #[test]
    fn creates_client_and_library_download_targets() {
        let client = parse(
            br#"{"id":"1.21.1","mainClass":"main","downloads":{"client":{"url":"https://example.test/client.jar","sha1":"abc"}},"libraries":[{"name":"x:y:1","downloads":{"artifact":{"path":"libraries/x.jar","url":"https://example.test/x.jar","sha1":"def"}}}]}"#,
        )
        .expect("client JSON");
        let plan = install_plan(
            std::path::Path::new("C:/Games"),
            "1.21.1",
            &client,
            "https://example.test/client.json",
        )
        .expect("Vanilla plan");
        assert_eq!(plan.downloads.len(), 2);
        assert!(plan.client_jar.ends_with("versions\\1.21.1\\1.21.1.jar"));
        assert_eq!(plan.downloads[0].artifact.sha1.as_deref(), Some("abc"));
    }
}
