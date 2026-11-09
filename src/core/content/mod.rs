//! Instance content: mods, resource packs, shader packs, worlds and the data
//! packs that belong to one world.
//!
//! Every category is "one directory, one extension set, one metadata parser,
//! one group of actions", so the scanner, list rendering, filtering and
//! destructive actions are shared instead of written once per kind.

pub mod backup;
pub mod export;
pub mod import;
pub mod load;
pub mod meta;
pub mod order;
pub mod scan;
pub mod shader;
pub mod toggle;
pub mod trash;
pub mod update;
pub mod world;

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentKind {
    Mod,
    ResourcePack,
    Shader,
    World,
    /// Lives inside one world, not at the instance root; callers that scan it
    /// pass `saves/<world>/datapacks` as the directory.
    Datapack,
}

impl ContentKind {
    pub const ALL: [Self; 4] = [Self::Mod, Self::ResourcePack, Self::Shader, Self::World];

    /// Directory name inside the instance directory.
    pub fn directory(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::ResourcePack => "resourcepacks",
            Self::Shader => "shaderpacks",
            Self::World => "saves",
            Self::Datapack => "datapacks",
        }
    }

    /// Whether an entry of this kind is a directory rather than a file.
    pub fn is_directory(self) -> bool {
        matches!(self, Self::World)
    }

    /// Files this category owns. `.disabled` files stay visible: they are the
    /// user's own "switched off" marker and must be listed and re-enableable.
    pub fn accepts(self, path: &Path) -> bool {
        if self.is_directory() {
            return path.is_dir();
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        // A data pack is a zip or an unpacked folder. The game reads either,
        // so the list has to show both.
        if self == Self::Datapack {
            return path.is_dir() || name.ends_with(".zip") || name.ends_with(".zip.disabled");
        }
        let extension = match self {
            Self::Mod => ".jar",
            Self::ResourcePack | Self::Shader => ".zip",
            Self::World | Self::Datapack => return false,
        };
        name.ends_with(extension) || name.ends_with(&format!("{extension}.disabled"))
    }

    pub fn title_key(self) -> &'static str {
        match self {
            Self::Mod => "workspace.category.mods",
            Self::ResourcePack => "workspace.category.resource_packs",
            Self::Shader => "workspace.category.shaders",
            Self::World => "workspace.category.worlds",
            Self::Datapack => "workspace.category.datapacks",
        }
    }

    /// Noun for messages: "not a mod file" reads better than "not a mods file".
    pub fn label(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::ResourcePack => "resource pack",
            Self::Shader => "shader",
            Self::World => "world",
            Self::Datapack => "data pack",
        }
    }

    /// Empty-state hint shown when the directory has no entries.
    pub fn empty_key(self) -> &'static str {
        match self {
            Self::Mod => "workspace.empty.mods",
            Self::ResourcePack => "workspace.empty.resource_packs",
            Self::Shader => "workspace.empty.shaders",
            Self::World => "workspace.empty.worlds",
            Self::Datapack => "workspace.empty.datapacks",
        }
    }
}

/// Directory one content kind is read from.
///
/// Everything else lives directly under the instance. A data pack does not: it
/// belongs to exactly one world, so the world name travels with the kind instead
/// of being spliced into a path by each caller. A data pack named without a world
/// points at `<instance>/datapacks`, which does not exist; callers that list packs
/// always know which world they mean.
pub fn content_directory(root: &Path, kind: ContentKind, world: Option<&str>) -> PathBuf {
    match (kind, world) {
        (ContentKind::Datapack, Some(world)) => root.join("saves").join(world).join("datapacks"),
        (kind, _) => root.join(kind.directory()),
    }
}

/// Vanilla `pack_format` for a Minecraft release.
///
/// Only a hint: a pack with another format is not incompatible, the game warns and
/// may render it wrong, so an unknown version gets no hint at all.
pub fn expected_pack_format(minecraft_version: &str) -> Option<u32> {
    /// Lowest version that uses each format, newest last.
    const TABLE: &[(&str, u32)] = &[
        ("1.6.1", 1),
        ("1.9", 2),
        ("1.11", 3),
        ("1.13", 4),
        ("1.15", 5),
        ("1.16.2", 6),
        ("1.17", 7),
        ("1.18", 8),
        ("1.19", 9),
        ("1.19.3", 12),
        ("1.19.4", 13),
        ("1.20", 15),
        ("1.20.2", 18),
        ("1.20.3", 22),
        ("1.20.5", 32),
        ("1.21", 34),
        ("1.21.2", 42),
        ("1.21.4", 46),
        ("1.21.5", 55),
        ("1.21.6", 63),
        ("1.21.7", 64),
    ];
    let version = release_triple(minecraft_version)?;
    TABLE
        .iter()
        .rev()
        .find(|(known, _)| release_triple(known).is_some_and(|known| known <= version))
        .map(|(_, format)| *format)
}

