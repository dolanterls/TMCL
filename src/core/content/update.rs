//! Check installed mods against Modrinth and report newer versions.
//!
//! The check is hash-based: a jar's SHA-1 identifies the exact version that
//! was downloaded, so a locally built or edited jar is reported as unknown
//! instead of being "updated" to a different mod with the same name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};
use crate::core::resource::model::{LoaderFilter, ReleaseChannel, ResourceFile, ResourceSource};
use crate::core::version::http::HttpClient;

pub const MODRINTH_BASE: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, PartialEq)]
pub struct ModUpdate {
    /// File currently installed.
    pub path: PathBuf,
    pub file_name: String,
    /// Version the installed file reports, when it can be read.
    pub current_version: String,
    pub new_version: String,
    /// The file to download in its place.
    pub file: ResourceFile,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpdateReport {
    /// Mods whose hash was sent to the service.
    pub checked: usize,
    pub updates: Vec<ModUpdate>,
    /// Mods the service did not recognise: built locally, edited, or from a
    /// source this check does not cover.
    pub unknown: usize,
}

#[derive(Debug, Serialize)]
struct UpdateRequest<'a> {
    hashes: &'a [String],
    algorithm: &'static str,
    loaders: Vec<&'static str>,
    game_versions: Vec<&'a str>,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    id: String,
    project_id: String,
    version_number: String,
    #[serde(default)]
    version_type: String,
    #[serde(default)]
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    #[serde(default)]
    hashes: HashMap<String, String>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    primary: bool,
}

/// Queries the batch update endpoint for every enabled jar in `mods_dir`.
pub async fn check_modrinth(
    client: &HttpClient,
    mods_dir: &Path,
    game_version: &str,
    loader: LoaderFilter,
) -> Result<UpdateReport> {
    check_modrinth_at(client, mods_dir, game_version, loader, MODRINTH_BASE).await
}

/// Same check against an explicit base URL; tests point it at a local server.
pub async fn check_modrinth_at(
    client: &HttpClient,
    mods_dir: &Path,
    game_version: &str,
    loader: LoaderFilter,
    base_url: &str,
) -> Result<UpdateReport> {
    let jars = local_jars(mods_dir);
    if jars.is_empty() {
        return Ok(UpdateReport::default());
    }
    // Hashing a hundred jars is blocking work; it must not run on an async
    // worker (the TUI shares those threads).
    let hashed = tokio::task::spawn_blocking(move || {
        jars.into_iter()
            .map(|path| {
                let hash = crate::core::download::hash::sha1_file(&path)?;
                let version = crate::core::content::meta::parse_mod(&path)
                    .and_then(|meta| meta.version())
                    .unwrap_or_default();
                Ok::<_, crate::core::error::Error>((path, hash, version))
            })
            .collect::<Result<Vec<_>>>()
    })
    .await
    .map_err(|error| Error::Download(format!("mod hashing task failed: {error}")))??;

    let hashes = hashed
        .iter()
        .map(|(_, hash, _)| hash.clone())
        .collect::<Vec<_>>();
    let request = UpdateRequest {
        hashes: &hashes,
        algorithm: "sha1",
        loaders: loader_names(loader),
        game_versions: vec![game_version],
    };
    let url = format!("{}/version_files/update", base_url.trim_end_matches('/'));
    let response: HashMap<String, ModrinthVersion> = client.post_json(&url, &request).await?;

    let mut report = UpdateReport {
        checked: hashed.len(),
        ..UpdateReport::default()
    };
    for (path, hash, current_version) in hashed {
        let Some(version) = response.get(&hash) else {
            report.unknown += 1;
            continue;
        };
        let Some(file) = primary_file(&version.files) else {
            report.unknown += 1;
            continue;
        };
        let new_hash = file.hashes.get("sha1").cloned().unwrap_or_default();
        if !new_hash.is_empty() && new_hash.eq_ignore_ascii_case(&hash) {
            // The service answered with the file that is already installed.
            continue;
        }
        let file_name = file.filename.clone();
        let installed_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        report.updates.push(ModUpdate {
            path,
            file_name: installed_name,
            current_version,
            new_version: version.version_number.clone(),
            file: ResourceFile {
                source: ResourceSource::Modrinth,
                project_id: version.project_id.clone(),
                file_id: version.id.clone(),
                name: file_name,
                download_url: file.url.clone(),
                sha1: (!new_hash.is_empty()).then_some(new_hash),
                size: (file.size > 0).then_some(file.size),
                game_versions: Vec::new(),
                loaders: Vec::new(),
                channel: channel(&version.version_type),
                dependencies: Vec::new(),
            },
        });
    }
    report
        .updates
        .sort_by(|left, right| left.file_name.cmp(&right.file_name));
    Ok(report)
}

