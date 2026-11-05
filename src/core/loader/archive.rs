use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::core::error::{Error, Result};

pub fn read_entry(archive_path: &Path, entry_name: &str) -> Result<Vec<u8>> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| Error::Loader(format!("open installer archive: {error}")))?;
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|error| Error::Loader(format!("installer has no {entry_name}: {error}")))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| Error::Loader(format!("read installer {entry_name}: {error}")))?;
    Ok(bytes)
}

/// Extract files under `prefix/` to a dedicated destination, stripping the prefix.
/// Entry names are validated before any file is created; symlink entries and
/// writes through existing symlink ancestors are rejected.
pub fn extract_prefix(
    archive_path: &Path,
    prefix: &str,
    destination: &Path,
) -> Result<Vec<PathBuf>> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| Error::Loader(format!("open installer archive: {error}")))?;
    fs::create_dir_all(destination)?;
    let canonical_root = destination.canonicalize()?;
    let prefix = format!("{}/", prefix.trim_matches('/'));
    let mut extracted = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| Error::Loader(format!("read installer archive entry: {error}")))?;
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(Error::Loader(format!(
                "unsafe installer archive entry: {:?}",
                entry.name()
            )));
        };
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(Error::Loader(format!(
                "installer archive contains a symbolic link: {:?}",
                entry.name()
            )));
        }
        let Some(relative_name) = enclosed
            .to_str()
            .and_then(|name| name.strip_prefix(&prefix))
        else {
            continue;
        };
        if relative_name.is_empty() {
            continue;
        }
        let relative = Path::new(relative_name);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(Error::Loader(format!(
                "unsafe installer archive path: {:?}",
                entry.name()
            )));
        }

        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)?;
            ensure_within_root(&canonical_root, &output)?;
            continue;
        }
        let parent = output
            .parent()
            .ok_or_else(|| Error::Loader("installer entry has no parent directory".into()))?;
        fs::create_dir_all(parent)?;
        ensure_within_root(&canonical_root, parent)?;
        if fs::symlink_metadata(&output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(Error::Loader(format!(
                "refusing to overwrite a symbolic link: {}",
                output.display()
            )));
        }
        let mut target = File::create(&output)?;
        std::io::copy(&mut entry, &mut target)?;
        target.flush()?;
        extracted.push(output);
    }
    Ok(extracted)
}

pub fn extract_all_except(
    archive_path: &Path,
    destination: &Path,
    excluded_prefixes: &[String],
) -> Result<Vec<PathBuf>> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| Error::Loader(format!("open native library archive: {error}")))?;
    fs::create_dir_all(destination)?;
    let canonical_root = destination.canonicalize()?;
    let mut extracted = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| Error::Loader(format!("read native library entry: {error}")))?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(Error::Loader(format!(
                "unsafe native library entry: {:?}",
                entry.name()
            )));
        };
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(Error::Loader(format!(
                "native library archive contains a symbolic link: {:?}",
                entry.name()
            )));
        }
        let normalized = relative.to_string_lossy().replace('\\', "/");
        if excluded_prefixes
            .iter()
            .any(|prefix| normalized.starts_with(prefix))
        {
            continue;
        }
        if entry.is_dir() {
            continue;
        }
        let output = destination.join(relative);
        let parent = output
            .parent()
            .ok_or_else(|| Error::Loader("native entry has no parent directory".into()))?;
        fs::create_dir_all(parent)?;
        ensure_within_root(&canonical_root, parent)?;
        if fs::symlink_metadata(&output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(Error::Loader(format!(
                "refusing to overwrite a symbolic link: {}",
                output.display()
            )));
        }
        let mut target = File::create(&output)?;
        std::io::copy(&mut entry, &mut target)?;
        target.flush()?;
        extracted.push(output);
    }
    Ok(extracted)
}

fn ensure_within_root(root: &Path, path: &Path) -> Result<()> {
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(Error::Loader(format!(
            "installer archive path escaped extraction root: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::extract_prefix;

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .expect("start archive entry");
            writer.write_all(bytes).expect("write archive entry");
        }
        writer.finish().expect("finish archive").into_inner()
    }

    #[test]
    fn extracts_only_the_requested_prefix_and_rejects_traversal() {
        let root = std::env::temp_dir().join(format!("tmcl-zip-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp root");
        let good_archive = root.join("good.jar");
        std::fs::write(
            &good_archive,
            archive(&[
                ("maven/com/example/lib.jar", b"jar"),
                ("other.txt", b"skip"),
            ]),
        )
        .expect("write archive");
        let extracted = extract_prefix(&good_archive, "maven", &root.join("libraries"))
            .expect("safe extraction");
        assert_eq!(extracted.len(), 1);
        assert_eq!(
            std::fs::read(root.join("libraries/com/example/lib.jar")).unwrap(),
            b"jar"
        );

        let bad_archive = root.join("bad.jar");
        std::fs::write(&bad_archive, archive(&[("maven/../../escape.txt", b"bad")]))
            .expect("write traversal archive");
        assert!(extract_prefix(&bad_archive, "maven", &root.join("bad-libraries")).is_err());
        assert!(!root.parent().unwrap().join("escape.txt").exists());
        std::fs::remove_dir_all(root).expect("clean test root");
    }
}
