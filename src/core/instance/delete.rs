use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::error::{Error, Result};

pub fn delete_metadata(game_directory: &Path, instance_directory: &Path) -> Result<()> {
    let game_directory = game_directory.canonicalize()?;
    let instance_directory = instance_directory.canonicalize()?;
    if instance_directory == game_directory {
        return Err(Error::Config(
            "refusing to delete metadata at the game directory root".into(),
        ));
    }
    let metadata = instance_directory.join("tmcl.json");
    if metadata.exists() {
        std::fs::remove_file(metadata)?;
    }
    Ok(())
}

/// Remove an instance directory and everything inside it.
///
/// `delete_metadata` only forgets an instance; this is the destructive one, so
/// it carries guards that no caller can skip: the target must sit directly
/// under `<game_directory>/versions/`, and it must look like an instance.
///
/// The directory is renamed before it is removed. A half-deleted directory that
/// still carries its original name would be picked up by the next scan as a
/// broken instance; one named `<name>.deleting-<stamp>` is ignored instead.
pub fn delete_instance(game_directory: &Path, instance_directory: &Path) -> Result<()> {
    let game_directory = game_directory.canonicalize().map_err(|error| {
        Error::Config(format!(
            "game directory is not readable: {}: {error}",
            game_directory.display()
        ))
    })?;
    let instance_directory = instance_directory.canonicalize().map_err(|error| {
        Error::Config(format!(
            "instance directory is not readable: {}: {error}",
            instance_directory.display()
        ))
    })?;

    if instance_directory == game_directory {
        return Err(Error::Config(
            "refusing to delete the game directory itself".into(),
        ));
    }
    let versions = game_directory.join("versions");
    if instance_directory == versions || !instance_directory.starts_with(&versions) {
        return Err(Error::Config(format!(
            "refusing to delete a directory outside versions: {}",
            instance_directory.display()
        )));
    }

    let name = instance_directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            Error::Config(format!(
                "instance directory has no name: {}",
                instance_directory.display()
            ))
        })?;
    if !instance_directory.join("tmcl.json").is_file()
        && !instance_directory.join(format!("{name}.json")).is_file()
    {
        return Err(Error::Config(format!(
            "refusing to delete a directory that is not an instance: {}",
            instance_directory.display()
        )));
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let tombstone = instance_directory.with_file_name(format!("{name}.deleting-{stamp}"));
    std::fs::rename(&instance_directory, &tombstone)?;
    std::fs::remove_dir_all(&tombstone)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::delete_instance;

    fn game_dir(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "tmcl-delete-instance-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("game dir");
        root
    }

    fn instance_dir(game: &Path, name: &str) -> PathBuf {
        let instance = game.join("versions").join(name);
        std::fs::create_dir_all(&instance).expect("instance dir");
        std::fs::write(instance.join("tmcl.json"), b"{}").expect("metadata");
        instance
    }

    #[test]
    fn an_instance_directory_is_removed_completely() {
        let game = game_dir("ok");
        let instance = instance_dir(&game, "demo");
        std::fs::write(instance.join("demo.jar"), b"jar").expect("jar");

        delete_instance(&game, &instance).expect("delete");

        assert!(!instance.exists());
        std::fs::remove_dir_all(game).expect("clean");
    }

    #[test]
    fn the_game_directory_itself_is_refused() {
        let game = game_dir("root");
        assert!(delete_instance(&game, &game).is_err());
        std::fs::remove_dir_all(game).expect("clean");
    }

    #[test]
    fn a_directory_outside_versions_is_refused() {
        let game = game_dir("outside");
        let other = game.join("other").join("demo");
        std::fs::create_dir_all(&other).expect("dir");
        std::fs::write(other.join("tmcl.json"), b"{}").expect("metadata");

        let error = delete_instance(&game, &other).expect_err("refused");

        assert!(error.to_string().contains("outside versions"), "{error}");
        assert!(other.exists());
        std::fs::remove_dir_all(game).expect("clean");
    }

    #[test]
    fn a_directory_that_is_not_an_instance_is_refused() {
        let game = game_dir("stranger");
        let stranger = game.join("versions").join("stranger");
        std::fs::create_dir_all(&stranger).expect("dir");

        let error = delete_instance(&game, &stranger).expect_err("refused");

        assert!(error.to_string().contains("not an instance"), "{error}");
        assert!(stranger.exists());
        std::fs::remove_dir_all(game).expect("clean");
    }

    #[test]
    fn a_version_json_alone_makes_a_directory_deletable() {
        let game = game_dir("version-only");
        let instance = game.join("versions").join("imported");
        std::fs::create_dir_all(&instance).expect("dir");
        std::fs::write(instance.join("imported.json"), b"{}").expect("client json");

        delete_instance(&game, &instance).expect("delete");

        assert!(!instance.exists());
        std::fs::remove_dir_all(game).expect("clean");
    }
}
