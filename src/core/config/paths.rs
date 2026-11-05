use std::env;
use std::path::{Path, PathBuf};

use crate::core::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    pub root: PathBuf,
    pub config_file: PathBuf,
    pub accounts_file: PathBuf,
    pub cache_dir: PathBuf,
    pub tasks_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub launcher_logs_dir: PathBuf,
    pub game_logs_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

impl DataPaths {
    pub fn from_appdata(appdata: impl AsRef<Path>) -> Self {
        let root = appdata.as_ref().join("TMCL");
        Self {
            config_file: root.join("config.json"),
            accounts_file: root.join("accounts.json"),
            cache_dir: root.join("cache"),
            tasks_dir: root.join("tasks"),
            logs_dir: root.join("logs"),
            launcher_logs_dir: root.join("logs").join("launcher"),
            game_logs_dir: root.join("logs").join("game"),
            runtime_dir: root.join("runtime"),
            root,
        }
    }

    pub fn discover() -> Result<Self> {
        let appdata =
            env::var_os("APPDATA").ok_or_else(|| Error::Config("APPDATA is not set".into()))?;
        Ok(Self::from_appdata(appdata))
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for path in [
            &self.root,
            &self.cache_dir,
            &self.tasks_dir,
            &self.logs_dir,
            &self.launcher_logs_dir,
            &self.game_logs_dir,
            &self.runtime_dir,
        ] {
            std::fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::DataPaths;

    fn temp_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "tmcl-path-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ))
    }

    #[test]
    fn builds_and_creates_expected_data_tree() {
        let appdata = temp_path();
        let paths = DataPaths::from_appdata(&appdata);
        paths.ensure_dirs().expect("create data tree");

        assert_eq!(paths.root, appdata.join("TMCL"));
        assert!(paths.config_file.ends_with("TMCL\\config.json"));
        assert!(paths.game_logs_dir.is_dir());
        assert!(paths.runtime_dir.is_dir());

        std::fs::remove_dir_all(appdata).expect("clean test tree");
    }
}
