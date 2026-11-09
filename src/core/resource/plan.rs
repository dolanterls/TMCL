//! What installing a project would put on disk.
//!
//! The plan is a value, not an action: `--dry-run` prints it, a real install
//! submits one `ResourceInstall` task per file. Building it in one place means a
//! dry run and the real run cannot disagree about which file or which
//! destination was chosen.

use std::path::PathBuf;

use crate::core::error::{Error, Result};
use crate::core::instance::models::InstanceConfig;
use crate::core::resource::compatibility::{resource_destination, target_context_for};
use crate::core::resource::dependencies::resolve_required;
use crate::core::resource::model::{ResourceFile, ResourceKind, ResourceProject};
use crate::core::resource::select::{choose_file, rejection_summary};
use crate::core::resource::service::{ResourceService, ResourceServiceRequest};

/// One file the install would write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub file: ResourceFile,
    pub destination: PathBuf,
    /// True when this file was pulled in because the primary one requires it.
    pub dependency: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub project: ResourceProject,
    pub files: Vec<PlannedFile>,
    /// Dependencies the resolver could not satisfy, `project id: reason`.
    pub unresolved: Vec<String>,
}

impl InstallPlan {
    pub fn total_size(&self) -> u64 {
        self.files.iter().filter_map(|item| item.file.size).sum()
    }
}

/// Asks the service what `project_id` would install, without touching disk.
///
/// `file_id` pins one version; without it the newest file the instance can run is
/// taken (see `select::choose_file`). `with_dependencies` resolves one level of
/// required dependencies, the same depth the workspace uses, because transitive
/// expansion drags in mods the author never intended.
pub async fn plan(
    service: &ResourceService,
    request: ResourceServiceRequest,
    kind: ResourceKind,
    project_id: &str,
    instance: &InstanceConfig,
    file_id: Option<&str>,
    with_dependencies: bool,
) -> Result<InstallPlan> {
    let directory = kind
        .install_directory()
        .ok_or_else(|| Error::Download(format!("{} is not a direct install", kind.label())))?;
    let project = service
        .project(request, kind, project_id.to_owned())
        .await?;
    let files = service.files(request, kind, project_id.to_owned()).await?;
    let target = target_context_for(instance, kind);

    let chosen = match file_id {
        Some(id) => files
            .iter()
            .find(|file| file.file_id == id)
            .ok_or_else(|| {
                Error::Download(format!(
                    "{project_id} has no file {id}; `tmcl project {project_id}` lists them"
                ))
            })?,
        None => choose_file(&files, Some(&target)).ok_or_else(|| {
            Error::Download(format!(
                "nothing in {project_id} can be installed here: {}",
                rejection_summary(&files, &target)
            ))
        })?,
    };

    let mut planned = vec![PlannedFile {
        destination: destination_for(instance, directory, chosen)?,
        file: on_disk(chosen),
        dependency: false,
    }];

    let mut unresolved = Vec::new();
    if with_dependencies {
        let provider = service.provider(request.source, kind)?;
        let resolution = resolve_required(chosen, &target, provider.as_ref()).await;
        unresolved = resolution.unresolved;
        for file in resolution.files {
            planned.push(PlannedFile {
                destination: destination_for(instance, directory, &file)?,
                file: on_disk(&file),
                dependency: true,
            });
        }
    }

    Ok(InstallPlan {
        project,
        files: planned,
        unresolved,
    })
}

fn destination_for(
    instance: &InstanceConfig,
    directory: &str,
    file: &ResourceFile,
) -> Result<PathBuf> {
    resource_destination(instance, directory, &file.name).map_err(Error::Download)
}

