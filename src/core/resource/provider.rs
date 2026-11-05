use std::future::Future;
use std::pin::Pin;

use crate::core::error::Result;

use super::model::{
    ResourceCategory, ResourceFile, ResourceKind, ResourceProject, ResourceQuery,
    ResourceSearchPage,
};

pub type ProviderFuture<T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub source: super::model::ResourceSource,
    pub available: bool,
    pub kinds: Vec<ResourceKind>,
    pub reason: Option<String>,
}

pub trait ResourceProvider: Send + Sync {
    fn capabilities(&self) -> ProviderCapabilities;
    fn search(&self, query: ResourceQuery) -> ProviderFuture<ResourceSearchPage>;
    /// Categories the source exposes for `kind`, already filtered and sorted.
    ///
    /// An empty list means the source has no category filter for that kind
    /// (for example Modrinth worlds); network and auth failures are `Err`.
    fn categories(&self, kind: ResourceKind) -> ProviderFuture<Vec<ResourceCategory>>;
    fn project(&self, project_id: String) -> ProviderFuture<ResourceProject>;
    fn files(&self, project_id: String) -> ProviderFuture<Vec<ResourceFile>>;

    /// One file addressed by id, used by modpack manifests that reference
    /// files instead of linking them.
    ///
    /// Sources without id lookup keep the default and say so, so a pack can
    /// report which entry it could not resolve instead of failing silently.
    fn file(&self, project_id: String, file_id: String) -> ProviderFuture<ResourceFile> {
        let _ = (project_id, file_id);
        Box::pin(async {
            Err(crate::core::error::Error::Download(
                "this source cannot resolve a file by id".into(),
            ))
        })
    }
}
