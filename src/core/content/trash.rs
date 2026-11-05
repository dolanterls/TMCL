//! Instance-local trash.
//!
//! Deleting never destroys content outright: the entry is moved under
//! `<instance>/.tmcl-trash/<stamp>/` and can be restored or purged from the
//! workspace. A JSON manifest per stamp records where each entry came from, so
//! restoring does not depend on guessing from the file name.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};

pub const TRASH: &str = ".tmcl-trash";
const MANIFEST: &str = ".manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashRecord {
    /// Path relative to the instance directory, using `/` separators.
    pub original: String,
    /// Path relative to the stamp directory.
    pub stored: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashEntry {
    pub stamp: String,
    pub original: PathBuf,
    pub stored: PathBuf,
}

/// Move `path` into the instance trash and return the stored entry.
pub fn move_to_trash(instance_root: &Path, path: &Path) -> Result<TrashEntry> {
    let original = relative(instance_root, path)?;
    let stored = original.clone();
    let trash = instance_root.join(TRASH);
    let stamp = next_stamp(&trash);
    let stamp_dir = trash.join(&stamp);
    let destination = stamp_dir.join(&stored);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(path, &destination)?;
    let mut records = read_manifest(&stamp_dir);
    records.push(TrashRecord {
        original: original.clone(),
        stored: stored.clone(),
    });
    write_manifest(&stamp_dir, &records)?;
    Ok(TrashEntry {
        stamp,
        original: instance_root.join(original),
        stored: destination,
    })
}

/// Every restorable entry, newest stamp first.
pub fn list(instance_root: &Path) -> Vec<TrashEntry> {
    let trash = instance_root.join(TRASH);
    let Ok(entries) = std::fs::read_dir(&trash) else {
        return Vec::new();
    };
    let mut stamps = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect::<Vec<_>>();
    stamps.sort();
    stamps.reverse();
    stamps
        .into_iter()
        .flat_map(|stamp| {
            let stamp_dir = trash.join(&stamp);
            read_manifest(&stamp_dir)
                .into_iter()
                .map(move |record| TrashEntry {
                    stamp: stamp.clone(),
                    original: instance_root.join(&record.original),
                    stored: stamp_dir.join(&record.stored),
                })
        })
        .collect()
}

/// Move an entry back. An existing file keeps its place and the restored copy
/// gets a ` (n)` suffix instead of overwriting either one.
pub fn restore(instance_root: &Path, entry: &TrashEntry) -> Result<PathBuf> {
    if !entry.stored.exists() {
        return Err(Error::Config(format!(
            "trash entry is missing: {}",
            entry.stored.display()
        )));
    }
    let target = unique_path(&entry.original);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&entry.stored, &target)?;

    let stamp_dir = instance_root.join(TRASH).join(&entry.stamp);
    let stored = relative(&stamp_dir, &entry.stored).unwrap_or_default();
    let mut records = read_manifest(&stamp_dir);
    records.retain(|record| record.stored != stored);
    if records.is_empty() {
        let _ = std::fs::remove_dir_all(&stamp_dir);
    } else {
        write_manifest(&stamp_dir, &records)?;
    }
    Ok(target)
}

/// Remove every stamp directory.
pub fn purge(instance_root: &Path) -> Result<()> {
    let trash = instance_root.join(TRASH);
    if trash.exists() {
        std::fs::remove_dir_all(trash)?;
    }
    Ok(())
}

pub fn total_size(instance_root: &Path) -> u64 {
    directory_size(&instance_root.join(TRASH), 0)
}

fn relative(root: &Path, path: &Path) -> Result<String> {
    let stripped = path.strip_prefix(root).map_err(|_| {
        Error::Config(format!(
            "{} is not inside {}",
            path.display(),
            root.display()
        ))
    })?;
    Ok(stripped
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/"))
}

/// `yyyyMMdd-HHmmss`, disambiguated with `-2`, `-3` when two entries land in
/// the same second.
/// The current local timestamp, in the same shape as a trash directory.
pub fn now_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    stamp(seconds)
}

