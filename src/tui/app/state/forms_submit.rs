//! Split out of the parent module during P7 acceptance.

use super::*;

impl App {
    pub(in crate::tui::app) fn submit_form(
        &mut self,
        action: FormAction,
        values: Vec<String>,
    ) -> Result<()> {
        match action {
            FormAction::ImportContent(kind) => {
                let value = values.first().map(String::as_str).unwrap_or_default();
                self.import_workspace_content(kind, value);
            }
            FormAction::CreateInstance => {
                let root = normalize_game_directory(required_value(&values, 0, "game directory")?)?;
                let name = required_value(&values, 1, "instance name")?;
                let version = required_value(&values, 2, "Minecraft version")?;
                let (loader, install_loader) = parse_install_loader(
                    values.get(3).map(String::as_str).unwrap_or("vanilla"),
                    values.get(4).map(String::as_str).unwrap_or(""),
                )?;
                let game_directory_id = game_directory_id(&root);
                add_game_directory(&mut self.config, &root);
                self.remove_exclusion(&root.join("versions").join(name));
                self.save_config()?;
                let mut instance = create_instance(&root, game_directory_id, name, version)?;
                instance.loader = loader;
                write_json_atomic(&instance.path.join("tmcl.json"), &instance)?;
                self.upsert_instance(instance);
                self.submit_install_task(
                    root,
                    name.to_owned(),
                    version.to_owned(),
                    install_loader,
                    None,
                )?;
            }
            FormAction::InstallLoader(kind) => {
                let value = values.first().map(String::as_str).unwrap_or_default();
                self.request_loader_install(kind, value);
            }
            FormAction::RestoreWorld => {
                let value = values.first().map(String::as_str).unwrap_or_default();
                let path = value
                    .rsplit_once('|')
                    .map(|(_, path)| path)
                    .unwrap_or(value);
                if !path.trim().is_empty() {
                    self.restore_workspace_world(std::path::PathBuf::from(path.trim()));
                }
            }
            FormAction::UpdateModpack => {
                let value = required_value(&values, 0, "modpack archive")?;
                self.start_modpack_update(value)?;
            }
            FormAction::ExportModpack => {
                let value = required_value(&values, 0, "export path")?;
                self.export_workspace_instance(std::path::Path::new(value))?;
            }
            FormAction::LaunchOverride(field) => {
                let value = values.first().map(String::as_str).unwrap_or_default();
                self.apply_launch_override(field, value);
            }
            FormAction::SettingValue(id) => {
                let value = values.first().map(String::as_str).unwrap_or_default();
                self.apply_setting_value(id, value)?;
            }
            FormAction::SearchLogs => {
                let query = values
                    .first()
                    .map(|value| value.trim().to_owned())
                    .unwrap_or_default();
                self.log_filter = (!query.is_empty()).then_some(query.clone());
                self.status = Status::info(if query.is_empty() {
                    self.i18n.text("status.logs_all")
                } else {
                    self.i18n
                        .format("status.logs_filtering", &[("query", &query)])
                });
            }
            FormAction::SearchDownload => {
                let query = values
                    .first()
                    .map(|value| value.trim().to_owned())
                    .unwrap_or_default();
                if self.download_center.category == DownloadCategory::GameVersions {
                    self.download_center.version_filter.set_search(&query);
                    self.reset_version_selection();
                } else {
                    self.download_center.search = query.clone();
                    self.resource_projects.clear();
                    self.resource_files.clear();
                    self.resource_detail = None;
                    self.resource_detail_project = None;
                    self.download_center.total = 0;
                    self.spawn_resource_search();
                }
                self.status = Status::info(if query.is_empty() {
                    self.i18n.text("status.search_cleared")
                } else {
                    self.i18n
                        .format("status.search_applied", &[("query", &query)])
                });
            }
            FormAction::DownloadJavaRuntime => {
                let major = parse_positive_u32(&values, 0, "Java major version")?;
                self.submit_java_runtime_task(major)?;
            }
            FormAction::AddJavaRuntime => {
                let executable = PathBuf::from(required_value(&values, 0, "Java executable")?);
                let installation = crate::core::java::scan::discover_one(executable)?;
                let mut next = self.config.clone();
                next.java.selected_path = Some(installation.executable.clone());
                self.commit_config(next)?;
                // Silent: the "added" message below is the one to keep.
                self.start_java_scan();
                self.java_selected = self
                    .java
                    .iter()
                    .position(|java| java.executable == installation.executable)
                    .unwrap_or(0);
                self.status = Status::info(self.i18n.format(
                    "status.java_added",
                    &[
                        ("major", &installation.major.to_string()),
                        ("path", &display_path(&installation.executable)),
                    ],
                ));
            }
            FormAction::ImportInstance => {
                let root = normalize_game_directory(required_value(&values, 0, "game directory")?)?;
                let discovery = discover(&root)?;
                let sources = if discovery.instances.is_empty() {
                    discovery
                        .child_candidates
                        .into_iter()
                        .filter_map(|candidate| {
                            discover(&candidate)
                                .ok()
                                .map(|found| (candidate, found.instances))
                        })
                        .filter(|(_, instances)| !instances.is_empty())
                        .collect::<Vec<_>>()
                } else {
                    vec![(root.clone(), discovery.instances)]
                };
                if sources.is_empty() {
                    return Err(Error::Config(format!(
                        "no importable instances found under {}",
                        display_path(&root)
                    )));
                }
                let mut imported = 0usize;
                for (game_root, instances) in sources {
                    let game_root = normalize_game_directory(game_root)?;
                    let directory_id = game_directory_id(&game_root);
                    add_game_directory(&mut self.config, &game_root);
                    for discovered in instances {
                        let version = infer_minecraft_version(&discovered.path);
                        let instance = import_instance(&discovered.path, directory_id, &version)?;
                        self.remove_exclusion(&discovered.path);
                        self.upsert_instance(instance);
                        imported += 1;
                    }
                }
                self.save_config()?;
                self.status = Status::info(self.i18n.format(
                    "status.instances_imported",
                    &[("count", &imported.to_string())],
                ));
            }
            FormAction::RenameInstance => {
                let name = required_value(&values, 0, "new instance name")?;
                let selected = self
                    .instances
                    .selected_item()
                    .cloned()
                    .ok_or_else(|| Error::Config("select an instance first".into()))?;
                let list_id = selected.id;
                let instance = rename_instance(&selected.path, name)?;
                let renamed_path = instance.path.clone();
                let renamed_name = instance.name.clone();
                self.upsert_instance(instance);
                // The workspace caches the old directory; without this an
                // export or scan started from it would address a path that no
                // longer exists.
                if let Some(workspace) = self
                    .workspace
                    .as_mut()
                    .filter(|workspace| workspace.instance == list_id)
                {
                    workspace.name = renamed_name.clone();
                    workspace.draft.name = renamed_name;
                    workspace.root = renamed_path.clone();
                    workspace.draft.path = renamed_path;
                    workspace.scan_stamps.clear();
                }
                self.status = Status::info(
                    self.i18n
                        .format("status.instance_renamed", &[("name", name)]),
                );
            }
            FormAction::AddOfflineAccount => {
                let username = required_value(&values, 0, "username")?;
                let account = create_account(username, None)?;
                AccountStore::new(&self.paths.accounts_file).add(account.clone())?;
                self.accounts.accounts.push(account);
                self.accounts.selected = self.accounts.accounts.len() - 1;
                self.status = Status::info(
                    self.i18n
                        .format("status.account_added", &[("username", username)]),
                );
            }
            FormAction::AddGameDirectory => {
                let path = normalize_game_directory(PathBuf::from(required_value(
                    &values,
                    0,
                    "directory path",
                )?))?;
                let mut next = self.config.clone();
                add_game_directory(&mut next, &path);
                self.commit_config(next)?;
                self.status = Status::info(
                    self.i18n
                        .format("status.directory_added", &[("path", &display_path(&path))]),
                );
                self.reload()?;
                self.settings
                    .select_category(SettingsCategory::GameDirectories);
                self.settings.selected = 2 + self.config.game_directories.len().saturating_sub(1);
            }
            FormAction::SelectResourceTarget => {
                let value = required_value(&values, 0, "target instance")?;
                let id = value
                    .split('|')
                    .next()
                    .and_then(|id| Uuid::parse_str(id).ok())
                    .ok_or_else(|| Error::Config("invalid target instance".into()))?;
                if !self.instances.items.iter().any(|item| item.id == id) {
                    return Err(Error::Config("target instance no longer exists".into()));
                }
                self.download_target_id = Some(id);
                self.status = Status::success(self.i18n.text("status.download_target_updated"));
                if self.download_center.category.resource_kind().is_some() {
                    self.resource_projects.clear();
                    self.resource_files.clear();
                    self.resource_detail = None;
                    self.resource_detail_project = None;
                    self.spawn_resource_search();
                }
            }
            FormAction::InstallModpack => {
                let staging = PathBuf::from(required_value(&values, 0, "download path")?);
                let root = normalize_game_directory(required_value(&values, 1, "game directory")?)?;
                let name = required_value(&values, 2, "instance name")?.to_owned();
                let pending = self.pending_modpack_file.take();
                let icon_url = pending
                    .as_ref()
                    .and_then(|pending| pending.icon_url.clone());
                let setup = PendingModpack {
                    root,
                    name,
                    icon_url,
                };
                if let Some(pending) = pending {
                    let file = pending.file;
                    let retry = crate::tui::app::PendingModpackFile {
                        file: file.clone(),
                        icon_url: pending.icon_url,
                    };
                    // One job id for the whole install: download, loader and
                    // files land in the task list as a single row.
                    let job = Uuid::new_v4();
                    self.modpack_job = Some(job);
                    let operation = TaskOperation::ModpackInstall(ModpackInstallSpec {
                        staging_path: staging.clone(),
                        cache_root: self.paths.cache_dir.clone(),
                        source: file.source,
                        project_id: file.project_id,
                        file_id: file.file_id,
                        file_name: file.name,
                        candidate_urls: vec![file.download_url],
                        expected_sha1: file.sha1,
                        expected_size: file.size,
                        download_snapshot: self.download_snapshot(),
                    });
                    let result = self.task_engine.submit_with_job(operation, Some(job));
                    match result {
                        Ok(task_id) => {
                            self.pending_modpack = Some(setup);
                            self.apply_task_snapshots(self.task_engine.snapshots());
                            self.status = Status::info(self.i18n.format(
                                "status.modpack_download_started",
                                &[
                                    ("path", &display_path(&staging)),
                                    ("id", &task_id.to_string()),
                                ],
                            ));
                        }
                        Err(error) => {
                            self.pending_modpack_file = Some(retry);
                            return Err(error);
                        }
                    }
                } else {
                    self.pending_modpack = Some(setup);
                    self.spawn_pending_modpack(&staging)?;
                }
            }
            FormAction::ImportLocalModpack => {
                let archive = PathBuf::from(required_value(&values, 0, "modpack archive")?);
                // Checked here rather than in the background job so the dialog
                // stays open with the path the player just typed.
                if !archive.is_file() {
                    return Err(Error::Download(self.i18n.format(
                        "status.modpack_file_missing",
                        &[("path", &display_path(&archive))],
                    )));
                }
                let root = normalize_game_directory(required_value(&values, 1, "game directory")?)?;
                let typed = values.get(2).map(|value| value.trim()).unwrap_or_default();
                let name = if typed.is_empty() {
                    archive
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("modpack")
                        .to_owned()
                } else {
                    typed.to_owned()
                };
                // A local archive has no project page, so there is no artwork
                // to fetch; the pack's own `icon.png` override is what remains.
                self.pending_modpack = Some(PendingModpack {
                    root,
                    name,
                    icon_url: None,
                });
                self.spawn_local_modpack(archive)?;
            }
            FormAction::InstallResource => {
                let file_name = required_value(&values, 0, "file name")?.to_owned();
                let file = self
                    .pending_resource_file
                    .clone()
                    .ok_or_else(|| Error::Download("resource confirmation expired".into()))?;
                let kind = self.download_center.category.resource_kind();
                let required = file
                    .dependencies
                    .iter()
                    .filter(|dependency| {
                        dependency.kind == crate::core::resource::model::DependencyKind::Required
                    })
                    .count();
                let target_id = self.download_target_id;
                let task_id = self.queue_resource_install(file.clone(), file_name)?;
                self.pending_resource_file = None;
                self.apply_task_snapshots(self.task_engine.snapshots());
                self.status = Status::info(self.i18n.format(
                    "status.resource_install_queued",
                    &[("id", &task_id.to_string())],
                ));
                if kind == Some(crate::core::resource::model::ResourceKind::Mod)
                    && required > 0
                    && let Some(target_id) = target_id
                {
                    self.spawn_dependency_resolution(file, target_id);
                }
            }
        }
        Ok(())
    }
}
