use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::core::launch::validate::{DownloadRequirement, FileArtifact};

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub objects: BTreeMap<String, AssetObject>,
    #[serde(default)]
    pub virtual_assets: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    #[serde(default)]
    pub size: Option<u64>,
}

pub fn asset_requirements(index: &AssetIndex) -> Vec<DownloadRequirement> {
    index
        .objects
        .iter()
        .map(|(name, object)| {
            let prefix = object.hash.get(..2).unwrap_or(&object.hash);
            DownloadRequirement {
                artifact: FileArtifact {
                    path: PathBuf::from(format!("assets/objects/{prefix}/{}", object.hash)),
                    url: format!(
                        "https://resources.download.minecraft.net/{prefix}/{}",
                        object.hash
                    ),
                    sha1: Some(object.hash.clone()),
                    size: object.size,
                },
                reason: format!("asset {name}"),
            }
        })
        .collect()
}
