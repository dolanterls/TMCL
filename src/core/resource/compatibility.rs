use std::path::{Path, PathBuf};

use crate::core::instance::models::{InstanceConfig, InstanceLoader};

use super::model::{Compatibility, LoaderFilter, ResourceFile, ResourceKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetContext {
    pub minecraft_version: String,
    pub loader: LoaderFilter,
    pub root: PathBuf,
}

pub fn target_context(instance: &InstanceConfig) -> TargetContext {
    TargetContext {
        minecraft_version: instance.minecraft_version.clone(),
        loader: loader_filter(&instance.loader),
        root: instance.path.clone(),
    }
}

pub fn loader_filter(loader: &InstanceLoader) -> LoaderFilter {
    match loader {
        InstanceLoader::Vanilla => LoaderFilter::Vanilla,
        InstanceLoader::Fabric { .. } => LoaderFilter::Fabric,
        InstanceLoader::Quilt { .. } => LoaderFilter::Quilt,
        InstanceLoader::Forge { .. } => LoaderFilter::Forge,
        InstanceLoader::NeoForge { .. } => LoaderFilter::NeoForge,
    }
}

/// Target context for one project kind.
///
/// Resource packs and shaders are not loader-specific: Modrinth labels their
/// files `minecraft`, and they run under vanilla, Fabric, Forge alike. Filtering
/// them by the instance loader would reject every file the source offers.
pub fn target_context_for(instance: &InstanceConfig, kind: ResourceKind) -> TargetContext {
    let mut target = target_context(instance);
    if matches!(kind, ResourceKind::ResourcePack | ResourceKind::Shader) {
        target.loader = LoaderFilter::Any;
    }
    target
}

pub fn file_compatibility(file: &ResourceFile, target: Option<&TargetContext>) -> Compatibility {
    let Some(target) = target else {
        return Compatibility::Unknown;
    };
    let version_known = !file.game_versions.is_empty();
    let loader_known = !file.loaders.is_empty();
    let version_matches = file
        .game_versions
        .iter()
        .any(|version| version == &target.minecraft_version);
    let loader_matches = target.loader == LoaderFilter::Any
        || file.loaders.iter().any(|loader| {
            loader.eq_ignore_ascii_case(loader_label(target.loader))
                || (target.loader == LoaderFilter::NeoForge && loader.eq_ignore_ascii_case("forge"))
                // Quilt loads Fabric mods, so a Fabric build is not incompatible
                // with a Quilt instance.
                || (target.loader == LoaderFilter::Quilt && loader.eq_ignore_ascii_case("fabric"))
        });
    if version_known && !version_matches || loader_known && !loader_matches {
        Compatibility::Incompatible
    } else if !version_known || !loader_known {
        Compatibility::Unknown
    } else {
        Compatibility::Compatible
    }
}

pub fn sort_compatibility(files: &mut [ResourceFile], target: Option<&TargetContext>) {
    files.sort_by_key(|file| file_compatibility(file, target).rank());
}

/// Every instance keeps its own content directory: the launched command always
/// points the game directory at the instance folder, so there is no shared mode
/// to branch on.
pub fn content_root(instance: &InstanceConfig) -> PathBuf {
    instance.path.clone()
}

pub fn resource_destination(
    instance: &InstanceConfig,
    directory: &str,
    file_name: &str,
) -> Result<PathBuf, String> {
    // Every install path goes through here, so this is the one place that has
    // to decide what the file is called on disk.
    let file_name = super::conflict::sanitize_file_name(file_name);
    super::conflict::validate_file_name(&file_name)?;
    let root = content_root(instance);
    let destination = root.join(directory).join(&file_name);
    if !is_within(&root, &destination) {
        return Err("resource destination escapes the instance content root".into());
    }
    Ok(destination)
}

fn is_within(root: &Path, candidate: &Path) -> bool {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let parent = candidate
        .parent()
        .and_then(|path| path.canonicalize().ok())
        .unwrap_or_else(|| root.clone());
    parent.starts_with(&root)
        && candidate.file_name().is_some_and(|name| {
            name != std::ffi::OsStr::new(".") && name != std::ffi::OsStr::new("..")
        })
}

/// Lower-case name of a loader filter, as the sources spell it.
pub fn loader_label(loader: LoaderFilter) -> &'static str {
    match loader {
        LoaderFilter::Any => "any",
        LoaderFilter::Vanilla => "vanilla",
        LoaderFilter::Fabric => "fabric",
        LoaderFilter::Quilt => "quilt",
        LoaderFilter::Forge => "forge",
        LoaderFilter::NeoForge => "neoforge",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{TargetContext, content_root, file_compatibility, resource_destination};
    use crate::core::instance::models::{InstanceConfig, InstanceLoader};
    use crate::core::resource::model::{Compatibility, LoaderFilter, ResourceFile};

    fn instance() -> InstanceConfig {
        let mut value = InstanceConfig::new(
            "C:/games/versions/demo",
            uuid::Uuid::new_v4(),
            "demo",
            "1.21.1",
        );
        value.loader = InstanceLoader::Fabric {
            version: "0.16.5".into(),
        };
        value
    }

    fn file(versions: Vec<&str>, loaders: Vec<&str>) -> ResourceFile {
        ResourceFile {
            source: super::super::model::ResourceSource::Modrinth,
            project_id: "p".into(),
            file_id: "f".into(),
            name: "demo.jar".into(),
            download_url: "https://example.test/demo.jar".into(),
            sha1: None,
            size: None,
            game_versions: versions.into_iter().map(str::to_owned).collect(),
            loaders: loaders.into_iter().map(str::to_owned).collect(),
            channel: super::super::model::ReleaseChannel::Release,
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn content_root_is_always_the_instance_directory() {
        assert_eq!(
            content_root(&instance()),
            PathBuf::from("C:/games/versions/demo")
        );
    }

    #[test]
    fn compatibility_distinguishes_known_and_unknown_metadata() {
        let target = TargetContext {
            minecraft_version: "1.21.1".into(),
            loader: LoaderFilter::Fabric,
            root: PathBuf::from("C:/games"),
        };
        assert_eq!(
            file_compatibility(&file(vec!["1.21.1"], vec!["fabric"]), Some(&target)),
            Compatibility::Compatible
        );
        assert_eq!(
            file_compatibility(&file(Vec::new(), vec!["fabric"]), Some(&target)),
            Compatibility::Unknown
        );
        assert_eq!(
            file_compatibility(&file(vec!["1.20.1"], vec!["fabric"]), Some(&target)),
            Compatibility::Incompatible
        );
    }

    #[test]
    fn a_pack_labelled_minecraft_matches_any_instance() {
        // Modrinth marks resource packs and shaders as `minecraft`, not as a
        // mod loader; `Any` is what `plan` uses for those kinds.
        let target = crate::core::resource::compatibility::target_context_for(
            &instance(),
            crate::core::resource::model::ResourceKind::ResourcePack,
        );
        assert_eq!(target.loader, LoaderFilter::Any);
        assert_eq!(
            file_compatibility(&file(vec!["1.21.1"], vec!["minecraft"]), Some(&target)),
            Compatibility::Compatible
        );
    }

    #[test]
    fn resource_destination_rejects_path_traversal() {
        let result = resource_destination(&instance(), "mods", "..\\outside.jar");
        assert!(result.is_err());
    }

    #[test]
    fn a_colour_coded_name_is_written_without_the_codes() {
        let destination = resource_destination(
            &instance(),
            "resourcepacks",
            "\u{a7}7Stay True\u{a7}0 1.21.1.zip",
        )
        .expect("destination");

        assert_eq!(
            destination,
            std::path::PathBuf::from("C:/games/versions/demo/resourcepacks/Stay True 1.21.1.zip")
        );
    }
}
