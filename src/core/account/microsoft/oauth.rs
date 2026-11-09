use std::future::Future;
use std::time::{Duration, Instant};

use serde::Deserialize;
use uuid::Uuid;

use crate::core::account::microsoft::constants::*;
use crate::core::account::microsoft::models::{
    DeviceCode, EntitlementsResponse, MicrosoftSession, MinecraftLoginResponse, MinecraftProfile,
    OAuthToken, XblProperties, XblRequest, XblResponse, XstsProperties, XstsRequest,
};
use crate::core::error::{Error, Result};
use crate::core::version::http::HttpClient;

pub struct MicrosoftOAuth {
    http: HttpClient,
}

impl MicrosoftOAuth {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn request_device_code(&self) -> Result<DeviceCode> {
        self.http
            .post_form(
                DEVICE_CODE_URL,
                &[("client_id", CLIENT_ID), ("scope", SCOPES)],
            )
            .await
            .map_err(auth_error)
    }

    pub async fn login(&self, device: &DeviceCode) -> Result<MicrosoftSession> {
        let token = poll_device_code(device, || async {
            let response = self
                .http
                .post_form_response(
                    TOKEN_URL,
                    &[
                        ("client_id", CLIENT_ID),
                        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                        ("device_code", device.device_code.as_str()),
                    ],
                )
                .await
                .map_err(auth_error)?;
            parse_device_poll_response(response)
                .await
                .map_err(auth_error)
        })
        .await?;
        self.exchange(token).await
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<OAuthToken> {
        let response = self
            .http
            .post_form_response(
                TOKEN_URL,
                &[
                    ("client_id", CLIENT_ID),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh_token),
                    ("scope", SCOPES),
                ],
            )
            .await?;
        parse_refresh_response(response).await.map_err(auth_error)
    }

    pub async fn refresh_session(&self, refresh_token: &str) -> Result<MicrosoftSession> {
        let token = self.refresh(refresh_token).await?;
        self.exchange(token).await
    }

