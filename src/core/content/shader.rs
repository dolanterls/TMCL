//! Shader packs.
//!
//! Minecraft itself cannot load a shader pack: Iris or OptiFine does. That makes
//! this category different from mods and resource packs. The first useful thing a
//! launcher can say is "you have shaders but nothing that reads them", and the
//! selection lives in the loader's own config, not in `options.txt`.

use std::path::Path;

use crate::core::error::Result;

/// Which loader reads `shaderpacks/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderLoader {
    Iris,
    /// OptiFine still uses its own `optionsshaders.txt`.
    OptiFine,
}

impl ShaderLoader {
    pub fn label(self) -> &'static str {
        match self {
            Self::Iris => "Iris",
            Self::OptiFine => "OptiFine",
        }
    }
}

const IRIS_CONFIG: &str = "config/iris.properties";
const OPTIFINE_CONFIG: &str = "optionsshaders.txt";
const PACK_KEY: &str = "shaderPack";

/// Detect the shader loader installed in `mods_dir`.
///
/// File names decide first because they are free; only jars whose name says
/// nothing are opened to read their descriptor id, which is what a renamed
/// download still carries.
pub fn detect(mods_dir: &Path) -> Option<ShaderLoader> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return None;
    };
    let paths = entries
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();

    for path in &paths {
        if let Some(loader) = loader_from_name(path) {
            return Some(loader);
        }
    }
    for path in paths.iter().filter(|path| is_jar(path)) {
        if let Some(loader) = loader_from_id(path) {
            return Some(loader);
        }
    }
    None
}

fn is_jar(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
}

fn loader_from_name(path: &Path) -> Option<ShaderLoader> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name.contains("iris") {
        Some(ShaderLoader::Iris)
    } else if name.contains("optifine") {
        Some(ShaderLoader::OptiFine)
    } else {
        None
    }
}

fn loader_from_id(path: &Path) -> Option<ShaderLoader> {
    let meta = crate::core::content::meta::parse_mod(path)?;
    let id = meta.mod_id()?.to_ascii_lowercase();
    if id.starts_with("iris") {
        Some(ShaderLoader::Iris)
    } else if id.contains("optifine") {
        Some(ShaderLoader::OptiFine)
    } else {
        None
    }
}

/// The shader pack currently selected, or `None` when shaders are off.
///
/// Iris is checked first: when both configs exist the player is running Iris
/// (OptiFine writes its file only for its own profile).
pub fn selected(root: &Path) -> Option<String> {
    selected_in(&root.join(IRIS_CONFIG), '=')
        .or_else(|| selected_in(&root.join(OPTIFINE_CONFIG), ':'))
}

fn selected_in(path: &Path, separator: char) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(separator)?;
            (key.trim() == PACK_KEY).then(|| value.trim().to_owned())
        })
        .next_back()
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("off"))
}

/// Select `name` (empty clears the selection) in the file the loader reads.
///
/// Without a detected loader Iris is assumed, because the empty state tells the
/// player to install Iris.
pub fn set_selected(root: &Path, loader: Option<ShaderLoader>, name: &str) -> Result<()> {
    let (relative, separator) = match loader {
        Some(ShaderLoader::OptiFine) => (OPTIFINE_CONFIG, ':'),
        _ => (IRIS_CONFIG, '='),
    };
    set_key(&root.join(relative), separator, PACK_KEY, name)
}

/// Replace one `key<sep>value` line, keeping every other line untouched.
fn set_key(path: &Path, separator: char, key: &str, value: &str) -> Result<()> {
    let existing = std::fs::read_to_string(path).ok();
    let mut lines = existing
        .as_deref()
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let line = format!("{key}{separator}{value}");
    match lines.iter().position(|current| {
        current
            .split_once(separator)
            .is_some_and(|(current, _)| current.trim() == key)
    }) {
        Some(index) => lines[index] = line,
        None => lines.push(line),
    }
    let mut text = lines.join("\n");
    text.push('\n');
    crate::core::config::storage::write_text_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::{ShaderLoader, detect, selected, set_selected};
    use std::path::{Path, PathBuf};

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tmcl-shader-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("mods")).expect("create mods dir");
        root
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, text).expect("write file");
    }

    #[test]
    fn shader_detects_iris_by_file_name() {
        let root = temp_root("detect");
        write(&root.join("mods/iris-1.7.0.jar"), "jar");
        assert_eq!(detect(&root.join("mods")), Some(ShaderLoader::Iris));
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn shader_detects_optifine_and_nothing() {
        let root = temp_root("none");
        assert_eq!(detect(&root.join("mods")), None);
        write(&root.join("mods/OptiFine_1.20.1_HD_U_I6.jar"), "jar");
        assert_eq!(detect(&root.join("mods")), Some(ShaderLoader::OptiFine));
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn shader_selection_round_trips() {
        let root = temp_root("roundtrip");
        assert_eq!(selected(&root), None);

        set_selected(&root, Some(ShaderLoader::Iris), "BSL_v8.zip").expect("select");
        assert_eq!(selected(&root).as_deref(), Some("BSL_v8.zip"));
        let config =
            std::fs::read_to_string(root.join("config/iris.properties")).expect("read config");
        assert!(config.contains("shaderPack=BSL_v8.zip"), "{config}");

        // An empty name clears the selection without deleting the file.
        set_selected(&root, Some(ShaderLoader::Iris), "").expect("clear");
        assert_eq!(selected(&root), None);
        assert!(root.join("config/iris.properties").is_file());
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn shader_preserves_other_properties() {
        let root = temp_root("preserve");
        write(
            &root.join("config/iris.properties"),
            "enableShaders=true\nshaderPack=old.zip\nmaxShadowRenderDistance=32\n",
        );
        set_selected(&root, Some(ShaderLoader::Iris), "new.zip").expect("select");
        let text = std::fs::read_to_string(root.join("config/iris.properties")).expect("read");
        assert!(text.contains("enableShaders=true"), "{text}");
        assert!(text.contains("maxShadowRenderDistance=32"), "{text}");
        assert!(text.contains("shaderPack=new.zip"), "{text}");
        assert_eq!(text.matches("shaderPack=").count(), 1, "{text}");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn optifine_uses_its_own_file() {
        let root = temp_root("optifine");
        set_selected(&root, Some(ShaderLoader::OptiFine), "SEUS.zip").expect("select");
        assert!(root.join("optionsshaders.txt").is_file());
        assert_eq!(selected(&root).as_deref(), Some("SEUS.zip"));
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn optifine_off_is_not_a_selection() {
        let root = temp_root("off");
        write(&root.join("optionsshaders.txt"), "shaderPack:OFF\n");
        assert_eq!(selected(&root), None);
        std::fs::remove_dir_all(root).expect("clean");
    }
}
