use serde::Deserialize;

use crate::core::error::{Error, Result};
use crate::core::loader::{LibraryPlan, library_plan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FabricInstallPlan {
    pub main_class: String,
    pub libraries: Vec<LibraryPlan>,
}

#[derive(Debug, Deserialize)]
struct FabricMeta {
    loader: FabricLoader,
    #[serde(default)]
    intermediary: Option<FabricMaven>,
    #[serde(rename = "launcherMeta")]
    launcher_meta: FabricLauncherMeta,
}

#[derive(Debug, Deserialize)]
struct FabricLoader {
    version: String,
    #[serde(default)]
    maven: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FabricMaven {
    maven: String,
}

#[derive(Debug, Deserialize)]
struct FabricLauncherMeta {
    #[serde(rename = "mainClass")]
    main_class: String,
    #[serde(default)]
    libraries: FabricLibraries,
}

#[derive(Debug, Default, Deserialize)]
struct FabricLibraries {
    #[serde(default)]
    common: Vec<FabricLibrary>,
    #[serde(default)]
    client: Vec<FabricLibrary>,
}

#[derive(Debug, Deserialize)]
struct FabricLibrary {
    name: String,
    #[serde(default = "fabric_maven_url")]
    url: String,
}

fn fabric_maven_url() -> String {
    "https://maven.fabricmc.net".into()
}

pub fn install_plan(bytes: &[u8]) -> Result<FabricInstallPlan> {
    let metadata: FabricMeta = serde_json::from_slice(bytes)
        .map_err(|error| Error::Loader(format!("parse Fabric launcher meta: {error}")))?;
    let mut libraries = Vec::new();
    if let Some(coordinate) = metadata.loader.maven.as_deref() {
        if let Some(library) = library_plan(coordinate, "https://maven.fabricmc.net") {
            libraries.push(library);
        }
    } else if let Some(library) = library_plan(
        &format!("net.fabricmc:fabric-loader:{}", metadata.loader.version),
        "https://maven.fabricmc.net",
    ) {
        libraries.push(library);
    }
    if let Some(intermediary) = metadata.intermediary {
        if let Some(library) = library_plan(&intermediary.maven, "https://maven.fabricmc.net") {
            libraries.push(library);
        }
    }
    for library in metadata
        .launcher_meta
        .libraries
        .common
        .into_iter()
        .chain(metadata.launcher_meta.libraries.client)
    {
        if let Some(plan) = library_plan(&library.name, &library.url) {
            libraries.push(plan);
        }
    }
    Ok(FabricInstallPlan {
        main_class: metadata.launcher_meta.main_class,
        libraries,
    })
}

#[cfg(test)]
mod tests {
    use super::install_plan;

    #[test]
    fn merges_fabric_loader_intermediary_and_client_libraries() {
        let plan = install_plan(
            br#"{"loader":{"version":"0.16.5","maven":"net.fabricmc:fabric-loader:0.16.5"},"intermediary":{"maven":"net.fabricmc:intermediary:1.21.1"},"launcherMeta":{"mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":{"common":[{"name":"net.fabricmc:sponge-mixin:0.13.3","url":"https://repo.example"}],"client":[{"name":"net.fabricmc:tiny-remapper:0.10.0"}]}}}"#,
        )
        .expect("Fabric plan");
        assert_eq!(
            plan.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(plan.libraries.len(), 4);
        assert!(
            plan.libraries[0]
                .path
                .ends_with("fabric-loader\\0.16.5\\fabric-loader-0.16.5.jar")
        );
    }
}
