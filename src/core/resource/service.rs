use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::error::{Error, Result};

use super::model::{
    ResourceCategory, ResourceFile, ResourceKind, ResourceProject, ResourceQuery,
    ResourceSearchPage, ResourceSource,
};
use super::provider::{ProviderCapabilities, ResourceProvider};

#[derive(Clone, Default)]
pub struct ProviderRegistry {
    providers: HashMap<ResourceSource, Arc<dyn ResourceProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn ResourceProvider>) {
        let source = provider.capabilities().source;
        self.providers.insert(source, provider);
    }

    pub fn provider(&self, source: ResourceSource) -> Option<Arc<dyn ResourceProvider>> {
        self.providers.get(&source).cloned()
    }

    pub fn capabilities(&self, kind: ResourceKind) -> Vec<ProviderCapabilities> {
        self.providers
            .values()
            .map(|provider| {
                let mut capabilities = provider.capabilities();
                if !capabilities.kinds.contains(&kind) {
                    capabilities.available = false;
                    capabilities.reason = Some(format!(
                        "{} does not support {}",
                        capabilities.source.label(),
                        kind.label()
                    ));
                }
                capabilities
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceServiceRequest {
    pub id: u64,
    pub source: ResourceSource,
}

pub struct ResourceService {
    registry: ProviderRegistry,
    next_request_id: AtomicU64,
}

impl ResourceService {
    pub fn new(registry: ProviderRegistry) -> Self {
        Self {
            registry,
            next_request_id: AtomicU64::new(1),
        }
    }

    pub fn begin(&self, source: ResourceSource) -> ResourceServiceRequest {
        ResourceServiceRequest {
            id: self.next_request_id.fetch_add(1, Ordering::Relaxed),
            source,
        }
    }

    pub fn capabilities(&self, kind: ResourceKind) -> Vec<ProviderCapabilities> {
        self.registry.capabilities(kind)
    }

    pub async fn search(
        &self,
        request: ResourceServiceRequest,
        query: ResourceQuery,
    ) -> Result<ResourceSearchPage> {
        self.provider(request.source, query.kind)?
            .search(query)
            .await
    }

    /// Category list for one source and kind, validated exactly like `search`.
    pub async fn categories(
        &self,
        request: ResourceServiceRequest,
        kind: ResourceKind,
    ) -> Result<Vec<ResourceCategory>> {
        self.provider(request.source, kind)?.categories(kind).await
    }

    pub async fn project(
        &self,
        request: ResourceServiceRequest,
        kind: ResourceKind,
        project_id: String,
    ) -> Result<ResourceProject> {
        self.provider(request.source, kind)?
            .project(project_id)
            .await
    }

    pub async fn files(
        &self,
        request: ResourceServiceRequest,
        kind: ResourceKind,
        project_id: String,
    ) -> Result<Vec<ResourceFile>> {
        self.provider(request.source, kind)?.files(project_id).await
    }

    /// The provider behind one source, validated for `kind`.
    ///
    /// Exposed so a caller can run a second query against the same provider —
    /// dependency resolution needs it, and building a fresh registry per
    /// dependency would lose the availability check that produced the error.
    pub fn provider(
        &self,
        source: ResourceSource,
        kind: ResourceKind,
    ) -> Result<Arc<dyn ResourceProvider>> {
        let provider = self.registry.provider(source).ok_or_else(|| {
            Error::Download(format!("{} provider is unavailable", source.label()))
        })?;
        let capabilities = provider.capabilities();
        if !capabilities.available {
            return Err(Error::Download(capabilities.reason.unwrap_or_else(|| {
                format!("{} provider is unavailable", source.label())
            })));
        }
        if !capabilities.kinds.contains(&kind) {
            return Err(Error::Download(format!(
                "{} does not support {}",
                source.label(),
                kind.label()
            )));
        }
        Ok(provider)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::core::resource::model::{RESOURCE_PAGE_SIZE, ResourceProject, ResourceSource};
    use crate::core::resource::provider::{ProviderFuture, ResourceProvider};

    struct FakeProvider {
        calls: Arc<Mutex<Vec<ResourceSource>>>,
        kinds: Vec<ResourceKind>,
    }
    impl ResourceProvider for FakeProvider {
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                source: ResourceSource::Modrinth,
                available: true,
                kinds: self.kinds.clone(),
                reason: None,
            }
        }
        fn search(&self, query: ResourceQuery) -> ProviderFuture<ResourceSearchPage> {
            self.calls.lock().expect("calls").push(query.source);
            Box::pin(async {
                Ok(ResourceSearchPage {
                    hits: Vec::new(),
                    total: 0,
                    page: 0,
                    page_size: RESOURCE_PAGE_SIZE,
                })
            })
        }
        fn categories(
            &self,
            kind: ResourceKind,
        ) -> ProviderFuture<Vec<crate::core::resource::model::ResourceCategory>> {
            Box::pin(async move {
                Ok(vec![crate::core::resource::model::ResourceCategory {
                    id: "adventure".into(),
                    name: "Adventure".into(),
                    kind,
                }])
            })
        }
        fn project(&self, _project_id: String) -> ProviderFuture<ResourceProject> {
            Box::pin(async { Err(Error::Download("not implemented".into())) })
        }
        fn files(&self, _project_id: String) -> ProviderFuture<Vec<ResourceFile>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    #[tokio::test]
    async fn service_assigns_monotonic_request_ids_and_calls_one_source() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(FakeProvider {
            calls: calls.clone(),
            kinds: ResourceKind::ALL.to_vec(),
        }));
        let service = ResourceService::new(registry);
        let first = service.begin(ResourceSource::Modrinth);
        let second = service.begin(ResourceSource::Modrinth);
        assert!(second.id > first.id);
        let _ = service
            .search(
                first,
                ResourceQuery::new(ResourceSource::Modrinth, ResourceKind::Mod),
            )
            .await
            .expect("search");
        assert_eq!(
            calls.lock().expect("calls").as_slice(),
            &[ResourceSource::Modrinth]
        );
    }

    #[tokio::test]
    async fn categories_are_forwarded_for_the_requested_kind() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(FakeProvider {
            calls: calls.clone(),
            kinds: ResourceKind::ALL.to_vec(),
        }));
        let service = ResourceService::new(registry);
        let request = service.begin(ResourceSource::Modrinth);
        let categories = service
            .categories(request, ResourceKind::Modpack)
            .await
            .expect("categories");
        assert_eq!(categories.len(), 1);
        assert_eq!(categories[0].id, "adventure");
        assert_eq!(categories[0].kind, ResourceKind::Modpack);
    }

    #[tokio::test]
    async fn categories_reject_a_kind_the_provider_does_not_serve() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(FakeProvider {
            calls,
            kinds: vec![ResourceKind::Mod],
        }));
        let service = ResourceService::new(registry);
        let request = service.begin(ResourceSource::Modrinth);
        assert!(
            service
                .categories(request, ResourceKind::World)
                .await
                .is_err()
        );
    }
}
