use std::path::Path;

use crate::core::config::storage::{read_json, write_json_atomic};
use crate::core::error::Result;
use crate::core::instance::models::{InstanceConfig, InstanceLoader};

pub fn load_or_create(
    instance_dir: &Path,
    game_directory_id: uuid::Uuid,
    minecraft_version: &str,
) -> Result<InstanceConfig> {
    let config_path = instance_dir.join("tmcl.json");
    if config_path.is_file() {
        let mut config: InstanceConfig = read_json(&config_path)?;
        config.path = instance_dir.to_path_buf();
        if repair_from_client_json(&mut config, instance_dir) {
            write_json_atomic(&config_path, &config)?;
        }
        return Ok(config);
    }

    let name = instance_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("instance");
    let mut config = InstanceConfig::new(
        instance_dir.to_path_buf(),
        game_directory_id,
        name,
        minecraft_version,
    );
    repair_from_client_json(&mut config, instance_dir);
    write_json_atomic(&config_path, &config)?;
    Ok(config)
}

fn repair_from_client_json(config: &mut InstanceConfig, instance_dir: &Path) -> bool {
    let client_json = instance_dir.join(format!("{}.json", config.name));
    let Ok(bytes) = std::fs::read(client_json) else {
        return false;
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };

    let mut changed = false;
    if let Some(version) = json
        .get("inheritsFrom")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            json.get("clientVersion")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| json.get("id").and_then(serde_json::Value::as_str))
        .filter(|version| !version.trim().is_empty())
        && config.minecraft_version != version
    {
        config.minecraft_version = version.to_owned();
        changed = true;
    }

    if let Some(loader) = detect_loader(&json, &config.minecraft_version)
        && config.loader != loader
    {
        config.loader = loader;
        changed = true;
    }

    changed
}

/// Which loader the client json describes, if any.
///
/// The client json is the authority; `InstanceLoader` in `tmcl.json` caches this
/// answer. The instance workspace therefore asks the file, not the metadata.
pub(crate) fn detect_loader(
    json: &serde_json::Value,
    minecraft_version: &str,
) -> Option<crate::core::instance::models::InstanceLoader> {
    let main_class = json
        .get("mainClass")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let libraries = json
        .get("libraries")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|library| library.get("name").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();

    let find_loader = |prefix: &str| {
        libraries
            .iter()
            .find(|name| name.starts_with(prefix))
            .and_then(|name| name.rsplit(':').next())
            .filter(|version| !version.is_empty())
            .map(|version| version.to_owned())
    };

    let has_library = |prefix: &str| libraries.iter().any(|name| name.starts_with(prefix));

    // Quilt's main class also mentions "knot", but its loader lives under a
    // different group, so it has to be checked before Fabric.
    if main_class.contains("quiltmc") || has_library("org.quiltmc:quilt-loader:") {
        return find_loader("org.quiltmc:quilt-loader:")
            .map(|version| crate::core::instance::models::InstanceLoader::Quilt { version });
    }

    if main_class.contains("fabricmc") || has_library("net.fabricmc:fabric-loader:") {
        return find_loader("net.fabricmc:fabric-loader:")
            .map(|version| crate::core::instance::models::InstanceLoader::Fabric { version });
    }

    if main_class.contains("neoforge")
        || has_library("net.neoforged:neoforge:")
        || has_library("net.neoforged.fancymodloader:loader:")
    {
        return find_loader("net.neoforged:neoforge:")
            .or_else(|| find_loader("net.neoforged.fancymodloader:loader:"))
            .map(|version| crate::core::instance::models::InstanceLoader::NeoForge { version });
    }

    if main_class.contains("forge")
        || has_library("net.minecraftforge:forge:")
        || has_library("net.minecraftforge:fmlloader:")
    {
        let version = find_loader("net.minecraftforge:forge:").or_else(|| {
            find_loader("net.minecraftforge:fmlloader:").map(|version| {
                version
                    .strip_prefix(&format!("{minecraft_version}-"))
                    .unwrap_or(&version)
                    .to_owned()
            })
        });
        return version
            .map(|version| crate::core::instance::models::InstanceLoader::Forge { version });
    }
    None
}

/// One of the loaders the install screen offers.
///
/// `Minecraft` is the game itself (switching its version reinstalls a Vanilla
/// client); the rest are loader families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallableLoader {
    Minecraft,
    Forge,
    NeoForge,
    Fabric,
    Quilt,
    OptiFine,
}

