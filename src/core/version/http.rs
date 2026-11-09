use std::time::Duration;

use reqwest::{
    Client, Response,
    header::{ACCEPT_ENCODING, IF_RANGE, RANGE},
};
use serde::{Serialize, de::DeserializeOwned};

use crate::core::download::DownloadAuthentication;
use crate::core::error::{Error, Result};

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    download_client: Client,
    download_header_timeout: Duration,
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        Self::build(Duration::from_secs(60), Duration::from_secs(60))
    }

    pub fn with_timeout(timeout_secs: u64) -> Result<Self> {
        let timeout = Duration::from_secs(timeout_secs.max(1));
        Self::build(timeout, timeout)
    }

    #[cfg(test)]
    pub(crate) fn new_with_timeouts(
        request_timeout: Duration,
        download_header_timeout: Duration,
    ) -> Result<Self> {
        Self::build(request_timeout, download_header_timeout)
    }

    fn build(request_timeout: Duration, download_header_timeout: Duration) -> Result<Self> {
        let user_agent = concat!("tmcl/", env!("CARGO_PKG_VERSION"));
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(request_timeout)
            .user_agent(user_agent)
            .build()
            .map_err(|error| Error::Network(format!("create HTTP client: {error}")))?;
        let download_client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent(user_agent)
            .build()
            .map_err(|error| Error::Network(format!("create download HTTP client: {error}")))?;
        Ok(Self {
            client,
            download_client,
            download_header_timeout,
        })
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let response = self.get_response_with_headers(url, &[]).await?;
        response
            .json()
            .await
            .map_err(|error| Error::Network(format!("parse {url}: {error}")))
    }

    pub async fn get_json_with_headers<T: DeserializeOwned>(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<T> {
        let response = self.get_response_with_headers(url, headers).await?;
        response
            .json()
            .await
            .map_err(|error| Error::Network(format!("parse {url}: {error}")))
    }

    pub async fn get_json_with_bearer<T: DeserializeOwned>(
        &self,
        url: &str,
        access_token: &str,
    ) -> Result<T> {
        let response = self
            .send_with_retry(|| self.client.get(url).bearer_auth(access_token))
            .await
            .map_err(|error| contextual(error, &format!("GET {url}")))?
            .error_for_status()
            .map_err(|error| Error::Network(format!("GET {url}: {}", describe(&error))))?;
        response
            .json()
            .await
            .map_err(|error| Error::Network(format!("parse {url}: {error}")))
    }

    pub async fn get_response(&self, url: &str) -> Result<reqwest::Response> {
        self.get_response_with_headers(url, &[]).await
    }

    pub async fn get_response_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<reqwest::Response> {
        crate::core::diagnostics::diagnostic!("http", "GET {url}");
        let response = self
            .send_with_retry(|| {
                let mut request = self.client.get(url);
                for (name, value) in headers {
                    request = request.header(*name, *value);
                }
                request
            })
            .await
            .map_err(|error| contextual(error, &format!("GET {url}")))?;
        crate::core::diagnostics::diagnostic!(
            "http",
            "GET {url} -> {}",
            response.status().as_u16()
        );
        response
            .error_for_status()
            .map_err(|error| match error.status() {
                Some(status) => Error::HttpStatus {
                    status: status.as_u16(),
                    request: format!("GET {url}"),
                },
                None => Error::Network(format!("GET {url}: {}", describe(&error))),
            })
    }

    pub(crate) async fn get_download_response(
        &self,
        url: &str,
        offset: u64,
        if_range: Option<&str>,
        authentication: DownloadAuthentication,
    ) -> Result<Response> {
        crate::core::diagnostics::diagnostic!("http", "GET {url} (download, offset {offset})");
        let mut request = self.download_client.get(url);
        request = request.header(ACCEPT_ENCODING, "identity");
        if authentication == DownloadAuthentication::CurseForge {
            let key = option_env!("TMCL_CURSEFORGE_API_KEY")
                .filter(|key| !key.trim().is_empty())
                .ok_or_else(|| Error::Download("CurseForge API Key is not configured".into()))?;
            request = request.header("x-api-key", key);
        }
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
            if let Some(if_range) = if_range {
                request = request.header(IF_RANGE, if_range);
            }
        }
        tokio::time::timeout(self.download_header_timeout, request.send())
            .await
            .map_err(|_| {
                Error::Network(format!("GET {url}: timed out waiting for response headers"))
            })?
            .map_err(|error| Error::Network(format!("GET {url}: {error}")))
    }

    pub async fn post_form<T: DeserializeOwned>(
        &self,
        url: &str,
        form: &[(&str, &str)],
    ) -> Result<T> {
        let response = self
            .post_form_response(url, form)
            .await?
            .error_for_status()
            .map_err(|error| Error::Network(format!("POST {url}: {}", describe(&error))))?;
        response
            .json()
            .await
            .map_err(|error| Error::Network(format!("parse {url}: {error}")))
    }

    pub async fn post_form_response(
        &self,
        url: &str,
        form: &[(&str, &str)],
    ) -> Result<reqwest::Response> {
        self.send_with_retry(|| self.client.post(url).form(form))
            .await
            .map_err(|error| contextual(error, &format!("POST {url}")))
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        let response = self
            .send_with_retry(|| self.client.post(url).json(body))
            .await
            .map_err(|error| contextual(error, &format!("POST {url}")))?
            .error_for_status()
            .map_err(|error| Error::Network(format!("POST {url}: {}", describe(&error))))?;
        response
            .json()
            .await
            .map_err(|error| Error::Network(format!("parse {url}: {error}")))
    }

    /// One request, tried again while the failure looks like a dropped
    /// connection.
    ///
    /// A reset from Xbox Live, common from mainland networks, kills a launch and
    /// is indistinguishable from "the launcher is broken" when the error text stops
    /// at "error sending request". Auth bodies are small and side-effect free, so
    /// re-sending one is safe.
    async fn send_with_retry<F>(&self, mut build: F) -> Result<reqwest::Response>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempt = 1;
        loop {
            match build().send().await {
                Ok(response) => return Ok(response),
                Err(error) if attempt < SEND_ATTEMPTS && transient(&error) => {
                    tokio::time::sleep(SEND_BACKOFF * attempt).await;
                    attempt += 1;
                }
                Err(error) => return Err(Error::Network(describe(&error))),
            }
        }
    }
}

