use uuid::Uuid;

use crate::core::account::models::{Account, AccountKind};
use crate::core::error::{Error, Result};

pub fn validate_username(username: &str) -> Result<()> {
    if !(3..=16).contains(&username.len())
        || !username
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(Error::Account(format!(
            "invalid offline username: {username:?}"
        )));
    }
    Ok(())
}

pub fn offline_uuid(username: &str) -> Result<Uuid> {
    validate_username(username)?;
    Ok(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("OfflinePlayer:{username}").as_bytes(),
    ))
}

pub fn create_account(username: &str, uuid: Option<&str>) -> Result<Account> {
    let player_uuid = match uuid {
        Some(uuid) => Uuid::parse_str(uuid)
            .map_err(|_| Error::Account(format!("invalid offline UUID: {uuid}")))?,
        None => offline_uuid(username)?,
    };
    Ok(Account {
        id: Uuid::new_v4(),
        kind: AccountKind::Offline,
        username: username.into(),
        uuid: player_uuid,
        skin: None,
        secret_ref: None,
        access_expires_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{create_account, offline_uuid};

    #[test]
    fn derives_stable_uuid_and_preserves_valid_custom_uuid() {
        assert_eq!(
            offline_uuid("Alex").expect("uuid"),
            uuid::Uuid::parse_str("a41c02b5-b608-5a06-838d-03cddd5e76c7")
                .expect("standard offline UUID")
        );
        let account = create_account("Alex", Some("00000000-0000-0000-0000-000000000001"))
            .expect("custom UUID");
        assert_eq!(
            account.uuid.to_string(),
            "00000000-0000-0000-0000-000000000001"
        );
    }

    #[test]
    fn rejects_invalid_username_and_uuid() {
        assert!(create_account("bad name", None).is_err());
        assert!(create_account("Alex", Some("not-a-uuid")).is_err());
    }
}
