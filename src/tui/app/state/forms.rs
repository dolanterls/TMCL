//! Form, picker and confirm flows.

use super::*;

impl App {
    pub(in crate::tui::app) fn open_selection_picker(&mut self, field: usize) {
        let loader_fields = self.loader_form_fields();
        match field {
            field
                if loader_fields.is_some_and(|fields| field == fields.0)
                    && self.versions.is_empty() =>
            {
                if let Some(modal) = &mut self.modal {
                    modal.set_selection_loading(field);
                }
                self.refresh_versions_for_picker();
            }
            field if loader_fields.is_some_and(|fields| field == fields.2) => {
                match self.current_create_loader() {
                    Some((_, None)) => {
                        if let Some(modal) = &mut self.modal {
                            modal.set_selection_unavailable(
                                field,
                                self.i18n.text("form.vanilla_no_loader_version"),
                            );
                        }
                    }
                    Some((minecraft_version, Some(kind))) if !minecraft_version.is_empty() => {
                        self.request_loader_versions(minecraft_version, kind);
                    }
                    Some((_, Some(_))) => {
                        if let Some(modal) = &mut self.modal {
                            modal.set_selection_unavailable(
                                field,
                                self.i18n.text("form.choose_minecraft_version"),
                            );
                        }
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    pub(in crate::tui::app) fn retry_selection_picker(&mut self, field: usize) {
        let loader_fields = self.loader_form_fields();
        match field {
            field if loader_fields.is_some_and(|fields| field == fields.0) => {
                if let Some(modal) = &mut self.modal {
                    modal.set_selection_loading(field);
                }
                self.refresh_versions_for_picker();
            }
            field if loader_fields.is_some_and(|fields| field == fields.2) => {
                match self.current_create_loader() {
                    Some((minecraft_version, Some(kind))) if !minecraft_version.is_empty() => {
                        self.request_loader_versions(minecraft_version, kind);
                    }
                    Some((_, None)) => {
                        if let Some(modal) = &mut self.modal {
                            modal.set_selection_unavailable(
                                field,
                                "Vanilla does not use a loader version.",
                            );
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Opens the search form for the download center. The active tab decides
    /// what the query targets, and the dialog starts from the current query so
    /// it doubles as the way to refine or clear an existing filter.
    pub(in crate::tui::app) fn open_download_search(&mut self) {
        let (title_key, query) = if self.download_center.category == DownloadCategory::GameVersions
        {
            (
                "search.title_versions",
                self.download_center.version_filter.search().to_owned(),
            )
        } else {
            (
                "search.title_resources",
                self.download_center.search.clone(),
            )
        };
        self.modal = Some(Modal::prompt(
            self.i18n.text(title_key),
            self.i18n.text("search.keyword"),
            self.i18n.text("search.help"),
            query,
            FormAction::SearchDownload,
        ));
    }

    pub(in crate::tui::app) fn create_selection_changed(
        &mut self,
        field: usize,
    ) -> Option<(String, MetadataLoaderKind)> {
        let (version_field, loader_field, loader_version_field) = self.loader_form_fields()?;
        if field != version_field && field != loader_field {
            return None;
        }
        self.current_create_loader()?;
        if let Some(modal) = &mut self.modal {
            modal.set_field_value(loader_version_field, String::new());
            modal.set_selection_options(loader_version_field, Vec::new());
        }

        match self.current_create_loader() {
            Some((_, None)) => {
                if let Some(modal) = &mut self.modal {
                    modal.set_selection_unavailable(
                        loader_version_field,
                        self.i18n.text("form.vanilla_no_loader_version"),
                    );
                }
                None
            }
            Some((minecraft_version, Some(kind))) if !minecraft_version.is_empty() => self
                .prepare_loader_version_request(&minecraft_version, kind)
                .then_some((minecraft_version, kind)),
            Some((_, Some(_))) => {
                if let Some(modal) = &mut self.modal {
                    modal.open_selection_picker(loader_version_field);
                    modal.set_selection_unavailable(
                        loader_version_field,
                        self.i18n.text("form.choose_minecraft_version"),
                    );
                }
                None
            }
            None => None,
        }
    }

    pub(in crate::tui::app) fn current_create_loader(
        &self,
    ) -> Option<(String, Option<MetadataLoaderKind>)> {
        let Some(Modal::Form { action, values, .. }) = self.modal.as_ref() else {
            return None;
        };
        let (version_field, loader_field, _) = match action {
            FormAction::CreateInstance => (
                CREATE_VERSION_FIELD,
                CREATE_LOADER_FIELD,
                CREATE_LOADER_VERSION_FIELD,
            ),
            FormAction::InstallModpack => (3, 4, 5),
            _ => return None,
        };
        let minecraft_version = values.get(version_field)?.clone();
        let kind = match values.get(loader_field)?.as_str() {
            "vanilla" => None,
            "fabric" => Some(MetadataLoaderKind::Fabric),
            "quilt" => Some(MetadataLoaderKind::Quilt),
            "forge" => Some(MetadataLoaderKind::Forge),
            "neoforge" => Some(MetadataLoaderKind::NeoForge),
            _ => return None,
        };
        Some((minecraft_version, kind))
    }

    pub(in crate::tui::app) fn loader_form_fields(&self) -> Option<(usize, usize, usize)> {
        let Modal::Form { action, .. } = self.modal.as_ref()? else {
            return None;
        };
        match action {
            FormAction::CreateInstance => Some((
                CREATE_VERSION_FIELD,
                CREATE_LOADER_FIELD,
                CREATE_LOADER_VERSION_FIELD,
            )),
            FormAction::InstallModpack => Some((3, 4, 5)),
            _ => None,
        }
    }

    pub(in crate::tui::app) fn request_loader_versions(
        &mut self,
        minecraft_version: String,
        kind: MetadataLoaderKind,
    ) {
        if self.prepare_loader_version_request(&minecraft_version, kind) {
            self.spawn_loader_version_fetch(minecraft_version, kind);
        }
    }

    pub(in crate::tui::app) fn prepare_loader_version_request(
        &mut self,
        minecraft_version: &str,
        kind: MetadataLoaderKind,
    ) -> bool {
        if minecraft_version.is_empty() {
            let field = self
                .loader_form_fields()
                .map(|fields| fields.2)
                .unwrap_or(CREATE_LOADER_VERSION_FIELD);
            if let Some(modal) = &mut self.modal {
                modal.open_selection_picker(field);
                modal.set_selection_unavailable(
                    field,
                    self.i18n.text("form.choose_minecraft_version"),
                );
            }
            return false;
        }
        let field = self
            .loader_form_fields()
            .map(|fields| fields.2)
            .unwrap_or(CREATE_LOADER_VERSION_FIELD);
        if let Some(modal) = &mut self.modal {
            modal.set_field_value(field, String::new());
            modal.set_selection_options(field, Vec::new());
            modal.open_selection_picker(field);
            modal.set_selection_loading(field);
        } else {
            return false;
        }

        self.status = Status::info(self.i18n.format(
            "status.loading_loader_versions",
            &[
                ("loader", &format!("{kind:?}")),
                ("version", minecraft_version),
            ],
        ));
        true
    }

    pub(in crate::tui::app) fn spawn_loader_version_fetch(
        &self,
        minecraft_version: String,
        kind: MetadataLoaderKind,
    ) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let result = async {
                list_loader_versions(
                    &HttpClient::new()?,
                    &[DownloadSource::Official, DownloadSource::Bmclapi],
                    kind,
                    &minecraft_version,
                )
                .await
            }
            .await
            .map_err(|error: Error| error.to_string());
            let _ = sender.send(AppEvent::LoaderVersionsLoaded {
                minecraft_version,
                kind,
                result,
            });
        });
    }

    pub(in crate::tui::app) fn apply_loader_versions(
        &mut self,
        minecraft_version: String,
        kind: MetadataLoaderKind,
        result: std::result::Result<Vec<crate::core::version::loader_meta::LoaderVersion>, String>,
    ) {
        let current = self.current_create_loader();
        if current != Some((minecraft_version.clone(), Some(kind))) {
            return;
        }

        match result {
            Ok(versions) => {
                let options = versions
                    .iter()
                    .map(|version| version.version.clone())
                    .collect();
                if let Some(field) = self.loader_form_fields().map(|fields| fields.2)
                    && let Some(modal) = &mut self.modal
                {
                    modal.set_selection_options(field, options);
                }
                self.status = Status::info(self.i18n.format(
                    "status.loader_versions_loaded",
                    &[
                        ("count", &versions.len().to_string()),
                        ("loader", &format!("{kind:?}")),
                        ("version", &minecraft_version),
                    ],
                ));
            }
            Err(error) => {
                if let Some(field) = self.loader_form_fields().map(|fields| fields.2)
                    && let Some(modal) = &mut self.modal
                {
                    modal.set_selection_error(field, error.clone());
                }
                self.status = Status::error(self.i18n.format(
                    "status.loader_versions_failed",
                    &[("loader", &format!("{kind:?}")), ("error", &error)],
                ));
            }
        }
    }

    /// Queues the loader install. `job` ties it to the rest of a modpack
    /// install; a plain "create instance" passes `None` and stands alone.
    pub(in crate::tui::app) fn submit_install_task(
        &mut self,
        root: PathBuf,
        name: String,
        version: String,
        loader: InstallLoader,
        job: Option<Uuid>,
    ) -> Result<Uuid> {
        let instance = self
            .instances
            .selected_item()
            .cloned()
            .ok_or_else(|| Error::Config("created instance was not registered".into()))?;
        let instance_id = instance.id;
        let required_java = minimum_java_major(&version).unwrap_or(8);
        let java_executable = select_java(&self.java, required_java)
            .ok()
            .map(|java| java.executable.clone());
        let loader = crate::core::task::task_loader(&loader);
        let operation = TaskOperation::InstanceInstall(InstanceInstallSpec {
            instance_id,
            game_directory: root,
            instance_directory: instance.path.clone(),
            instance_name: name.clone(),
            minecraft_version: version,
            loader: loader.clone(),
            java_executable,
            download_snapshot: self.download_snapshot(),
        });
        let task_id = self.task_engine.submit_with_job(operation, job)?;
        self.apply_task_snapshots(self.task_engine.snapshots());
        self.status = Status::info(self.i18n.format(
            "status.instance_install_queued",
            &[("name", &name), ("id", &task_id.to_string())],
        ));
        Ok(task_id)
    }

    pub(in crate::tui::app) fn submit_java_runtime_task(&mut self, major: u32) -> Result<()> {
        let root = self.paths.root.clone();
        let task_id = self
            .task_engine
            .submit(TaskOperation::JavaRuntime(JavaRuntimeSpec {
                data_root: root,
                major,
                download_snapshot: self.download_snapshot(),
            }))?;
        self.apply_task_snapshots(self.task_engine.snapshots());
        if self
            .task_snapshots
            .iter()
            .any(|snapshot| snapshot.id == task_id)
        {
            self.set_task_selection_by_id(task_id);
        }
        self.status = Status::info(self.i18n.format(
            "status.java_install_queued",
            &[("major", &major.to_string()), ("id", &task_id.to_string())],
        ));
        Ok(())
    }

    pub(in crate::tui::app) fn open_remove_confirmation(&mut self) {
        let (prompt, action) = match self.page {
            Page::Instances => {
                let Some(instance) = self.instances.selected_item() else {
                    self.status = Status::warning(self.i18n.text("status.select_instance"));
                    return;
                };
                (
                    self.i18n
                        .format("confirm.remove_instance", &[("name", &instance.name)]),
                    ConfirmAction::RemoveInstance(instance.id),
                )
            }
            Page::Accounts => {
                let Some(account) = self.accounts.accounts.get(self.accounts.selected) else {
                    self.status = Status::warning(self.i18n.text("status.select_account"));
                    return;
                };
                (
                    self.i18n
                        .format("confirm.remove_account", &[("name", &account.username)]),
                    ConfirmAction::RemoveAccount,
                )
            }
            Page::Settings if self.settings.category == SettingsCategory::Java => {
                let Some(java) = self.java.get(self.java_selected) else {
                    self.status = Status::warning(self.i18n.text("status.scan_java_first"));
                    return;
                };
                if !self.is_managed_java(java) {
                    self.status =
                        Status::warning(self.i18n.text("status.java_remove_managed_only"));
                    return;
                }
                (
                    self.i18n.format(
                        "confirm.remove_java",
                        &[("path", &display_path(&java.executable))],
                    ),
                    ConfirmAction::RemoveJavaRuntime(self.java_selected),
                )
            }
            Page::Settings => {
                if self.settings.category != SettingsCategory::GameDirectories {
                    self.status = Status::warning(self.i18n.text("status.select_directory"));
                    return;
                }
                let Some(directory_index) = self.settings.selected.checked_sub(2) else {
                    self.status = Status::warning(self.i18n.text("status.select_directory"));
                    return;
                };
                if self.config.game_directories.len() <= 1 {
                    self.status = Status::warning(self.i18n.text("status.directory_required"));
                    return;
                }
                let Some(path) = self.config.game_directories.get(directory_index) else {
                    self.status = Status::warning(self.i18n.text("status.select_directory"));
                    return;
                };
                (
                    self.i18n
                        .format("confirm.remove_directory", &[("path", &display_path(path))]),
                    ConfirmAction::RemoveGameDirectory(directory_index),
                )
            }
            _ => return,
        };
        self.modal = Some(Modal::confirm(prompt, action));
    }

    pub(in crate::tui::app) fn confirm_action(&mut self, action: ConfirmAction) -> Result<()> {
        match action {
            ConfirmAction::DeleteContent {
                instance,
                kind,
                index,
            } => {
                // The index refers to the list that produced the prompt; if the
                // workspace moved on, do nothing instead of deleting the wrong
                // file.
                let Some(workspace) = self
                    .workspace
                    .as_ref()
                    .filter(|workspace| workspace.instance == instance)
                else {
                    return Ok(());
                };
                let entry = workspace
                    .visible_entries(kind)
                    .get(index)
                    .map(|entry| (*entry).clone());
                let Some(entry) = entry else {
                    return Ok(());
                };
                let root = workspace.root.clone();
                crate::core::content::trash::move_to_trash(&root, &entry.path)?;
                // A deleted file must not stay "enabled" in the config that
                // still names it, or the game keeps a dangling entry.
                crate::core::content::toggle::forget(&root, kind, &entry.display_file_name());
                self.status = Status::info(self.i18n.format(
                    "workspace.status.deleted",
                    &[("name", &entry.display_file_name())],
                ));
                self.rescan_workspace_content(kind);
            }
            ConfirmAction::ApplyModUpdates => {
                self.apply_pending_mod_updates()?;
            }
            ConfirmAction::SwitchLoader => {
                // The dialog only carries the yes/no; the payload waited in
                // `pending_loader_install` so this enum could stay `Copy`.
                if let Some(pending) = self.pending_loader_install.take() {
                    self.start_loader_install(pending);
                }
            }
            ConfirmAction::RemoveInstance(instance_id) => {
                let index = self
                    .instances
                    .items
                    .iter()
                    .position(|item| item.id == instance_id)
                    .ok_or_else(|| Error::Config("selected instance no longer exists".into()))?;
                let instance = self.instances.items[index].clone();
                let game_directory =
                    crate::core::instance::path::game_directory_of(&instance.path)?;
                let path = instance.path.canonicalize()?;
                if !self.config.excluded_instances.contains(&path) {
                    self.config.excluded_instances.push(path);
                }
                self.save_config()?;
                delete_metadata(&game_directory, &instance.path)?;
                self.instances.items.remove(index);
                if self
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.instance == instance_id)
                {
                    self.workspace = None;
                }
                // Removing a row above the cursor must keep the same instance
                // selected instead of sliding the cursor onto its neighbour.
                if index < self.instances.focused {
                    self.instances.focused -= 1;
                }
                self.instances.focused = self
                    .instances
                    .focused
                    .min(self.instances.items.len().saturating_sub(1));
                self.status = Status::info(
                    self.i18n
                        .format("status.instance_removed", &[("name", &instance.name)]),
                );
            }
            ConfirmAction::RemoveAccount => {
                let account = self
                    .accounts
                    .accounts
                    .get(self.accounts.selected)
                    .cloned()
                    .ok_or_else(|| Error::Account("select an account first".into()))?;
                let mut accounts = AccountStore::new(&self.paths.accounts_file).load()?;
                accounts.retain(|candidate| candidate.id != account.id);
                AccountStore::new(&self.paths.accounts_file).save(&accounts)?;
                // Removing the account removes its tokens too: the refresh
                // token was already left behind, the cached access token is new.
                let secrets = crate::core::account::secrets::SecretStore::new(
                    self.paths.root.join("secrets"),
                );
                crate::core::account::microsoft::session::clear_cached(&secrets, &account);
                if let Some(secret_ref) = account.secret_ref.as_deref() {
                    secrets.clear(secret_ref);
                }
                if self.config.state.current_account_id.as_deref() == Some(&account.id.to_string())
                {
                    self.config.state.current_account_id = None;
                    self.save_config()?;
                }
                self.accounts.accounts = accounts;
                if self.accounts.current == Some(account.id) {
                    self.accounts.current = None;
                }
                self.accounts.selected = self
                    .accounts
                    .selected
                    .min(self.accounts.accounts.len().saturating_sub(1));
                self.status = Status::info(
                    self.i18n
                        .format("status.account_removed", &[("name", &account.username)]),
                );
            }
            ConfirmAction::RemoveGameDirectory(directory_index) => {
                let Some(path) = self.config.game_directories.get(directory_index).cloned() else {
                    return Err(Error::Config("select a game directory first".into()));
                };
                let mut next = self.config.clone();
                next.game_directories.remove(directory_index);
                self.commit_config(next)?;
                self.reload()?;
                self.settings.selected =
                    2 + directory_index.min(self.config.game_directories.len().saturating_sub(1));
                self.status = Status::info(self.i18n.format(
                    "status.directory_removed",
                    &[("path", &display_path(&path))],
                ));
            }
            ConfirmAction::RemoveJavaRuntime(java_index) => {
                let java =
                    self.java.get(java_index).cloned().ok_or_else(|| {
                        Error::Java("selected Java runtime no longer exists".into())
                    })?;
                if !self.is_managed_java(&java) {
                    self.status =
                        Status::warning(self.i18n.text("status.java_remove_managed_only"));
                    return Ok(());
                }
                let runtime_root = java
                    .executable
                    .parent()
                    .and_then(Path::parent)
                    .ok_or_else(|| Error::Java("invalid managed Java path".into()))?;
                std::fs::remove_dir_all(runtime_root)?;
                let mut next = self.config.clone();
                if next.java.selected_path.as_ref() == Some(&java.executable) {
                    next.java.selected_path = None;
                }
                self.commit_config(next)?;
                // Silent: the caller reports the removal below, and a scan
                // result arriving a moment later would replace that message.
                self.start_java_scan();
                self.java_selected = self.java_selected.min(self.java.len().saturating_sub(1));
                self.status = Status::info(self.i18n.format(
                    "status.java_removed",
                    &[("path", &display_path(&java.executable))],
                ));
            }
            ConfirmAction::RepairInstance(instance_id) => {
                let Some(instance) = self
                    .instances
                    .items
                    .iter()
                    .find(|item| item.id == instance_id)
                    .cloned()
                else {
                    return Err(Error::Config("selected instance no longer exists".into()));
                };
                match self.launch_blockers(&instance) {
                    Ok(missing) if !missing.is_empty() => {
                        self.start_instance_repair(&instance, missing)
                    }
                    Ok(_) => {
                        self.status = Status::info(self.i18n.text("status.repair_not_needed"));
                    }
                    Err(error) => self.status = Status::error(error.to_string()),
                }
            }
            ConfirmAction::PurgeInstanceTrash(instance_id) => {
                let root = self
                    .instances
                    .items
                    .iter()
                    .find(|item| item.id == instance_id)
                    .map(|item| item.path.clone())
                    .ok_or_else(|| Error::Config("selected instance no longer exists".into()))?;
                match crate::core::content::trash::purge(&root) {
                    Ok(()) => self.status = Status::success(self.i18n.text("status.trash_emptied")),
                    Err(error) => self.status = Status::error(error.to_string()),
                }
            }
            ConfirmAction::AddToPath => match crate::core::shell::path::add() {
                Ok(change) if change.changed => {
                    self.status = Status::success(self.i18n.format(
                        "status.path_added",
                        &[("directory", &display_path(&change.directory))],
                    ));
                }
                Ok(change) => {
                    self.status = Status::success(self.i18n.format(
                        "status.path_already",
                        &[("directory", &display_path(&change.directory))],
                    ));
                }
                Err(error) => self.status = Status::error(error.to_string()),
            },
            ConfirmAction::ClearDownloadCache => {
                let root = self.cache_root();
                let report = cache::clear_with_report(&root)?;
                self.status = Status::success(if report.failures.is_empty() {
                    self.i18n.format(
                        "status.cache_cleared",
                        &[
                            ("files", &report.removed_files.to_string()),
                            ("bytes", &format_size(report.removed_bytes)),
                        ],
                    )
                } else {
                    self.i18n.format(
                        "status.cache_clear_partial",
                        &[
                            ("files", &report.removed_files.to_string()),
                            ("failures", &report.failures.len().to_string()),
                        ],
                    )
                });
            }
        }
        Ok(())
    }
}
