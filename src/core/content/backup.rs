//! World backups: zip in, zip out, nothing hidden.
//!
//! A world is a directory of regions, dat files and player data, so copying it can
//! take seconds and hundreds of megabytes and every entry point here runs off the
//! UI thread. The archive lives inside the instance (`.tmcl-backups/`), so moving
//! or exporting the instance carries its backups, and a restore is always preceded
//! by a safety backup.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::core::error::{Error, Result};

/// Where backups live, relative to the instance directory.
pub const BACKUP_DIR: &str = ".tmcl-backups";
/// How many backups a single world keeps before the oldest is dropped.
const KEEP_PER_WORLD: usize = 5;
/// Guard against a symlink loop while walking a world directory.
const MAX_DEPTH: u8 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupEntry {
    pub path: PathBuf,
    /// `yyyyMMdd-HHmmss` taken from the file name.
    pub stamp: String,
    pub size: u64,
}

/// Pack `world` into `<instance>/.tmcl-backups/<world>-<stamp>.zip`.
///
/// Entries are stored relative to the world directory, so the archive root *is*
/// the world: restoring it does not depend on where it is unpacked.
pub fn backup_world(instance_root: &Path, world: &Path) -> Result<PathBuf> {
    let name = file_name(world)?;
    let directory = instance_root.join(BACKUP_DIR);
    fs::create_dir_all(&directory)?;
    // Two backups of the same world inside one second would otherwise write to the
    // same file, which is exactly what a restore does (safety copy, then unpack).
    let stamp = crate::core::content::trash::now_stamp();
    let mut path = directory.join(format!("{name}-{stamp}.zip"));
    let mut index = 2;
    while path.exists() {
        path = directory.join(format!("{name}-{stamp}-{index}.zip"));
        index += 1;
    }
    write_archive(world, &path)?;
    prune(&directory, name);
    Ok(path)
}

/// Backups of one world, newest first.
pub fn list_backups(instance_root: &Path, world_name: &str) -> Vec<BackupEntry> {
    let directory = instance_root.join(BACKUP_DIR);
    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };
    let prefix = format!("{world_name}-");
    let mut items = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        })
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_owned();
            let stamp = name.strip_prefix(&prefix)?.strip_suffix(".zip")?.to_owned();
            let size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            Some(BackupEntry { path, stamp, size })
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| b.stamp.cmp(&a.stamp));
    items
}

/// Replace `world` with the contents of `archive`, returning the safety backup.
///
/// The current world is backed up first, the archive is unpacked beside it, and
/// only then is the old directory swapped out; if unpacking fails the world is
/// untouched.
pub fn restore_world(instance_root: &Path, world: &Path, archive: &Path) -> Result<PathBuf> {
    let name = file_name(world)?;
    let safety = backup_world(instance_root, world)?;
    let parent = world
        .parent()
        .ok_or_else(|| Error::Config("world directory has no parent".into()))?;
    let stamp = crate::core::content::trash::now_stamp();
    let staging = parent.join(format!(".{name}.restore-{stamp}"));
    if let Err(error) = extract(archive, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let displaced = parent.join(format!(".{name}.old-{stamp}"));
    fs::rename(world, &displaced)?;
    match fs::rename(&staging, world) {
        Ok(()) => {
            let _ = fs::remove_dir_all(&displaced);
            Ok(safety)
        }
        Err(error) => {
            // Put the original back; the caller keeps its world either way.
            let _ = fs::rename(&displaced, world);
            let _ = fs::remove_dir_all(&staging);
            Err(error.into())
        }
    }
}

/// Copy `world` to a sibling named `<name><suffix>`, numbering clashes.
pub fn clone_world(world: &Path, suffix: &str) -> Result<PathBuf> {
    let name = file_name(world)?;
    let parent = world
        .parent()
        .ok_or_else(|| Error::Config("world directory has no parent".into()))?;
    let base = format!("{name}{suffix}");
    let mut target = parent.join(&base);
    let mut index = 2;
    while target.exists() {
        target = parent.join(format!("{base}-{index}"));
        index += 1;
    }
    copy_directory(world, &target, 0)?;
    Ok(target)
}

fn file_name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Config(format!("invalid world path: {}", path.display())))
}

