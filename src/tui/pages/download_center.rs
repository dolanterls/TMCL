use crate::core::resource::model::{ResourceKind, ResourceSort, ResourceSource};
use crate::core::version::models::VersionFilter;

pub use crate::core::resource::model::RESOURCE_PAGE_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadCategory {
    GameVersions,
    Resource(ResourceKind),
}

impl DownloadCategory {
    pub const ALL: [Self; 6] = [
        Self::GameVersions,
        Self::Resource(ResourceKind::Mod),
        Self::Resource(ResourceKind::Modpack),
        Self::Resource(ResourceKind::ResourcePack),
        Self::Resource(ResourceKind::Shader),
        Self::Resource(ResourceKind::World),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::GameVersions => "Game Versions",
            Self::Resource(kind) => kind.label(),
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::GameVersions => "download.category_game_versions",
            Self::Resource(ResourceKind::Mod) => "download.category_mods",
            Self::Resource(ResourceKind::Modpack) => "download.category_modpacks",
            Self::Resource(ResourceKind::ResourcePack) => "download.category_resource_packs",
            Self::Resource(ResourceKind::Shader) => "download.category_shaders",
            Self::Resource(ResourceKind::World) => "download.category_worlds",
        }
    }

    pub fn source_default(self) -> ResourceSource {
        match self {
            Self::GameVersions
            | Self::Resource(
                ResourceKind::Mod
                | ResourceKind::Modpack
                | ResourceKind::ResourcePack
                | ResourceKind::Shader,
            ) => ResourceSource::Modrinth,
            Self::Resource(ResourceKind::World) => ResourceSource::CurseForge,
        }
    }

    pub fn resource_kind(self) -> Option<ResourceKind> {
        match self {
            Self::GameVersions => None,
            Self::Resource(kind) => Some(kind),
        }
    }

    /// Where a download of this category ends up.
    ///
    /// Only [`DownloadDestination::Instance`] has a target instance. Showing
    /// one for the other two - and checking a file's compatibility against it -
    /// promises an install into the instance the player picked, which is not
    /// what Enter does.
    pub fn destination(self) -> DownloadDestination {
        match self {
            Self::GameVersions | Self::Resource(ResourceKind::Modpack) => {
                DownloadDestination::NewInstance
            }
            Self::Resource(ResourceKind::World) => DownloadDestination::SavePath,
            Self::Resource(_) => DownloadDestination::Instance,
        }
    }

    /// Whether this category installs into an instance that already exists.
    pub fn targets_instance(self) -> bool {
        self.destination() == DownloadDestination::Instance
    }
}

/// What happens to a download once it finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadDestination {
    /// Installed into the instance the player picked with `t`.
    Instance,
    /// Unpacked into a brand-new instance of its own.
    NewInstance,
    /// Downloaded, then written to a path the player picks.
    SavePath,
}

#[derive(Debug, Clone)]
pub struct DownloadCenterState {
    pub category: DownloadCategory,
    pub source: ResourceSource,
    pub search: String,
    pub sort: ResourceSort,
    pub selected: usize,
    pub page: u32,
    pub total: u64,
    pub request_id: u64,
    pub loading: bool,
    pub error: Option<String>,
    pub version_filter: VersionFilter,
    /// Selected category ids for the current source; empty means no filter.
    pub categories: Vec<String>,
}

impl Default for DownloadCenterState {
    fn default() -> Self {
        let category = DownloadCategory::GameVersions;
        Self {
            category,
            source: category.source_default(),
            search: String::new(),
            sort: ResourceSort::Relevance,
            selected: 0,
            page: 0,
            total: 0,
            request_id: 0,
            loading: false,
            error: None,
            version_filter: VersionFilter::default(),
            categories: Vec::new(),
        }
    }
}

impl DownloadCenterState {
    pub fn set_category(&mut self, category: DownloadCategory) {
        self.category = category;
        self.source = category.source_default();
        self.search.clear();
        self.sort = ResourceSort::Relevance;
        self.selected = 0;
        self.page = 0;
        self.total = 0;
        self.error = None;
        self.version_filter = VersionFilter::default();
        // Category ids are source-native; a new resource kind starts unfiltered.
        self.categories.clear();
    }

    pub fn set_source(&mut self, source: ResourceSource) {
        self.source = source;
        self.search.clear();
        self.sort = ResourceSort::Relevance;
        self.selected = 0;
        self.page = 0;
        self.total = 0;
        self.error = None;
        self.categories.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{DownloadCategory, DownloadCenterState};
    use crate::core::resource::model::{RESOURCE_PAGE_SIZE, ResourceKind, ResourceSource};

    #[test]
    fn switching_category_resets_provider_filters() {
        let mut state = DownloadCenterState {
            search: "sodium".into(),
            selected: 4,
            ..DownloadCenterState::default()
        };
        state.set_category(DownloadCategory::Resource(ResourceKind::World));
        assert_eq!(state.source, ResourceSource::CurseForge);
        assert!(state.search.is_empty());
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn switching_category_or_source_drops_the_category_filter() {
        let mut state = DownloadCenterState {
            categories: vec!["adventure".into(), "magic".into()],
            ..DownloadCenterState::default()
        };
        state.set_category(DownloadCategory::Resource(ResourceKind::Modpack));
        assert!(state.categories.is_empty());

        state.categories = vec!["4474".into()];
        state.set_source(ResourceSource::Modrinth);
        assert!(state.categories.is_empty());
    }

    #[test]
    fn only_categories_that_write_into_an_instance_have_a_target() {
        use super::DownloadDestination;

        assert_eq!(
            DownloadCategory::GameVersions.destination(),
            DownloadDestination::NewInstance
        );
        assert_eq!(
            DownloadCategory::Resource(ResourceKind::Modpack).destination(),
            DownloadDestination::NewInstance
        );
        // A world is saved wherever the player points the save dialog, so it
        // has no target instance either.
        assert_eq!(
            DownloadCategory::Resource(ResourceKind::World).destination(),
            DownloadDestination::SavePath
        );
        assert!(!DownloadCategory::Resource(ResourceKind::World).targets_instance());
        for kind in [
            ResourceKind::Mod,
            ResourceKind::ResourcePack,
            ResourceKind::Shader,
        ] {
            assert_eq!(
                DownloadCategory::Resource(kind).destination(),
                DownloadDestination::Instance
            );
            assert!(
                DownloadCategory::Resource(kind).targets_instance(),
                "{kind:?} installs into an existing instance"
            );
        }
    }

    #[test]
    fn download_center_exposes_the_shared_page_size() {
        assert_eq!(super::RESOURCE_PAGE_SIZE, RESOURCE_PAGE_SIZE);
        assert_eq!(super::RESOURCE_PAGE_SIZE, 50);
    }
}
