pub const CLIENT_ID: &str = "424cf174-6aee-43ca-af92-50ecdb79e1b9";
pub const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
pub const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
pub const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
pub const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
pub const MINECRAFT_LOGIN_URL: &str =
    "https://api.minecraftservices.com/authentication/login_with_xbox";
pub const ENTITLEMENTS_URL: &str = "https://api.minecraftservices.com/entitlements/mcstore";
pub const PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

pub const SCOPES: &str = "XboxLive.signin offline_access";

#[cfg(test)]
mod tests {
    use super::CLIENT_ID;

    /// Pinned literally on purpose. This GUID is the app's public identity, so
    /// the test has to fail when it changes; comparing the constant with
    /// itself would pass whatever value ships, which is how the launcher once
    /// ended up signing in as another project's registration.
    #[test]
    fn uses_the_registered_microsoft_device_flow_client_id() {
        assert_eq!(CLIENT_ID, "424cf174-6aee-43ca-af92-50ecdb79e1b9");
    }

    #[test]
    fn client_id_is_a_guid() {
        assert_eq!(CLIENT_ID.len(), 36);
        assert!(uuid::Uuid::parse_str(CLIENT_ID).is_ok());
    }
}
