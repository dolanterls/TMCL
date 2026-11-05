use std::path::Path;

use crate::core::config::storage::{read_json, write_json_atomic};
use crate::core::error::{Error, Result};
use crate::core::instance::models::InstanceConfig;
use crate::core::instance::service::validate_name;

pub fn rename_instance(old_directory: &Path, new_name: &str) -> Result<InstanceConfig> {
    validate_name(new_name)?;
    let parent = old_directory
        .parent()
        .ok_or_else(|| Error::Config("instance has no parent directory".into()))?;
    let old_name = old_directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Config("instance directory name is not valid UTF-8".into()))?;
    let new_directory = parent.join(new_name);
    if new_directory.exists() {
        return Err(Error::Config(format!(
            "instance already exists: {}",
            new_directory.display()
        )));
    }

    let config_path = old_directory.join("tmcl.json");
    let mut config: InstanceConfig = read_json(&config_path)?;
    std::fs::rename(old_directory, &new_directory)?;
    let result = (|| {
        rename_if_present(
            &new_directory.join(format!("{old_name}.jar")),
            &new_directory.join(format!("{new_name}.jar")),
        )?;
        rename_if_present(
            &new_directory.join(format!("{old_name}.json")),
            &new_directory.join(format!("{new_name}.json")),
        )?;
        config.name = new_name.into();
        config.path = new_directory.clone();
        write_json_atomic(&new_directory.join("tmcl.json"), &config)?;
        Ok(config)
    })();
    if result.is_err() {
        let _ = std::fs::rename(&new_directory, old_directory);
    }
    result
}

fn rename_if_present(from: &Path, to: &Path) -> Result<()> {
    if from.is_file() {
        std::fs::rename(from, to)?;
    }
    Ok(())
}