    async fn exchange(&self, token: OAuthToken) -> Result<MicrosoftSession> {
        let xbl: XblResponse = self
            .http
            .post_json(
                XBL_AUTH_URL,
                &XblRequest {
                    properties: XblProperties {
                        auth_method: "RPS",
                        site_name: "user.auth.xboxlive.com",
                        rps_ticket: format!("d={}", token.access_token),
                    },
                    relying_party: "http://auth.xboxlive.com",
                    token_type: "JWT",
                },
            )
            .await
            .map_err(auth_error)?;
        let user_hash = xbl
            .display_claims
            .xui
            .first()
            .map(|claim| claim.uhs.as_str())
            .ok_or_else(|| Error::Account("Xbox Live response has no user hash".into()))?;
        let xsts: XblResponse = self
            .http
            .post_json(
                XSTS_AUTH_URL,
                &XstsRequest {
                    properties: XstsProperties {
                        sandbox_id: "RETAIL",
                        user_tokens: vec![&xbl.token],
                    },
                    relying_party: "rp://api.minecraftservices.com/",
                    token_type: "JWT",
                },
            )
            .await
            .map_err(auth_error)?;
        let minecraft: MinecraftLoginResponse = self
            .http
            .post_json(
                MINECRAFT_LOGIN_URL,
                &serde_json::json!({
                    "identityToken": format!("XBL3.0 x={user_hash};{}", xsts.token)
                }),
            )
            .await
            .map_err(xbox_login_error)?;
        let entitlements: EntitlementsResponse = self
            .http
            .get_json_with_bearer(ENTITLEMENTS_URL, &minecraft.access_token)
            .await
            .map_err(auth_error)?;
        if entitlements.items.is_empty() {
            return Err(Error::Account("Minecraft entitlement not found".into()));
        }
        let profile: MinecraftProfile = self
            .http
            .get_json_with_bearer(PROFILE_URL, &minecraft.access_token)
            .await
            .map_err(auth_error)?;
        let uuid = Uuid::parse_str(&profile.id)
            .map_err(|_| Error::Account("Minecraft profile has invalid UUID".into()))?;
        Ok(MicrosoftSession {
            uuid,
            username: profile.name,
            access_token: minecraft.access_token,
            refresh_token: token.refresh_token,
            expires_in: minecraft.expires_in,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollResult {
    Pending,
    SlowDown,
    Authorized(OAuthToken),
    Rejected,
    Expired,
}

pub async fn poll_device_code<F, Fut>(device: &DeviceCode, mut request: F) -> Result<OAuthToken>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<PollResult>>,
{
    let deadline = Instant::now() + Duration::from_secs(device.expires_in);
    let mut interval = Duration::from_secs(device.interval.max(1));
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(Error::Account("Microsoft device code expired".into()));
        }
        tokio::time::sleep(interval.min(deadline.duration_since(now))).await;
        if Instant::now() >= deadline {
            return Err(Error::Account("Microsoft device code expired".into()));
        }

        match request().await? {
            PollResult::Authorized(token) => return Ok(token),
            PollResult::Rejected => {
                return Err(Error::Account("Microsoft authorization denied".into()));
            }
            PollResult::Expired => {
                return Err(Error::Account("Microsoft device code expired".into()));
            }
            PollResult::SlowDown => {
                interval += Duration::from_secs(5);
            }
            PollResult::Pending => {}
        }
    }
}

async fn parse_device_poll_response(response: reqwest::Response) -> Result<PollResult> {
    let status = response.status();
    if status.is_success() {
        return response
            .json::<OAuthToken>()
            .await
            .map(PollResult::Authorized)
            .map_err(|error| Error::Network(format!("parse {TOKEN_URL}: {error}")));
    }

    let error = response.json::<OAuthError>().await.map_err(|error| {
        Error::Network(format!(
            "parse Microsoft token response from {TOKEN_URL} (HTTP {status}): {error}"
        ))
    })?;
    match error.error.as_str() {
        "authorization_pending" => Ok(PollResult::Pending),
        "slow_down" => Ok(PollResult::SlowDown),
        "authorization_declined" | "access_denied" => Ok(PollResult::Rejected),
        "expired_token" => Ok(PollResult::Expired),
        "bad_verification_code" => Err(Error::Account(
            "Microsoft rejected the device code; start a new login".into(),
        )),
        code => Err(Error::Account(format!(
            "Microsoft token polling failed: {code} (HTTP {status})"
        ))),
    }
}

/// A rejected refresh token is not a transport problem. `invalid_grant` means the
/// grant was revoked, expired, or issued to a different client, which is what
/// every account saved before the client id changed now holds. "Sign in again" is
/// the only useful answer; retrying cannot succeed.
async fn parse_refresh_response(response: reqwest::Response) -> Result<OAuthToken> {
    let status = response.status();
    if status.is_success() {
        return response
            .json::<OAuthToken>()
            .await
            .map_err(|error| Error::Network(format!("parse {TOKEN_URL}: {error}")));
    }
    let body = response.text().await.unwrap_or_default();
    Err(refresh_error(status, &body))
}

fn refresh_error(status: reqwest::StatusCode, body: &str) -> Error {
    match serde_json::from_str::<OAuthError>(body) {
        Ok(error) if error.error == "invalid_grant" => Error::AccountExpired(format!(
            "Microsoft refresh token was rejected; sign in again{}",
            error.reason()
        )),
        Ok(error) => Error::Network(format!("POST {TOKEN_URL}: {} (HTTP {status})", error.error)),
        Err(_) => Error::Network(format!("POST {TOKEN_URL}: HTTP {status}")),
    }
}

/// Names the step that refused the sign-in instead of pasting a URL.
///
/// The raw text (`POST https://api.minecraftservices.com/authentication/
/// login_with_xbox: HTTP status client error (403 Forbidden)`) reads like a broken
/// account. It says something narrower: Minecraft Services refused
/// `login_with_xbox` with a 403.
///
/// The message names no cause. A 403 here usually means the app registration has
/// not been authorised yet, but that is a state of this build, not a property of
/// the call, and a message that asserts it becomes a lie once the registration is
/// approved.
fn xbox_login_error(error: Error) -> Error {
    if let Error::Network(message) = &error
        && message.contains("403")
    {
        return Error::Account(
            "Minecraft Services refused this sign-in for the app (HTTP 403)".into(),
        );
    }
    auth_error(error)
}

fn auth_error(error: Error) -> Error {
    match error {
        Error::Network(message) => Error::Account(format!(
            "Microsoft authentication request failed: {message}"
        )),
        other => other,
    }
}

#[derive(Debug, Deserialize)]
struct OAuthError {
    error: String,
    /// AAD's human-readable half: `AADSTS70000: The token was issued for a
    /// different client id …`. The code is what makes a rejection diagnosable
    /// after the fact; without it the only evidence is "sign in again".
    #[serde(default)]
    error_description: Option<String>,
}

impl OAuthError {
    /// ` (AADSTS70000: the token was issued for a different client id)`.
    ///
    /// Trimmed at AAD's `Trace ID:` tail, which is noise in a status line.
    fn reason(&self) -> String {
        let Some(description) = self
            .error_description
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        else {
            return String::new();
        };
        let detail = description
            .split(" Trace ID:")
            .next()
            .unwrap_or(description)
            .trim();
        format!(" ({detail})")
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::{PollResult, poll_device_code};
    use crate::core::account::microsoft::models::{DeviceCode, OAuthToken};
    use crate::core::error::Error;
    use crate::core::version::http::HttpClient;

    fn device() -> DeviceCode {
        DeviceCode {
            device_code: "device".into(),
            user_code: "code".into(),
            verification_uri: "https://microsoft.com/devicelogin".into(),
            expires_in: 5,
            interval: 0,
            message: None,
        }
    }

    #[tokio::test]
    async fn pending_polling_reaches_authorized_token() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let token = poll_device_code(&device(), {
            let attempts = attempts.clone();
            move || {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                async move {
                    if attempt == 0 {
                        Ok(PollResult::Pending)
                    } else {
                        Ok(PollResult::Authorized(OAuthToken {
                            access_token: "access".into(),
                            refresh_token: Some("refresh".into()),
                            expires_in: 3600,
                            token_type: "Bearer".into(),
                        }))
                    }
                }
            }
        })
        .await
        .expect("authorized token");
        assert_eq!(token.access_token, "access");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn first_device_code_poll_waits_for_the_server_interval() {
        let device = DeviceCode {
            interval: 1,
            ..device()
        };
        let started = std::time::Instant::now();

        poll_device_code(&device, || async {
            Ok(PollResult::Authorized(OAuthToken {
                access_token: "access".into(),
                refresh_token: Some("refresh".into()),
                expires_in: 3600,
                token_type: "Bearer".into(),
            }))
        })
        .await
        .expect("authorized token");

        assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    }

    /// Read until the client stops sending, so closing the socket afterwards
    /// cannot answer with RST. A single `read` can return before the request
    /// body arrives; the abrupt close then makes the HTTP client report a send
    /// error and retry, which the two-response fixture cannot serve.
    fn drain_request(stream: &mut std::net::TcpStream) {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(20)));
        let mut chunk = [0u8; 1024];
        while matches!(stream.read(&mut chunk), Ok(read) if read > 0) {}
        let _ = stream.set_read_timeout(None);
    }

    #[tokio::test]
    async fn authorization_pending_http_response_continues_polling() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind OAuth fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = thread::spawn(move || {
            for (status, body) in [
                ("400 Bad Request", r#"{"error":"authorization_pending"}"#),
                (
                    "200 OK",
                    r#"{"access_token":"access","refresh_token":"refresh","expires_in":3600,"token_type":"Bearer"}"#,
                ),
            ] {
                let (mut stream, _) = listener.accept().expect("accept request");
                drain_request(&mut stream);
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("write OAuth response");
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);
            }
        });

        let client = HttpClient::new().expect("HTTP client");
        let url = format!("http://{address}/token");
        let token = poll_device_code(&device(), || {
            let client = client.clone();
            let url = url.clone();
            async move {
                let response = client
                    .post_form_response(&url, &[("device_code", "fixture")])
                    .await?;
                super::parse_device_poll_response(response).await
            }
        })
        .await
        .expect("polling should continue until authorized");

        assert_eq!(token.access_token, "access");
        server.join().expect("OAuth fixture thread");
    }

