use std::path::Path;

use uuid::Uuid;

use crate::core::config::storage::write_json_atomic;
use crate::core::error::{Error, Result};
use crate::core::instance::models::InstanceConfig;
use crate::core::instance::path::normalize_game_directory;

pub fn validate_name(name: &str) -> Result<()> {
    const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|character| "<>:\"/\\|?*".contains(character))
        || RESERVED
            .iter()
            .any(|reserved| name.eq_ignore_ascii_case(reserved))
    {
        return Err(Error::Config(format!("invalid instance name: {name:?}")));
    }
    Ok(())
}

pub fn create_instance(
    game_directory: &Path,
    game_directory_id: Uuid,
    name: &str,
    minecraft_version: &str,
) -> Result<InstanceConfig> {
    validate_name(name)?;
    let game_directory = normalize_game_directory(game_directory)?;
    let versions = game_directory.join("versions");
    std::fs::create_dir_all(&versions)?;
    let target = versions.join(name);
    if target.exists() {
        return Err(Error::Config(format!(
            "instance already exists: {}",
            target.display()
        )));
    }

    let temporary = versions.join(format!(".tmcl-create-{}", Uuid::new_v4()));
    std::fs::create_dir(&temporary)?;
    let result = (|| {
        let config = InstanceConfig::new(&temporary, game_directory_id, name, minecraft_version);
        write_json_atomic(&temporary.join("tmcl.json"), &config)?;
        std::fs::rename(&temporary, &target)?;
        let mut config = config;
        config.path = target;
        Ok(config)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&temporary);
    }
    result
}

pub fn import_instance(
    instance_directory: &Path,
    game_directory_id: Uuid,
    minecraft_version: &str,
) -> Result<InstanceConfig> {
    if !instance_directory.is_dir() {
        return Err(Error::Config(format!(
            "instance directory does not exist: {}",
            instance_directory.display()
        )));
    }
    crate::core::instance::config::load_or_create(
        instance_directory,
        game_directory_id,
        minecraft_version,
    )
}
