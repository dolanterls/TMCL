//! Directory scanning for the four content kinds.
//!
//! Reading a descriptor means opening the archive, so a scan is I/O heavy by
//! nature. Callers run it off the UI thread and cache the result keyed by the
//! directory timestamp; this module stays pure and synchronous.

use std::path::Path;

use crate::core::content::{ContentEntry, ContentKind, ContentMeta, meta};

/// List the entries of `root` that belong to `kind`, sorted by display name.
///
/// A missing directory is not an error: a fresh instance simply has no mods
/// yet, and the UI shows its empty state instead of an error.
pub fn scan(root: &Path, kind: ContentKind) -> Vec<ContentEntry> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut items = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| kind.accepts(path))
        .map(|path| describe(&path, kind))
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.name.to_lowercase());
    items
}

/// Overlay the enable state the game reads from its own config.
///
/// Mods are switched on and off by renaming the file; a resource pack or a
/// shader is switched on by a line in a config file and the file never moves.
/// The scanner reports the file name state, this function corrects it.
pub fn apply_saved_state(root: &Path, kind: ContentKind, entries: &mut [ContentEntry]) {
    match kind {
        ContentKind::ResourcePack => {
            let saved = crate::core::content::toggle::read_resource_packs(root);
            for entry in entries.iter_mut() {
                let name = entry.display_file_name();
                let position = saved.iter().position(|pack| {
                    crate::core::content::toggle::pack_file_name(pack).eq_ignore_ascii_case(&name)
                });
                // `.disabled` is a hard off: the game does not even list it.
                let hard_off = crate::core::content::toggle::is_disabled(&entry.path);
                entry.enabled = !hard_off && position.is_some();
                entry.order = entry.enabled.then_some(position.unwrap_or(0));
            }
        }
        ContentKind::Shader => {
            let loader = crate::core::content::shader::detect(&root.join("mods"));
            let selected = crate::core::content::shader::selected(root);
            for entry in entries.iter_mut() {
                let name = entry.display_file_name();
                let on = loader.is_some()
                    && selected
                        .as_deref()
                        .is_some_and(|current| current.eq_ignore_ascii_case(&name));
                entry.enabled = on;
                entry.order = on.then_some(0);
            }
        }
        ContentKind::Mod | ContentKind::World | ContentKind::Datapack => {}
    }
}

/// Whether duplicate mod ids are present. The first entry keeps the id; every
/// copy after it is flagged with the file it clashes with.
pub fn flag_duplicate_mods(items: &mut [ContentEntry]) {
    for index in 0..items.len() {
        let Some(id) = items[index].meta.mod_id().map(str::to_owned) else {
            continue;
        };
        let clash = items[..index]
            .iter()
            .find(|other| other.meta.mod_id() == Some(id.as_str()))
            .map(|other| other.display_file_name());
        if let Some(other) = clash {
            items[index].warning = Some(other);
        }
    }
}

fn describe(path: &Path, kind: ContentKind) -> ContentEntry {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let enabled = !file_name.ends_with(".disabled");
    let meta = match kind {
        ContentKind::Mod => meta::parse_mod(path).unwrap_or_default(),
        ContentKind::ResourcePack => meta::parse_pack(path),
        ContentKind::Shader => meta::parse_shader(path),
        ContentKind::World => world_meta(path),
        // A data pack uses the same `pack.mcmeta`; a folder-shaped pack has no
        // archive to read and falls back to its name.
        ContentKind::Datapack => meta::parse_pack(path),
    };
    let fallback = file_stem(path);
    let name = meta
        .display_name()
        .map(str::to_owned)
        .unwrap_or_else(|| fallback.clone());
    ContentEntry {
        name,
        version: meta.version().unwrap_or_default(),
        path: path.to_path_buf(),
        size: entry_size(path, kind),
        enabled,
        order: None,
        badge: None,
        warning: None,
        meta,
    }
}

/// A world is named and dated by `level.dat`; when that file cannot be read the
/// folder name stands in and the flag lets the list say why the columns are bare.
fn world_meta(path: &Path) -> ContentMeta {
    let folder = file_stem(path);
    match crate::core::content::world::read(&path.join("level.dat")) {
        Some(info) => ContentMeta::World {
            level_name: if info.level_name.is_empty() {
                folder
            } else {
                info.level_name
            },
            version: info.version,
            last_played: info.last_played,
            game_mode: info.game_mode,
            hardcore: info.hardcore,
            parsed: true,
        },
        None => ContentMeta::World {
            level_name: folder,
            version: None,
            last_played: None,
            game_mode: None,
            hardcore: false,
            parsed: false,
        },
    }
}

fn file_stem(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let name = name.strip_suffix(".disabled").unwrap_or(name);
    Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name)
        .to_owned()
}