fn write_archive(source: &Path, target: &Path) -> Result<()> {
    let file = File::create(target)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    add_tree(&mut zip, source, source, 0, options)?;
    zip.finish().map_err(io::Error::other)?;
    Ok(())
}

fn add_tree(
    zip: &mut zip::ZipWriter<File>,
    source: &Path,
    current: &Path,
    depth: u8,
    options: zip::write::SimpleFileOptions,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    for entry in fs::read_dir(current)?.flatten() {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(source) else {
            continue;
        };
        let name = relative.to_string_lossy().replace('\\', "/");
        if name.is_empty() {
            continue;
        }
        if path.is_dir() {
            // Directories are recreated by the extractor from the file paths;
            // storing them as entries keeps empty folders intact.
            zip.add_directory(format!("{name}/"), options)
                .map_err(io::Error::other)?;
            add_tree(zip, source, &path, depth + 1, options)?;
        } else if path.is_file() {
            zip.start_file(name, options).map_err(io::Error::other)?;
            let mut file = File::open(&path)?;
            io::copy(&mut file, zip)?;
        }
    }
    Ok(())
}

/// Unpack `archive` into `target`, refusing any entry that escapes it.
fn extract(archive: &Path, target: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(io::Error::other)?;
    fs::create_dir_all(target)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(io::Error::other)?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(Error::Config(format!(
                "backup entry escapes the world directory: {}",
                entry.name()
            )));
        };
        let destination = target.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&destination)?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&destination)?;
        io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

fn copy_directory(source: &Path, target: &Path, depth: u8) -> Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)?.flatten() {
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_directory(&from, &to, depth + 1)?;
        } else if from.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn prune(directory: &Path, world_name: &str) {
    let prefix = format!("{world_name}-");
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut backups = entries
        .flatten()
        .map(|entry| entry.path())
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_owned();
            let rest = name.strip_prefix(&prefix)?.strip_suffix(".zip")?;
            Some((rest.to_owned(), path))
        })
        .collect::<Vec<_>>();
    backups.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in backups.into_iter().skip(KEEP_PER_WORLD) {
        let _ = fs::remove_file(path);
    }
}

