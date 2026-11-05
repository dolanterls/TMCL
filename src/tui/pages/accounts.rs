use uuid::Uuid;

use crate::core::account::models::Account;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCodeView {
    pub user_code: String,
    pub verification_uri: String,
    pub remaining_seconds: u64,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct AccountPage {
    pub accounts: Vec<Account>,
    pub selected: usize,
    pub current: Option<Uuid>,
    pub device_code: Option<DeviceCodeView>,
}

impl AccountPage {
    pub fn select_next(&mut self) {
        if !self.accounts.is_empty() {
            self.selected = (self.selected + 1).min(self.accounts.len() - 1);
        }
    }

    pub fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_id(&self) -> Option<Uuid> {
        self.accounts.get(self.selected).map(|account| account.id)
    }

    pub fn choose_selected(&mut self) -> Option<Uuid> {
        let id = self.selected_id()?;
        self.current = Some(id);
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::AccountPage;
    use crate::core::account::offline::create_account;

    #[test]
    fn selecting_account_never_exposes_secret_fields() {
        let account = create_account("Alex", None).expect("account");
        let id = account.id;
        let mut page = AccountPage {
            accounts: vec![account],
            ..AccountPage::default()
        };
        assert_eq!(page.choose_selected(), Some(id));
        assert!(page.device_code.is_none());
    }
}
