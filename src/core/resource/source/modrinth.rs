use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::core::error::{Error, Result};
use crate::core::version::http::HttpClient;

use super::super::model::{
    DependencyKind, LoaderFilter, ReleaseChannel, ResourceCategory, ResourceDependency,
    ResourceFile, ResourceKind, ResourceProject, ResourceQuery, ResourceSearchPage, ResourceSort,
    ResourceSource,
};
use super::super::provider::{ProviderCapabilities, ProviderFuture, ResourceProvider};

const DEFAULT_BASE: &str = "https://api.modrinth.com/v2";

pub struct ModrinthProvider {
    client: Arc<HttpClient>,
    base_url: String,
}

impl ModrinthProvider {
    pub fn new(client: Arc<HttpClient>) -> Self {
        Self {
            client,
            base_url: DEFAULT_BASE.into(),
        }
    }

    pub fn with_base_url(client: Arc<HttpClient>, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').into(),
        }
    }
}

impl ResourceProvider for ModrinthProvider {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            source: ResourceSource::Modrinth,
            available: true,
            kinds: vec![
                ResourceKind::Mod,
                ResourceKind::Modpack,
                ResourceKind::ResourcePack,
                ResourceKind::Shader,
            ],
            reason: None,
        }
    }

    fn search(&self, query: ResourceQuery) -> ProviderFuture<ResourceSearchPage> {
        let client = self.client.clone();
        let base = self.base_url.clone();
        Box::pin(async move {
            let facets = search_facets(
                query.kind,
                query.game_version.as_deref(),
                query.loader,
                &query.categories,
            )?;
            let url = format!(
                "{base}/search?query={}&facets={}&limit={}&offset={}&index={}",
                encode(&query.search),
                encode(&facets),
                query.page_size,
                query.page.saturating_mul(query.page_size),
                sort_value(query.sort)
            );
            let response: ModrinthSearchResponse = client.get_json(&url).await?;
            Ok(ResourceSearchPage {
                hits: response
                    .hits
                    .into_iter()
                    .map(|hit| hit.into_project())
                    .collect(),
                total: response.total_hits,
                page: query.page,
                page_size: query.page_size,
            })
        })
    }

    fn categories(&self, kind: ResourceKind) -> ProviderFuture<Vec<ResourceCategory>> {
        let client = self.client.clone();
        let base = self.base_url.clone();
        Box::pin(async move {
            let Ok(expected) = project_type(kind) else {
                // Modrinth has no world category list.
                return Ok(Vec::new());
            };
            let tags: Vec<ModrinthTagCategory> =
                client.get_json(&format!("{base}/tag/category")).await?;
            Ok(categories_for(kind, expected, tags))
        })
    }

    fn project(&self, project_id: String) -> ProviderFuture<ResourceProject> {
        let client = self.client.clone();
        let url = format!("{}/project/{project_id}", self.base_url);
        Box::pin(async move {
            Ok(client
                .get_json::<ModrinthProject>(&url)
                .await?
                .into_project())
        })
    }

    fn files(&self, project_id: String) -> ProviderFuture<Vec<ResourceFile>> {
        let client = self.client.clone();
        let url = format!("{}/project/{project_id}/version", self.base_url);
        Box::pin(async move {
            let versions: Vec<ModrinthVersion> = client.get_json(&url).await?;
            let files = versions
                .into_iter()
                .flat_map(|version| {
                    let metadata = version.clone();
                    let project_id = project_id.clone();
                    version
                        .files
                        .into_iter()
                        .map(move |file| file.into_resource(project_id.clone(), &metadata))
                })
                .flatten()
                .collect::<Vec<_>>();
            if files.is_empty() {
                return Err(Error::Download(
                    "resource project has no downloadable files".into(),
                ));
            }
            Ok(files)
        })
    }
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    #[serde(default)]
    hits: Vec<ModrinthHit>,
    #[serde(default)]
    total_hits: u64,
}

