//! Which file of a project an install should take.
//!
//! The provider lists versions newest first, and no `ResourceFile` carries a
//! publication date, so "newest" can only mean "first in the order the source
//! returned". What this module adds is the filter that matters more: a file that
//! the target instance cannot run is never the answer, even if it is newest.

use crate::core::resource::compatibility::{TargetContext, file_compatibility};
use crate::core::resource::model::{Compatibility, ReleaseChannel, ResourceFile};

/// The file to install when the caller did not name one.
///
/// Release builds win over beta/alpha among the compatible ones: a launcher
/// should not silently install a pre-release unless nothing else fits. Returns
/// `None` when every file is incompatible with the target, so the caller can say
/// so instead of installing something that will crash the game.
pub fn choose_file<'a>(
    files: &'a [ResourceFile],
    target: Option<&TargetContext>,
) -> Option<&'a ResourceFile> {
    let compatible = files
        .iter()
        .filter(|file| file_compatibility(file, target) != Compatibility::Incompatible)
        .collect::<Vec<_>>();
    compatible
        .iter()
        .find(|file| file.channel == ReleaseChannel::Release)
        .or_else(|| compatible.first())
        .copied()
}

/// Everything a project offers, described by why it cannot be used.
///
/// Only for error messages: "no file runs on 1.21.1 (fabric), the project has
/// 1.20.1/1.19.4 for forge" is actionable, "no file found" is not.
pub fn rejection_summary(files: &[ResourceFile], target: &TargetContext) -> String {
    if files.is_empty() {
        return "the project has no files".to_owned();
    }
    let versions = files
        .iter()
        .flat_map(|file| file.game_versions.iter())
        .collect::<std::collections::BTreeSet<_>>();
    let loaders = files
        .iter()
        .flat_map(|file| file.loaders.iter())
        .collect::<std::collections::BTreeSet<_>>();
    format!(
        "no file runs on Minecraft {} ({}); the project offers {} for {}",
        target.minecraft_version,
        crate::core::resource::compatibility::loader_label(target.loader),
        if versions.is_empty() {
            "unknown versions".to_owned()
        } else {
            versions
                .iter()
                .take(6)
                .map(|version| version.as_str())
                .collect::<Vec<_>>()
                .join("/")
        },
        if loaders.is_empty() {
            "unknown loaders".to_owned()
        } else {
            loaders
                .iter()
                .take(6)
                .map(|loader| loader.as_str())
                .collect::<Vec<_>>()
                .join("/")
        }
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{choose_file, rejection_summary};
    use crate::core::resource::compatibility::TargetContext;
    use crate::core::resource::model::{
        LoaderFilter, ReleaseChannel, ResourceFile, ResourceSource,
    };

    fn file(
        name: &str,
        channel: ReleaseChannel,
        versions: &[&str],
        loaders: &[&str],
    ) -> ResourceFile {
        ResourceFile {
            source: ResourceSource::Modrinth,
            project_id: "sodium".into(),
            file_id: name.into(),
            name: name.into(),
            download_url: format!("http://example/{name}"),
            sha1: None,
            size: Some(10),
            game_versions: versions.iter().map(|value| (*value).to_owned()).collect(),
            loaders: loaders.iter().map(|value| (*value).to_owned()).collect(),
            channel,
            dependencies: Vec::new(),
        }
    }

    fn target() -> TargetContext {
        TargetContext {
            minecraft_version: "1.21.1".into(),
            loader: LoaderFilter::Fabric,
            root: PathBuf::from("C:/game/versions/demo"),
        }
    }

    #[test]
    fn an_incompatible_file_is_never_chosen() {
        let files = [
            file("newest", ReleaseChannel::Release, &["1.22"], &["fabric"]),
            file("older", ReleaseChannel::Release, &["1.21.1"], &["fabric"]),
        ];
        assert_eq!(
            choose_file(&files, Some(&target())).map(|file| file.name.as_str()),
            Some("older")
        );
    }

    #[test]
    fn a_release_wins_over_a_beta_for_the_same_target() {
        let files = [
            file("beta", ReleaseChannel::Beta, &["1.21.1"], &["fabric"]),
            file("release", ReleaseChannel::Release, &["1.21.1"], &["fabric"]),
        ];
        assert_eq!(
            choose_file(&files, Some(&target())).map(|file| file.name.as_str()),
            Some("release")
        );
    }

    #[test]
    fn a_beta_is_used_when_it_is_all_there_is() {
        let files = [file("beta", ReleaseChannel::Beta, &["1.21.1"], &["fabric"])];
        assert_eq!(
            choose_file(&files, Some(&target())).map(|file| file.name.as_str()),
            Some("beta")
        );
    }

    #[test]
    fn nothing_compatible_means_no_choice() {
        let files = [file(
            "other",
            ReleaseChannel::Release,
            &["1.20.1"],
            &["forge"],
        )];
        assert!(choose_file(&files, Some(&target())).is_none());

        let summary = rejection_summary(&files, &target());
        assert!(summary.contains("1.20.1"), "{summary}");
        assert!(summary.contains("forge"), "{summary}");
    }

    #[test]
    fn without_a_target_the_first_file_is_taken() {
        let files = [
            file("first", ReleaseChannel::Release, &["1.21.1"], &["fabric"]),
            file("second", ReleaseChannel::Release, &["1.20.1"], &["fabric"]),
        ];
        assert_eq!(
            choose_file(&files, None).map(|file| file.name.as_str()),
            Some("first")
        );
    }
}
