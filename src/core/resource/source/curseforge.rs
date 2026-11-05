use std::sync::Arc;

use serde::Deserialize;

use crate::core::error::Error;
use crate::core::version::http::HttpClient;

use super::super::model::{
    DependencyKind, LoaderFilter, ReleaseChannel, ResourceCategory, ResourceDependency,
    ResourceFile, ResourceKind, ResourceProject, ResourceQuery, ResourceSearchPage, ResourceSort,
    ResourceSource,
};
use super::super::provider::{ProviderCapabilities, ProviderFuture, ResourceProvider};

const DEFAULT_BASE: &str = "https://api.curseforge.com/v1";
const MINECRAFT_GAME_ID: u32 = 432;

pub struct CurseForgeProvider {
    client: Arc<HttpClient>,
    base_url: String,
    api_key: Option<String>,
}

impl CurseForgeProvider {
    pub fn new(client: Arc<HttpClient>) -> Self {
        Self {
            client,
            base_url: DEFAULT_BASE.into(),
            api_key: option_env!("TMCL_CURSEFORGE_API_KEY")
                .filter(|key| !key.trim().is_empty())
                .map(str::to_owned),
        }
    }

    pub fn with_base_url_and_key(
        client: Arc<HttpClient>,
        base_url: impl Into<String>,
        key: impl Into<String>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').into(),
            api_key: Some(key.into()),
        }
    }
}

impl ResourceProvider for CurseForgeProvider {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            source: ResourceSource::CurseForge,
            available: self.api_key.is_some(),
            kinds: vec![
                ResourceKind::Mod,
                ResourceKind::Modpack,
                ResourceKind::ResourcePack,
                ResourceKind::Shader,
                ResourceKind::World,
            ],
            reason: (!self.api_key.is_some())
                .then(|| "CurseForge API Key is not configured".into()),
        }
    }

    fn search(&self, query: ResourceQuery) -> ProviderFuture<ResourceSearchPage> {
        let client = self.client.clone();
        let base = self.base_url.clone();
        let key = self.api_key.clone();
        Box::pin(async move {
            let key =
                key.ok_or_else(|| Error::Download("CurseForge API Key is not configured".into()))?;
            let url = search_url(&base, &query);
            let response: CurseForgeList<CurseForgeMod> = client
                .get_json_with_headers(&url, &[("x-api-key", key.as_str())])
                .await?;
            Ok(ResourceSearchPage {
                hits: response
                    .data
                    .into_iter()
                    .map(|item| item.into_project())
                    .collect(),
                total: response
                    .pagination
                    .and_then(|pagination| pagination.total_count)
                    .unwrap_or(0),
                page: query.page,
                page_size: query.page_size,
            })
        })
    }

    fn categories(&self, kind: ResourceKind) -> ProviderFuture<Vec<ResourceCategory>> {
        let client = self.client.clone();
        let base = self.base_url.clone();
        let key = self.api_key.clone();
        Box::pin(async move {
            let key =
                key.ok_or_else(|| Error::Download("CurseForge API Key is not configured".into()))?;
            let response: CurseForgeEnvelope<Vec<CurseForgeCategoryEntry>> = client
                .get_json_with_headers(
                    &format!("{base}/categories?gameId={MINECRAFT_GAME_ID}"),
                    &[("x-api-key", key.as_str())],
                )
                .await?;
            Ok(categories_for(kind, response.data))
        })
    }

    fn project(&self, project_id: String) -> ProviderFuture<ResourceProject> {
        let client = self.client.clone();
        let base = self.base_url.clone();
        let key = self.api_key.clone();
        Box::pin(async move {
            let key =
                key.ok_or_else(|| Error::Download("CurseForge API Key is not configured".into()))?;
            let response: CurseForgeEnvelope<CurseForgeMod> = client
                .get_json_with_headers(
                    &format!("{base}/mods/{project_id}"),
                    &[("x-api-key", key.as_str())],
                )
                .await?;
            Ok(response.data.into_project())
        })
    }

    fn files(&self, project_id: String) -> ProviderFuture<Vec<ResourceFile>> {
        let client = self.client.clone();
        let base = self.base_url.clone();
        let key = self.api_key.clone();
        Box::pin(async move {
            let key =
                key.ok_or_else(|| Error::Download("CurseForge API Key is not configured".into()))?;
            let response: CurseForgeList<CurseForgeFile> = client
                .get_json_with_headers(
                    &format!("{base}/mods/{project_id}/files?pageSize=50"),
                    &[("x-api-key", key.as_str())],
                )
                .await?;
            let files = response
                .data
                .into_iter()
                .filter_map(|file| file.into_resource(project_id.clone()))
                .collect::<Vec<_>>();
            if files.is_empty() {
                return Err(Error::Download(
                    "resource project has no downloadable files".into(),
                ));
            }
            Ok(files)
        })
    }

    fn file(&self, project_id: String, file_id: String) -> ProviderFuture<ResourceFile> {
        let client = self.client.clone();
        let base = self.base_url.clone();
        let key = self.api_key.clone();
        Box::pin(async move {
            let key =
                key.ok_or_else(|| Error::Download("CurseForge API Key is not configured".into()))?;
            let response: CurseForgeEnvelope<CurseForgeFile> = client
                .get_json_with_headers(
                    &format!("{base}/mods/{project_id}/files/{file_id}"),
                    &[("x-api-key", key.as_str())],
                )
                .await?;
            response
                .data
                .into_resource(project_id)
                .ok_or_else(|| Error::Download("CurseForge file has no download URL".into()))
        })
    }
}

