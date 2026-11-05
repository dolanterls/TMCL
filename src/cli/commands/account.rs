use uuid::Uuid;

use crate::cli::args::AccountCommand;
use crate::cli::output::OutputOptions;
use crate::core::account::models::{Account, AccountKind};
use crate::core::account::offline::create_account;
use crate::core::account::secrets::SecretStore;
use crate::core::account::store::AccountStore;
use crate::core::config::paths::DataPaths;
use crate::core::error::{Error, Result};
use crate::core::version::http::HttpClient;

use super::common::current_unix_seconds;

pub(super) async fn run(command: AccountCommand, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    paths.ensure_dirs()?;
    let store = AccountStore::new(&paths.accounts_file);
    match command {
        AccountCommand::List => {
            let accounts = store.load()?;
            output.print(&accounts, || {
                accounts
                    .iter()
                    .map(|account| {
                        format!("{}  {:?}  {}", account.id, account.kind, account.username)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
        AccountCommand::AddOffline(args) => {
            let account = create_account(&args.username, args.uuid.as_deref())?;
            store.add(account.clone())?;
            output.print(&account, || {
                format!("Added offline account {}", account.username)
            })
        }
        AccountCommand::Remove(args) => {
            let mut accounts = store.load()?;
            let before = accounts.len();
            accounts.retain(|account| account.id != args.id);
            if accounts.len() == before {
                return Err(Error::Account(format!("account not found: {}", args.id)));
            }
            store.save(&accounts)?;
            output.print(&serde_json::json!({"id": args.id}), || {
                format!("Removed account {}", args.id)
            })
        }
        AccountCommand::LoginMicrosoft => {
            let http = HttpClient::new()?;
            let oauth = crate::core::account::microsoft::oauth::MicrosoftOAuth::new(http);
            let device = oauth.request_device_code().await?;
            println!(
                "Open {} and enter code {}.",
                device.verification_uri, device.user_code
            );
            let session = oauth.login(&device).await?;
            let id = Uuid::new_v4();
            let secrets = SecretStore::new(paths.root.join("secrets"));
            crate::core::account::microsoft::session::remember(&secrets, id, &session);
            let account = Account {
                id,
                kind: AccountKind::Microsoft,
                username: session.username,
                uuid: session.uuid,
                skin: None,
                secret_ref: None,
                access_expires_at: Some(current_unix_seconds() + session.expires_in),
            };
            if let Some(refresh_token) = session.refresh_token {
                let key = secrets.save(account.id, refresh_token.as_bytes())?;
                let mut account = account;
                account.secret_ref = Some(key);
                store.add(account.clone())?;
                output.print(&account, || format!("Logged in as {}", account.username))
            } else {
                Err(Error::Account(
                    "Microsoft login returned no refresh token".into(),
                ))
            }
        }
    }
}
