use serde::{Deserialize, Serialize};

/// Number of resource projects fetched for each download-center page.
///
/// Keep this in the resource model so every caller (UI and non-UI) uses the
/// same request/page-count contract.
pub const RESOURCE_PAGE_SIZE: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    Mod,
    Modpack,
    ResourcePack,
    Shader,
    World,
}

impl ResourceKind {
    pub const ALL: [Self; 5] = [
        Self::Mod,
        Self::Modpack,
        Self::ResourcePack,
        Self::Shader,
        Self::World,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Mod => "Mods",
            Self::Modpack => "Modpacks",
            Self::ResourcePack => "Resource Packs",
            Self::Shader => "Shaders",
            Self::World => "Worlds",
        }
    }

    pub fn install_directory(self) -> Option<&'static str> {
        match self {
            Self::Mod => Some("mods"),
            Self::ResourcePack => Some("resourcepacks"),
            Self::Shader => Some("shaderpacks"),
            Self::Modpack | Self::World => None,
        }
    }

    pub fn supports_loader_filter(self) -> bool {
        matches!(self, Self::Mod)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceSource {
    Modrinth,
    CurseForge,
}

impl ResourceSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Modrinth => "Modrinth",
            Self::CurseForge => "CurseForge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoaderFilter {
    Any,
    Vanilla,
    Fabric,
    /// Quilt runs Fabric mods as well, so compatibility checks accept both.
    Quilt,
    Forge,
    NeoForge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceSort {
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReleaseChannel {
    Release,
    Beta,
    Alpha,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyKind {
    Required,
    Optional,
    Embedded,
    Incompatible,
    Unknown,
}

/// One category a source exposes for a resource kind, in the source's own id
/// space (`adventure` on Modrinth, `4474` on CurseForge).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCategory {
    pub id: String,
    pub name: String,
    pub kind: ResourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceQuery {
    pub source: ResourceSource,
    pub kind: ResourceKind,
    pub game_version: Option<String>,
    pub loader: Option<LoaderFilter>,
    pub search: String,
    /// Source-native category ids; empty means "no category filter".
    pub categories: Vec<String>,
    pub sort: ResourceSort,
    pub page: u32,
    pub page_size: u32,
}

impl ResourceQuery {
    pub fn new(source: ResourceSource, kind: ResourceKind) -> Self {
        Self {
            source,
            kind,
            game_version: None,
            loader: None,
            search: String::new(),
            categories: Vec::new(),
            sort: ResourceSort::Relevance,
            page: 0,
            page_size: RESOURCE_PAGE_SIZE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceProject {
    pub source: ResourceSource,
    pub project_id: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub categories: Vec<String>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub downloads: Option<u64>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFile {
    pub source: ResourceSource,
    pub project_id: String,
    pub file_id: String,
    pub name: String,
    pub download_url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub channel: ReleaseChannel,
    pub dependencies: Vec<ResourceDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDependency {
    pub project_id: Option<String>,
    pub file_id: Option<String>,
    pub name: String,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSearchPage {
    pub hits: Vec<ResourceProject>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compatibility {
    Compatible,
    Unknown,
    Incompatible,
}

impl Compatibility {
    pub fn rank(self) -> u8 {
        match self {
            Self::Compatible => 0,
            Self::Unknown => 1,
            Self::Incompatible => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DependencyKind, RESOURCE_PAGE_SIZE, ResourceFile, ResourceKind, ResourceQuery,
        ResourceSource,
    };

    #[test]
    fn resource_file_round_trips_provider_identity() {
        let file = ResourceFile {
            source: ResourceSource::CurseForge,
            project_id: "42".into(),
            file_id: "84".into(),
            name: "example.jar".into(),
            download_url: "https://example.test/example.jar".into(),
            sha1: Some("abc".into()),
            size: Some(3),
            game_versions: vec!["1.21.1".into()],
            loaders: vec!["fabric".into()],
            channel: super::ReleaseChannel::Release,
            dependencies: vec![super::ResourceDependency {
                project_id: None,
                file_id: None,
                name: "unknown".into(),
                kind: DependencyKind::Unknown,
            }],
        };
        let json = serde_json::to_string(&file).expect("serialize");
        let restored: ResourceFile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.source, ResourceSource::CurseForge);
        assert_eq!(restored.project_id, "42");
        assert_eq!(restored.file_id, "84");
    }

    #[test]
    fn only_installable_resource_kinds_have_directories() {
        assert_eq!(ResourceKind::Mod.install_directory(), Some("mods"));
        assert_eq!(ResourceKind::World.install_directory(), None);
        assert_eq!(ResourceKind::Modpack.install_directory(), None);
    }

    #[test]
    fn only_loader_based_resources_support_loader_filters() {
        assert!(ResourceKind::Mod.supports_loader_filter());
        assert!(!ResourceKind::Modpack.supports_loader_filter());
        assert!(!ResourceKind::ResourcePack.supports_loader_filter());
        assert!(!ResourceKind::Shader.supports_loader_filter());
        assert!(!ResourceKind::World.supports_loader_filter());
    }

    #[test]
    fn resource_queries_use_the_shared_download_page_size() {
        let query = ResourceQuery::new(ResourceSource::Modrinth, ResourceKind::Mod);
        assert_eq!(query.page_size, RESOURCE_PAGE_SIZE);
        assert_eq!(RESOURCE_PAGE_SIZE, 50);
    }
}
