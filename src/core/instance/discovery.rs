use std::path::{Path, PathBuf};

use crate::core::error::Result;
use crate::core::instance::path::normalize_game_directory;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredInstance {
    pub id: String,
    pub path: PathBuf,
    pub jar: PathBuf,
    pub json: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiscoveryResult {
    pub instances: Vec<DiscoveredInstance>,
    pub child_candidates: Vec<PathBuf>,
}

pub fn discover(root: impl AsRef<Path>) -> Result<DiscoveryResult> {
    let root = normalize_game_directory(root)?;
    let mut result = DiscoveryResult::default();
    let versions = root.join("versions");

    if versions.is_dir() {
        for entry in std::fs::read_dir(&versions)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let path = entry.path();
            let name = match entry.file_name().to_str() {
                Some(name) if !name.is_empty() => name.to_owned(),
                _ => continue,
            };
            if !path.is_dir() {
                continue;
            }
            let jar = path.join(format!("{name}.jar"));
            let json = path.join(format!("{name}.json"));
            if jar.is_file() && json.is_file() {
                result.instances.push(DiscoveredInstance {
                    id: name,
                    path,
                    jar,
                    json,
                });
            }
        }
    }

    if result.instances.is_empty() {
        for name in [".minecraft", "minecraft"] {
            let candidate = root.join(name);
            if candidate.is_dir() {
                result.child_candidates.push(candidate);
            }
        }
    }
    Ok(result)
}