#[derive(Debug, Deserialize, Clone)]
struct ModrinthHit {
    project_id: String,
    title: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    date_modified: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthProject {
    id: String,
    title: String,
    #[serde(default)]
    team: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    updated: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct ModrinthVersion {
    id: String,
    #[serde(default)]
    version_type: Option<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    dependencies: Vec<ModrinthDependency>,
    #[serde(default)]
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize, Clone)]
struct ModrinthFile {
    hashes: HashMap<String, String>,
    url: Option<String>,
    filename: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
struct ModrinthDependency {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    dependency_type: Option<String>,
}

impl ModrinthHit {
    fn into_project(self) -> ResourceProject {
        ResourceProject {
            source: ResourceSource::Modrinth,
            project_id: self.project_id,
            title: self.title,
            author: self.author,
            description: self.description,
            icon_url: self.icon_url,
            categories: self.categories,
            game_versions: self.versions,
            loaders: Vec::new(),
            downloads: self.downloads,
            updated_at: self.date_modified,
        }
    }
}
impl ModrinthProject {
    fn into_project(self) -> ResourceProject {
        ResourceProject {
            source: ResourceSource::Modrinth,
            project_id: self.id,
            title: self.title,
            author: if self.author.is_empty() {
                self.team
            } else {
                self.author
            },
            description: self.description,
            icon_url: self.icon_url,
            categories: self.categories,
            game_versions: self.game_versions,
            loaders: self.loaders,
            downloads: self.downloads,
            updated_at: self.updated,
        }
    }
}
impl ModrinthFile {
    fn into_resource(self, project_id: String, version: &ModrinthVersion) -> Option<ResourceFile> {
        Some(ResourceFile {
            source: ResourceSource::Modrinth,
            project_id,
            file_id: version.id.clone(),
            name: self.filename,
            download_url: self.url?,
            sha1: self.hashes.get("sha1").cloned(),
            size: self.size,
            game_versions: version.game_versions.clone(),
            loaders: version.loaders.clone(),
            channel: version
                .version_type
                .as_deref()
                .map(channel)
                .unwrap_or(ReleaseChannel::Unknown),
            dependencies: version
                .dependencies
                .iter()
                .map(|dependency| ResourceDependency {
                    project_id: dependency.project_id.clone(),
                    file_id: dependency.version_id.clone(),
                    name: dependency
                        .project_id
                        .clone()
                        .unwrap_or_else(|| "unknown dependency".into()),
                    kind: dependency
                        .dependency_type
                        .as_deref()
                        .map(dependency_kind)
                        .unwrap_or(DependencyKind::Unknown),
                })
                .collect(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct ModrinthTagCategory {
    name: String,
    #[serde(default, rename = "project_type")]
    project_type: String,
}

/// Filters the tag endpoint down to one project type and normalises the rows.
///
/// The tag endpoint returns every project type in one list, so the provider
/// filters and sorts here instead of asking the UI to do it.
fn categories_for(
    kind: ResourceKind,
    project_type: &str,
    tags: Vec<ModrinthTagCategory>,
) -> Vec<ResourceCategory> {
    let mut categories: Vec<ResourceCategory> = Vec::new();
    for tag in tags {
        if tag.project_type != project_type || !is_category_id(&tag.name) {
            continue;
        }
        if categories.iter().any(|category| category.id == tag.name) {
            continue;
        }
        categories.push(ResourceCategory {
            id: tag.name.clone(),
            name: tag.name,
            kind,
        });
    }
    categories.sort_by(|left, right| left.name.cmp(&right.name));
    categories
}

fn project_type(kind: ResourceKind) -> Result<&'static str> {
    match kind {
        ResourceKind::Mod => Ok("mod"),
        ResourceKind::Modpack => Ok("modpack"),
        ResourceKind::ResourcePack => Ok("resourcepack"),
        ResourceKind::Shader => Ok("shader"),
        ResourceKind::World => Err(Error::Download(
            "Modrinth does not provide worlds in the first release".into(),
        )),
    }
}

fn search_facets(
    kind: ResourceKind,
    game_version: Option<&str>,
    loader: Option<LoaderFilter>,
    categories: &[String],
) -> Result<String> {
    let project_type = project_type(kind)?;
    let mut facet_groups = vec![format!("[\"project_type:{project_type}\"]")];
    if kind != ResourceKind::Modpack
        && let Some(version) = game_version
    {
        facet_groups.push(format!("[\"versions:{version}\"]"));
    }
    if kind.supports_loader_filter()
        && let Some(loader) = loader.filter(|loader| *loader != LoaderFilter::Any)
    {
        facet_groups.push(format!("[\"categories:{}\"]", loader_value(loader)));
    }
    if let Some(group) = category_facet_group(categories) {
        facet_groups.push(group);
    }
    Ok(format!("[{}]", facet_groups.join(",")))
}

/// Builds one OR group for the selected categories.
///
/// Modrinth treats entries inside one group as OR and separate groups as AND,
/// so multi-select must stay in a single group. Only slug-shaped ids are
/// accepted; anything else is dropped instead of being injected into the
/// facet JSON.
fn category_facet_group(categories: &[String]) -> Option<String> {
    let mut values: Vec<String> = Vec::new();
    for id in categories {
        if !is_category_id(id) || values.iter().any(|value| value == id) {
            continue;
        }
        values.push(id.clone());
    }
    if values.is_empty() {
        return None;
    }
    let entries = values
        .iter()
        .map(|id| format!("\"categories:{id}\""))
        .collect::<Vec<_>>()
        .join(",");
    Some(format!("[{entries}]"))
}

fn is_category_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn sort_value(sort: ResourceSort) -> &'static str {
    match sort {
        ResourceSort::Relevance => "relevance",
        ResourceSort::Downloads => "downloads",
        ResourceSort::Follows => "follows",
        ResourceSort::Newest => "newest",
        ResourceSort::Updated => "updated",
    }
}
fn loader_value(loader: LoaderFilter) -> &'static str {
    match loader {
        LoaderFilter::Any => "",
        LoaderFilter::Vanilla => "minecraft",
        LoaderFilter::Fabric => "fabric",
        LoaderFilter::Quilt => "quilt",
        LoaderFilter::Forge => "forge",
        LoaderFilter::NeoForge => "neoforge",
    }
}
fn channel(value: &str) -> ReleaseChannel {
    match value {
        "release" => ReleaseChannel::Release,
        "beta" => ReleaseChannel::Beta,
        "alpha" => ReleaseChannel::Alpha,
        _ => ReleaseChannel::Unknown,
    }
}
fn dependency_kind(value: &str) -> DependencyKind {
    match value {
        "required" => DependencyKind::Required,
        "optional" => DependencyKind::Optional,
        "embedded" => DependencyKind::Embedded,
        "incompatible" => DependencyKind::Incompatible,
        _ => DependencyKind::Unknown,
    }
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
    use super::{ModrinthSearchResponse, ModrinthVersion, channel, dependency_kind, search_facets};
    use crate::core::resource::model::{
        DependencyKind, LoaderFilter, ReleaseChannel, ResourceKind,
    };
    #[test]
    fn maps_release_and_unknown_types() {
        assert_eq!(channel("release"), ReleaseChannel::Release);
        assert_eq!(channel("nightly"), ReleaseChannel::Unknown);
        assert_eq!(dependency_kind("required"), DependencyKind::Required);
        assert_eq!(dependency_kind("mystery"), DependencyKind::Unknown);
    }

    #[test]
    fn fixture_payloads_convert_to_unified_models() {
        let search: ModrinthSearchResponse = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/resource/modrinth-search.json"
        ))
        .expect("search fixture");
        assert_eq!(search.hits[0].clone().into_project().project_id, "demo-mod");
        let versions: Vec<ModrinthVersion> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/resource/modrinth-versions.json"
        ))
        .expect("versions fixture");
        assert_eq!(
            versions[0].files[0].hashes.get("sha1").map(String::as_str),
            Some("0123456789012345678901234567890123456789")
        );
    }

