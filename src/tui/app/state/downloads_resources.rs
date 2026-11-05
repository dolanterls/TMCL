//! Split out of the parent module during P7 acceptance.

use super::*;

impl App {
    pub(in crate::tui::app) fn spawn_resource_search(&mut self) {
        let Some(kind) = self.download_center.category.resource_kind() else {
            return;
        };
        self.download_center.request_id = self.download_center.request_id.saturating_add(1);
        let request_id = self.download_center.request_id;
        let source = self.download_center.source;
        let search = self.download_center.search.clone();
        let categories = self.download_center.categories.clone();
        let sort = self.download_center.sort;
        let page = self.download_center.page;
        // Only a category that installs into an existing instance may narrow
        // its search by that instance's version and loader.
        let target = if self.download_center.category.targets_instance() {
            self.download_target_id
                .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
                .cloned()
        } else {
            None
        };
        let target_context = target.and_then(|target| {
            let instance: InstanceConfig = read_json(&target.path.join("tmcl.json")).ok()?;
            Some((
                instance.minecraft_version,
                crate::core::resource::compatibility::loader_filter(&instance.loader),
            ))
        });
        let (game_version, loader) = resource_search_context(kind, target_context);
        self.download_center.loading = true;
        self.download_center.error = None;
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let result = async {
                let client = Arc::new(HttpClient::new()?);
                let service =
                    crate::core::resource::ResourceService::new(resource_registry(client));
                let request = service.begin(source);
                service
                    .search(
                        request,
                        crate::core::resource::ResourceQuery {
                            source,
                            kind,
                            game_version,
                            loader,
                            search,
                            categories,
                            sort,
                            page,
                            page_size: RESOURCE_PAGE_SIZE,
                        },
                    )
                    .await
            }
            .await;
            match result {
                Ok(result) => {
                    let _ = sender.send(AppEvent::ResourceSearchLoaded {
                        request_id,
                        source,
                        result,
                    });
                }
                Err(error) => {
                    let _ = sender.send(AppEvent::ResourceSearchFailed {
                        request_id,
                        source,
                        error: error.to_string(),
                    });
                }
            }
        });
    }

    pub(in crate::tui::app) fn apply_resource_search(
        &mut self,
        request_id: u64,
        source: ResourceSource,
        result: ResourceSearchPage,
    ) {
        if request_id != self.download_center.request_id || source != self.download_center.source {
            return;
        }
        self.resource_projects = result.hits;
        self.resource_files.clear();
        self.resource_detail = None;
        self.resource_detail_project = None;
        self.download_center.selected = 0;
        self.reset_detail_scroll();
        self.download_center.total = result.total;
        self.download_center.loading = false;
        let page_count = resource_page_count(result.total, result.page_size);
        self.status = Status::info(self.i18n.format(
            "status.resource_projects_loaded",
            &[
                ("count", &self.resource_projects.len().to_string()),
                ("total", &result.total.to_string()),
                ("page", &result.page.saturating_add(1).to_string()),
                ("pages", &page_count.to_string()),
            ],
        ));
    }

    pub(in crate::tui::app) fn resource_target_context(
        &self,
    ) -> Option<crate::core::resource::TargetContext> {
        // Modpacks install into an instance created by the pack itself, so
        // there is nothing to match a file against. Returning a context here
        // would paint every pack version as compatible or incompatible with an
        // instance that has nothing to do with it.
        if !self.download_center.category.targets_instance() {
            return None;
        }
        let target_id = self.download_target_id?;
        let target = self
            .instances
            .items
            .iter()
            .find(|item| item.id == target_id)?;
        let mut instance: InstanceConfig = read_json(&target.path.join("tmcl.json")).ok()?;
        instance.path = target.path.clone();
        Some(crate::core::resource::compatibility::target_context(
            &instance,
        ))
    }

    pub(in crate::tui::app) fn open_resource_project(&mut self, project: ResourceProject) {
        let Some(kind) = self.download_center.category.resource_kind() else {
            return;
        };
        self.resource_detail = Some(project.project_id.clone());
        self.resource_detail_project = Some(project.clone());
        self.resource_files.clear();
        self.download_center.selected = 0;
        self.reset_detail_scroll();
        self.download_center.request_id = self.download_center.request_id.saturating_add(1);
        let request_id = self.download_center.request_id;
        let source = self.download_center.source;
        let project_id = project.project_id;
        self.download_center.loading = true;
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let result = async {
                let client = Arc::new(HttpClient::new()?);
                let service =
                    crate::core::resource::ResourceService::new(resource_registry(client));
                service
                    .files(service.begin(source), kind, project_id.clone())
                    .await
            }
            .await;
            match result {
                Ok(files) => {
                    let _ = sender.send(AppEvent::ResourceFilesLoaded {
                        request_id,
                        source,
                        project_id,
                        files,
                    });
                }
                Err(error) => {
                    let _ = sender.send(AppEvent::ResourceFilesFailed {
                        request_id,
                        source,
                        error: error.to_string(),
                    });
                }
            }
        });
    }

    pub(in crate::tui::app) fn open_download_target_picker(&mut self) {
        // Versions and modpacks create the instance they install into, and a
        // world is saved wherever the player points the dialog. None of them
        // has a target to pick.
        match self.download_center.category.destination() {
            DownloadDestination::Instance => {}
            DownloadDestination::NewInstance => {
                self.status = Status::info(self.i18n.text("status.target_not_needed_new_instance"));
                return;
            }
            DownloadDestination::SavePath => {
                self.status = Status::info(self.i18n.text("status.target_not_needed_save_path"));
                return;
            }
        }
        if self.instances.items.is_empty() {
            self.status = Status::warning(self.i18n.text("status.create_target_first"));
            return;
        }
        let values = self
            .instances
            .items
            .iter()
            .map(|item| format!("{}|{}", item.id, item.name))
            .collect::<Vec<_>>();
        let selected = self
            .download_target_id
            .and_then(|id| self.instances.items.iter().position(|item| item.id == id))
            .unwrap_or(0);
        let mut modal = Modal::form(
            self.i18n.text("form.download_target"),
            &[self.i18n.text("form.target_instance_id")],
            &[values[selected].clone()],
            FormAction::SelectResourceTarget,
        )
        .with_selection_picker(0, values);
        if let Modal::Form { values, .. } = &mut modal {
            values[0] = self
                .instances
                .items
                .get(selected)
                .map(|item| format!("{}|{}", item.id, item.name))
                .unwrap_or_default();
        }
        self.modal = Some(modal);
    }

    pub(in crate::tui::app) fn submit_selected_resource_task(&mut self) {
        let Some(kind) = self.download_center.category.resource_kind() else {
            return;
        };
        let Some(file) = self
            .resource_files
            .get(self.download_center.selected)
            .cloned()
        else {
            self.status = Status::warning(self.i18n.text("status.select_file"));
            return;
        };
        if kind == ResourceKind::Modpack {
            self.open_modpack_setup_form(file);
            return;
        }
        if kind == ResourceKind::World {
            let initial = self
                .config
                .game_directories
                .first()
                .cloned()
                .unwrap_or_else(|| self.paths.root.clone());
            self.pending_world_file = Some(file.clone());
            self.save_file_browse_request = Some((initial, file.name.clone()));
            self.status = Status::info(self.i18n.text("status.save_file_target"));
            return;
        }
        self.open_resource_install_form(file);
    }

    pub(in crate::tui::app) fn open_resource_install_form(&mut self, file: ResourceFile) {
        let Some(target_id) = self.download_target_id else {
            self.status = Status::warning(self.i18n.text("status.target_first"));
            return;
        };
        let Some(target) = self
            .instances
            .items
            .iter()
            .find(|item| item.id == target_id)
        else {
            self.status = Status::warning(self.i18n.text("status.target_missing"));
            return;
        };
        let required = file
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.kind == crate::core::resource::model::DependencyKind::Required
            })
            .count();
        self.pending_resource_file = Some(file.clone());
        self.modal = Some(Modal::form(
            self.i18n.text("form.confirm_resource"),
            &[
                self.i18n.text("form.file_name"),
                self.i18n.text("form.target_instance_id"),
            ],
            &[file.name, target.name.clone()],
            FormAction::InstallResource,
        ));
        if required > 0 {
            self.status = Status::info(self.i18n.format(
                "status.dependencies_will_install",
                &[("count", &required.to_string())],
            ));
        }
    }

    pub(in crate::tui::app) fn queue_resource_install(
        &mut self,
        file: ResourceFile,
        file_name: String,
    ) -> Result<Uuid> {
        let kind = self
            .download_center
            .category
            .resource_kind()
            .ok_or_else(|| Error::Download("this resource type is not a direct install".into()))?;
        let target_id = self
            .download_target_id
            .ok_or_else(|| Error::Config("choose a target instance first".into()))?;
        self.queue_resource_install_on(file, file_name, target_id, kind)
    }

    pub(in crate::tui::app) fn queue_resource_install_on(
        &mut self,
        file: ResourceFile,
        file_name: String,
        target_id: Uuid,
        kind: ResourceKind,
    ) -> Result<Uuid> {
        // The queued task must name the same file the destination does; the
        // cleanup of Minecraft colour codes lives in the core so both agree.
        let file_name = crate::core::resource::conflict::sanitize_file_name(&file_name);
        crate::core::resource::conflict::validate_file_name(&file_name).map_err(Error::Download)?;
        let (instance, destination) = self.install_target(target_id, kind, &file_name)?;
        crate::core::resource::conflict::check_available(&destination).map_err(Error::Download)?;
        self.task_engine
            .submit(TaskOperation::ResourceInstall(ResourceInstallSpec {
                instance_id: target_id,
                instance_root: crate::core::resource::compatibility::content_root(&instance),
                destination,
                cache_root: self.paths.cache_dir.clone(),
                source: file.source,
                kind,
                project_id: file.project_id,
                file_id: file.file_id,
                file_name,
                candidate_urls: vec![file.download_url],
                expected_sha1: file.sha1,
                expected_size: file.size,
                download_snapshot: self.download_snapshot(),
            }))
    }

    /// Instance and destination a resource would be written to.
    ///
    /// Shared by the install queue and the dependency pass, which has to know
    /// whether a dependency is already present before creating a task that
    /// would then fail its own collision check.
    fn install_target(
        &self,
        target_id: Uuid,
        kind: ResourceKind,
        file_name: &str,
    ) -> Result<(InstanceConfig, PathBuf)> {
        let target = self
            .instances
            .items
            .iter()
            .find(|item| item.id == target_id)
            .cloned()
            .ok_or_else(|| Error::Config("selected target instance no longer exists".into()))?;
        let mut instance: InstanceConfig = read_json(&target.path.join("tmcl.json"))?;
        instance.path = target.path.clone();
        // The instance must still belong to a registered game directory; the
        // path itself is no longer needed to resolve the content root.
        if !self
            .config
            .game_directories
            .iter()
            .any(|path| game_directory_id(path) == instance.game_directory_id)
        {
            return Err(Error::Config(
                "target game directory is not registered".into(),
            ));
        }
        let directory = kind
            .install_directory()
            .ok_or_else(|| Error::Download("this resource type is not a direct install".into()))?;
        let destination = crate::core::resource::compatibility::resource_destination(
            &instance, directory, file_name,
        )
        .map_err(Error::Download)?;
        Ok((instance, destination))
    }

    pub(in crate::tui::app) fn open_modpack_setup_form(&mut self, file: ResourceFile) {
        let staging_path =
            self.paths
                .cache_dir
                .join("modpacks")
                .join(format!("{}-{}", Uuid::new_v4(), file.name));
        let default_root = self
            .config
            .game_directories
            .first()
            .map(|path| display_path(path))
            .unwrap_or_default();
        let default_name = Path::new(&file.name)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("modpack")
            .to_owned();
        let modal = Modal::form(
            self.i18n.text("form.install_modpack"),
            &[
                self.i18n.text("form.modpack_download_path"),
                self.i18n.text("form.game_directory"),
                self.i18n.text("form.new_instance_name"),
            ],
            &[
                staging_path.display().to_string(),
                default_root,
                default_name,
            ],
            FormAction::InstallModpack,
        )
        .with_directory_picker(1);
        // The icon belongs to the project, not to the file, and it is only
        // reachable while its page is open — bind the two here.
        let icon_url = self
            .resource_detail_project
            .as_ref()
            .and_then(|project| project.icon_url.clone());
        self.pending_modpack_file = Some(crate::tui::app::PendingModpackFile { file, icon_url });
        self.modal = Some(modal);
        self.status = Status::info(self.i18n.text("form.modpack_hint"));
    }

    /// Fallback for a completed Modpack download whose pre-queued setup was
    /// lost (for example after an application restart). The archive is already
    /// on disk, so only its destination has to be confirmed.
    pub(in crate::tui::app) fn open_modpack_wizard(&mut self, spec: ModpackInstallSpec) {
        let default_root = self
            .config
            .game_directories
            .first()
            .map(|path| display_path(path))
            .unwrap_or_default();
        let default_name = Path::new(&spec.file_name)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("modpack")
            .to_owned();
        let modal = Modal::form(
            self.i18n.text("form.install_modpack"),
            &[
                self.i18n.text("form.modpack_download_path"),
                self.i18n.text("form.game_directory"),
                self.i18n.text("form.new_instance_name"),
            ],
            &[
                spec.staging_path.display().to_string(),
                default_root,
                default_name,
            ],
            FormAction::InstallModpack,
        )
        .with_directory_picker(1);
        self.pending_modpack_file = None;
        self.modal = Some(modal);
        self.status = Status::info(self.i18n.text("form.modpack_hint"));
    }

    pub(in crate::tui::app) fn finish_modpack_install(&mut self, spec: ModpackInstallSpec) {
        if self.pending_modpack.is_none() {
            self.open_modpack_wizard(spec);
            return;
        }
        if let Err(error) = self.spawn_pending_modpack(&spec.staging_path) {
            self.status = Status::error(error.to_string());
        }
    }

    /// Run metadata reading, instance creation and archive extraction off the
    /// UI thread; the prepared instance comes back through AppEvent.
    pub(in crate::tui::app) fn spawn_pending_modpack(&mut self, staging: &Path) -> Result<()> {
        let Some(setup) = self.pending_modpack.take() else {
            return Err(Error::Config("modpack setup is missing".into()));
        };
        let staging = staging.to_path_buf();
        let snapshot = self.download_snapshot();
        self.begin_modpack_install(setup, move |setup| async move {
            prepare_modpack_install(setup, staging, snapshot).await
        })
    }

    /// Import an archive the player picked from disk.
    ///
    /// The file is copied into the cache first: `prepare_modpack_install`
    /// deletes its staging archive after extracting, and that must never be a
    /// file the player chose themselves.
    pub(in crate::tui::app) fn spawn_local_modpack(&mut self, source: PathBuf) -> Result<()> {
        let Some(setup) = self.pending_modpack.take() else {
            return Err(Error::Config("modpack setup is missing".into()));
        };
        let staging = self.local_modpack_staging(&source);
        let snapshot = self.download_snapshot();
        self.begin_modpack_install(setup, move |setup| async move {
            prepare_local_modpack(setup, source, staging, snapshot).await
        })
    }

    /// Cache path a local archive is copied to before extraction.
    fn local_modpack_staging(&self, source: &Path) -> PathBuf {
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("modpack.zip");
        self.paths
            .cache_dir
            .join("modpacks")
            .join(format!("{}-{name}", Uuid::new_v4()))
    }

    /// Shared tail of every modpack install: remember the game directory, drop
    /// any exclusion for the new instance and run the heavy part off-thread.
    fn begin_modpack_install<F, Fut>(&mut self, setup: PendingModpack, prepare: F) -> Result<()>
    where
        F: FnOnce(PendingModpack) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<crate::tui::event::PreparedModpack>>
            + Send
            + 'static,
    {
        add_game_directory(&mut self.config, &setup.root);
        self.remove_exclusion(&setup.root.join("versions").join(&setup.name));
        self.save_config()?;
        let sender = self.sender.clone();
        self.status = Status::info(self.i18n.text("status.modpack_installing"));
        tokio::spawn(async move {
            let prepared = prepare(setup).await.map_err(|error| error.to_string());
            let _ = sender.send(AppEvent::ModpackPrepared(prepared));
        });
        Ok(())
    }

    /// Resolves a just-queued mod's required dependencies in the background.
    ///
    /// The parent task is already running; dependencies arrive as an event so
    /// the queue is touched on the UI thread, where the engine lives.
    pub(in crate::tui::app) fn spawn_dependency_resolution(
        &mut self,
        file: ResourceFile,
        target_id: Uuid,
    ) {
        let Some(target) = self
            .instances
            .items
            .iter()
            .find(|item| item.id == target_id)
            .cloned()
        else {
            return;
        };
        let Ok(mut instance) = read_json::<InstanceConfig>(&target.path.join("tmcl.json")) else {
            return;
        };
        instance.path = target.path.clone();
        let context = crate::core::resource::compatibility::target_context(&instance);
        let snapshot = self.download_snapshot();
        let sender = self.sender.clone();
        let parent = file.name.clone();
        tokio::spawn(async move {
            let result: Result<crate::core::resource::dependencies::DependencyResolution> = async {
                let client = std::sync::Arc::new(HttpClient::with_timeout(snapshot.timeout_secs)?);
                let registry = crate::tui::app::resource_registry(client);
                let provider = registry
                    .provider(file.source)
                    .ok_or_else(|| Error::Download(format!("no provider for {:?}", file.source)))?;
                Ok(crate::core::resource::dependencies::resolve_required(
                    &file,
                    &context,
                    provider.as_ref(),
                )
                .await)
            }
            .await;
            let (files, unresolved) = match result {
                Ok(resolution) => (resolution.files, resolution.unresolved),
                Err(error) => (Vec::new(), vec![error.to_string()]),
            };
            let _ = sender.send(AppEvent::DependenciesResolved {
                parent,
                target_id,
                files,
                unresolved,
            });
        });
    }

    pub(in crate::tui::app) fn apply_dependencies_resolved(
        &mut self,
        parent: String,
        target_id: Uuid,
        files: Vec<ResourceFile>,
        unresolved: Vec<String>,
    ) {
        let mut queued = 0usize;
        let mut present = 0usize;
        let mut problems = unresolved;
        for file in files {
            let name = file.name.clone();
            match self.install_target(target_id, ResourceKind::Mod, &name) {
                Ok((_, destination)) if destination.exists() => {
                    present += 1;
                    continue;
                }
                Ok(_) => {}
                Err(error) => {
                    problems.push(error.to_string());
                    continue;
                }
            }
            match self.queue_resource_install_on(file, name, target_id, ResourceKind::Mod) {
                Ok(_) => queued += 1,
                Err(error) => problems.push(error.to_string()),
            }
        }
        if !problems.is_empty() {
            let sample = problems
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            self.status = Status::warning(self.i18n.format(
                "status.dependencies_partial",
                &[
                    ("count", &queued.to_string()),
                    ("failed", &problems.len().to_string()),
                    ("sample", &sample),
                ],
            ));
        } else if queued > 0 {
            self.status = Status::info(self.i18n.format(
                "status.dependencies_queued",
                &[("count", &queued.to_string()), ("parent", &parent)],
            ));
        } else if present > 0 {
            self.status = Status::info(self.i18n.format(
                "status.dependencies_present",
                &[("count", &present.to_string())],
            ));
        }
        self.apply_task_snapshots(self.task_engine.snapshots());
    }

    /// `U`: ask for a newer pack archive and update the open instance.
    pub(in crate::tui::app) fn open_modpack_update_form(&mut self) {
        if self.workspace.is_none() {
            self.status = Status::warning(self.i18n.text("status.open_instance_first"));
            return;
        }
        self.modal = Some(Modal::prompt(
            self.i18n.text("form.update_modpack_title"),
            self.i18n.text("form.update_modpack_label"),
            self.i18n.text("form.update_modpack_help"),
            String::new(),
            crate::tui::modal::FormAction::UpdateModpack,
        ));
    }

    /// Reads the new pack, checks it belongs to this instance, then applies it
    /// off the UI thread.
    pub(in crate::tui::app) fn start_modpack_update(&mut self, archive: &str) -> Result<()> {
        let archive = PathBuf::from(archive.trim());
        if !archive.is_file() {
            return Err(Error::Download(format!(
                "modpack archive not found: {}",
                archive.display()
            )));
        }
        let Some(workspace) = self.workspace.as_ref() else {
            return Err(Error::Config("open an instance first".into()));
        };
        let instance_id = workspace.instance;
        let instance_root = workspace.root.clone();
        let Some(summary) = self
            .instances
            .items
            .iter()
            .find(|item| item.id == instance_id)
            .cloned()
        else {
            return Err(Error::Config(
                "the open instance is no longer listed".into(),
            ));
        };
        let mut instance: crate::core::instance::models::InstanceConfig =
            read_json(&summary.path.join("tmcl.json"))?;
        instance.path = summary.path.clone();
        let snapshot = self.download_snapshot();
        let cache_root = self.paths.cache_dir.clone();
        let sender = self.sender.clone();
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return Ok(());
        };
        self.status = Status::info(self.i18n.text("status.modpack_updating"));
        handle.spawn(async move {
            let result = async {
                let prepared = crate::core::resource::modpack_install::prepare_update(
                    crate::core::resource::modpack_install::UpdateRequest {
                        instance: instance.clone(),
                        archive: archive.clone(),
                    },
                    modpack_provider(&snapshot).as_deref(),
                )
                .await?;
                let version = prepared.version;
                let files = prepared.files;
                let staging =
                    crate::core::resource::modpack_update::staging_dir(&cache_root, instance_id);
                let root = instance_root.clone();
                let outcome = tokio::task::spawn_blocking(move || {
                    crate::core::resource::modpack_update::apply(
                        &root, &archive, &staging, &version, files,
                    )
                })
                .await
                .map_err(preparation_task_failed)??;
                Ok(outcome)
            }
            .await
            .map_err(|error: crate::core::error::Error| error.to_string());
            let _ = sender.send(crate::tui::event::AppEvent::ModpackUpdated {
                instance: instance_id,
                result,
            });
        });
        Ok(())
    }

    /// Queues the downloads the update still needs and reports what changed.
    pub(in crate::tui::app) fn apply_modpack_updated(
        &mut self,
        instance: uuid::Uuid,
        result: std::result::Result<crate::core::resource::modpack_update::UpdateOutcome, String>,
    ) {
        if self
            .workspace
            .as_ref()
            .is_none_or(|workspace| workspace.instance != instance)
        {
            return;
        }
        match result {
            Err(error) => self.status = Status::error(error),
            Ok(outcome) => {
                let Some(root) = self
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.root.clone())
                else {
                    return;
                };
                let count = outcome.files.len();
                if count > 0
                    && let Err(error) =
                        self.task_engine
                            .submit(crate::core::task::TaskOperation::ModpackFiles(
                                crate::core::task::ModpackFilesSpec {
                                    instance_id: instance,
                                    instance_root: root,
                                    cache_root: self.paths.cache_dir.clone(),
                                    files: outcome.files,
                                    download_snapshot: self.download_snapshot(),
                                },
                            ))
                {
                    self.status = Status::error(error.to_string());
                    return;
                }
                if count > 0 {
                    self.apply_task_snapshots(self.task_engine.snapshots());
                }
                self.status = Status::info(self.i18n.format(
                    if count > 0 {
                        "status.modpack_updated"
                    } else {
                        "status.modpack_updated_none"
                    },
                    &[
                        ("overwritten", &outcome.overwritten.to_string()),
                        ("removed", &outcome.removed.to_string()),
                        ("count", &count.to_string()),
                    ],
                ));
                self.rescan_workspace_content(crate::core::content::ContentKind::Mod);
            }
        }
    }

    /// One line for the pack entries the manifest listed but the API could
    /// not resolve. The install still runs; the player needs to know a few
    /// mods are missing and which ones to look at.
    fn unresolved_modpack_status(&self, unresolved: &[String]) -> String {
        let sample = unresolved
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if unresolved.len() > 3 { " …" } else { "" };
        self.i18n.format(
            "status.modpack_files_unresolved",
            &[
                ("count", &unresolved.len().to_string()),
                ("sample", &format!("{sample}{suffix}")),
            ],
        )
    }

    pub(in crate::tui::app) fn apply_modpack_prepared(
        &mut self,
        result: std::result::Result<crate::tui::event::PreparedModpack, String>,
    ) {
        let prepared = match result {
            Ok(prepared) => prepared,
            Err(error) => {
                self.status = Status::error(error);
                return;
            }
        };
        let crate::tui::event::PreparedModpack {
            instance,
            root,
            name,
            loader,
            loader_version,
            files,
            unresolved,
        } = prepared;
        let instance_id = instance.id;
        let instance_root = instance.path.clone();
        let version = instance.minecraft_version.clone();
        // The download task carried the job id in; a local archive has none yet.
        let job = self.modpack_job.unwrap_or_else(Uuid::new_v4);
        self.modpack_job = Some(job);
        self.upsert_instance(instance);
        // The pack's artwork was written during `prepare`, before this instance
        // had a row. Drop anything cached for the path so the first frame after
        // the row appears reads the real cover instead of a placeholder.
        self.refresh_cover(&instance_root);
        let loader = parse_install_loader(&loader, &loader_version);
        let (_, install_loader) = match loader {
            Ok(loader) => loader,
            Err(error) => {
                self.status = Status::error(error.to_string());
                return;
            }
        };
        let install_task =
            match self.submit_install_task(root, name, version, install_loader, Some(job)) {
                Ok(task) => task,
                Err(error) => {
                    self.status = Status::error(error.to_string());
                    return;
                }
            };
        if files.is_empty() {
            // Nothing left to queue; the job id has done its work.
            self.modpack_job = None;
            if !unresolved.is_empty() {
                self.status = Status::warning(self.unresolved_modpack_status(&unresolved));
            }
            return;
        }
        let count = files.len();
        // The pack files land in the instance directory the loader step is
        // still writing, so the worker parks until that step is done instead
        // of the two racing over one folder.
        let result = self.task_engine.submit_with_job_after(
            TaskOperation::ModpackFiles(crate::core::task::ModpackFilesSpec {
                instance_id,
                instance_root,
                cache_root: self.paths.cache_dir.clone(),
                files,
                download_snapshot: self.download_snapshot(),
            }),
            Some(job),
            Some(install_task),
        );
        // Every step of this install is queued; the next one starts fresh.
        self.modpack_job = None;
        match result {
            Ok(task_id) => {
                self.apply_task_snapshots(self.task_engine.snapshots());
                self.status = if unresolved.is_empty() {
                    Status::info(self.i18n.format(
                        "status.modpack_files_queued",
                        &[("count", &count.to_string()), ("id", &task_id.to_string())],
                    ))
                } else {
                    Status::warning(self.unresolved_modpack_status(&unresolved))
                };
            }
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }

    pub(in crate::tui::app) fn submit_world_save(
        &mut self,
        file: ResourceFile,
        destination: PathBuf,
    ) {
        let result = (|| {
            let destination = crate::core::resource::conflict::normalize_target(&destination)
                .map_err(Error::Download)?;
            crate::core::resource::conflict::check_available(&destination)
                .map_err(Error::Download)?;
            self.task_engine
                .submit(TaskOperation::ResourceSaveAs(ResourceSaveAsSpec {
                    destination,
                    cache_root: self.paths.cache_dir.clone(),
                    source: file.source,
                    kind: ResourceKind::World,
                    project_id: file.project_id,
                    file_id: file.file_id,
                    file_name: file.name,
                    candidate_urls: vec![file.download_url],
                    expected_sha1: file.sha1,
                    expected_size: file.size,
                    download_snapshot: self.download_snapshot(),
                }))
        })();
        match result {
            Ok(task_id) => {
                self.apply_task_snapshots(self.task_engine.snapshots());
                self.status = Status::info(
                    self.i18n
                        .format("status.world_save_queued", &[("id", &task_id.to_string())]),
                );
            }
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }
}

async fn prepare_modpack_install(
    setup: PendingModpack,
    staging: PathBuf,
    snapshot: crate::core::download::DownloadSourceSnapshot,
) -> Result<crate::tui::event::PreparedModpack> {
    // The pipeline itself lives in `core` so the CLI installs a pack the same
    // way; only the event shape below is the TUI's business.
    let prepared = crate::core::resource::modpack_install::prepare(
        crate::core::resource::modpack_install::InstallRequest {
            game_directory: setup.root.clone(),
            name: setup.name.clone(),
            archive: staging,
            icon_url: setup.icon_url.clone(),
        },
        modpack_provider(&snapshot).as_deref(),
    )
    .await?;
    Ok(crate::tui::event::PreparedModpack {
        instance: prepared.instance,
        root: setup.root,
        name: setup.name,
        loader: prepared.metadata.loader,
        loader_version: prepared.metadata.loader_version,
        files: prepared.files,
        unresolved: prepared.unresolved,
    })
}

/// The provider a CurseForge-flavoured pack needs, when there is one.
///
/// A Modrinth pack carries URLs, so a missing provider only matters for packs
/// that list ids — the resolution inside `core` says so in that case.
fn modpack_provider(
    snapshot: &crate::core::download::DownloadSourceSnapshot,
) -> Option<std::sync::Arc<dyn crate::core::resource::ResourceProvider>> {
    let client = std::sync::Arc::new(
        crate::core::version::http::HttpClient::with_timeout(snapshot.timeout_secs).ok()?,
    );
    crate::tui::app::resource_registry(client)
        .provider(crate::core::resource::model::ResourceSource::CurseForge)
}

fn preparation_task_failed(error: tokio::task::JoinError) -> Error {
    Error::Download(format!("modpack preparation task failed: {error}"))
}

/// Import a modpack archive the player picked, without touching their file.
///
/// `prepare_modpack_install` removes its staging archive once extraction is
/// done, so the source is copied into the cache first. A failed import leaves
/// no staging file behind.
pub(in crate::tui::app) async fn prepare_local_modpack(
    setup: PendingModpack,
    source: PathBuf,
    staging: PathBuf,
    snapshot: crate::core::download::DownloadSourceSnapshot,
) -> Result<crate::tui::event::PreparedModpack> {
    if !source.is_file() {
        return Err(Error::Download(format!(
            "modpack archive not found: {}",
            source.display()
        )));
    }
    let copy_from = source.clone();
    let copy_to = staging.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        if let Some(parent) = copy_to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&copy_from, &copy_to)?;
        Ok(())
    })
    .await
    .map_err(preparation_task_failed)??;
    let result = prepare_modpack_install(setup, staging.clone(), snapshot).await;
    if result.is_err() {
        // A half-written copy must not be mistaken for a cached archive.
        let _ = std::fs::remove_file(&staging);
    }
    result
}