fn local_jars(mods_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    let mut jars = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
        })
        .collect::<Vec<_>>();
    jars.sort();
    jars
}

fn primary_file(files: &[ModrinthFile]) -> Option<&ModrinthFile> {
    files
        .iter()
        .find(|file| file.primary)
        .or_else(|| files.first())
}

fn loader_names(loader: LoaderFilter) -> Vec<&'static str> {
    match loader {
        LoaderFilter::Fabric => vec!["fabric"],
        LoaderFilter::Quilt => vec!["quilt"],
        LoaderFilter::Forge => vec!["forge"],
        LoaderFilter::NeoForge => vec!["neoforge"],
        // Vanilla and "any" do not narrow the search: a mod that supports the
        // game version may target any loader the instance has.
        LoaderFilter::Vanilla | LoaderFilter::Any => Vec::new(),
    }
}

fn channel(value: &str) -> ReleaseChannel {
    match value {
        "beta" => ReleaseChannel::Beta,
        "alpha" => ReleaseChannel::Alpha,
        _ => ReleaseChannel::Release,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::{check_modrinth_at, loader_names};
    use crate::core::resource::model::LoaderFilter;

    fn jar(dir: &std::path::Path, name: &str, body: &[u8]) -> String {
        std::fs::create_dir_all(dir).expect("mods dir");
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write jar");
        crate::core::download::hash::sha1_file(&path).expect("hash")
    }

    #[test]
    fn only_loader_aware_loaders_narrow_the_search() {
        assert_eq!(loader_names(LoaderFilter::Fabric), ["fabric"]);
        assert_eq!(loader_names(LoaderFilter::NeoForge), ["neoforge"]);
        assert!(loader_names(LoaderFilter::Vanilla).is_empty());
    }

    #[tokio::test]
    async fn a_newer_version_is_reported_and_an_unknown_jar_is_counted() {
        let root = std::env::temp_dir().join(format!("tmcl-updates-{}", uuid::Uuid::new_v4()));
        let mods = root.join("mods");
        let old = jar(&mods, "sodium-0.5.8.jar", b"old sodium");
        let current = jar(&mods, "lithium-0.11.jar", b"current lithium");
        jar(&mods, "local-build.jar", b"not on modrinth");

        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("connection");
            // Headers and body can arrive in separate reads; a partial request
            // would make the assertions below fail for the wrong reason.
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let read = stream.read(&mut buffer).expect("request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&request);
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8_lossy(&request).to_string();
            assert!(
                request.contains("/version_files/update"),
                "unexpected request: {request}"
            );
            assert!(request.contains("\"algorithm\":\"sha1\""), "{request}");
            let body = format!(
                r#"{{"{old}":{{"id":"v2","project_id":"sodium","version_number":"0.5.11","version_type":"release","files":[{{"url":"https://cdn.example/sodium-0.5.11.jar","filename":"sodium-0.5.11.jar","hashes":{{"sha1":"newhash"}},"size":1234,"primary":true}}]}},
                    "{current}":{{"id":"v1","project_id":"lithium","version_number":"0.11","files":[{{"url":"https://cdn.example/lithium.jar","filename":"lithium-0.11.jar","hashes":{{"sha1":"{current}"}},"primary":true}}]}}}}"#
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });

        let client = crate::core::version::http::HttpClient::new().expect("client");
        let report = check_modrinth_at(
            &client,
            &mods,
            "1.20.1",
            LoaderFilter::Fabric,
            &format!("http://{address}/v2"),
        )
        .await
        .expect("check");
        server.join().expect("server");

        assert_eq!(report.checked, 3);
        assert_eq!(report.unknown, 1, "the locally built jar is unknown");
        assert_eq!(report.updates.len(), 1, "{:?}", report.updates);
        let update = &report.updates[0];
        assert_eq!(update.file_name, "sodium-0.5.8.jar");
        assert_eq!(update.new_version, "0.5.11");
        assert_eq!(update.file.name, "sodium-0.5.11.jar");
        assert_eq!(update.file.sha1.as_deref(), Some("newhash"));
        assert_eq!(update.file.size, Some(1234));
        std::fs::remove_dir_all(root).expect("clean");
    }
}
