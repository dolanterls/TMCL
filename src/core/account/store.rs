use std::path::{Path, PathBuf};

use crate::core::account::models::Account;
use crate::core::config::storage::{read_json, write_json_atomic};
use crate::core::error::Result;

#[derive(Debug, Clone)]
pub struct AccountStore {
    path: PathBuf,
}

impl AccountStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn load(&self) -> Result<Vec<Account>> {
        if self.path.is_file() {
            read_json(&self.path)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn save(&self, accounts: &[Account]) -> Result<()> {
        write_json_atomic(&self.path, &accounts)
    }

    pub fn add(&self, account: Account) -> Result<()> {
        let mut accounts = self.load()?;
        accounts.push(account);
        self.save(&accounts)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::AccountStore;
    use crate::core::account::offline::create_account;

    #[test]
    fn account_json_contains_no_token_plaintext() {
        let root = std::env::temp_dir().join(format!(
            "tmcl-account-store-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let store = AccountStore::new(root.join("accounts.json"));
        store
            .add(create_account("Alex", None).expect("offline account"))
            .expect("save account");
        let raw = std::fs::read_to_string(root.join("accounts.json")).expect("account JSON");
        assert!(!raw.contains("refresh_token"));
        assert!(!raw.contains("access_token"));
        assert_eq!(store.load().expect("load accounts").len(), 1);
        std::fs::remove_dir_all(root).expect("clean account store");
    }
}