fn next_stamp(trash: &Path) -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let base = stamp(seconds);
    if !trash.join(&base).exists() {
        return base;
    }
    let mut index = 2;
    loop {
        let candidate = format!("{base}-{index}");
        if !trash.join(&candidate).exists() {
            return candidate;
        }
        index += 1;
    }
}

/// Timestamp used in directory and file names: `yyyyMMdd-HHmmss`, local time.
pub fn stamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let time = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        time / 3_600,
        (time % 3_600) / 60,
        time % 60
    )
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn read_manifest(stamp_dir: &Path) -> Vec<TrashRecord> {
    std::fs::read(stamp_dir.join(MANIFEST))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_manifest(stamp_dir: &Path, records: &[TrashRecord]) -> Result<()> {
    std::fs::create_dir_all(stamp_dir)?;
    let bytes = serde_json::to_vec_pretty(records)?;
    std::fs::write(stamp_dir.join(MANIFEST), bytes)?;
    Ok(())
}

fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("entry");
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut index = 1;
    loop {
        let candidate = parent.join(format!("{stem} ({index}){extension}"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

fn directory_size(path: &Path, depth: u8) -> u64 {
    if depth > 12 {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let Ok(metadata) = entry.metadata() else {
                return 0;
            };
            if metadata.is_dir() {
                directory_size(&entry.path(), depth + 1)
            } else {
                metadata.len()
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{TRASH, list, move_to_trash, purge, restore, total_size};

    fn instance(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("tmcl-trash-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("mods")).expect("create mods");
        root
    }

    #[test]
    fn move_to_trash_and_list() {
        let root = instance("move");
        let file = root.join("mods/a.jar");
        std::fs::write(&file, b"payload").expect("write mod");

        let entry = move_to_trash(&root, &file).expect("move to trash");
        assert!(!file.exists());
        assert!(entry.stored.is_file());
        assert!(entry.stamp.starts_with("20"));
        assert!(entry.stored.starts_with(root.join(TRASH)));

        let entries = list(&root);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].original, file);
        assert!(total_size(&root) > 0);
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn restore_returns_file_to_original_place() {
        let root = instance("restore");
        let file = root.join("mods/a.jar");
        std::fs::write(&file, b"payload").expect("write mod");

        let entry = move_to_trash(&root, &file).expect("move to trash");
        let restored = restore(&root, &entry).expect("restore");
        assert_eq!(restored, file);
        assert_eq!(std::fs::read(&file).expect("read restored"), b"payload");
        assert!(list(&root).is_empty());
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn restore_does_not_overwrite_existing_file() {
        let root = instance("collision");
        let file = root.join("mods/a.jar");
        std::fs::write(&file, b"first").expect("write mod");

        let entry = move_to_trash(&root, &file).expect("move to trash");
        std::fs::write(&file, b"second").expect("write new file");
        let restored = restore(&root, &entry).expect("restore");
        assert_ne!(restored, file);
        assert_eq!(std::fs::read(&file).expect("read kept"), b"second");
        assert_eq!(std::fs::read(&restored).expect("read restored"), b"first");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn same_second_entries_use_separate_stamps() {
        let root = instance("stamps");
        let first = root.join("mods/a.jar");
        let second = root.join("mods/b.jar");
        std::fs::write(&first, b"a").expect("write a");
        std::fs::write(&second, b"b").expect("write b");
        let one = move_to_trash(&root, &first).expect("move a");
        let two = move_to_trash(&root, &second).expect("move b");
        // Same-second collisions must not overwrite each other.
        assert!(one.stamp != two.stamp || one.stored != two.stored);
        assert_eq!(list(&root).len(), 2);
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn purge_removes_everything() {
        let root = instance("purge");
        let file = root.join("mods/a.jar");
        std::fs::write(&file, b"a").expect("write a");
        move_to_trash(&root, &file).expect("move");
        purge(&root).expect("purge");
        assert!(!root.join(TRASH).exists());
        std::fs::remove_dir_all(root).expect("clean instance");
    }
}
