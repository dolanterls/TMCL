//! Switching a content entry off without deleting it.
//!
//! The marker is the file name suffix `.disabled`, which is what other
//! launchers and manual users already do: Minecraft only loads `.jar`/`.zip`
//! entries, so renaming is enough and nothing inside the file changes.

use std::path::{Path, PathBuf};

use crate::core::error::{Error, Result};

pub const DISABLED: &str = ".disabled";

/// The game writes its client options as `key:value` lines.
pub const OPTIONS_FILE: &str = "options.txt";
/// Copy taken before the first write, so a bad edit can be undone by hand.
pub const OPTIONS_BACKUP: &str = "options.txt.bak";
/// The one key this module rewrites; every other line is preserved verbatim.
const RESOURCE_PACKS_KEY: &str = "resourcePacks:";

/// Enabled resource packs, in stack order (later entries draw on top).
///
/// A missing file, a missing key or an unparsable array all mean "nothing is
/// enabled": the game shows the same thing, and a launcher must not refuse to open
/// because a player edited the file by hand.
pub fn read_resource_packs(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join(OPTIONS_FILE)) else {
        return Vec::new();
    };
    text.lines()
        .find_map(|line| line.strip_prefix(RESOURCE_PACKS_KEY))
        .and_then(|value| serde_json::from_str::<Vec<String>>(value.trim()).ok())
        .unwrap_or_default()
}

/// Replace the `resourcePacks` line, leaving every other byte of the file alone.
///
/// Minecraft expects `vanilla` in the list (it is the fallback pack), so it is
/// kept first even when the caller forgot it.
pub fn write_resource_packs(root: &Path, enabled: &[String]) -> Result<()> {
    let path = root.join(OPTIONS_FILE);
    let existing = std::fs::read_to_string(&path).ok();
    if let Some(text) = &existing {
        std::fs::write(root.join(OPTIONS_BACKUP), text)?;
    }

    let mut packs = enabled.to_vec();
    if !packs.iter().any(|pack| pack == "vanilla") {
        packs.insert(0, "vanilla".to_owned());
    }
    let value = serde_json::to_string(&packs).map_err(std::io::Error::other)?;

    let mut lines = existing
        .as_deref()
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let line = format!("{RESOURCE_PACKS_KEY}{value}");
    match lines
        .iter()
        .position(|current| current.starts_with(RESOURCE_PACKS_KEY))
    {
        Some(index) => lines[index] = line,
        None => lines.push(line),
    }
    let mut text = lines.join("\n");
    text.push('\n');
    crate::core::config::storage::write_text_atomic(&path, &text)
}

/// The file name an `options.txt` entry points at.
///
/// The game stores packs as `file/<name>` (and directories as `file/<dir>/`),
/// so comparing entries with scanned files needs the last path component.
pub fn pack_file_name(entry: &str) -> &str {
    entry
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(entry)
}

/// Rename `path` to its enabled or disabled form and return the new path.
///
/// Idempotent: asking for the state a file is already in only normalises the
/// name. A target that already exists is an error instead of an overwrite.
pub fn set_enabled(path: &Path, enabled: bool) -> Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Config(format!("invalid content path: {}", path.display())))?;
    let stem = name.strip_suffix(DISABLED).unwrap_or(name);
    let target = path.with_file_name(if enabled {
        stem.to_owned()
    } else {
        format!("{stem}{DISABLED}")
    });
    if target == path {
        return Ok(target);
    }
    if target.exists() {
        // The source is gone and the destination is there: this file has already
        // been moved, and the caller holds the name it had before (a background
        // scan refreshes the workspace list, so a second keypress arrives inside
        // that window). That is the state being asked for, not a collision; only
        // two live files are.
        if !path.exists() {
            return Ok(target);
        }
        return Err(Error::Config(format!(
            "cannot rename {}: {} already exists",
            path.display(),
            target.display()
        )));
    }
    std::fs::rename(path, &target)?;
    Ok(target)
}

pub fn is_disabled(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(DISABLED))
}