/// `1.20.2` to `(1, 20, 2)`. Snapshots (`24w14a`) and modded ids
/// (`1.20.1-forge`) have no format entry, so anything unparsable is `None`
/// rather than a guess.
fn release_triple(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = match parts.next() {
        Some(value) => value.parse().ok()?,
        None => 0,
    };
    parts.next().is_none().then_some((major, minor, patch))
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ContentMeta {
    #[default]
    Unknown,
    Mod {
        id: String,
        name: String,
        version: String,
        authors: Vec<String>,
        description: String,
        depends: Vec<String>,
    },
    ResourcePack {
        description: String,
        pack_format: Option<u32>,
    },
    Shader {
        description: String,
    },
    World {
        level_name: String,
        /// The version name recorded in `level.dat`; `None` when unreadable.
        version: Option<String>,
        last_played: Option<i64>,
        game_mode: Option<String>,
        hardcore: bool,
        /// False when `level.dat` could not be parsed; the name is then the
        /// folder name and the list says so.
        parsed: bool,
    },
}

impl ContentMeta {
    pub fn description(&self) -> &str {
        match self {
            Self::Unknown => "",
            Self::Mod { description, .. } => description,
            Self::ResourcePack { description, .. } => description,
            Self::Shader { description } => description,
            Self::World { .. } => "",
        }
    }

    /// Name the descriptor advertises, if any.
    pub fn display_name(&self) -> Option<&str> {
        match self {
            Self::Mod { name, .. } => Some(name),
            Self::World { level_name, .. } => Some(level_name),
            _ => None,
        }
    }

    pub fn version(&self) -> Option<String> {
        match self {
            Self::Mod { version, .. } if !version.is_empty() => Some(version.clone()),
            Self::ResourcePack {
                pack_format: Some(format),
                ..
            } => Some(format.to_string()),
            Self::World { version, .. } => version.clone().filter(|value| !value.is_empty()),
            _ => None,
        }
    }

    /// Identity used for duplicate detection.
    pub fn mod_id(&self) -> Option<&str> {
        match self {
            Self::Mod { id, .. } => Some(id.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentEntry {
    /// Display name: the metadata name, falling back to the file stem.
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub size: u64,
    pub enabled: bool,
    /// Position in the saved enable order.
    ///
    /// Only resource packs and shaders have one: their "on" state lives in a
    /// file the game reads in order, so the list has to show that order instead
    /// of sorting the same way it does for mods.
    pub order: Option<usize>,
    /// Short right-aligned tag: loader, `pack_format`, game version.
    pub badge: Option<String>,
    /// Why this entry will not work as-is. Shown even on narrow terminals.
    /// Duplicate mods store the file name they clash with; the renderer adds
    /// the localized "duplicate of" phrasing.
    pub warning: Option<String>,
    pub meta: ContentMeta,
}

impl ContentEntry {
    /// File name without the `.disabled` marker, for display.
    pub fn display_file_name(&self) -> String {
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        name.strip_suffix(".disabled").unwrap_or(name).to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{ContentEntry, ContentKind};
    use std::path::{Path, PathBuf};

    #[test]
    fn expected_pack_format_follows_the_release_table() {
        assert_eq!(super::expected_pack_format("1.20.1"), Some(15));
        assert_eq!(super::expected_pack_format("1.20.4"), Some(22));
        assert_eq!(super::expected_pack_format("1.21"), Some(34));
        assert_eq!(super::expected_pack_format("1.12.2"), Some(3));
        assert_eq!(super::expected_pack_format("1.16.5"), Some(6));
        // Snapshots and modded ids have no entry; a hint must not be invented.
        assert_eq!(super::expected_pack_format("24w14a"), None);
        assert_eq!(super::expected_pack_format("1.20.1-forge"), None);
    }

    #[test]
    fn content_kind_directories() {
        assert_eq!(ContentKind::Mod.directory(), "mods");
        assert_eq!(ContentKind::ResourcePack.directory(), "resourcepacks");
        assert_eq!(ContentKind::Shader.directory(), "shaderpacks");
        assert_eq!(ContentKind::World.directory(), "saves");
        assert_eq!(ContentKind::Datapack.directory(), "datapacks");
    }

    #[test]
    fn a_data_pack_is_a_zip_or_an_unpacked_folder() {
        assert!(ContentKind::Datapack.accepts(Path::new("packs/terrain.zip")));
        assert!(ContentKind::Datapack.accepts(Path::new("packs/terrain.zip.disabled")));
        assert!(!ContentKind::Datapack.accepts(Path::new("packs/readme.txt")));
        // A mod is not a data pack, even though both are archives.
        assert!(!ContentKind::Mod.accepts(Path::new("packs/terrain.zip")));
    }

    #[test]
    fn only_worlds_are_directory_entries() {
        assert!(!ContentKind::Mod.is_directory());
        assert!(ContentKind::World.is_directory());
        assert!(!ContentKind::World.accepts(Path::new("saves/world.zip")));
        assert!(ContentKind::Mod.accepts(Path::new("mods/a.jar")));
        assert!(ContentKind::Mod.accepts(Path::new("mods/a.jar.disabled")));
        assert!(!ContentKind::Mod.accepts(Path::new("mods/readme.txt")));
        assert!(ContentKind::Shader.accepts(Path::new("shaderpacks/x.zip")));
    }

    #[test]
    fn display_name_drops_the_disabled_marker() {
        let entry = ContentEntry {
            name: "Example".into(),
            version: "1.0".into(),
            path: PathBuf::from("mods/example.jar.disabled"),
            size: 0,
            enabled: false,
            order: None,
            badge: None,
            warning: None,
            meta: Default::default(),
        };
        assert_eq!(entry.display_file_name(), "example.jar");
    }
}