/// The file as it will be named on disk.
///
/// Modrinth keeps a publisher's Minecraft colour codes in the file name; the
/// destination already drops them (`resource_destination`), and the plan has to
/// agree with it or the printed name and the written path would differ.
fn on_disk(file: &ResourceFile) -> ResourceFile {
    let mut file = file.clone();
    file.name = crate::core::resource::conflict::sanitize_file_name(&file.name);
    file
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::plan;
    use crate::core::error::Error;
    use crate::core::instance::models::InstanceConfig;
    use crate::core::resource::model::{
        ReleaseChannel, ResourceFile, ResourceKind, ResourceProject, ResourceSource,
    };
    use crate::core::resource::provider::{ProviderCapabilities, ProviderFuture, ResourceProvider};
    use crate::core::resource::service::{ProviderRegistry, ResourceService};

    /// One project whose files are whatever the test says.
    struct FakeProvider {
        files: Vec<ResourceFile>,
        searches: Arc<Mutex<usize>>,
    }

    impl ResourceProvider for FakeProvider {
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                source: ResourceSource::Modrinth,
                available: true,
                kinds: ResourceKind::ALL.to_vec(),
                reason: None,
            }
        }

        fn search(
            &self,
            _query: crate::core::resource::model::ResourceQuery,
        ) -> ProviderFuture<crate::core::resource::model::ResourceSearchPage> {
            *self.searches.lock().expect("searches") += 1;
            Box::pin(async {
                Ok(crate::core::resource::model::ResourceSearchPage {
                    hits: Vec::new(),
                    total: 0,
                    page: 0,
                    page_size: 10,
                })
            })
        }

        fn categories(
            &self,
            _kind: ResourceKind,
        ) -> ProviderFuture<Vec<crate::core::resource::model::ResourceCategory>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn project(&self, project_id: String) -> ProviderFuture<ResourceProject> {
            Box::pin(async move {
                Ok(ResourceProject {
                    source: ResourceSource::Modrinth,
                    project_id,
                    title: "Sodium".into(),
                    author: "jellysquid".into(),
                    description: "Rendering".into(),
                    icon_url: None,
                    categories: Vec::new(),
                    game_versions: vec!["1.21.1".into()],
                    loaders: vec!["fabric".into()],
                    downloads: Some(1),
                    updated_at: None,
                })
            })
        }

        fn files(&self, _project_id: String) -> ProviderFuture<Vec<ResourceFile>> {
            let files = self.files.clone();
            Box::pin(async move { Ok(files) })
        }
    }

    fn file(name: &str, versions: &[&str], loaders: &[&str]) -> ResourceFile {
        ResourceFile {
            source: ResourceSource::Modrinth,
            project_id: "sodium".into(),
            file_id: name.into(),
            name: name.into(),
            download_url: format!("http://example/{name}"),
            sha1: None,
            size: Some(1024),
            game_versions: versions.iter().map(|value| (*value).to_owned()).collect(),
            loaders: loaders.iter().map(|value| (*value).to_owned()).collect(),
            channel: ReleaseChannel::Release,
            dependencies: Vec::new(),
        }
    }

    fn instance() -> InstanceConfig {
        let mut config =
            InstanceConfig::new("C:/game/versions/demo", uuid::Uuid::nil(), "demo", "1.21.1");
        config.loader = crate::core::instance::models::InstanceLoader::Fabric {
            version: "0.16.5".into(),
        };
        config
    }

    fn service(files: Vec<ResourceFile>) -> (ResourceService, Arc<Mutex<usize>>) {
        let searches = Arc::new(Mutex::new(0));
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(FakeProvider {
            files,
            searches: searches.clone(),
        }));
        (ResourceService::new(registry), searches)
    }

    #[tokio::test]
    async fn the_plan_names_a_destination_inside_the_instance() {
        let (service, _) = service(vec![file("sodium.jar", &["1.21.1"], &["fabric"])]);
        let request = service.begin(ResourceSource::Modrinth);

        let plan = plan(
            &service,
            request,
            ResourceKind::Mod,
            "sodium",
            &instance(),
            None,
            false,
        )
        .await
        .expect("plan");

        assert_eq!(plan.files.len(), 1);
        assert_eq!(
            plan.files[0].destination,
            std::path::PathBuf::from("C:/game/versions/demo/mods/sodium.jar")
        );
        assert!(!plan.files[0].dependency);
        assert_eq!(plan.total_size(), 1024);
    }

    #[tokio::test]
    async fn an_incompatible_project_is_an_error_not_a_guess() {
        let (service, _) = service(vec![file("old.jar", &["1.20.1"], &["forge"])]);
        let request = service.begin(ResourceSource::Modrinth);

        let error = plan(
            &service,
            request,
            ResourceKind::Mod,
            "sodium",
            &instance(),
            None,
            false,
        )
        .await
        .expect_err("no compatible file");

        let message = error.to_string();
        assert!(message.contains("1.20.1"), "{message}");
        assert!(message.contains("forge"), "{message}");
    }

    #[tokio::test]
    async fn an_explicit_file_id_is_honoured_even_when_it_is_older() {
        let files = vec![
            file("new.jar", &["1.21.1"], &["fabric"]),
            file("old.jar", &["1.20.1"], &["fabric"]),
        ];
        let (service, _) = service(files);
        let request = service.begin(ResourceSource::Modrinth);

        let plan = plan(
            &service,
            request,
            ResourceKind::Mod,
            "sodium",
            &instance(),
            Some("old.jar"),
            false,
        )
        .await
        .expect("plan");

        assert_eq!(plan.files[0].file.name, "old.jar");
    }

    #[tokio::test]
    async fn an_unknown_file_id_says_so() {
        let (service, _) = service(vec![file("new.jar", &["1.21.1"], &["fabric"])]);
        let request = service.begin(ResourceSource::Modrinth);

        let error = plan(
            &service,
            request,
            ResourceKind::Mod,
            "sodium",
            &instance(),
            Some("nope.jar"),
            false,
        )
        .await
        .expect_err("unknown file");

        assert!(
            error.to_string().contains("has no file nope.jar"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn a_kind_that_is_not_a_direct_install_is_refused() {
        let (service, _) = service(Vec::new());
        let request = service.begin(ResourceSource::Modrinth);

        let error = plan(
            &service,
            request,
            ResourceKind::Modpack,
            "pack",
            &instance(),
            None,
            false,
        )
        .await
        .expect_err("modpacks install instances");

        assert!(matches!(error, Error::Download(_)), "{error}");
    }

    #[tokio::test]
    async fn a_planned_file_is_named_the_way_it_lands_on_disk() {
        // Publishers sometimes leave their in-game colour codes in the file
        // name; the plan must report the cleaned name and path, otherwise the
        // printed "name -> path" pair contradicts itself.
        let (service, _) = service(vec![file(
            "\u{a7}7Stay True\u{a7}0 1.21.1.zip",
            &["1.21.1"],
            &[],
        )]);
        let request = service.begin(ResourceSource::Modrinth);

        let plan = plan(
            &service,
            request,
            ResourceKind::ResourcePack,
            "stay-true",
            &instance(),
            None,
            false,
        )
        .await
        .expect("plan");

        assert_eq!(plan.files[0].file.name, "Stay True 1.21.1.zip");
        assert_eq!(
            plan.files[0].destination,
            std::path::PathBuf::from("C:/game/versions/demo/resourcepacks/Stay True 1.21.1.zip")
        );
    }
}
