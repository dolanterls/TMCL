pub mod archive;
pub mod compatibility;
pub mod conflict;
pub mod dependencies;
pub mod install;
pub mod model;
pub mod modpack;
pub mod modpack_install;
pub mod modpack_update;
pub mod plan;
pub mod provider;
pub mod select;
pub mod service;
pub mod source;

pub use compatibility::TargetContext;
pub use model::{
    Compatibility, DependencyKind, LoaderFilter, ReleaseChannel, ResourceDependency, ResourceFile,
    ResourceKind, ResourceProject, ResourceQuery, ResourceSearchPage, ResourceSort, ResourceSource,
};
pub use provider::{ProviderCapabilities, ResourceProvider};
pub use service::{ProviderRegistry, ResourceService, ResourceServiceRequest};

/// The providers every frontend uses.
///
/// Both are registered even when CurseForge has no API key: its capabilities
/// then say exactly why it cannot be used, which is a better answer than a
/// missing provider ("unavailable" with a reason beats "unknown source").
pub fn registry(
    client: std::sync::Arc<crate::core::version::http::HttpClient>,
) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();
    registry.register(std::sync::Arc::new(
        source::modrinth::ModrinthProvider::new(client.clone()),
    ));
    registry.register(std::sync::Arc::new(
        source::curseforge::CurseForgeProvider::new(client),
    ));
    registry
}