/// Write a file into a zip for tests: `name` may contain `/`, and the caller
/// decides the contents.
#[cfg(test)]
fn read_zip_names(path: &Path) -> Vec<String> {
    let file = File::open(path).expect("open zip");
    let mut zip = zip::ZipArchive::new(file).expect("read zip");
    (0..zip.len())
        .map(|index| zip.by_index(index).expect("entry").name().to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        BACKUP_DIR, backup_world, clone_world, list_backups, read_zip_names, restore_world,
    };
    use std::io::Write;
    use std::path::{Path, PathBuf};

    struct TempRoot(PathBuf);

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn instance(tag: &str) -> (TempRoot, PathBuf) {
        let root = TempRoot(
            std::env::temp_dir().join(format!("tmcl-backup-{tag}-{}", uuid::Uuid::new_v4())),
        );
        let saves = root.0.join("saves");
        std::fs::create_dir_all(&saves).expect("create saves");
        (root, saves)
    }

    fn world(saves: &Path, name: &str, level_name: &str) -> PathBuf {
        let directory = saves.join(name);
        std::fs::create_dir_all(directory.join("region")).expect("create world");
        std::fs::write(directory.join("level.dat"), level_name.as_bytes())
            .expect("write level.dat");
        std::fs::write(directory.join("region/r.0.0.mca"), b"region").expect("write region");
        directory
    }

    #[test]
    fn backup_writes_zip_inside_instance() {
        let (root, saves) = instance("write");
        let directory = world(&saves, "castle", "Castle");
        let archive = backup_world(&root.0, &directory).expect("backup");

        assert!(
            archive.starts_with(root.0.join(BACKUP_DIR)),
            "{}",
            archive.display()
        );
        assert!(archive.is_file());
        let names = read_zip_names(&archive);
        assert!(names.contains(&"level.dat".to_owned()), "{names:?}");
        assert!(names.contains(&"region/r.0.0.mca".to_owned()), "{names:?}");
        assert!(
            names.iter().all(|name| !name.contains("castle")),
            "entries are relative to the world: {names:?}"
        );
    }

    #[test]
    fn backup_keeps_only_five_latest() {
        let (root, saves) = instance("prune");
        let directory = world(&saves, "castle", "Castle");
        let backups = root.0.join(BACKUP_DIR);
        std::fs::create_dir_all(&backups).expect("create backup dir");
        // Six older archives the prune has to outrank.
        for hour in 0..6 {
            std::fs::write(
                backups.join(format!("castle-2026010{hour}-000000.zip")),
                b"old",
            )
            .expect("write old backup");
        }
        backup_world(&root.0, &directory).expect("backup");
        let listed = list_backups(&root.0, "castle");
        assert_eq!(listed.len(), 5, "{listed:?}");
        assert!(
            listed[0].stamp.as_str() > "20260105-000000",
            "the newest backup survives: {listed:?}"
        );
    }

    #[test]
    fn restore_world_replaces_content_and_keeps_a_backup() {
        let (root, saves) = instance("restore");
        let directory = world(&saves, "castle", "Castle");
        let archive = backup_world(&root.0, &directory).expect("backup");

        // Corrupt the live world after the backup was taken.
        std::fs::write(directory.join("level.dat"), b"broken").expect("break world");
        std::fs::write(directory.join("junk.txt"), b"junk").expect("add junk");

        let safety = restore_world(&root.0, &directory, &archive).expect("restore");
        assert!(safety.is_file(), "the pre-restore backup must exist");
        assert_eq!(
            std::fs::read(directory.join("level.dat")).expect("read level.dat"),
            b"Castle"
        );
        assert!(
            !directory.join("junk.txt").exists(),
            "restore replaces the directory instead of merging"
        );
        // Nothing left behind beside the world.
        let leftovers = std::fs::read_dir(&saves)
            .expect("read saves")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with('.'))
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn restore_rejects_path_traversal() {
        let (root, saves) = instance("escape");
        let directory = world(&saves, "castle", "Castle");
        let archive = root.0.join(BACKUP_DIR).join("evil.zip");
        std::fs::create_dir_all(root.0.join(BACKUP_DIR)).expect("create backup dir");
        let file = std::fs::File::create(&archive).expect("create archive");
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file(
            "../escape.txt",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .expect("start entry");
        zip.write_all(b"no").expect("write entry");
        zip.finish().expect("finish");

        assert!(
            restore_world(&root.0, &directory, &archive).is_err(),
            "an entry that escapes the world directory must abort the restore"
        );
        assert_eq!(
            std::fs::read(directory.join("level.dat")).expect("world untouched"),
            b"Castle"
        );
        assert!(!saves.join("escape.txt").exists());
    }

    #[test]
    fn clone_world_creates_unique_copy() {
        let (root, saves) = instance("clone");
        let directory = world(&saves, "castle", "Castle");
        let first = clone_world(&directory, " - copy").expect("clone");
        assert_eq!(first.file_name().unwrap(), "castle - copy");
        assert!(first.join("region/r.0.0.mca").is_file());

        let second = clone_world(&directory, " - copy").expect("clone again");
        assert_eq!(second.file_name().unwrap(), "castle - copy-2");
        // The original is untouched by either copy.
        assert_eq!(
            std::fs::read(directory.join("level.dat")).expect("read original"),
            b"Castle"
        );
        drop(root);
    }
}
