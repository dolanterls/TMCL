use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountKind {
    Offline,
    Microsoft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub kind: AccountKind,
    pub username: String,
    pub uuid: Uuid,
    #[serde(default)]
    pub skin: Option<SkinMetadata>,
    #[serde(default)]
    pub secret_ref: Option<String>,
    #[serde(default)]
    pub access_expires_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinMetadata {
    pub skin_url: Option<String>,
    pub cape_url: Option<String>,
}
