use std::path::Path;

use serde_json::{Value, json};

use crate::core::config::models::{CURRENT_SCHEMA_VERSION, Config};
use crate::core::config::storage::read_json;
use crate::core::error::{Error, Result};

pub fn load_and_migrate(path: &Path) -> Result<Config> {
    let raw: Value = read_json(path)?;
    let version = raw
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;

    if version > CURRENT_SCHEMA_VERSION {
        return Err(Error::Config(format!(
            "unsupported config schema version {version}"
        )));
    }
    if version == CURRENT_SCHEMA_VERSION {
        return Ok(serde_json::from_value(raw)?);
    }

    let config: Config = if version == 0 {
        serde_json::from_value(migrate_v0(raw)?)?
    } else {
        serde_json::from_value(raw)?
    };
    Ok(Config {
        schema_version: CURRENT_SCHEMA_VERSION,
        ..config
    })
}

fn migrate_v0(raw: Value) -> Result<Value> {
    let object = raw
        .as_object()
        .ok_or_else(|| Error::Config("legacy config must be a JSON object".into()))?;

    let game_directories = object
        .get("game_dirs")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let java_path = object.get("java_path").cloned().unwrap_or(Value::Null);
    let auto_download_java = object
        .get("auto_download_java")
        .cloned()
        .unwrap_or(json!(false));
    let source_priority = object
        .get("download_sources")
        .cloned()
        .unwrap_or_else(|| json!(["official", "bmclapi"]));
    let current_account_id = object
        .get("current_account")
        .cloned()
        .unwrap_or(Value::Null);
    let current_instance_id = object
        .get("current_instance")
        .cloned()
        .unwrap_or(Value::Null);
    let retention_days = object
        .get("log_retention_days")
        .cloned()
        .unwrap_or_else(|| json!(30));

    Ok(json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "game_directories": game_directories,
        "java": {
            "selected_path": java_path,
            "auto_download": auto_download_java
        },
        "downloads": {
            "source_priority": source_priority
        },
        "state": {
            "current_account_id": current_account_id,
            "current_instance_id": current_instance_id,
            "ui": {
                "color_scheme": "default",
                "show_mouse": true
            }
        },
        "logs": {
            "retention_days": retention_days
        }
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::migrate_v0;
    use crate::core::config::models::CURRENT_SCHEMA_VERSION;

    #[test]
    fn migrates_legacy_keys_to_current_shape() {
        let value = migrate_v0(json!({
            "game_dirs": ["C:/Games"],
            "java_path": "C:/Java/bin/java.exe",
            "auto_download_java": true
        }))
        .expect("migration");

        assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
        assert_eq!(value["java"]["auto_download"], true);
        assert_eq!(value["game_directories"][0], "C:/Games");
    }
}
