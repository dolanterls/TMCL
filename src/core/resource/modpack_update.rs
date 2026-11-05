//! Update an existing instance from a newer modpack archive.
//!
//! The instance keeps its identity: only files the pack owns are replaced or
//! removed, everything the player added stays. Files that are replaced or
//! removed go through the instance trash, so an update that goes wrong is
//! undone file by file instead of by restoring a full copy.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};
use crate::core::task::ModpackFileSpec;

/// Ownership record written next to `tmcl.json`.
pub const MANIFEST_FILE: &str = ".tmcl-pack.json";

/// Which files of an instance came from the modpack.
///
/// An instance installed before this file existed has no record; an update
/// then treats every existing file as the player's and only adds/overwrites,
/// never deletes. That is the safe direction: a stale mod is visible in the
/// list, a deleted player mod is not.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackManifest {
    #[serde(default)]
    pub version: String,
    /// Files listed by the pack manifest (usually `mods/...`).
    #[serde(default)]
    pub files: Vec<String>,
    /// Files unpacked from `overrides/`.
    #[serde(default)]
    pub overrides: Vec<String>,
}

impl PackManifest {
    fn owned(&self) -> impl Iterator<Item = &String> {
        self.files.iter().chain(self.overrides.iter())
    }
}

pub fn read_manifest(instance_root: &Path) -> Option<PackManifest> {
    let bytes = std::fs::read(instance_root.join(MANIFEST_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write_manifest(instance_root: &Path, manifest: &PackManifest) -> Result<()> {
    crate::core::config::storage::write_json_atomic(&instance_root.join(MANIFEST_FILE), manifest)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateOutcome {
    pub overwritten: usize,
    pub removed: usize,
    /// Files the caller still has to download.
    pub files: Vec<ModpackFileSpec>,
}

/// Applies a prepared pack to `instance_root`.
///
/// Synchronous on purpose: extraction, copying and trashing are blocking I/O,
/// so callers run this in `spawn_blocking`. The archive is unpacked into
/// `staging` first; the instance is only touched after the whole pack has been
/// read successfully.
pub fn apply(
    instance_root: &Path,
    archive: &Path,
    staging: &Path,
    version: &str,
    files: Vec<ModpackFileSpec>,
) -> Result<UpdateOutcome> {
    if staging.exists() {
        std::fs::remove_dir_all(staging)?;
    }
    std::fs::create_dir_all(staging)?;
    let overrides = crate::core::resource::install::extract_modpack_archive(archive, staging)?;
    let previous = read_manifest(instance_root);

    let mut manifest = PackManifest {
        version: version.to_owned(),
        files: files.iter().map(|file| file.path.clone()).collect(),
        overrides: overrides.clone(),
    };
    manifest.files.sort();
    manifest.overrides.sort();
    let owned = manifest
        .owned()
        .cloned()
        .collect::<std::collections::HashSet<_>>();

    // Only a known previous pack can own files that the new one drops; without
    // that record nothing is deleted.
    let removed = previous
        .as_ref()
        .map(|old| {
            old.owned()
                .filter(|path| !owned.contains(*path))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut outcome = UpdateOutcome {
        files,
        ..UpdateOutcome::default()
    };

    // Move the old versions aside before writing: the trash keeps the relative
    // path, so the workspace's restore action puts them back in place.
    for relative in overrides.iter().filter(|relative| {
        let target = instance_root.join(relative);
        target.is_file() || target.is_dir()
    }) {
        let target = instance_root.join(relative);
        crate::core::content::trash::move_to_trash(instance_root, &target)?;
        outcome.overwritten += 1;
    }
    for relative in &removed {
        let target = instance_root.join(relative);
        if target.is_file() || target.is_dir() {
            crate::core::content::trash::move_to_trash(instance_root, &target)?;
            outcome.removed += 1;
        }
    }

    for relative in &overrides {
        let source = staging.join(relative);
        let destination = instance_root.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if source.is_dir() {
            std::fs::create_dir_all(&destination)?;
        } else {
            std::fs::copy(&source, &destination)?;
        }
    }
    write_manifest(instance_root, &manifest)?;
    let _ = std::fs::remove_dir_all(staging);
    Ok(outcome)
}

/// Refuses an update that would change the game version or the loader.
///
/// Replacing either means reinstalling the client and the loader, which is a
/// different operation from swapping a pack's files; doing it silently here
/// would leave the instance's `tmcl.json` and client JSON out of step.
pub fn ensure_compatible(
    instance: &crate::core::instance::models::InstanceConfig,
    metadata: &crate::core::resource::modpack::ModpackMetadata,
) -> Result<()> {
    if instance.minecraft_version != metadata.minecraft_version {
        return Err(Error::Download(format!(
            "this pack targets Minecraft {}, but the instance is {}; install it as a new instance instead",
            metadata.minecraft_version, instance.minecraft_version
        )));
    }
    let current = instance.loader.key().to_ascii_lowercase();
    if metadata.loader != current {
        return Err(Error::Download(format!(
            "this pack uses {}, but the instance runs {current}; changing loaders during an update is not supported",
            metadata.loader
        )));
    }
    Ok(())
}

/// Removes a staging directory left behind by a failed update.
pub fn discard_staging(staging: &Path) {
    let _ = std::fs::remove_dir_all(staging);
}

/// Staging directory for an update of this instance.
pub fn staging_dir(cache_root: &Path, instance_id: uuid::Uuid) -> PathBuf {
    cache_root
        .join("modpack-update")
        .join(instance_id.to_string())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{MANIFEST_FILE, PackManifest, apply, read_manifest};

    fn write_zip(path: &std::path::Path, files: &[(&str, &[u8])]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("zip parent");
        }
        let file = std::fs::File::create(path).expect("create zip");
        let mut writer = zip::ZipWriter::new(file);
        for (name, contents) in files {
            writer
                .start_file(name, zip::write::FileOptions::<()>::default())
                .expect("start file");
            writer.write_all(contents).expect("write file");
        }
        writer.finish().expect("finish zip");
    }

    #[test]
    fn an_update_replaces_pack_files_and_keeps_user_files() {
        let root = std::env::temp_dir().join(format!("tmcl-pack-update-{}", uuid::Uuid::new_v4()));
        let instance = root.join("instance");
        std::fs::create_dir_all(instance.join("config")).expect("config dir");
        std::fs::create_dir_all(instance.join("mods")).expect("mods dir");
        std::fs::write(instance.join("config/a.txt"), b"old").expect("old config");
        std::fs::write(instance.join("mods/old.jar"), b"old mod").expect("old mod");
        // Not listed by the previous pack: the player added it.
        std::fs::write(instance.join("mods/user.jar"), b"user mod").expect("user mod");
        crate::core::config::storage::write_json_atomic(
            &instance.join(MANIFEST_FILE),
            &PackManifest {
                version: "1.0".into(),
                files: vec!["mods/old.jar".into()],
                overrides: vec!["config/a.txt".into()],
            },
        )
        .expect("old manifest");

        let archive = root.join("pack-2.zip");
        write_zip(
            &archive,
            &[
                ("overrides/config/a.txt", b"new"),
                ("overrides/config/b.txt", b"added"),
            ],
        );
        let staging = root.join("staging");
        let outcome = apply(
            &instance,
            &archive,
            &staging,
            "2.0",
            vec![crate::core::task::ModpackFileSpec {
                path: "mods/new.jar".into(),
                urls: vec!["https://example.invalid/new.jar".into()],
                sha1: None,
                size: Some(4),
            }],
        )
        .expect("apply update");

        assert_eq!(outcome.overwritten, 1);
        assert_eq!(outcome.removed, 1);
        assert_eq!(outcome.files.len(), 1);
        assert_eq!(
            std::fs::read(instance.join("config/a.txt")).expect("config"),
            b"new"
        );
        assert_eq!(
            std::fs::read(instance.join("config/b.txt")).expect("added"),
            b"added"
        );
        assert!(
            instance.join("mods/user.jar").is_file(),
            "a player mod must survive the update"
        );
        assert!(
            !instance.join("mods/old.jar").exists(),
            "the old pack file is gone from mods/"
        );
        assert!(
            !staging.exists(),
            "staging is cleaned up after a successful update"
        );
        let manifest = read_manifest(&instance).expect("new manifest");
        assert_eq!(manifest.version, "2.0");
        assert!(manifest.files.iter().any(|file| file == "mods/new.jar"));
        assert!(manifest.overrides.iter().any(|file| file == "config/a.txt"));

        // The replaced file is restorable from the trash.
        let trashed = crate::core::content::trash::list(&instance);
        assert!(
            trashed
                .iter()
                .any(|entry| entry.original.ends_with("config/a.txt")),
            "{trashed:?}"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn an_update_refuses_to_change_the_game_version_or_loader() {
        use crate::core::instance::models::{InstanceConfig, InstanceLoader};
        use crate::core::resource::modpack::ModpackMetadata;

        let mut instance = InstanceConfig::new("C:/demo", uuid::Uuid::nil(), "demo", "1.20.1");
        instance.loader = InstanceLoader::Fabric {
            version: "0.15.11".into(),
        };
        let mut metadata = ModpackMetadata {
            minecraft_version: "1.20.1".into(),
            loader: "fabric".into(),
            loader_version: "0.16.0".into(),
            files: Vec::new(),
        };
        super::ensure_compatible(&instance, &metadata).expect("same version and loader");

        metadata.minecraft_version = "1.20.6".into();
        let error = super::ensure_compatible(&instance, &metadata).expect_err("version change");
        assert!(error.to_string().contains("1.20.6"), "{error}");

        metadata.minecraft_version = "1.20.1".into();
        metadata.loader = "quilt".into();
        let error = super::ensure_compatible(&instance, &metadata).expect_err("loader change");
        assert!(error.to_string().contains("quilt"), "{error}");
    }

    #[test]
    fn an_instance_without_a_manifest_never_deletes_files() {
        let root = std::env::temp_dir().join(format!("tmcl-pack-noman-{}", uuid::Uuid::new_v4()));
        let instance = root.join("instance");
        std::fs::create_dir_all(instance.join("mods")).expect("mods dir");
        std::fs::write(instance.join("mods/unknown.jar"), b"jar").expect("jar");

        let archive = root.join("pack.zip");
        write_zip(&archive, &[("overrides/config/a.txt", b"a")]);
        let outcome =
            apply(&instance, &archive, &root.join("staging"), "1", Vec::new()).expect("apply");

        assert_eq!(outcome.removed, 0);
        assert!(
            instance.join("mods/unknown.jar").is_file(),
            "nothing may be deleted without knowing what the old pack owned"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }
}
