//! Copying a file the player picked into the instance.
//!
//! Dropping a jar into `mods/` by hand is what everyone does; this is the same
//! thing with the two mistakes checked first: a name that would escape the
//! directory, and a target that already exists (which would silently replace a
//! working mod).

use std::path::{Path, PathBuf};

use crate::core::content::ContentKind;
use crate::core::error::{Error, Result};

/// Copy `source` into `directory` under its own file name.
///
/// The file is rejected, not renamed: an import that quietly becomes
/// `a (1).jar` leaves the player with two copies and no way to tell which one
/// the game loads.
pub fn copy_in(directory: &Path, kind: ContentKind, source: &Path) -> Result<PathBuf> {
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Config(format!("invalid file name: {}", source.display())))?;
    if !source.is_file() {
        return Err(Error::Config(format!("not a file: {}", source.display())));
    }
    if !kind.accepts(source) {
        return Err(Error::Config(format!(
            "{} is not a {} file",
            source.display(),
            kind.label()
        )));
    }
    crate::core::resource::conflict::validate_file_name(name)
        .map_err(|reason| Error::Config(format!("cannot import {name}: {reason}")))?;
    std::fs::create_dir_all(directory)?;
    let target = directory.join(name);
    if target.exists() {
        return Err(Error::Config(format!(
            "{} already exists; remove or rename it first",
            target.display()
        )));
    }
    std::fs::copy(source, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::copy_in;
    use crate::core::content::ContentKind;
    use std::path::PathBuf;

    fn roots(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("tmcl-import-{tag}-{}", uuid::Uuid::new_v4()));
        let mods = root.join("mods");
        std::fs::create_dir_all(&mods).expect("mods");
        (root, mods)
    }

    #[test]
    fn a_jar_is_copied_under_its_own_name() {
        let (root, mods) = roots("copy");
        let source = root.join("downloads").join("sodium.jar");
        std::fs::create_dir_all(source.parent().expect("dir")).expect("downloads");
        std::fs::write(&source, b"jar").expect("write");

        let target = copy_in(&mods, ContentKind::Mod, &source).expect("copy");

        assert_eq!(target, mods.join("sodium.jar"));
        assert_eq!(std::fs::read(&target).expect("read"), b"jar");
        assert!(source.exists(), "the source is a copy, not a move");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn the_directory_is_created_when_missing() {
        let (root, _) = roots("create");
        let source = root.join("pack.zip");
        std::fs::write(&source, b"zip").expect("write");

        let target = copy_in(
            &root.join("resourcepacks"),
            ContentKind::ResourcePack,
            &source,
        )
        .expect("copy");

        assert!(target.exists());
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn an_existing_target_is_not_replaced() {
        let (root, mods) = roots("collision");
        std::fs::write(mods.join("sodium.jar"), b"installed").expect("existing");
        let source = root.join("sodium.jar");
        std::fs::write(&source, b"new").expect("write");

        let error = copy_in(&mods, ContentKind::Mod, &source).expect_err("rejected");

        assert!(error.to_string().contains("already exists"), "{error}");
        assert_eq!(
            std::fs::read(mods.join("sodium.jar")).expect("read"),
            b"installed"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_file_of_another_kind_is_refused() {
        let (root, mods) = roots("kind");
        let source = root.join("pack.zip");
        std::fs::write(&source, b"zip").expect("write");

        let error = copy_in(&mods, ContentKind::Mod, &source).expect_err("wrong kind");

        assert!(error.to_string().contains("is not a mod file"), "{error}");
        std::fs::remove_dir_all(root).expect("clean");
    }
}
