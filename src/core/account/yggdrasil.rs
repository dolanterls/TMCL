use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::core::account::skin::SkinStore;
use crate::core::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct PlayerProfile {
    pub uuid: Uuid,
    pub name: String,
    pub properties: Vec<ProfileProperty>,
}

#[derive(Clone, Default)]
pub struct YggdrasilRegistry {
    players: Arc<RwLock<HashMap<Uuid, PlayerProfile>>>,
}

impl YggdrasilRegistry {
    pub fn register(&self, profile: PlayerProfile) {
        self.players
            .write()
            .expect("Yggdrasil registry lock")
            .insert(profile.uuid, profile);
    }

    pub fn profile(&self, uuid: Uuid) -> Option<PlayerProfile> {
        self.players
            .read()
            .expect("Yggdrasil registry lock")
            .get(&uuid)
            .cloned()
    }

    pub fn has_joined(&self, username: &str) -> Option<PlayerProfile> {
        self.players
            .read()
            .expect("Yggdrasil registry lock")
            .values()
            .find(|profile| profile.name == username)
            .cloned()
    }
}

pub struct YggdrasilServer {
    pub endpoint: String,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl YggdrasilServer {
    pub async fn bind(registry: YggdrasilRegistry, skins: SkinStore) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|error| Error::Account(format!("bind local Yggdrasil: {error}")))?;
        let address = listener
            .local_addr()
            .map_err(|error| Error::Account(format!("read Yggdrasil address: {error}")))?;
        let (shutdown, shutdown_signal) = oneshot::channel();
        let task = tokio::spawn(serve(listener, registry, skins, shutdown_signal));
        Ok(Self {
            endpoint: format!("http://{address}"),
            shutdown: Some(shutdown),
            task,
        })
    }

    pub async fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let _ = self.task.await;
    }
}

async fn serve(
    listener: TcpListener,
    registry: YggdrasilRegistry,
    skins: SkinStore,
    mut shutdown: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { continue };
                let registry = registry.clone();
                let skins = skins.clone();
                tokio::spawn(async move {
                    let _ = handle(stream, registry, skins).await;
                });
            }
        }
    }
}

async fn handle(
    mut stream: TcpStream,
    registry: YggdrasilRegistry,
    skins: SkinStore,
) -> Result<()> {
    const MAX_REQUEST_BYTES: usize = 1024 * 1024;
    let mut request = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];
    let (header_end, content_length) = loop {
        if let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if content_length > MAX_REQUEST_BYTES
                || request.len() + content_length > MAX_REQUEST_BYTES
            {
                return write_response(
                    &mut stream,
                    "413 Payload Too Large",
                    "application/json",
                    b"{}".to_vec(),
                )
                .await;
            }
            if request.len() >= header_end + 4 + content_length {
                break (header_end, content_length);
            }
        }
        let size = stream
            .read(&mut chunk)
            .await
            .map_err(|error| Error::Account(format!("read Yggdrasil request: {error}")))?;
        if size == 0 {
            return Err(Error::Account(
                "Yggdrasil request ended before headers were complete".into(),
            ));
        }
        request.extend_from_slice(&chunk[..size]);
        if request.len() > MAX_REQUEST_BYTES {
            return write_response(
                &mut stream,
                "413 Payload Too Large",
                "application/json",
                b"{}".to_vec(),
            )
            .await;
        }
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let mut request_line = headers
        .lines()
        .next()
        .unwrap_or("GET / HTTP/1.1")
        .split_whitespace();
    let method = request_line.next().unwrap_or("GET");
    let path = request_line.next().unwrap_or("/");
    let body_start = header_end + 4;
    let body = &request[body_start..body_start + content_length];
    let (status, content_type, body) = route(method, path, body, &registry, &skins);
    write_response(&mut stream, status, &content_type, body).await
}

async fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: Vec<u8>,
) -> Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&body).await?;
    Ok(())
}

