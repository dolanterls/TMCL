//! Reusing the Minecraft access token between launches.
//!
//! Every Microsoft launch used to run the whole chain: OAuth refresh → Xbox
//! Live → XSTS → `login_with_xbox` → entitlements → profile. That is six
//! requests per launch, and `login_with_xbox` is rate limited — a handful of
//! relaunches in a row is enough to earn HTTP 429 (it is in the launcher log),
//! and any transient failure anywhere in the chain aborts the launch even
//! though the token from the previous launch was still valid.
//!
//! The token lives for about a day, so keep it in the secret store and reuse
//! it until it is nearly expired. Only an expiring token pays for the chain.

use crate::core::account::models::Account;
use crate::core::account::secrets::SecretStore;
use crate::core::error::{Error, Result};

use super::models::MicrosoftSession;
use super::oauth::MicrosoftOAuth;

/// Secret-store slot holding the Minecraft access token.
const TOKEN_SLOT: &str = "minecraft";

/// Stop reusing a token this close to its expiry, so a launch cannot start the
/// game with a token that dies while it is still loading.
const EXPIRY_MARGIN_SECS: u64 = 300;

/// Where the token for one launch came from.
pub enum LaunchToken {
    /// Kept from an earlier launch; no request was made.
    Cached(String),
    /// Fresh from the authentication chain; the caller persists the account.
    Refreshed(Box<MicrosoftSession>),
}

impl LaunchToken {
    pub fn access_token(&self) -> &str {
        match self {
            Self::Cached(token) => token,
            Self::Refreshed(session) => &session.access_token,
        }
    }
}

/// The access token to start a launch with.
///
/// A cached token short-circuits the whole chain. Otherwise the refresh token
/// is exchanged, the new access token is cached, and the caller gets the
/// session back to persist the rotated refresh token.
pub async fn launch_token(
    oauth: &MicrosoftOAuth,
    store: &SecretStore,
    account: &Account,
    now: u64,
) -> Result<LaunchToken> {
    if let Some(token) = cached(store, account, now) {
        return Ok(LaunchToken::Cached(token));
    }
    let refresh_token = refresh_token(store, account)?;
    let session = oauth.refresh_session(&refresh_token).await?;
    // Checked before the cache write: a token for another profile must never
    // land under this account's slot.
    if session.uuid != account.uuid {
        return Err(Error::Account(
            "refreshed Microsoft profile does not match the selected account; sign in again".into(),
        ));
    }
    // A cache write must never fail a launch: the worst case is one more chain
    // next time.
    let _ = store.save_slot(account.id, TOKEN_SLOT, session.access_token.as_bytes());
    Ok(LaunchToken::Refreshed(Box::new(session)))
}

/// Forget the cached token, e.g. after the user signs out.
pub fn clear_cached(store: &SecretStore, account: &Account) {
    store.clear_slot(account.id, TOKEN_SLOT);
}

/// Seed the cache from a fresh device-code login, so the first launch after
/// signing in does not pay for the chain either.
pub fn remember(store: &SecretStore, account_id: uuid::Uuid, session: &MicrosoftSession) {
    let _ = store.save_slot(account_id, TOKEN_SLOT, session.access_token.as_bytes());
}

fn cached(store: &SecretStore, account: &Account, now: u64) -> Option<String> {
    let expires_at = account.access_expires_at?;
    if expires_at <= now.saturating_add(EXPIRY_MARGIN_SECS) {
        return None;
    }
    let bytes = store.load_slot(account.id, TOKEN_SLOT).ok()?;
    let token = String::from_utf8(bytes).ok()?;
    if token.trim().is_empty() {
        return None;
    }
    Some(token)
}

fn refresh_token(store: &SecretStore, account: &Account) -> Result<String> {
    let secret_ref = account.secret_ref.as_deref().ok_or_else(|| {
        Error::Account("Microsoft account has no refresh token; sign in again".into())
    })?;
    let bytes = store
        .load(secret_ref)
        .map_err(|_| Error::Account("cannot read Microsoft refresh token; sign in again".into()))?;
    String::from_utf8(bytes)
        .map_err(|_| Error::Account("stored Microsoft refresh token is invalid".into()))
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{EXPIRY_MARGIN_SECS, cached, clear_cached};
    use crate::core::account::models::{Account, AccountKind};
    use crate::core::account::secrets::SecretStore;

    fn account(expires_at: Option<u64>) -> Account {
        Account {
            id: Uuid::new_v4(),
            kind: AccountKind::Microsoft,
            username: "Alex".into(),
            uuid: Uuid::new_v4(),
            skin: None,
            secret_ref: None,
            access_expires_at: expires_at,
        }
    }

    fn store() -> (SecretStore, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("tmcl-token-cache-{}", Uuid::new_v4()));
        (SecretStore::new(&root), root)
    }

    #[test]
    fn a_valid_token_is_reused_without_any_request() {
        let (store, root) = store();
        let account = account(Some(10_000));
        store
            .save_slot(account.id, "minecraft", b"access")
            .expect("cache token");

        assert_eq!(cached(&store, &account, 9_000).as_deref(), Some("access"));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_token_inside_the_margin_is_not_reused() {
        let (store, root) = store();
        let account = account(Some(10_000));
        store
            .save_slot(account.id, "minecraft", b"access")
            .expect("cache token");

        // Too close to expiry to survive a launch.
        assert!(cached(&store, &account, 10_000 - EXPIRY_MARGIN_SECS).is_none());
        // And long expired, obviously.
        assert!(cached(&store, &account, 20_000).is_none());

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn no_expiry_means_no_cache() {
        let (store, root) = store();
        let account = account(None);
        store
            .save_slot(account.id, "minecraft", b"access")
            .expect("cache token");

        assert!(cached(&store, &account, 0).is_none());

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn clearing_the_cache_forces_the_next_chain() {
        let (store, root) = store();
        let account = account(Some(10_000));
        store
            .save_slot(account.id, "minecraft", b"access")
            .expect("cache token");

        clear_cached(&store, &account);

        assert!(cached(&store, &account, 9_000).is_none());
        std::fs::remove_dir_all(root).ok();
    }
}