    #[tokio::test]
    async fn rejection_is_an_auth_error_without_token_text() {
        let error = poll_device_code(&device(), || async { Ok(PollResult::Rejected) })
            .await
            .expect_err("rejected login");
        assert!(matches!(error, Error::Account(ref message) if message.contains("denied")));
        assert!(!error.to_string().contains("refresh"));
    }

    #[test]
    fn network_auth_errors_keep_http_status_context() {
        let error = super::auth_error(Error::Network(
            "GET https://api.minecraftservices.com/minecraft/profile: 401 Unauthorized".into(),
        ));

        assert!(error.to_string().contains("401 Unauthorized"));
    }

    #[test]
    fn a_rejected_refresh_token_asks_for_a_new_sign_in() {
        let error = super::refresh_error(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":"invalid_grant","error_description":"AADSTS70000: the grant is expired"}"#,
        );

        assert!(
            matches!(error, Error::AccountExpired(ref message) if message.contains("sign in again")),
            "invalid_grant must be its own kind, not a retryable network error: {error}"
        );
        assert!(!error.is_retryable(), "retrying the same token cannot work");
    }

    /// The reason travels with the rejection.
    ///
    /// AAD knows why: expired, revoked, or issued for another client id, which is
    /// what the client-id switch produces. The status line is the only place a
    /// player would see it, and dropping it leaves "sign in again" as the whole
    /// diagnosis.
    #[test]
    fn a_rejected_refresh_token_keeps_the_aad_reason() {
        let error = super::refresh_error(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":"invalid_grant","error_description":"AADSTS70000: The token was issued for a different client id Trace ID: abc Correlation ID: def"}"#,
        );

        let Error::AccountExpired(message) = error else {
            panic!("invalid_grant must be `AccountExpired`");
        };
        assert!(
            message.contains("AADSTS70000") && message.contains("different client id"),
            "{message}"
        );
        // AAD's tracing tail is noise in a status line.
        assert!(!message.contains("Trace ID"), "{message}");
    }