impl InstallableLoader {
    pub const ALL: [Self; 6] = [
        Self::Minecraft,
        Self::Forge,
        Self::NeoForge,
        Self::Fabric,
        Self::Quilt,
        Self::OptiFine,
    ];

    /// Stable identifier used in messages and for matching the installed state.
    pub fn key(self) -> &'static str {
        match self {
            Self::Minecraft => "Minecraft",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::OptiFine => "OptiFine",
        }
    }

    pub fn row_key(self) -> &'static str {
        match self {
            Self::Minecraft => "workspace.install.minecraft",
            Self::Forge => "workspace.install.forge",
            Self::NeoForge => "workspace.install.neoforge",
            Self::Fabric => "workspace.install.fabric",
            Self::Quilt => "workspace.install.quilt",
            Self::OptiFine => "workspace.install.optifine",
        }
    }

    /// Whether this launcher can install it at all.
    ///
    /// OptiFine's licence forbids redistributing its installer, so that row is
    /// read-only; every other loader has an install path.
    pub fn installable(self) -> bool {
        !matches!(self, Self::OptiFine)
    }
}

/// What the install screen says next to one loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoaderState {
    Installed(String),
    NotInstalled,
    /// Another loader owns the instance; installing this one replaces it.
    Incompatible {
        current: &'static str,
    },
    Unsupported,
}

impl InstanceLoader {
    /// Stable identifier, used to name the loader a row conflicts with.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Forge { .. } => "Forge",
            Self::NeoForge { .. } => "NeoForge",
            Self::Fabric { .. } => "Fabric",
            Self::Quilt { .. } => "Quilt",
        }
    }

    pub fn version(&self) -> Option<&str> {
        match self {
            Self::Vanilla => None,
            Self::Forge { version }
            | Self::NeoForge { version }
            | Self::Fabric { version }
            | Self::Quilt { version } => Some(version),
        }
    }
}

/// The loader the instance's client json declares, if the file is readable.
pub fn client_loader(
    instance_dir: &Path,
    name: &str,
    minecraft_version: &str,
) -> Option<InstanceLoader> {
    let bytes = std::fs::read(instance_dir.join(format!("{name}.json"))).ok()?;
    let json = serde_json::from_slice::<serde_json::Value>(&bytes).ok()?;
    detect_loader(&json, minecraft_version)
}

/// Refresh the cached loader and game version from the client json.
///
/// Installation rewrites that file, so the workspace has to re-read it instead
/// of trusting the copy it opened with. Returns whether anything changed.
pub fn sync_from_client_json(config: &mut InstanceConfig, instance_dir: &Path) -> bool {
    repair_from_client_json(config, instance_dir)
}

