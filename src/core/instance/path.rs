use std::path::{Path, PathBuf};

use crate::core::error::{Error, Result};

pub fn normalize_game_directory(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    if !path.is_dir() {
        return Err(Error::Config(format!(
            "game directory does not exist: {}",
            path.display()
        )));
    }
    Ok(normalize_canonical(path.canonicalize()?))
}

pub fn display_path(path: &Path) -> String {
    normalize_canonical(path.to_path_buf())
        .display()
        .to_string()
}

pub fn executable_directory() -> Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let directory = executable.parent().ok_or_else(|| {
        Error::Config(format!(
            "cannot determine launcher directory from {}",
            executable.display()
        ))
    })?;
    normalize_game_directory(directory)
}

#[cfg(windows)]
pub(crate) fn normalize_canonical(path: PathBuf) -> PathBuf {
    let Some(value) = path.to_str() else {
        return path;
    };
    let Some(value) = value.strip_prefix(r"\\?\") else {
        return path;
    };

    if let Some(unc) = value
        .strip_prefix("UNC\\")
        .or_else(|| value.strip_prefix("UNC/"))
    {
        return PathBuf::from(format!(r"\\{unc}"));
    }

    let bytes = value.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
    {
        return PathBuf::from(value);
    }

    path
}

#[cfg(not(windows))]
pub(crate) fn normalize_canonical(path: PathBuf) -> PathBuf {
    path
}

pub fn deduplicate(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for path in paths {
        if !result.iter().any(|existing| existing == &path) {
            result.push(path);
        }
    }
    result
}

/// The game directory an instance belongs to.
///
/// Instances live at `<game_directory>/versions/<name>`, so the game directory is
/// the grandparent. Anything that does not sit under a `versions` directory is
/// rejected instead of guessed: a caller inventing its own fallback
/// (`parent().parent()` with an `unwrap`) disagrees with the others about a
/// malformed path.
pub fn game_directory_of(instance_directory: &Path) -> Result<PathBuf> {
    let normalized = normalize_canonical(
        instance_directory
            .canonicalize()
            .unwrap_or_else(|_| instance_directory.to_path_buf()),
    );
    let versions = normalized.parent().ok_or_else(|| {
        Error::Config(format!(
            "instance directory has no parent: {}",
            normalized.display()
        ))
    })?;
    if versions.file_name().and_then(|name| name.to_str()) != Some("versions") {
        return Err(Error::Config(format!(
            "instance directory must be under a versions directory: {}",
            normalized.display()
        )));
    }
    let game_directory = versions.parent().ok_or_else(|| {
        Error::Config(format!(
            "versions directory has no parent: {}",
            versions.display()
        ))
    })?;
    if game_directory.as_os_str().is_empty() {
        return Err(Error::Config(format!(
            "instance directory must be under a versions directory: {}",
            normalized.display()
        )));
    }
    Ok(normalize_canonical(game_directory.to_path_buf()))
}

/// Stable identifier of a game directory, derived from its path.
///
/// `tmcl.json` records this id, so exactly one function may produce it: two
/// implementations would disagree as soon as one of them normalized the path
/// differently, and every instance would look like it belonged to a foreign
/// launcher.
pub fn game_directory_id(path: &Path) -> uuid::Uuid {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        path.to_string_lossy().as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::{executable_directory, game_directory_of, normalize_canonical};

    #[test]
    fn game_directory_is_the_grandparent_of_the_instance() {
        let instance = std::path::Path::new("games")
            .join("versions")
            .join("survival");
        assert_eq!(
            game_directory_of(&instance).expect("game directory"),
            std::path::PathBuf::from("games")
        );
    }

    #[test]
    fn a_path_outside_versions_is_rejected() {
        let instance = std::path::Path::new("games").join("survival");
        let error = game_directory_of(&instance).expect_err("rejected");
        assert!(
            error.to_string().contains("under a versions directory"),
            "{error}"
        );
    }

    #[test]
    fn a_versions_directory_itself_is_not_an_instance() {
        let versions = std::path::Path::new("games").join("versions");
        assert!(game_directory_of(&versions).is_err());
    }

    #[test]
    fn a_relative_versions_root_without_a_game_directory_is_rejected() {
        let instance = std::path::Path::new("versions").join("survival");
        assert!(game_directory_of(&instance).is_err());
    }

    #[test]
    fn the_game_directory_id_is_derived_from_the_path() {
        let first = super::game_directory_id(std::path::Path::new("games"));
        let second = super::game_directory_id(std::path::Path::new("games"));
        let other = super::game_directory_id(std::path::Path::new("other"));
        assert_eq!(first, second);
        assert_ne!(first, other);
    }

    #[cfg(windows)]
    #[test]
    fn strips_windows_verbatim_prefix_from_drive_path() {
        assert_eq!(
            normalize_canonical(std::path::PathBuf::from(r"\\?\D:\Games\.minecraft")),
            std::path::PathBuf::from(r"D:\Games\.minecraft")
        );
    }

    #[cfg(windows)]
    #[test]
    fn converts_windows_verbatim_unc_prefix_to_unc_path() {
        assert_eq!(
            normalize_canonical(std::path::PathBuf::from(
                r"\\?\UNC\server\share\Games\.minecraft"
            )),
            std::path::PathBuf::from(r"\\server\share\Games\.minecraft")
        );
    }

    #[cfg(windows)]
    #[test]
    fn display_path_hides_verbatim_drive_and_unc_prefixes() {
        assert_eq!(
            super::display_path(std::path::Path::new(r"\\?\D:\Games\.minecraft")),
            r"D:\Games\.minecraft"
        );
        assert_eq!(
            super::display_path(std::path::Path::new(r"\\?\UNC\server\share\Games")),
            r"\\server\share\Games"
        );
    }

    #[test]
    fn default_game_directory_is_the_executable_parent() {
        let expected = std::env::current_exe()
            .expect("current executable")
            .parent()
            .expect("executable parent")
            .canonicalize()
            .expect("canonical executable parent");

        assert_eq!(
            executable_directory().expect("executable directory"),
            normalize_canonical(expected)
        );
    }
}
