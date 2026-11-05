//! Resolve a mod's required dependencies into installable files.
//!
//! Modrinth versions carry both a project id and (usually) a version id;
//! CurseForge files carry only project ids. The resolver prefers the exact
//! version when it matches the target instance, and otherwise searches the
//! project's recent files for one that does. Installing the wrong version of a
//! dependency is worse than installing none: it turns a missing-mod crash into
//! a mixin crash.
//!
//! One level is resolved on purpose. Transitive expansion multiplies API calls
//! and can drag in mods the pack author never intended; the log keeps saying
//! which dependencies were skipped so the next round is explicit.

use std::collections::HashSet;

use futures_util::StreamExt;

use crate::core::resource::compatibility::{TargetContext, file_compatibility};
use crate::core::resource::model::{Compatibility, DependencyKind, ResourceFile};
use crate::core::resource::provider::ResourceProvider;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DependencyResolution {
    pub files: Vec<ResourceFile>,
    /// `project id: reason`, capped by the caller for display.
    pub unresolved: Vec<String>,
}

pub async fn resolve_required(
    parent: &ResourceFile,
    target: &TargetContext,
    provider: &dyn ResourceProvider,
) -> DependencyResolution {
    let mut seen = HashSet::new();
    let dependencies = parent
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind == DependencyKind::Required)
        .filter_map(|dependency| {
            dependency
                .project_id
                .clone()
                .map(|project| (project, dependency.file_id.clone()))
        })
        .filter(|(project, _)| *project != parent.project_id && seen.insert(project.clone()))
        .collect::<Vec<_>>();

    let mut resolution = DependencyResolution::default();
    let mut stream = futures_util::stream::iter(
        dependencies
            .into_iter()
            .map(|(project, file)| resolve_one(project, file, target, provider)),
    )
    .buffer_unordered(4);
    while let Some(result) = stream.next().await {
        match result {
            Ok(file) => resolution.files.push(file),
            Err(reason) => resolution.unresolved.push(reason),
        }
    }
    resolution
}

async fn resolve_one(
    project: String,
    exact_file: Option<String>,
    target: &TargetContext,
    provider: &dyn ResourceProvider,
) -> std::result::Result<ResourceFile, String> {
    if let Some(file_id) = exact_file
        && let Ok(file) = provider.file(project.clone(), file_id).await
        && compatible(&file, target)
    {
        return Ok(file);
    }
    let files = provider
        .files(project.clone())
        .await
        .map_err(|error| format!("{project}: {error}"))?;
    files
        .into_iter()
        .find(|file| compatible(file, target))
        .ok_or_else(|| format!("{project}: no file for this game version and loader"))
}

