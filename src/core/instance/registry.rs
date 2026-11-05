use std::path::{Path, PathBuf};

use crate::core::error::Result;
use crate::core::instance::config::load_or_create;
use crate::core::instance::discovery::discover;
use crate::core::instance::models::InstanceConfig;

pub fn scan(game_directory: &Path, game_directory_id: uuid::Uuid) -> Result<Vec<InstanceConfig>> {
    scan_excluding(game_directory, game_directory_id, &[])
}

pub fn scan_excluding(
    game_directory: &Path,
    game_directory_id: uuid::Uuid,
    excluded_instances: &[PathBuf],
) -> Result<Vec<InstanceConfig>> {
    let discovered = discover(game_directory)?;
    discovered
        .instances
        .into_iter()
        .filter(|instance| {
            let path = normalize_path(&instance.path);
            !excluded_instances
                .iter()
                .any(|excluded| normalize_path(excluded) == path)
        })
        .map(|instance| load_or_create(&instance.path, game_directory_id, &instance.id))
        .collect()
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