    /// A 403 names the step, and refuses to guess at the reason.
    ///
    /// `login_with_xbox` is the call. The raw URL reads like a broken account; the
    /// cause, usually an unapproved app registration, is a state of the build, not
    /// of the call, so the message must not assert it. The same sentence has to
    /// stay true after the registration is approved.
    #[test]
    fn a_403_on_login_with_xbox_names_the_step_without_a_verdict() {
        let error = super::xbox_login_error(Error::Network(
            "POST https://api.minecraftservices.com/authentication/login_with_xbox: \
             HTTP status client error (403 Forbidden)"
                .into(),
        ));

        let Error::Account(message) = error else {
            panic!("a refused sign-in must be an account error");
        };
        assert!(message.contains("403"), "{message}");
        assert!(
            !message.contains("login_with_xbox"),
            "the URL is not the diagnosis: {message}"
        );
        // No claim about *why*: an approval, a region block and a revoked app
        // all answer 403, and the message outlives all of them.
        assert!(
            !message.contains("AppID") && !message.contains("review"),
            "the message must not pin a temporary cause: {message}"
        );
    }

    /// Every other failure on that call keeps the generic wording, so the
    /// network case is not silently relabelled as a review problem.
    #[test]
    fn other_login_with_xbox_failures_keep_the_generic_wording() {
        let error = super::xbox_login_error(Error::Network("connection reset by peer".into()));

        assert!(
            error
                .to_string()
                .contains("Microsoft authentication request failed"),
            "{error}"
        );
    }

    #[test]
    fn refresh_errors_keep_the_status_when_the_body_is_not_a_token_error() {
        let error = super::refresh_error(reqwest::StatusCode::BAD_GATEWAY, "<html>502</html>");

        assert!(error.to_string().contains("502"));
        assert!(matches!(error, Error::Network(_)));
    }
}
