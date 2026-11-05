use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn separator() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

pub fn build(game_directory: &Path, libraries: &[PathBuf]) -> String {
    let mut seen = HashSet::with_capacity(libraries.len());
    let mut paths = Vec::with_capacity(libraries.len());
    for library in libraries {
        let path = game_directory.join(library).to_string_lossy().into_owned();
        let key = if cfg!(windows) {
            path.to_lowercase()
        } else {
            path.clone()
        };
        if seen.insert(key) {
            paths.push(path);
        }
    }
    paths.join(&separator().to_string())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{build, separator};

    #[test]
    fn uses_platform_classpath_separator() {
        assert_eq!(separator(), if cfg!(windows) { ';' } else { ':' });
        let value = build(
            std::path::Path::new("C:/Games"),
            &[std::path::PathBuf::from("libraries/a.jar")],
        );
        assert!(value.contains("libraries"));
    }

    #[test]
    fn removes_repeated_library_paths_without_reordering() {
        let duplicate = PathBuf::from("libraries/a.jar");
        let value = build(
            Path::new("C:/Games"),
            &[
                duplicate.clone(),
                PathBuf::from("libraries/b.jar"),
                duplicate,
            ],
        );

        let paths = value.split(separator()).collect::<Vec<_>>();
        assert_eq!(paths.len(), 2);
        assert!(PathBuf::from(paths[0]).ends_with(Path::new("libraries/a.jar")));
        assert!(PathBuf::from(paths[1]).ends_with(Path::new("libraries/b.jar")));
    }
}
