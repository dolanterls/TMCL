use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::core::instance::overrides::LaunchOverrides;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub id: Uuid,
    pub name: String,
    pub game_directory_id: Uuid,
    pub minecraft_version: String,
    pub loader: InstanceLoader,
    /// Launch settings this instance overrides. Empty means "follow the
    /// global config", which is what every instance written by an older build
    /// gets after the field is defaulted in.
    #[serde(default)]
    pub overrides: LaunchOverrides,
    pub stats: InstanceStats,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceLoader {
    Vanilla,
    Fabric { version: String },
    Quilt { version: String },
    Forge { version: String },
    NeoForge { version: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InstanceStats {
    pub launch_count: u64,
    pub play_time_seconds: u64,
}

impl InstanceConfig {
    pub fn new(
        path: impl Into<PathBuf>,
        game_directory_id: Uuid,
        name: impl Into<String>,
        minecraft_version: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            game_directory_id,
            minecraft_version: minecraft_version.into(),
            loader: InstanceLoader::Vanilla,
            overrides: LaunchOverrides::default(),
            stats: InstanceStats::default(),
            path: path.into(),
            extra: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{InstanceConfig, InstanceLoader};
    use crate::core::config::storage::{read_json, write_json_atomic};
    use std::path::PathBuf;

    fn temp_file(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tmcl-instance-json-{tag}-{}.json",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn legacy_instance_json_loads_without_overrides() {
        let path = temp_file("legacy");
        let instance = InstanceConfig::new(
            "C:/games/versions/demo",
            uuid::Uuid::new_v4(),
            "demo",
            "1.20.1",
        );
        let mut value = serde_json::to_value(&instance).expect("serialize instance");
        // Simulate a file written by a build that predates the field.
        value.as_object_mut().expect("object").remove("overrides");
        std::fs::write(&path, serde_json::to_vec(&value).expect("encode")).expect("write file");

        let loaded: InstanceConfig = read_json(&path).expect("load legacy instance");
        assert!(loaded.overrides.is_empty());
        assert_eq!(loaded.loader, InstanceLoader::Vanilla);
        std::fs::remove_file(path).expect("clean temp file");
    }

    #[test]
    fn overrides_round_trip_through_disk() {
        let path = temp_file("roundtrip");
        let mut instance = InstanceConfig::new(
            "C:/games/versions/demo",
            uuid::Uuid::new_v4(),
            "demo",
            "1.20.1",
        );
        instance.overrides.max_memory_mb = Some(8192);
        instance.overrides.jvm_args = Some(Vec::new());
        instance.overrides.server_address = Some(String::new());
        instance.extra.insert(
            "third_party".into(),
            serde_json::Value::String("kept".into()),
        );
        write_json_atomic(&path, &instance).expect("write instance");

        let loaded: InstanceConfig = read_json(&path).expect("load instance");
        assert_eq!(loaded.overrides, instance.overrides);
        assert_eq!(
            loaded.extra.get("third_party"),
            Some(&serde_json::Value::String("kept".into()))
        );
        // An explicitly empty argument list survives as "clear", not as "follow".
        assert_eq!(loaded.overrides.jvm_args, Some(Vec::new()));
        std::fs::remove_file(path).expect("clean temp file");
    }
}