/// The six rows of the install screen, in display order.
pub fn loader_status(
    instance_dir: &Path,
    name: &str,
    minecraft_version: &str,
) -> Vec<(InstallableLoader, LoaderState)> {
    let current = client_loader(instance_dir, name, minecraft_version);
    InstallableLoader::ALL
        .iter()
        .map(|loader| {
            let state = match loader {
                InstallableLoader::Minecraft => {
                    LoaderState::Installed(minecraft_version.to_owned())
                }
                InstallableLoader::OptiFine => LoaderState::Unsupported,
                _ => match &current {
                    Some(installed) if installed.key() == loader.key() => {
                        LoaderState::Installed(installed.version().unwrap_or_default().to_owned())
                    }
                    Some(installed) => LoaderState::Incompatible {
                        current: installed.key(),
                    },
                    None => LoaderState::NotInstalled,
                },
            };
            (*loader, state)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::load_or_create;
    use crate::core::config::storage::write_json_atomic;
    use crate::core::instance::models::{InstanceConfig, InstanceLoader};

    #[test]
    fn repairs_legacy_display_version_and_loader_from_client_json() {
        let root =
            std::env::temp_dir().join(format!("tmcl-instance-repair-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create instance directory");
        let mut config = InstanceConfig::new(
            &root,
            uuid::Uuid::new_v4(),
            "1.20.1-Fabric 0.19.3",
            "1.20.1-Fabric 0.19.3",
        );
        write_json_atomic(&root.join("tmcl.json"), &config).expect("write legacy metadata");
        std::fs::write(
            root.join("1.20.1-Fabric 0.19.3.json"),
            br#"{"id":"1.20.1-Fabric 0.19.3","inheritsFrom":"1.20.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[{"name":"net.fabricmc:fabric-loader:0.19.3"}]}"#,
        )
        .expect("write client metadata");

        config = load_or_create(&root, config.game_directory_id, "ignored")
            .expect("load repaired metadata");

        assert_eq!(config.minecraft_version, "1.20.1");
        assert_eq!(
            config.loader,
            InstanceLoader::Fabric {
                version: "0.19.3".into()
            }
        );
        let saved: InstanceConfig =
            crate::core::config::storage::read_json(&root.join("tmcl.json"))
                .expect("read repaired metadata");
        assert_eq!(saved.minecraft_version, "1.20.1");
        assert_eq!(saved.loader, config.loader);

        std::fs::remove_dir_all(root).expect("remove instance directory");
    }

    #[test]
    fn repairs_forge_loader_from_fml_loader_library() {
        let root = std::env::temp_dir().join(format!(
            "tmcl-forge-instance-repair-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create instance directory");
        let mut config = InstanceConfig::new(&root, uuid::Uuid::new_v4(), "farming", "1.20.1");
        write_json_atomic(&root.join("tmcl.json"), &config).expect("write instance metadata");
        std::fs::write(
            root.join("farming.json"),
            br#"{"id":"farming","clientVersion":"1.20.1","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","libraries":[{"name":"net.minecraftforge:fmlloader:1.20.1-47.4.10"}]}"#,
        )
        .expect("write client metadata");

        config = load_or_create(&root, config.game_directory_id, "ignored")
            .expect("load repaired Forge metadata");

        assert_eq!(
            config.loader,
            InstanceLoader::Forge {
                version: "47.4.10".into()
            }
        );
        std::fs::remove_dir_all(root).expect("remove instance directory");
    }

    fn instance_dir(tag: &str, client: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("tmcl-loader-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create instance dir");
        std::fs::write(root.join("demo.json"), client).expect("write client json");
        root
    }

    const FORGE_CLIENT: &str = r#"{
        "id": "demo",
        "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
        "libraries": [
            {"name": "net.minecraftforge:forge:1.20.1-47.4.22"},
            {"name": "net.minecraftforge:fmlloader:1.20.1-47.4.22"}
        ]
    }"#;

    #[test]
    fn loader_status_lists_six_rows() {
        let root = instance_dir("rows", "{}");
        let rows = super::loader_status(&root, "demo", "1.20.1");
        assert_eq!(rows.len(), 6);
        assert_eq!(
            rows.iter()
                .map(|(loader, _)| loader.key())
                .collect::<Vec<_>>(),
            [
                "Minecraft",
                "Forge",
                "NeoForge",
                "Fabric",
                "Quilt",
                "OptiFine"
            ]
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn loader_status_reads_installed_version() {
        let root = instance_dir("installed", FORGE_CLIENT);
        let rows = super::loader_status(&root, "demo", "1.20.1");
        let forge = rows
            .iter()
            .find(|(loader, _)| *loader == super::InstallableLoader::Forge)
            .expect("forge row");
        assert_eq!(
            forge.1,
            super::LoaderState::Installed("1.20.1-47.4.22".into())
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn loader_status_flags_incompatible_fabric() {
        let root = instance_dir("incompatible", FORGE_CLIENT);
        let rows = super::loader_status(&root, "demo", "1.20.1");
        let fabric = rows
            .iter()
            .find(|(loader, _)| *loader == super::InstallableLoader::Fabric)
            .expect("fabric row");
        assert_eq!(
            fabric.1,
            super::LoaderState::Incompatible { current: "Forge" }
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn vanilla_instance_has_no_loader() {
        let root = instance_dir(
            "vanilla",
            r#"{"mainClass":"net.minecraft.client.main.Main","libraries":[]}"#,
        );
        assert_eq!(super::client_loader(&root, "demo", "1.20.1"), None);
        let rows = super::loader_status(&root, "demo", "1.20.1");
        assert!(
            rows.iter().any(
                |(loader, state)| *loader == super::InstallableLoader::Fabric
                    && *state == super::LoaderState::NotInstalled
            ),
            "{rows:?}"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }
}
