use crate::core::account::models::{Account, AccountKind};
use crate::core::error::{Error, Result};

pub fn offline_arguments(account: &Account) -> Result<Vec<String>> {
    if account.kind != AccountKind::Offline {
        return Err(Error::Account(
            "offline launch arguments require an offline account".into(),
        ));
    }
    Ok(account_arguments(account, "0"))
}

pub fn account_arguments(account: &Account, access_token: &str) -> Vec<String> {
    let user_type = match account.kind {
        AccountKind::Offline => "legacy",
        AccountKind::Microsoft => "msa",
    };
    vec![
        "--username".into(),
        account.username.clone(),
        "--uuid".into(),
        account.uuid.simple().to_string(),
        "--accessToken".into(),
        access_token.into(),
        "--userType".into(),
        user_type.into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{account_arguments, offline_arguments};
    use crate::core::account::models::AccountKind;
    use crate::core::account::offline::create_account;

    #[test]
    fn basic_offline_arguments_contain_no_refresh_token() {
        let account = create_account("Alex", None).expect("offline account");
        let arguments = offline_arguments(&account).expect("launch arguments");
        assert!(arguments.contains(&"--username".into()));
        assert!(
            !arguments
                .iter()
                .any(|argument| argument.contains("refresh"))
        );
    }

    #[test]
    fn microsoft_arguments_use_minecraft_token_and_msa_user_type() {
        let mut account = create_account("Alex", None).expect("account");
        account.kind = AccountKind::Microsoft;
        let arguments = account_arguments(&account, "minecraft-token");

        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--accessToken", "minecraft-token"])
        );
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--userType", "msa"])
        );
    }
}