fn route(
    method: &str,
    path: &str,
    body: &[u8],
    registry: &YggdrasilRegistry,
    skins: &SkinStore,
) -> (&'static str, String, Vec<u8>) {
    if path == "/" || path == "/status" {
        return json_response(
            200,
            serde_json::json!({
                "meta": {
                    "serverName":"TMCL Local Offline Service",
                    "implementationName":"TMCL",
                    "implementationVersion":env!("CARGO_PKG_VERSION")
                },
                "skinDomains": []
            }),
        );
    }
    if method == "POST"
        && matches!(
            path,
            "/authserver/validate"
                | "/authserver/invalidate"
                | "/authserver/signout"
                | "/sessionserver/session/minecraft/join"
        )
    {
        return ("204 No Content", "application/json".into(), Vec::new());
    }
    if method == "POST" && path == "/authserver/authenticate" {
        let username = serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .get("username")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            });
        return username
            .and_then(|name| registry.has_joined(&name))
            .map_or_else(
                || json_response(403, serde_json::json!({"error":"Forbidden","errorMessage":"Unknown local profile"})),
                |profile| json_response(200, serde_json::json!({
                    "accessToken":"0",
                    "clientToken":"tmcl-local",
                    "selectedProfile": profile_json(&profile),
                    "availableProfiles": [profile_json(&profile)]
                })),
            );
    }
    let path_without_query = path.split('?').next().unwrap_or(path);
    if let Some(uuid) = path_without_query
        .strip_prefix("/sessionserver/session/minecraft/profile/")
        .map(|value| value.split('/').next().unwrap_or(value))
        .and_then(|id| Uuid::parse_str(id).ok())
    {
        return registry.profile(uuid).map_or_else(
            || json_response(404, serde_json::json!({"error":"Not Found"})),
            |profile| json_response(200, profile_json(&profile)),
        );
    }
    if let Some(query) = path.strip_prefix("/sessionserver/session/minecraft/hasJoined?") {
        if let Some(username) = query
            .split('&')
            .find_map(|part| part.strip_prefix("username="))
        {
            return registry.has_joined(username).map_or_else(
                || ("204 No Content", "application/json".into(), Vec::new()),
                |profile| json_response(200, profile_json(&profile)),
            );
        }
    }
    if let Some(query) = path.strip_prefix("/sessionserver/session/minecraft/hasJoined/") {
        let username = query.split('/').next().unwrap_or_default();
        return registry.has_joined(username).map_or_else(
            || ("204 No Content", "application/json".into(), Vec::new()),
            |profile| json_response(200, profile_json(&profile)),
        );
    }
    if let Some(hash) = path_without_query.strip_prefix("/textures/") {
        return skins.get(hash).map_or_else(
            || json_response(404, serde_json::json!({"error":"Not Found"})),
            |texture| ("200 OK", texture.content_type, texture.bytes),
        );
    }
    json_response(404, serde_json::json!({"error":"Not Found"}))
}

fn profile_json(profile: &PlayerProfile) -> serde_json::Value {
    serde_json::json!({
        "id": profile.uuid.simple().to_string(),
        "name": profile.name,
        "properties": profile.properties,
    })
}

fn json_response(status: u16, body: serde_json::Value) -> (&'static str, String, Vec<u8>) {
    let status = match status {
        200 => "200 OK",
        400 => "400 Bad Request",
        403 => "403 Forbidden",
        404 => "404 Not Found",
        _ => "500 Internal Server Error",
    };
    (
        status,
        "application/json".into(),
        serde_json::to_vec(&body).expect("JSON response"),
    )
}

#[cfg(test)]
mod tests {
    use super::{PlayerProfile, ProfileProperty, YggdrasilRegistry, YggdrasilServer, route};
    use crate::core::account::skin::SkinStore;
    use uuid::Uuid;

    #[test]
    fn registry_returns_only_registered_players() {
        let registry = YggdrasilRegistry::default();
        let uuid = Uuid::new_v4();
        registry.register(PlayerProfile {
            uuid,
            name: "Alex".into(),
            properties: vec![ProfileProperty {
                name: "textures".into(),
                value: "encoded".into(),
            }],
        });
        assert_eq!(registry.profile(uuid).expect("profile").name, "Alex");
        assert!(registry.has_joined("Steve").is_none());
    }

    #[test]
    fn yggdrasil_routes_publish_metadata_profiles_and_accept_join() {
        let registry = YggdrasilRegistry::default();
        let uuid = Uuid::new_v4();
        registry.register(PlayerProfile {
            uuid,
            name: "Alex".into(),
            properties: Vec::new(),
        });
        let skins = SkinStore::default();
        let (status, _, meta) = route("GET", "/", &[], &registry, &skins);
        assert_eq!(status, "200 OK");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&meta).expect("metadata")["meta"]["serverName"],
            "TMCL Local Offline Service"
        );
        let path = format!("/sessionserver/session/minecraft/profile/{}", uuid.simple());
        let (status, _, profile) = route("GET", &path, &[], &registry, &skins);
        assert_eq!(status, "200 OK");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&profile).expect("profile")["name"],
            "Alex"
        );
        let (status, _, _) = route(
            "POST",
            "/sessionserver/session/minecraft/join",
            b"{}",
            &registry,
            &skins,
        );
        assert_eq!(status, "204 No Content");
    }

    #[tokio::test]
    async fn server_binds_loopback_only() {
        let server = YggdrasilServer::bind(YggdrasilRegistry::default(), SkinStore::default())
            .await
            .expect("bind server");
        assert!(server.endpoint.starts_with("http://127.0.0.1:"));
        server.stop().await;
    }
}