/// Add or remove one pack from `options.txt`, keeping the rest of the stack.
///
/// A pack that is switched on is appended last: in Minecraft the last entry wins,
/// which is what "enabled" is expected to mean.
pub fn set_pack_enabled(root: &Path, file: &str, enabled: bool) -> Result<()> {
    let mut packs = read_resource_packs(root);
    packs.retain(|pack| !pack_file_name(pack).eq_ignore_ascii_case(file));
    if enabled {
        packs.push(format!("file/{file}"));
    }
    write_resource_packs(root, &packs)
}

/// Swap two packs in the saved stack, leaving unknown entries where they are.
pub fn swap_packs(root: &Path, first: &str, second: &str) -> Result<()> {
    let mut packs = read_resource_packs(root);
    let index_of = |packs: &[String], name: &str| {
        packs
            .iter()
            .position(|pack| pack_file_name(pack).eq_ignore_ascii_case(name))
    };
    let (Some(left), Some(right)) = (index_of(&packs, first), index_of(&packs, second)) else {
        return Err(Error::Config("the pack is no longer in options.txt".into()));
    };
    packs.swap(left, right);
    write_resource_packs(root, &packs)
}

/// Drop an entry from whichever config still names it.
///
/// A removed file must not stay "enabled" in a file that outlives it, or the
/// game keeps a dangling entry. Only the two config-backed kinds have anything
/// to forget; mods and worlds are their own state.
pub fn forget(root: &Path, kind: crate::core::content::ContentKind, file: &str) {
    use crate::core::content::ContentKind;

    match kind {
        ContentKind::ResourcePack => {
            let mut packs = read_resource_packs(root);
            let before = packs.len();
            packs.retain(|pack| !pack_file_name(pack).eq_ignore_ascii_case(file));
            if packs.len() != before {
                let _ = write_resource_packs(root, &packs);
            }
        }
        ContentKind::Shader => {
            let selected = crate::core::content::shader::selected(root);
            if selected.is_some_and(|current| current.eq_ignore_ascii_case(file)) {
                let loader = crate::core::content::shader::detect(&root.join("mods"));
                let _ = crate::core::content::shader::set_selected(root, loader, "");
            }
        }
        ContentKind::Mod | ContentKind::World | ContentKind::Datapack => {}
    }
}

#[cfg(test)]
mod stack_tests {
    use super::{read_resource_packs, set_pack_enabled, swap_packs, write_resource_packs};
    use std::path::PathBuf;

    fn instance(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tmcl-stack-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create root");
        root
    }