#[derive(Debug, Deserialize)]
struct CurseForgeEnvelope<T> {
    data: T,
}
#[derive(Debug, Deserialize)]
struct CurseForgeList<T> {
    data: Vec<T>,
    #[serde(default)]
    pagination: Option<CurseForgePagination>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgePagination {
    #[serde(default)]
    total_count: Option<u64>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeMod {
    id: u64,
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    authors: Vec<CurseForgeAuthor>,
    #[serde(default)]
    logo: Option<CurseForgeLogo>,
    #[serde(default)]
    categories: Vec<CurseForgeCategory>,
    #[serde(default)]
    latest_files_indexes: Vec<CurseForgeLatestFile>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeAuthor {
    #[serde(default)]
    name: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeLogo {
    #[serde(default)]
    url: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeCategory {
    #[serde(default)]
    name: String,
}

/// One entry of `GET /v1/categories`; unlike the project tag above it carries
/// the numeric id and the class it belongs to.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeCategoryEntry {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    class_id: u32,
}

/// Filters the category list down to one resource class and normalises it.
fn categories_for(
    kind: ResourceKind,
    entries: Vec<CurseForgeCategoryEntry>,
) -> Vec<ResourceCategory> {
    let class = class_id(kind);
    let mut categories: Vec<ResourceCategory> = Vec::new();
    for entry in entries {
        if entry.class_id != class {
            continue;
        }
        let id = entry.id.to_string();
        if categories.iter().any(|category| category.id == id) {
            continue;
        }
        categories.push(ResourceCategory {
            id,
            name: entry.name,
            kind,
        });
    }
    categories.sort_by(|left, right| left.name.cmp(&right.name));
    categories
}

/// CurseForge accepts a single numeric `categoryId`; anything else is ignored.
fn category_param(categories: &[String]) -> Option<&str> {
    categories
        .iter()
        .map(String::as_str)
        .find(|id| !id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeLatestFile {
    #[serde(default)]
    game_version: String,
    #[serde(default)]
    mod_loader: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeFile {
    id: u64,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    file_length: u64,
    #[serde(default)]
    hashes: Vec<CurseForgeHash>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    mod_loaders: Vec<CurseForgeLoader>,
    #[serde(default)]
    release_type: u8,
    #[serde(default)]
    dependencies: Vec<CurseForgeDependency>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeHash {
    #[serde(default)]
    value: String,
    #[serde(default)]
    algo: u8,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeLoader {
    #[serde(default)]
    name: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeDependency {
    #[serde(default)]
    mod_id: u64,
    #[serde(default)]
    relation_type: u8,
}

impl CurseForgeMod {
    fn into_project(self) -> ResourceProject {
        ResourceProject {
            source: ResourceSource::CurseForge,
            project_id: self.id.to_string(),
            title: self.name,
            author: self
                .authors
                .into_iter()
                .map(|author| author.name)
                .find(|name| !name.is_empty())
                .unwrap_or_else(|| "unknown".into()),
            description: self.summary,
            icon_url: self.logo.and_then(|logo| logo.url),
            categories: self
                .categories
                .into_iter()
                .map(|category| category.name)
                .collect(),
            game_versions: self
                .latest_files_indexes
                .iter()
                .map(|file| file.game_version.clone())
                .filter(|version| !version.is_empty())
                .collect(),
            loaders: self
                .latest_files_indexes
                .iter()
                .map(|file| file.mod_loader.clone())
                .filter(|loader| !loader.is_empty())
                .collect(),
            downloads: None,
            updated_at: None,
        }
    }
}
impl CurseForgeFile {
    fn into_resource(self, project_id: String) -> Option<ResourceFile> {
        let name = if self.file_name.is_empty() {
            self.display_name.clone()
        } else {
            self.file_name.clone()
        };
        // CurseForge nulls `downloadUrl` when the author opted out of the API,
        // but the CDN still serves the file at its id-derived path. Without
        // this fallback those packs install everything except the mods that
        // mattered.
        let download_url = self
            .download_url
            .clone()
            .filter(|url| !url.trim().is_empty())
            .or_else(|| {
                (!name.trim().is_empty()).then(|| {
                    format!(
                        "https://edge.forgecdn.net/files/{}/{}/{}",
                        self.id / 1000,
                        self.id % 1000,
                        name
                    )
                })
            })?;
        Some(ResourceFile {
            source: ResourceSource::CurseForge,
            project_id,
            file_id: self.id.to_string(),
            name,
            download_url,
            sha1: self
                .hashes
                .into_iter()
                .find(|hash| hash.algo == 1)
                .map(|hash| hash.value),
            size: (self.file_length > 0).then_some(self.file_length),
            game_versions: self.game_versions,
            loaders: self
                .mod_loaders
                .into_iter()
                .map(|loader| loader.name)
                .collect(),
            channel: match self.release_type {
                1 => ReleaseChannel::Release,
                2 => ReleaseChannel::Beta,
                3 => ReleaseChannel::Alpha,
                _ => ReleaseChannel::Unknown,
            },
            dependencies: self
                .dependencies
                .into_iter()
                .map(|dependency| ResourceDependency {
                    project_id: (dependency.mod_id > 0).then(|| dependency.mod_id.to_string()),
                    file_id: None,
                    name: dependency.mod_id.to_string(),
                    kind: match dependency.relation_type {
                        1 => DependencyKind::Embedded,
                        2 => DependencyKind::Optional,
                        3 => DependencyKind::Required,
                        5 => DependencyKind::Incompatible,
                        _ => DependencyKind::Unknown,
                    },
                })
                .collect(),
        })
    }
}

fn class_id(kind: ResourceKind) -> u32 {
    match kind {
        ResourceKind::Mod => 6,
        ResourceKind::Modpack => 4471,
        ResourceKind::ResourcePack => 12,
        ResourceKind::Shader => 6552,
        ResourceKind::World => 17,
    }
}

/// Builds the search URL so the query mapping is testable without a server.
fn search_url(base: &str, query: &ResourceQuery) -> String {
    let class = class_id(query.kind);
    let mut url = format!(
        "{base}/mods/search?gameId={MINECRAFT_GAME_ID}&classId={class}&pageSize={}&index={}&sortField={}&sortOrder=desc",
        query.page_size,
        query.page.saturating_mul(query.page_size),
        sort_field(query.sort)
    );
    if !query.search.trim().is_empty() {
        url.push_str("&searchFilter=");
        url.push_str(&encode(&query.search));
    }
    if let Some(version) = game_version_filter(query.kind, query.game_version.as_deref()) {
        url.push_str("&gameVersion=");
        url.push_str(&encode(version));
    }
    if let Some(loader) = query
        .loader
        .filter(|loader| *loader != LoaderFilter::Any)
        .and_then(|loader| loader_type(query.kind, loader))
    {
        url.push_str("&modLoaderType=");
        url.push_str(loader);
    }
    if let Some(category) = category_param(&query.categories) {
        url.push_str("&categoryId=");
        url.push_str(category);
    }
    url
}

fn sort_field(sort: ResourceSort) -> u8 {
    match sort {
        ResourceSort::Relevance | ResourceSort::Follows => 2,
        ResourceSort::Downloads => 6,
        ResourceSort::Newest => 11,
        ResourceSort::Updated => 3,
    }
}
fn loader_type(kind: ResourceKind, loader: LoaderFilter) -> Option<&'static str> {
    if !kind.supports_loader_filter() {
        return None;
    }
    match loader {
        LoaderFilter::Any | LoaderFilter::Vanilla => None,
        LoaderFilter::Forge => Some("1"),
        // CurseForge has no Quilt category; Quilt packs are published as
        // Fabric there.
        LoaderFilter::Fabric | LoaderFilter::Quilt => Some("4"),
        LoaderFilter::NeoForge => Some("6"),
    }
}

fn game_version_filter(kind: ResourceKind, game_version: Option<&str>) -> Option<&str> {
    (kind != ResourceKind::Modpack)
        .then_some(game_version)
        .flatten()
}
fn encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => vec![byte as char],
            b' ' => vec!['+'],
            other => format!("%{other:02X}").chars().collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CurseForgeCategoryEntry, CurseForgeFile, CurseForgeList, CurseForgeProvider,
        categories_for, category_param, class_id, game_version_filter, loader_type, search_url,
    };
    use crate::core::resource::model::{LoaderFilter, ResourceKind, ResourceQuery, ResourceSource};
    use crate::core::resource::provider::ResourceProvider;
    use crate::core::version::http::HttpClient;
    use std::sync::Arc;
    #[test]
    fn maps_all_curseforge_class_ids() {
        assert_eq!(class_id(ResourceKind::Mod), 6);
        assert_eq!(class_id(ResourceKind::World), 17);
    }

    #[test]
    fn category_entries_are_filtered_by_class_and_sorted() {
        let entries = vec![
            CurseForgeCategoryEntry {
                id: 4483,
                name: "Combat".into(),
                class_id: 6,
            },
            CurseForgeCategoryEntry {
                id: 4474,
                name: "Adventure".into(),
                class_id: 6,
            },
            CurseForgeCategoryEntry {
                id: 12,
                name: "Resource Pack Thing".into(),
                class_id: 12,
            },
        ];
        let mods = categories_for(ResourceKind::Mod, entries);
        assert_eq!(
            mods.iter()
                .map(|category| category.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Adventure", "Combat"]
        );
        assert!(
            mods.iter()
                .all(|category| category.kind == ResourceKind::Mod)
        );
        assert_eq!(mods[0].id, "4474");
    }

    #[test]
    fn only_the_first_numeric_category_is_sent() {
        assert_eq!(category_param(&[]), None);
        assert_eq!(category_param(&["adventure".into()]), None);
        assert_eq!(
            category_param(&["adventure".into(), "4474".into(), "4483".into()]),
            Some("4474")
        );
    }

    #[test]
    fn search_url_appends_one_numeric_category() {
        let mut query = ResourceQuery::new(ResourceSource::CurseForge, ResourceKind::Mod);
        query.categories = vec!["4474".into(), "4483".into()];
        let url = search_url("https://example", &query);
        assert!(url.contains("&categoryId=4474"), "{url}");
        assert!(!url.contains("4483"), "{url}");

        query.categories = vec!["adventure".into()];
        assert!(!search_url("https://example", &query).contains("categoryId"));

        query.categories.clear();
        assert!(!search_url("https://example", &query).contains("categoryId"));
    }

    #[test]
    fn non_loader_resources_do_not_send_mod_loader_filter() {
        assert_eq!(
            loader_type(ResourceKind::ResourcePack, LoaderFilter::Fabric),
            None
        );
        assert_eq!(
            loader_type(ResourceKind::Shader, LoaderFilter::Fabric),
            None
        );
        assert_eq!(
            loader_type(ResourceKind::Mod, LoaderFilter::Fabric),
            Some("4")
        );
    }

    #[test]
    fn modpacks_do_not_send_game_version_filter() {
        assert_eq!(
            game_version_filter(ResourceKind::Modpack, Some("1.20.1")),
            None
        );
        assert_eq!(
            game_version_filter(ResourceKind::Mod, Some("1.20.1")),
            Some("1.20.1")
        );
    }
    #[test]
    fn missing_key_makes_provider_unavailable_without_a_request() {
        let provider = CurseForgeProvider::new(Arc::new(HttpClient::new().expect("client")));
        let capabilities = provider.capabilities();
        assert_eq!(capabilities.source, ResourceSource::CurseForge);
        assert!(!capabilities.available || option_env!("TMCL_CURSEFORGE_API_KEY").is_some());
    }

    #[tokio::test]
    async fn file_lookup_hits_the_id_endpoint_with_the_api_key() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("connection");
            let mut buffer = [0u8; 2048];
            let read = stream.read(&mut buffer).expect("request");
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            assert!(request.contains("/v1/mods/238222/files/1234"), "{request}");
            assert!(
                request.to_lowercase().contains("x-api-key: test-key"),
                "{request}"
            );
            let body = r#"{"data":{"id":1234,"fileName":"sodium.jar","downloadUrl":"https://cdn.example/sodium.jar","fileLength":42,"hashes":[{"value":"abc","algo":1}]}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("response");
        });

        let provider = CurseForgeProvider::with_base_url_and_key(
            Arc::new(HttpClient::new().expect("client")),
            format!("http://{address}/v1"),
            "test-key",
        );
        let file = provider
            .file("238222".into(), "1234".into())
            .await
            .expect("file");
        assert_eq!(file.name, "sodium.jar");
        assert_eq!(file.download_url, "https://cdn.example/sodium.jar");
        assert_eq!(file.sha1.as_deref(), Some("abc"));
        assert_eq!(file.size, Some(42));
        server.join().expect("server");
    }

    #[test]
    fn files_without_a_download_url_fall_back_to_the_cdn() {
        let file: CurseForgeFile = serde_json::from_str(
            r#"{"id":4096,"fileName":"demo.jar","downloadUrl":null,"hashes":[]}"#,
        )
        .expect("file");
        let resource = file.into_resource("7".into()).expect("resource");
        assert_eq!(
            resource.download_url,
            "https://edge.forgecdn.net/files/4/96/demo.jar"
        );
    }

    #[test]
    fn a_file_without_a_name_cannot_be_resolved() {
        let file: CurseForgeFile =
            serde_json::from_str(r#"{"id":4096,"displayName":"","fileName":""}"#).expect("file");
        assert!(file.into_resource("7".into()).is_none());
    }

    #[test]
    fn fixture_payload_uses_curseforge_camel_case_fields() {
        let response: CurseForgeList<CurseForgeFile> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/resource/curseforge-files.json"
        ))
        .expect("files fixture");
        assert_eq!(response.data[0].id, 84);
        assert_eq!(
            response.data[0].download_url.as_deref(),
            Some("https://example.test/demo.jar")
        );
    }
}
