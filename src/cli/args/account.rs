use clap::{Args, Subcommand};
use uuid::Uuid;

#[derive(Debug, Subcommand)]
pub enum AccountCommand {
    /// List saved accounts
    List,
    /// Sign in with a Microsoft account
    LoginMicrosoft,
    /// Add an offline account
    AddOffline(AccountOfflineArgs),
    /// Remove a saved account
    Remove(AccountRemoveArgs),
}

#[derive(Debug, Args)]
pub struct AccountOfflineArgs {
    #[arg(value_name = "USERNAME")]
    pub username: String,
    #[arg(long, value_name = "UUID")]
    pub uuid: Option<String>,
}

#[derive(Debug, Args)]
pub struct AccountRemoveArgs {
    #[arg(value_name = "ACCOUNT_ID")]
    pub id: Uuid,
}
