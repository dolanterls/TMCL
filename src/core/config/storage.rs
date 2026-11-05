use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::core::error::Result;

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary = temporary_path(path);
    let result = (|| {
        let mut file = File::create(&temporary)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_all()?;
        replace_path(&temporary, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
}

/// Write text through a temporary file, so a crash never leaves a half-written
/// file where the game expects a complete one (a truncated `options.txt` makes
/// Minecraft reset every setting it holds).
pub fn write_text_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary = temporary_path(path);
    let result = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        replace_path(&temporary, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
}

pub fn backup_file(path: &Path) -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let backup = path.with_file_name(format!(
        "{}.{}.bak",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.json"),
        timestamp
    ));
    fs::copy(path, &backup)?;
    Ok(backup)
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.json");
    path.with_file_name(format!(".{name}.tmp-{}", Uuid::new_v4()))
}

#[cfg(windows)]
pub fn replace_path(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MoveFileExW};
    use windows::core::PCWSTR;

    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING,
        )
    }
    .map_err(io::Error::other)
}

#[cfg(not(windows))]
pub fn replace_path(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{backup_file, read_json, write_json_atomic};
    use crate::core::config::models::Config;

    fn temp_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "tmcl-storage-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ))
    }

    #[test]
    fn writes_reads_replaces_and_backups_json() {
        let root = temp_path();
        let path = root.join("config.json");
        let mut config = Config::default();
        config.game_directories.push("C:/Games/.minecraft".into());

        write_json_atomic(&path, &config).expect("write config");
        let loaded: Config = read_json(&path).expect("read config");
        assert_eq!(loaded, config);

        config.java.auto_download = true;
        let backup = backup_file(&path).expect("backup config");
        write_json_atomic(&path, &config).expect("replace config");
        assert!(backup.is_file());
        assert_eq!(
            read_json::<Config>(&path).expect("read replacement"),
            config
        );
        assert_eq!(std::fs::read_dir(&root).expect("list temp tree").count(), 2);

        std::fs::remove_dir_all(root).expect("clean test tree");
    }
}
