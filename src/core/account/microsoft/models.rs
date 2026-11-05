use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct OAuthToken {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    #[serde(default)]
    pub token_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrosoftSession {
    pub uuid: Uuid,
    pub username: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct XblResponse {
    #[serde(rename = "Token")]
    pub token: String,
    #[serde(rename = "DisplayClaims")]
    pub display_claims: DisplayClaims,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DisplayClaims {
    pub xui: Vec<XuiClaim>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct XuiClaim {
    pub uhs: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct XblRequest<'a> {
    #[serde(rename = "Properties")]
    pub properties: XblProperties<'a>,
    #[serde(rename = "RelyingParty")]
    pub relying_party: &'a str,
    #[serde(rename = "TokenType")]
    pub token_type: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct XblProperties<'a> {
    #[serde(rename = "AuthMethod")]
    pub auth_method: &'a str,
    #[serde(rename = "SiteName")]
    pub site_name: &'a str,
    #[serde(rename = "RpsTicket")]
    pub rps_ticket: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct XstsRequest<'a> {
    #[serde(rename = "Properties")]
    pub properties: XstsProperties<'a>,
    #[serde(rename = "RelyingParty")]
    pub relying_party: &'a str,
    #[serde(rename = "TokenType")]
    pub token_type: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct XstsProperties<'a> {
    #[serde(rename = "SandboxId")]
    pub sandbox_id: &'a str,
    #[serde(rename = "UserTokens")]
    pub user_tokens: Vec<&'a str>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MinecraftLoginResponse {
    pub access_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EntitlementsResponse {
    #[serde(default)]
    pub items: Vec<Entitlement>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entitlement {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MinecraftProfile {
    pub id: String,
    pub name: String,
}

fn default_interval() -> u64 {
    5
}
