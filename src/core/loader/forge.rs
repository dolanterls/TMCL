use std::collections::BTreeMap;

use serde::Deserialize;

use crate::core::error::{Error, Result};
use crate::core::loader::{LibraryPlan, library_plan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeInstallPlan {
    pub installer_url: String,
    pub installer_path: std::path::PathBuf,
    pub libraries: Vec<LibraryPlan>,
    pub processors: Vec<ProcessorPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorPlan {
    pub jar: String,
    pub args: Vec<String>,
    pub classpath: Vec<String>,
    pub sides: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ForgeProfile {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub libraries: Vec<ForgeLibrary>,
    #[serde(default)]
    pub processors: Vec<ForgeProcessor>,
    #[serde(default)]
    pub data: BTreeMap<String, serde_json::Value>,
    #[serde(rename = "versionInfo", default)]
    pub version_info: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ForgeLibrary {
    pub name: String,
    #[serde(default = "forge_maven_url")]
    pub url: String,
    #[serde(default)]
    pub downloads: ForgeLibraryDownloads,
}

#[derive(Debug, Default, Deserialize)]
pub struct ForgeLibraryDownloads {
    #[serde(default)]
    pub artifact: Option<ForgeLibraryArtifact>,
}

#[derive(Debug, Deserialize)]
pub struct ForgeLibraryArtifact {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ForgeProcessor {
    pub jar: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub sides: Option<Vec<String>>,
}

fn forge_maven_url() -> String {
    "https://maven.minecraftforge.net".into()
}

pub fn parse_profile(bytes: &[u8]) -> Result<ForgeProfile> {
    serde_json::from_slice(bytes)
        .map_err(|error| Error::Loader(format!("parse Forge installer profile: {error}")))
}

pub fn install_plan(
    minecraft_version: &str,
    loader_version: &str,
    installer_url: &str,
    installer_path: std::path::PathBuf,
    bytes: &[u8],
) -> Result<ForgeInstallPlan> {
    let profile = parse_profile(bytes)?;
    let libraries = profile
        .libraries
        .into_iter()
        .filter_map(|library| library_plan(&library.name, &library.url))
        .collect();
    let processors = profile
        .processors
        .into_iter()
        .map(|processor| ProcessorPlan {
            jar: processor.jar,
            args: processor.args,
            classpath: processor.classpath,
            sides: processor.sides,
        })
        .collect();
    let _ = (minecraft_version, loader_version);
    Ok(ForgeInstallPlan {
        installer_url: installer_url.into(),
        installer_path,
        libraries,
        processors,
    })
}

#[cfg(test)]
mod tests {
    use super::{install_plan, parse_profile};

    #[test]
    fn parses_profile_libraries_and_processors() {
        let plan = install_plan(
            "1.21.1",
            "52.0.1",
            "https://example.test/forge-installer.jar",
            "installer.jar".into(),
            br#"{"libraries":[{"name":"net.minecraftforge:forge:1.21.1-52.0.1"}],"processors":[{"jar":"net.minecraftforge:binarypatcher:1.0","args":["--clean"],"classpath":["a:b:1"]}]}"#,
        )
        .expect("Forge plan");
        assert_eq!(
            plan.libraries[0].path,
            std::path::PathBuf::from(
                "net/minecraftforge/forge/1.21.1-52.0.1/forge-1.21.1-52.0.1.jar"
            )
        );
        assert_eq!(plan.processors[0].args, vec!["--clean"]);
    }

    #[test]
    fn preserves_library_download_integrity_metadata() {
        let profile = parse_profile(
            br#"{"libraries":[{"name":"net.neoforged.installertools:installertools:4.0.17:fatjar","downloads":{"artifact":{"url":"https://maven.neoforged.net/releases/net/neoforged/installertools/installertools/4.0.17/installertools-4.0.17-fatjar.jar","sha1":"d0e22dfef7eb7eeffa20fee4c393b3f7cad0323b","size":814132}}}]}"#,
        )
        .expect("Forge profile");
        let artifact = profile.libraries[0]
            .downloads
            .artifact
            .as_ref()
            .expect("library artifact metadata");
        assert_eq!(
            artifact.sha1.as_deref(),
            Some("d0e22dfef7eb7eeffa20fee4c393b3f7cad0323b")
        );
        assert_eq!(artifact.size, Some(814132));
    }
}