fn entry_size(path: &Path, _kind: ContentKind) -> u64 {
    // A data pack is either a zip or an unpacked folder, so the path decides,
    // not the kind.
    if path.is_dir() {
        return directory_size(path, 0);
    }
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Total size of a directory tree, for the overview's size row.
pub fn tree_size(path: &Path) -> u64 {
    directory_size(path, 0)
}

/// Recursive size, bounded so a symlink loop cannot hang the scanner.
fn directory_size(path: &Path, depth: u8) -> u64 {
    const MAX_DEPTH: u8 = 12;
    if depth > MAX_DEPTH {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                return 0;
            };
            if metadata.is_dir() {
                directory_size(&path, depth + 1)
            } else if metadata.is_symlink() {
                0
            } else {
                metadata.len()
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{flag_duplicate_mods, scan};
    use crate::core::content::{ContentEntry, ContentKind, ContentMeta};

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("tmcl-scan-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create temp root");
        path
    }

    #[test]
    fn scan_mods_lists_jars_and_marks_disabled() {
        let root = temp_root("mods");
        std::fs::write(root.join("a.jar"), b"x").expect("write a");
        std::fs::write(root.join("b.jar.disabled"), b"x").expect("write b");
        std::fs::write(root.join("readme.txt"), b"x").expect("write readme");

        let items = scan(&root, ContentKind::Mod);
        assert_eq!(items.len(), 2, "{items:?}");
        let b = items
            .iter()
            .find(|item| item.path.ends_with("b.jar.disabled"))
            .expect("disabled entry");
        assert!(!b.enabled);
        assert_eq!(b.display_file_name(), "b.jar");
        std::fs::remove_dir_all(root).expect("clean temp root");
    }

    #[test]
    fn scan_missing_directory_is_empty() {
        let missing =
            std::env::temp_dir().join(format!("tmcl-scan-missing-{}", uuid::Uuid::new_v4()));
        assert!(scan(&missing, ContentKind::Mod).is_empty());
    }

    #[test]
    fn scan_skips_meta_files() {
        let root = temp_root("meta");
        std::fs::write(root.join(".DS_Store"), b"x").expect("write ds_store");
        std::fs::write(root.join("desktop.ini"), b"x").expect("write ini");
        assert!(scan(&root, ContentKind::Mod).is_empty());
        std::fs::remove_dir_all(root).expect("clean temp root");
    }

    #[test]
    fn scan_worlds_lists_directories() {
        let root = temp_root("worlds");
        std::fs::create_dir_all(root.join("world one")).expect("create world");
        std::fs::create_dir_all(root.join("world two")).expect("create world");
        std::fs::write(root.join("notes.txt"), b"x").expect("write note");
        let items = scan(&root, ContentKind::World);
        assert_eq!(items.len(), 2, "{items:?}");
        assert!(items.iter().all(|item| item.name.starts_with("world")));
        std::fs::remove_dir_all(root).expect("clean temp root");
    }

    fn entry(name: &str, id: &str) -> ContentEntry {
        ContentEntry {
            name: name.into(),
            version: String::new(),
            path: std::path::PathBuf::from(format!("mods/{name}.jar")),
            size: 0,
            enabled: true,
            order: None,
            badge: None,
            warning: None,
            meta: ContentMeta::Mod {
                id: id.into(),
                name: name.into(),
                version: String::new(),
                authors: Vec::new(),
                description: String::new(),
                depends: Vec::new(),
            },
        }
    }

    #[test]
    fn duplicate_mod_ids_are_flagged() {
        let mut items = vec![
            entry("first", "create"),
            entry("second", "create"),
            entry("other", "jei"),
        ];
        flag_duplicate_mods(&mut items);
        assert!(items[0].warning.is_none());
        assert_eq!(items[1].warning.as_deref(), Some("first.jar"));
        assert!(items[2].warning.is_none());
    }
}

#[cfg(test)]
mod world_tests {
    use super::scan;
    use crate::core::content::{ContentKind, ContentMeta};

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("tmcl-scan-world-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create temp root");
        path
    }

    /// Root compound → `Data` with a name, a version and a timestamp.
    fn level_dat(name: &str) -> Vec<u8> {
        let mut data = vec![8];
        data.extend((9u16).to_be_bytes());
        data.extend(b"LevelName");
        data.extend((name.len() as u16).to_be_bytes());
        data.extend(name.as_bytes());
        data.push(10);
        data.extend((7u16).to_be_bytes());
        data.extend(b"Version");
        data.push(8);
        data.extend((4u16).to_be_bytes());
        data.extend(b"Name");
        data.extend((6u16).to_be_bytes());
        data.extend(b"1.20.1");
        data.push(0);
        data.push(0);

        let mut root = vec![10];
        root.extend(0u16.to_be_bytes());
        root.push(10);
        root.extend((4u16).to_be_bytes());
        root.extend(b"Data");
        root.extend(data);
        root.push(0);
        root
    }

    #[test]
    fn scan_worlds_fills_level_metadata() {
        let root = temp_root("meta");
        let world = root.join("saves/Birthday World");
        std::fs::create_dir_all(&world).expect("create world");
        std::fs::write(world.join("level.dat"), level_dat("Birthday World"))
            .expect("write level.dat");

        let items = scan(&root.join("saves"), ContentKind::World);
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].name, "Birthday World");
        assert_eq!(items[0].version, "1.20.1");
        match &items[0].meta {
            ContentMeta::World {
                parsed, version, ..
            } => {
                assert!(*parsed);
                assert_eq!(version.as_deref(), Some("1.20.1"));
            }
            other => panic!("expected world metadata, got {other:?}"),
        }
        std::fs::remove_dir_all(root).expect("clean temp root");
    }

    #[test]
    fn scan_worlds_falls_back_to_the_folder_name() {
        let root = temp_root("broken");
        let world = root.join("saves/Broken Save");
        std::fs::create_dir_all(&world).expect("create world");
        std::fs::write(world.join("level.dat"), b"not nbt").expect("write level.dat");

        let items = scan(&root.join("saves"), ContentKind::World);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Broken Save");
        assert!(items[0].version.is_empty());
        assert!(matches!(
            items[0].meta,
            ContentMeta::World { parsed: false, .. }
        ));
        std::fs::remove_dir_all(root).expect("clean temp root");
    }
}