fn compatible(file: &ResourceFile, target: &TargetContext) -> bool {
    file_compatibility(file, Some(target)) == Compatibility::Compatible
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::resolve_required;
    use crate::core::resource::compatibility::TargetContext;
    use crate::core::resource::model::{
        DependencyKind, LoaderFilter, ReleaseChannel, ResourceDependency, ResourceFile,
        ResourceProject, ResourceQuery, ResourceSearchPage, ResourceSource,
    };
    use crate::core::resource::provider::{ProviderCapabilities, ProviderFuture, ResourceProvider};

    fn file(name: &str, version: &str, loader: &str) -> ResourceFile {
        ResourceFile {
            source: ResourceSource::Modrinth,
            project_id: format!("project-{name}"),
            file_id: format!("file-{name}"),
            name: name.into(),
            download_url: format!("https://cdn.example/{name}"),
            sha1: None,
            size: None,
            game_versions: if version.is_empty() {
                Vec::new()
            } else {
                vec![version.into()]
            },
            loaders: if loader.is_empty() {
                Vec::new()
            } else {
                vec![loader.into()]
            },
            channel: ReleaseChannel::Release,
            dependencies: Vec::new(),
        }
    }

    fn parent(dependencies: Vec<(&str, Option<&str>, DependencyKind)>) -> ResourceFile {
        let mut value = file("parent.jar", "1.20.1", "fabric");
        value.dependencies = dependencies
            .into_iter()
            .map(|(project, file, kind)| ResourceDependency {
                project_id: Some(project.into()),
                file_id: file.map(str::to_owned),
                name: project.into(),
                kind,
            })
            .collect();
        value
    }

    fn target() -> TargetContext {
        TargetContext {
            minecraft_version: "1.20.1".into(),
            loader: LoaderFilter::Fabric,
            root: std::env::temp_dir(),
        }
    }

    struct FakeProvider {
        exact: HashMap<(String, String), ResourceFile>,
        lists: HashMap<String, Vec<ResourceFile>>,
    }

    impl ResourceProvider for FakeProvider {
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                source: ResourceSource::Modrinth,
                available: true,
                kinds: vec![crate::core::resource::model::ResourceKind::Mod],
                reason: None,
            }
        }
        fn search(&self, _query: ResourceQuery) -> ProviderFuture<ResourceSearchPage> {
            Box::pin(async { Err(crate::core::error::Error::Download("unused".into())) })
        }
        fn categories(
            &self,
            _kind: crate::core::resource::model::ResourceKind,
        ) -> ProviderFuture<Vec<crate::core::resource::model::ResourceCategory>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn project(&self, _project_id: String) -> ProviderFuture<ResourceProject> {
            Box::pin(async { Err(crate::core::error::Error::Download("unused".into())) })
        }
        fn files(&self, project_id: String) -> ProviderFuture<Vec<ResourceFile>> {
            let value = self.lists.get(&project_id).cloned().unwrap_or_default();
            Box::pin(async move { Ok(value) })
        }
        fn file(&self, project_id: String, file_id: String) -> ProviderFuture<ResourceFile> {
            let value = self.exact.get(&(project_id, file_id)).cloned();
            Box::pin(async move {
                value.ok_or_else(|| crate::core::error::Error::Download("no such file".into()))
            })
        }
    }

    #[tokio::test]
    async fn the_exact_version_wins_when_it_fits_the_instance() {
        let mut exact = HashMap::new();
        exact.insert(
            ("fabric-api".to_string(), "v1".to_string()),
            file("fabric-api.jar", "1.20.1", "fabric"),
        );
        let provider = FakeProvider {
            exact,
            lists: HashMap::new(),
        };
        let resolution = resolve_required(
            &parent(vec![("fabric-api", Some("v1"), DependencyKind::Required)]),
            &target(),
            &provider,
        )
        .await;
        assert!(
            resolution.unresolved.is_empty(),
            "{:?}",
            resolution.unresolved
        );
        assert_eq!(resolution.files.len(), 1);
        assert_eq!(resolution.files[0].name, "fabric-api.jar");
    }

    #[tokio::test]
    async fn an_incompatible_exact_version_falls_back_to_the_project_list() {
        let mut exact = HashMap::new();
        exact.insert(
            ("fabric-api".to_string(), "old".to_string()),
            file("fabric-api-old.jar", "1.19.2", "fabric"),
        );
        let mut lists = HashMap::new();
        lists.insert(
            "fabric-api".to_string(),
            vec![
                file("fabric-api-wrong.jar", "1.19.2", "fabric"),
                file("fabric-api-good.jar", "1.20.1", "fabric"),
            ],
        );
        let provider = FakeProvider { exact, lists };
        let resolution = resolve_required(
            &parent(vec![("fabric-api", Some("old"), DependencyKind::Required)]),
            &target(),
            &provider,
        )
        .await;
        assert_eq!(resolution.files.len(), 1);
        assert_eq!(resolution.files[0].name, "fabric-api-good.jar");
    }

    #[tokio::test]
    async fn a_missing_compatible_file_is_reported_not_installed() {
        let mut lists = HashMap::new();
        lists.insert(
            "cloth-config".to_string(),
            vec![file("cloth-config-forge.jar", "1.20.1", "forge")],
        );
        let provider = FakeProvider {
            exact: HashMap::new(),
            lists,
        };
        let resolution = resolve_required(
            &parent(vec![("cloth-config", None, DependencyKind::Required)]),
            &target(),
            &provider,
        )
        .await;
        assert!(resolution.files.is_empty());
        assert_eq!(resolution.unresolved.len(), 1);
        assert!(
            resolution.unresolved[0].contains("cloth-config"),
            "{:?}",
            resolution.unresolved
        );
    }

    #[tokio::test]
    async fn optional_and_duplicate_dependencies_are_not_installed() {
        let mut lists = HashMap::new();
        lists.insert(
            "sodium".to_string(),
            vec![file("sodium.jar", "1.20.1", "fabric")],
        );
        let provider = FakeProvider {
            exact: HashMap::new(),
            lists,
        };
        let resolution = resolve_required(
            &parent(vec![
                ("sodium", None, DependencyKind::Required),
                ("sodium", None, DependencyKind::Required),
                ("iris", None, DependencyKind::Optional),
                ("parent.jar", None, DependencyKind::Required),
            ]),
            &target(),
            &provider,
        )
        .await;
        assert_eq!(resolution.files.len(), 1);
        assert_eq!(resolution.files[0].name, "sodium.jar");
    }
}
