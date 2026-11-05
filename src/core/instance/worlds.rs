//! The worlds an instance owns, for the instance page's world panel.
//!
//! Deliberately not the workspace scanner: that one walks each world's files to
//! report a size and parses every `level.dat`. This runs while drawing, so it
//! stats one file per world and nothing else.
//!
//! The listed name is the folder name. Minecraft renames the folder when a world
//! is renamed in game, so the two agree in practice; reading `level.dat` for a
//! display name is the workspace's job, not a summary panel's.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// One row of the instance page's world panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldSummary {
    pub name: String,
    /// `level.dat`'s modification time — the closest thing to "last played"
    /// without opening the world.
    pub modified: SystemTime,
}

impl WorldSummary {
    /// Seconds since the epoch, for the age formatter.
    pub fn modified_seconds(&self) -> u64 {
        self.modified
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or(0)
    }
}

/// Worlds under `<instance>/saves`, newest first.
///
/// A folder without `level.dat` is not a world the game can load, so it is not
/// listed: a stray directory in `saves/` must not invent one.
pub fn list(instance: &Path) -> Vec<WorldSummary> {
    let Ok(entries) = std::fs::read_dir(instance.join("saves")) else {
        return Vec::new();
    };
    let mut worlds: Vec<(SystemTime, String)> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let path = entry.path();
            // `level.dat` is the marker and the clock in one stat.
            let modified = std::fs::metadata(path.join("level.dat"))
                .ok()?
                .modified()
                .ok()?;
            let name = path.file_name()?.to_str()?.to_owned();
            Some((modified, name))
        })
        .collect();
    // Newest first. The name breaks ties so two worlds written in the same
    // filesystem tick cannot swap places between frames.
    worlds.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    worlds
        .into_iter()
        .map(|(modified, name)| WorldSummary { name, modified })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    use super::list;

    fn temp_instance(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("tmcl-worlds-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create instance");
        path
    }

    /// Writes `saves/<name>/level.dat` and stamps it, so order is deterministic.
    fn world(instance: &Path, name: &str, seconds: u64) {
        let dir = instance.join("saves").join(name);
        std::fs::create_dir_all(&dir).expect("create world");
        let level = dir.join("level.dat");
        std::fs::write(&level, b"level").expect("write level");
        let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
        std::fs::File::options()
            .write(true)
            .open(&level)
            .expect("open level")
            .set_modified(stamp)
            .expect("set mtime");
    }

    #[test]
    fn worlds_are_listed_newest_first() {
        let root = temp_instance("order");
        world(&root, "Castle", 1_700_000_100);
        world(&root, "Skyblock", 1_700_000_300);
        world(&root, "Old", 1_700_000_000);

        let names = list(&root)
            .into_iter()
            .map(|world| world.name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["Skyblock", "Castle", "Old"]);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_folder_without_level_dat_is_not_a_world() {
        let root = temp_instance("no-level");
        world(&root, "Castle", 1_700_000_000);
        std::fs::create_dir_all(root.join("saves/not-a-world")).expect("stray dir");
        std::fs::write(root.join("saves/notes.txt"), b"x").expect("stray file");

        let names = list(&root)
            .into_iter()
            .map(|world| world.name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["Castle"]);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn an_instance_without_saves_lists_nothing() {
        let root = temp_instance("empty");
        assert!(list(&root).is_empty());
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn equal_timestamps_are_broken_by_name_so_the_order_is_stable() {
        let root = temp_instance("tie");
        world(&root, "Beta", 1_700_000_000);
        world(&root, "Alpha", 1_700_000_000);

        let names = list(&root)
            .into_iter()
            .map(|world| world.name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["Alpha", "Beta"]);
        std::fs::remove_dir_all(root).expect("clean");
    }
}
