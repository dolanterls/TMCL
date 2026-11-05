use std::fs::File;
use std::path::Path;

use crate::core::download::hash::verify_sha1;
use crate::core::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityKind {
    File,
    ZipJar,
    JavaRaw,
    JavaCompressed,
}

pub fn verify_download(
    path: &Path,
    expected_sha1: Option<&str>,
    expected_size: Option<u64>,
    integrity: IntegrityKind,
) -> Result<()> {
    if let Some(expected_size) = expected_size {
        let actual = std::fs::metadata(path)?.len();
        if actual != expected_size {
            return Err(Error::Download(format!(
                "size mismatch for {}: expected {expected_size}, got {actual}",
                path.display()
            )));
        }
    }
    verify_sha1(path, expected_sha1)?;
    if matches!(integrity, IntegrityKind::ZipJar) {
        let file = File::open(path)?;
        let archive = zip::ZipArchive::new(file).map_err(|error| {
            Error::Download(format!("invalid ZIP/JAR {}: {error}", path.display()))
        })?;
        let _ = archive.len();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{IntegrityKind, verify_download};

    #[test]
    fn verifies_size_and_zip_structure() {
        let root = std::env::temp_dir().join(format!("tmcl-verify-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create root");
        let path = root.join("bad.jar");
        std::fs::write(&path, b"not a zip").expect("write fixture");
        assert!(verify_download(&path, None, Some(9), IntegrityKind::File).is_ok());
        assert!(verify_download(&path, None, None, IntegrityKind::ZipJar).is_err());
        std::fs::remove_dir_all(root).expect("clean root");
    }
}