    #[test]
    fn non_loader_resources_do_not_receive_loader_facets() {
        let resource_pack = search_facets(
            ResourceKind::ResourcePack,
            Some("1.20.1"),
            Some(LoaderFilter::Fabric),
            &[],
        )
        .expect("resource pack facets");
        let shader = search_facets(
            ResourceKind::Shader,
            Some("1.20.1"),
            Some(LoaderFilter::Fabric),
            &[],
        )
        .expect("shader facets");
        let mods = search_facets(
            ResourceKind::Mod,
            Some("1.20.1"),
            Some(LoaderFilter::Fabric),
            &[],
        )
        .expect("mod facets");
        let modpacks = search_facets(
            ResourceKind::Modpack,
            Some("1.20.1"),
            Some(LoaderFilter::Fabric),
            &[],
        )
        .expect("modpack facets");

        assert!(!resource_pack.contains("categories:fabric"));
        assert!(!shader.contains("categories:fabric"));
        assert!(mods.contains("categories:fabric"));
        assert!(!modpacks.contains("versions:1.20.1"));
        assert!(!modpacks.contains("categories:fabric"));
        assert!(search_facets(ResourceKind::World, None, None, &[]).is_err());
    }

    #[test]
    fn selected_categories_share_one_or_facet_group() {
        let facets = search_facets(
            ResourceKind::Mod,
            None,
            None,
            &["adventure".into(), "decoration".into()],
        )
        .expect("category facets");
        assert!(facets.contains("[\"categories:adventure\",\"categories:decoration\"]"));
        // A single group means OR; two groups would have meant AND.
        assert_eq!(facets.matches("categories:").count(), 2);
    }

    #[test]
    fn empty_and_invalid_categories_add_no_group() {
        let empty = search_facets(ResourceKind::Mod, None, None, &[]).expect("no categories");
        assert!(!empty.contains("categories:"));
        let invalid = search_facets(
            ResourceKind::Mod,
            None,
            None,
            &[String::new(), "bad\"id".into(), "Adventure".into()],
        )
        .expect("invalid categories are dropped");
        assert!(!invalid.contains("categories:"));
    }

    #[test]
    fn tag_categories_are_filtered_sorted_and_deduped() {
        use super::{ModrinthTagCategory, categories_for};
        let tags = vec![
            ModrinthTagCategory {
                name: "magic".into(),
                project_type: "mod".into(),
            },
            ModrinthTagCategory {
                name: "adventure".into(),
                project_type: "mod".into(),
            },
            ModrinthTagCategory {
                name: "adventure".into(),
                project_type: "mod".into(),
            },
            ModrinthTagCategory {
                name: "decoration".into(),
                project_type: "resourcepack".into(),
            },
        ];
        let categories = categories_for(ResourceKind::Mod, "mod", tags);
        assert_eq!(
            categories
                .iter()
                .map(|category| category.name.as_str())
                .collect::<Vec<_>>(),
            vec!["adventure", "magic"]
        );
        assert!(
            categories
                .iter()
                .all(|category| category.kind == ResourceKind::Mod)
        );
        assert!(categories_for(ResourceKind::World, "world", Vec::new()).is_empty());
    }
}