/// Attempts per request, including the first.
const SEND_ATTEMPTS: u32 = 3;
/// Wait before attempt 2, then twice that before attempt 3.
const SEND_BACKOFF: Duration = Duration::from_millis(400);

/// A failure that is worth repeating: connect/TLS/timeout and transport errors.
///
/// `is_request` also covers a request that never left because the body could
/// not be serialised, but every caller passes a value this module owns.
fn transient(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request() || error.is_body()
}

/// The whole error chain.
///
/// `reqwest::Error`'s `Display` stops at "error sending request", which hides the
/// part that says what happened (DNS failure, TLS rejection, connection reset).
/// Joining the sources keeps those in the status line and the launcher log.
fn describe(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !text.contains(&cause_text) {
            text.push_str(": ");
            text.push_str(&cause_text);
        }
        source = cause.source();
    }
    text
}

/// Prefixes a network failure with the request it belonged to.
fn contextual(error: Error, request: &str) -> Error {
    match error {
        Error::Network(message) => Error::Network(format!("{request}: {message}")),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::HttpClient;
    use crate::core::error::Error;

    async fn get_status_error(status: &'static str) -> Error {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept HTTP request");
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let read = stream.read(&mut buffer).expect("read HTTP request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .expect("write HTTP response");
        });

        let error = HttpClient::new()
            .expect("HTTP client")
            .get_response(&format!("http://{address}/status"))
            .await
            .expect_err("status should be returned as an error");
        server.join().expect("HTTP fixture thread");
        error
    }

    #[tokio::test]
    async fn get_response_preserves_http_status_retry_semantics() {
        let bad_request = get_status_error("400 Bad Request").await;
        assert!(matches!(bad_request, Error::HttpStatus { status: 400, .. }));
        assert!(!bad_request.is_retryable());

        let unavailable = get_status_error("503 Service Unavailable").await;
        assert!(matches!(unavailable, Error::HttpStatus { status: 503, .. }));
        assert!(unavailable.is_retryable());
    }

    /// Reads one request off the wire so closing afterwards cannot answer with
    /// RST instead of the empty reply this test wants.
    fn read_request(stream: &mut std::net::TcpStream) {
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            let read = stream.read(&mut buffer).expect("read HTTP request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
    }

    /// Xbox Live drops connections from time to time, more so from mainland
    /// networks. A launch that dies on the first drop never learns that a second
    /// attempt would have answered.
    #[tokio::test]
    async fn a_dropped_connection_is_retried() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().expect("accept dropped request");
            read_request(&mut first);
            drop(first);
            let (mut second, _) = listener.accept().expect("accept retried request");
            read_request(&mut second);
            write!(
                second,
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
            )
            .expect("write retried response");
            let _ = second.flush();
        });

        let response = HttpClient::new()
            .expect("HTTP client")
            .get_response(&format!("http://{address}/retry"))
            .await
            .expect("the second attempt should succeed");
        assert_eq!(response.text().await.expect("body"), "ok");
        server.join().expect("HTTP fixture thread");
    }

    #[tokio::test]
    async fn minecraft_account_endpoints_require_a_bearer_token() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept HTTP request");
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let read = stream.read(&mut buffer).expect("read HTTP request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let authenticated = String::from_utf8_lossy(&request)
                .lines()
                .any(|line| line.eq_ignore_ascii_case("authorization: Bearer test-access-token"));
            let (status, body) = if authenticated {
                ("200 OK", r#"{"ok":true}"#)
            } else {
                ("401 Unauthorized", r#"{"error":"missing bearer token"}"#)
            };
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("write HTTP response");
            authenticated
        });

        let response = HttpClient::new()
            .expect("HTTP client")
            .get_json_with_bearer::<serde_json::Value>(
                &format!("http://{address}/minecraft/profile"),
                "test-access-token",
            )
            .await;

        assert!(response.is_ok(), "endpoint rejected request: {response:?}");
        assert!(server.join().expect("HTTP fixture thread"));
    }
}