    #[test]
    fn enabling_appends_to_the_top_of_the_stack() {
        let root = instance("enable");
        write_resource_packs(&root, &["file/a.zip".to_owned()]).expect("seed");

        set_pack_enabled(&root, "b.zip", true).expect("enable");

        assert_eq!(
            read_resource_packs(&root),
            ["vanilla", "file/a.zip", "file/b.zip"]
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn enabling_twice_does_not_duplicate_the_entry() {
        let root = instance("twice");
        write_resource_packs(&root, &["file/a.zip".to_owned()]).expect("seed");

        set_pack_enabled(&root, "a.zip", true).expect("enable again");

        assert_eq!(read_resource_packs(&root), ["vanilla", "file/a.zip"]);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn disabling_removes_only_that_pack() {
        let root = instance("disable");
        write_resource_packs(&root, &["file/a.zip".to_owned(), "file/b.zip".to_owned()])
            .expect("seed");

        set_pack_enabled(&root, "a.zip", false).expect("disable");

        assert_eq!(read_resource_packs(&root), ["vanilla", "file/b.zip"]);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn swapping_moves_one_pack_past_its_neighbour() {
        let root = instance("swap");
        write_resource_packs(&root, &["file/a.zip".to_owned(), "file/b.zip".to_owned()])
            .expect("seed");

        swap_packs(&root, "a.zip", "b.zip").expect("swap");

        assert_eq!(
            read_resource_packs(&root),
            ["vanilla", "file/b.zip", "file/a.zip"]
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn swapping_a_pack_that_is_not_enabled_is_an_error() {
        let root = instance("swap-missing");
        write_resource_packs(&root, &["file/a.zip".to_owned()]).expect("seed");

        assert!(swap_packs(&root, "a.zip", "gone.zip").is_err());
        std::fs::remove_dir_all(root).expect("clean");
    }
}

#[cfg(test)]
mod tests {
    use super::{is_disabled, set_enabled};

    fn temp_file(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("tmcl-toggle-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp dir");
        let file = root.join("example.jar");
        std::fs::write(&file, b"x").expect("write file");
        (root, file)
    }

    #[test]
    fn disable_appends_suffix() {
        let (root, file) = temp_file("disable");
        let moved = set_enabled(&file, false).expect("disable");
        assert_eq!(moved.file_name().unwrap(), "example.jar.disabled");
        assert!(!file.exists());
        assert!(is_disabled(&moved));
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn enable_removes_suffix() {
        let (root, file) = temp_file("enable");
        let moved = set_enabled(&file, false).expect("disable");
        let back = set_enabled(&moved, true).expect("enable");
        assert_eq!(back, file);
        assert!(file.exists());
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn toggle_is_idempotent() {
        let (root, file) = temp_file("idempotent");
        let again = set_enabled(&file, true).expect("enable while enabled");
        assert_eq!(again, file);
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    /// A caller still holding the pre-rename name (a list scanned a moment
    /// ago) must land on the file that exists now instead of colliding with
    /// its own move.
    #[test]
    fn a_stale_name_resolves_to_the_file_that_moved() {
        let (root, file) = temp_file("stale");
        let moved = set_enabled(&file, false).expect("disable");
        let again = set_enabled(&file, false).expect("disable again from the old name");
        assert_eq!(again, moved);
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn existing_target_is_rejected() {
        let (root, file) = temp_file("collision");
        std::fs::write(root.join("example.jar.disabled"), b"x").expect("write collision");
        assert!(set_enabled(&file, false).is_err());
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }
}

#[cfg(test)]
mod options_tests {
    use super::{read_resource_packs, write_resource_packs};
    use std::path::{Path, PathBuf};

    fn temp_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("tmcl-options-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn write_options(root: &Path, text: &str) {
        std::fs::write(root.join("options.txt"), text).expect("write options");
    }

    #[test]
    fn options_reads_pack_array() {
        let root = temp_root("read");
        write_options(
            &root,
            "version:3955\nfov:0.5\nresourcePacks:[\"vanilla\",\"file/a.zip\"]\nlang:zh_cn\n",
        );
        assert_eq!(
            read_resource_packs(&root),
            vec!["vanilla".to_owned(), "file/a.zip".to_owned()]
        );
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let root = temp_root("missing");
        assert!(read_resource_packs(&root).is_empty());
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn options_write_preserves_other_keys() {
        let root = temp_root("preserve");
        write_options(
            &root,
            "version:3955\nfov:0.5\nresourcePacks:[\"vanilla\"]\nlang:zh_cn\n",
        );
        write_resource_packs(&root, &["vanilla".to_owned(), "file/b.zip".to_owned()])
            .expect("write packs");
        let text = std::fs::read_to_string(root.join("options.txt")).expect("read options");
        assert!(text.contains("version:3955"), "{text}");
        assert!(text.contains("fov:0.5"), "{text}");
        assert!(text.contains("lang:zh_cn"), "{text}");
        assert!(
            text.contains("resourcePacks:[\"vanilla\",\"file/b.zip\"]"),
            "{text}"
        );
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn options_write_creates_backup() {
        let root = temp_root("backup");
        let original = "fov:0.5\nresourcePacks:[\"vanilla\"]\n";
        write_options(&root, original);
        write_resource_packs(&root, &["vanilla".to_owned()]).expect("write packs");
        let backup = std::fs::read_to_string(root.join("options.txt.bak")).expect("read backup");
        assert_eq!(backup, original, "the backup must be the previous text");
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn options_write_without_file_creates_minimal() {
        let root = temp_root("minimal");
        write_resource_packs(&root, &["file/a.zip".to_owned()]).expect("write packs");
        let text = std::fs::read_to_string(root.join("options.txt")).expect("read options");
        assert_eq!(text, "resourcePacks:[\"vanilla\",\"file/a.zip\"]\n");
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn vanilla_is_never_dropped() {
        let root = temp_root("vanilla");
        write_resource_packs(&root, &["file/a.zip".to_owned()]).expect("write packs");
        assert_eq!(
            read_resource_packs(&root),
            vec!["vanilla".to_owned(), "file/a.zip".to_owned()]
        );
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }
}
