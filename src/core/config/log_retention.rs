use std::path::Path;
use std::time::SystemTime;

use crate::core::error::{Error, Result};

pub fn cleanup_logs(logs_root: &Path, older_than: SystemTime) -> Result<usize> {
    let root = logs_root.canonicalize()?;
    let mut removed = 0;
    for directory in ["launcher", "game"] {
        let directory = root.join(directory);
        if !directory.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || !path.starts_with(&root) {
                continue;
            }
            if entry.metadata()?.modified().unwrap_or(SystemTime::now()) < older_than {
                std::fs::remove_file(path).map_err(|error| Error::Config(error.to_string()))?;
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::cleanup_logs;

    #[test]
    fn removes_only_logs_under_the_owned_root() {
        let root =
            std::env::temp_dir().join(format!("tmcl-log-retention-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("launcher")).expect("launcher directory");
        std::fs::create_dir_all(root.join("game")).expect("game directory");
        std::fs::write(root.join("launcher/old.log"), b"old").expect("old log");
        std::fs::write(root.join("game/new.log"), b"new").expect("new log");
        let threshold = SystemTime::now() + Duration::from_secs(1);
        assert_eq!(cleanup_logs(&root, threshold).expect("cleanup"), 2);
        assert!(!root.join("launcher/old.log").exists());
        assert!(!root.join("game/new.log").exists());
        std::fs::remove_dir_all(root).expect("clean logs");
    }
}
