use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::core::error::{Error, Result};
use crate::platform::windows;

pub struct SecretStore {
    root: PathBuf,
}

impl SecretStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn save(&self, account_id: Uuid, secret: &[u8]) -> Result<String> {
        std::fs::create_dir_all(&self.root)?;
        let key = account_id.to_string();
        let path = self.path_for(&key)?;
        std::fs::write(path, windows::protect(secret)?)?;
        Ok(key)
    }

    pub fn load(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.path_for(key)?;
        windows::unprotect(&std::fs::read(path)?)
    }

    /// Store a second secret for one account, under its own slot.
    ///
    /// The refresh token keeps the unnamed slot so existing accounts keep
    /// working; the Minecraft access token lives in `minecraft`, and losing it
    /// only costs a refresh.
    pub fn save_slot(&self, account_id: Uuid, slot: &str, secret: &[u8]) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::write(self.slot_path(account_id, slot)?, windows::protect(secret)?)?;
        Ok(())
    }

    pub fn load_slot(&self, account_id: Uuid, slot: &str) -> Result<Vec<u8>> {
        let path = self.slot_path(account_id, slot)?;
        windows::unprotect(&std::fs::read(path)?)
    }

    /// Drop a slot; a missing file is not an error, the caller is discarding it.
    pub fn clear_slot(&self, account_id: Uuid, slot: &str) {
        if let Ok(path) = self.slot_path(account_id, slot) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Drop the unnamed secret (the refresh token) of a removed account.
    pub fn clear(&self, key: &str) {
        if let Ok(path) = self.path_for(key) {
            let _ = std::fs::remove_file(path);
        }
    }

    fn path_for(&self, key: &str) -> Result<PathBuf> {
        let id = Uuid::parse_str(key)
            .map_err(|_| Error::Account("secret key must be an account UUID".into()))?;
        Ok(self.root.join(format!("{id}.bin")))
    }

    fn slot_path(&self, account_id: Uuid, slot: &str) -> Result<PathBuf> {
        if slot.is_empty()
            || !slot
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return Err(Error::Account(format!("invalid secret slot: {slot}")));
        }
        Ok(self.root.join(format!("{account_id}.{slot}.bin")))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::SecretStore;
    use uuid::Uuid;

    #[cfg(windows)]
    #[test]
    fn round_trips_secret_through_dpapi() {
        let root = std::env::temp_dir().join(format!(
            "tmcl-secret-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let store = SecretStore::new(&root);
        let id = Uuid::new_v4();
        let key = store.save(id, b"refresh-token").expect("save secret");

        assert_eq!(store.load(&key).expect("load secret"), b"refresh-token");
        assert!(
            !std::fs::read_dir(&root)
                .expect("list secrets")
                .next()
                .expect("secret file")
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .contains("refresh-token")
        );
        std::fs::remove_dir_all(root).expect("clean secrets");
    }

    #[test]
    fn rejects_non_uuid_secret_keys() {
        let store = SecretStore::new("C:/tmcl-secrets");
        assert!(store.load("../../token").is_err());
        let _ = (SystemTime::now(), UNIX_EPOCH, Uuid::nil());
    }
}
