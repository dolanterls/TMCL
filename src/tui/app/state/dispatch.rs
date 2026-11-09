//! Terminal event dispatch: key, mouse and the AppEvent entry point.

use super::*;

impl App {
    pub fn take_directory_browse_request(&mut self) -> Option<PathBuf> {
        let mut initial = self.directory_browse_request.take()?;
        if !initial.is_dir() {
            initial = self
                .config
                .game_directories
                .first()
                .cloned()
                .or_else(|| executable_directory().ok())?;
        }
        Some(initial)
    }

    pub fn take_save_file_browse_request(&mut self) -> Option<(PathBuf, String)> {
        self.save_file_browse_request.take()
    }

    /// Where the local-modpack file picker should start: the folder of the
    /// path already typed, else the last downloaded modpack, else the game
    /// directory's parent.
    pub fn take_modpack_browse_request(&mut self) -> Option<PathBuf> {
        let requested = self.modpack_browse_request.take()?;
        let folder = requested.parent().map(Path::to_path_buf);
        match folder.filter(|folder| folder.is_dir()) {
            Some(folder) => Some(folder),
            None => self
                .config
                .game_directories
                .first()
                .cloned()
                .filter(|directory| directory.is_dir())
                .or_else(|| executable_directory().ok()),
        }
    }

    /// Put the chosen archive into the import form and fill the instance name
    /// from it, unless the player has already typed one.
    pub fn set_browsed_modpack_file(&mut self, path: PathBuf) {
        let fill_name = self.modal.as_ref().is_some_and(|modal| match modal {
            Modal::Form { values, .. } => {
                values.get(2).is_some_and(|value| value.trim().is_empty())
            }
            _ => false,
        });
        let Some(modal) = self.modal.as_mut() else {
            return;
        };
        if !modal.set_browsed_file(path.clone()) {
            return;
        }
        if !fill_name {
            return;
        }
        if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
            modal.set_field_value(2, stem.to_owned());
        }
    }

    pub fn set_browsed_save_file(&mut self, path: PathBuf) {
        if let Some(file) = self.pending_world_file.take() {
            self.submit_world_save(file, path);
        } else if let Some(kind) = self.pending_log_export.take() {
            if matches!(kind, LogExportKind::Bundle) {
                self.export_diagnosis_bundle(&path);
                return;
            }
            let content = match &kind {
                LogExportKind::Launcher => {
                    Ok(self.launcher_logs.matching_lines("").join("\n") + "\n")
                }
                LogExportKind::Game => Ok(self.logs.matching_lines("").join("\n") + "\n"),
                // Handled above: the bundle is a zip of several files, not one
                // buffer written to disk.
                LogExportKind::Bundle => Ok(String::new()),
            };
            match content.and_then(|content| std::fs::write(&path, content)) {
                Ok(()) => {
                    self.status = Status::info(
                        self.i18n
                            .format("status.logs_exported", &[("path", &display_path(&path))]),
                    );
                }
                Err(error) => {
                    self.status = Status::error(self.i18n.format(
                        "status.logs_export_failed",
                        &[("error", &error.to_string())],
                    ));
                }
            }
        }
    }

    pub fn cancel_pending_save_file(&mut self) {
        self.pending_world_file = None;
        self.pending_log_export = None;
    }

    pub fn set_browsed_directory(&mut self, path: PathBuf) {
        if self
            .modal
            .as_mut()
            .is_some_and(|modal| modal.set_browsed_directory(path.clone()))
        {
            self.status = Status::info(self.i18n.format(
                "status.directory_selected",
                &[("path", &display_path(&path))],
            ));
        }
    }

    pub fn handle(&mut self, event: AppEvent) {
        match event {
            AppEvent::DiagnosisReady { seq, diagnosis } => {
                self.handle_diagnosis_ready(seq, diagnosis);
            }
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Mouse(mouse) => self.handle_mouse(mouse),
            AppEvent::Resize(width, height) => {
                self.terminal_width = width;
                self.terminal_height = height;
                // Any hit regions drawn for the old size are now wrong.
                self.resize_gen = self.resize_gen.wrapping_add(1);
                self.keep_version_selected_visible();
                self.status = Status::info(self.i18n.format(
                    "status.terminal_resized",
                    &[
                        ("width", &width.to_string()),
                        ("height", &height.to_string()),
                    ],
                ));
            }
            AppEvent::WorkspaceScanned {
                instance,
                kind,
                entries,
                stamp,
                shader_loader,
            } => self.apply_workspace_scan_for(instance, kind, entries, stamp, shader_loader),
            AppEvent::WorldJob(result) => self.finish_world_job(result),
            AppEvent::ModpackExported(result) => self.finish_workspace_export(result),
            AppEvent::WorkspaceSize { instance, bytes } => {
                self.apply_workspace_size(instance, bytes)
            }
            AppEvent::WorkspaceLoaderVersions {
                kind,
                minecraft_version,
                result,
            } => self.apply_workspace_loader_versions(kind, minecraft_version, result),
            AppEvent::Tick => {}
            AppEvent::Status(status) => {
                self.record_launcher_log(&status);
                self.status = Status::info(status);
            }
            AppEvent::UpdateCheck(result) => match result {
                Ok(info) if info.version.trim_start_matches('v') != env!("CARGO_PKG_VERSION") => {
                    self.update_check_in_flight = false;
                    self.status = Status::info(self.i18n.format(
                        "status.update_available",
                        &[("version", &info.version), ("url", &info.page_url)],
                    ));
                }
                Ok(_) => {
                    self.update_check_in_flight = false;
                    self.status = Status::info(self.i18n.text("status.update_current"));
                }
                Err(error) => {
                    self.update_check_in_flight = false;
                    self.record_launcher_log(&error);
                    self.status = Status::info(
                        self.i18n
                            .format("status.update_check_failed", &[("error", &error)]),
                    );
                }
            },
            AppEvent::JavaScanned(java) => self.apply_java_scan(java),
            AppEvent::ModpackUpdated { instance, result } => {
                self.apply_modpack_updated(instance, result);
            }
            AppEvent::ModUpdatesChecked { instance, result } => {
                self.apply_mod_updates_checked(instance, result);
            }
            AppEvent::CacheEvicted { files, bytes } => {
                self.status = Status::info(self.i18n.format(
                    "status.cache_evicted",
                    &[("files", &files.to_string()), ("size", &format_size(bytes))],
                ));
            }
            AppEvent::TerminalError(error) => {
                self.record_launcher_log(&error);
                self.status = Status::error(error);
                self.quit = true;
            }
            AppEvent::Reload => {
                if let Err(error) = self.reload() {
                    self.status = Status::error(error.to_string());
                }
            }
            AppEvent::DeviceCode {
                user_code,
                verification_uri,
                expires_in,
                browser_status,
            } => {
                self.accounts.device_code = Some(DeviceCodeView {
                    user_code,
                    verification_uri,
                    remaining_seconds: expires_in,
                    status: browser_status,
                });
            }
            AppEvent::VersionsLoaded {
                versions,
                from_cache,
            } => {
                self.versions = versions;
                if let Some(field) = self.loader_form_fields().map(|fields| fields.0)
                    && let Some(modal) = &mut self.modal
                {
                    modal.set_version_picker_options(field, self.versions.clone());
                }
                self.version_selected = 0;
                self.version_scroll = 0;
                self.reset_detail_scroll();
                self.status = Status::info(if from_cache {
                    self.i18n.format(
                        "status.loaded_versions_cache",
                        &[("count", &self.versions.len().to_string())],
                    )
                } else {
                    self.i18n.format(
                        "status.loaded_versions",
                        &[("count", &self.versions.len().to_string())],
                    )
                });
            }
            AppEvent::VersionsLoadFailed { for_picker, error } => {
                if for_picker
                    && let Some(field) = self.loader_form_fields().map(|fields| fields.0)
                    && let Some(modal) = &mut self.modal
                {
                    modal.set_selection_error(field, error.clone());
                }
                self.status = Status::info(
                    self.i18n
                        .format("status.refresh_failed", &[("error", &error)]),
                );
            }
            AppEvent::ResourceSearchLoaded {
                request_id,
                source,
                result,
            } => self.apply_resource_search(request_id, source, result),
            AppEvent::ResourceSearchFailed {
                request_id,
                source,
                error,
            } => {
                if request_id == self.download_center.request_id
                    && source == self.download_center.source
                {
                    self.download_center.loading = false;
                    self.download_center.error = Some(error.clone());
                    self.status = Status::info(
                        self.i18n
                            .format("status.resource_search_failed", &[("error", &error)]),
                    );
                }
            }
            AppEvent::ResourceCategoriesLoaded {
                request_id,
                source,
                kind,
                result,
            } => self.apply_resource_categories(request_id, source, kind, result),
            AppEvent::ResourceFilesLoaded {
                request_id,
                source,
                project_id,
                files,
            } => {
                if request_id == self.download_center.request_id
                    && source == self.download_center.source
                {
                    self.resource_files = files;
                    self.resource_detail = Some(project_id.clone());
                    self.reset_detail_scroll();
                    if let Some(project) = self
                        .resource_projects
                        .iter()
                        .find(|project| project.project_id == project_id)
                        .cloned()
                    {
                        self.resource_detail_project = Some(project);
                    }
                    let target = self.resource_target_context();
                    crate::core::resource::compatibility::sort_compatibility(
                        &mut self.resource_files,
                        target.as_ref(),
                    );
                    self.download_center.loading = false;
                    self.status = Status::info(self.i18n.format(
                        "status.resource_files_loaded",
                        &[
                            ("count", &self.resource_files.len().to_string()),
                            ("project", &project_id),
                        ],
                    ));
                }
            }
            AppEvent::ResourceFilesFailed {
                request_id,
                source,
                error,
            } => {
                if request_id == self.download_center.request_id
                    && source == self.download_center.source
                {
                    self.download_center.loading = false;
                    self.download_center.error = Some(error.clone());
                    self.status = Status::info(
                        self.i18n
                            .format("status.resource_files_failed", &[("error", &error)]),
                    );
                }
            }
            AppEvent::LoaderVersionsLoaded {
                minecraft_version,
                kind,
                result,
            } => self.apply_loader_versions(minecraft_version, kind, result),
            AppEvent::ModpackPrepared(result) => self.apply_modpack_prepared(result),
            AppEvent::DependenciesResolved {
                parent,
                target_id,
                files,
                unresolved,
            } => self.apply_dependencies_resolved(parent, target_id, files, unresolved),
            AppEvent::TasksUpdated(snapshots) => self.apply_task_snapshots(snapshots),
            AppEvent::Game(event) => match event {
                GameEvent::GameStarted { pid } => {
                    self.logs.append(format!("Minecraft started (PID {pid})"));
                    self.record_launcher_log(&format!("Minecraft started (PID {pid})"));
                    let launched = self
                        .active_launch
                        .as_ref()
                        .map(|launch| launch.instance_name.clone());
                    if let Some(launch) = self.active_launch.as_mut() {
                        launch.phase = LaunchPhase::Running;
                    }
                    self.sync_launch_status();
                    // Without this the footer kept saying `正在校验实例文件…`
                    // for the whole session: the stage messages stop here, and
                    // nothing replaced the last one.
                    if let Some(name) = launched {
                        self.status = Status::info(
                            self.i18n.format("status.game_running", &[("name", &name)]),
                        );
                    }
                    match self.config.launcher.visibility {
                        LauncherVisibility::KeepOpen => {}
                        LauncherVisibility::HideAfterLaunch => {
                            let _ = crate::platform::windows::set_console_visibility(false);
                        }
                        LauncherVisibility::ExitAfterLaunch => self.quit = true,
                    }
                }
                GameEvent::GameLog { stream, line } => {
                    if self.config.logs.game_debug || stream == "stderr" {
                        self.logs.append(format!("[{stream}] {line}"));
                    }
                }
                GameEvent::GameExited { code } => {
                    self.logs.append(format!("Minecraft exited with {code:?}"));
                    self.record_launcher_log(&format!("Minecraft exited with {code:?}"));
                }
            },
            AppEvent::LaunchFinished {
                instance_id,
                instance_directory,
                play_time_seconds,
                exit_code,
                log_path,
                crash_report,
                launch_command,
            } => {
                self.active_launch = None;
                // A relative crash-report path is relative to `--gameDir`,
                // which is the instance directory, not the game root.
                let crash_report = crash_report.map(|path| {
                    if path.is_absolute() {
                        path
                    } else {
                        instance_directory.join(path)
                    }
                });
                self.record_launcher_log(&format!("Game launch completed: {exit_code:?}"));
                if let Err(error) = crate::core::instance::stats::record_session(
                    &instance_directory,
                    play_time_seconds,
                ) {
                    self.status = Status::error(self.i18n.format(
                        "status.launch_exit_stats_failed",
                        &[
                            ("code", &format!("{exit_code:?}")),
                            ("error", &error.to_string()),
                        ],
                    ));
                } else {
                    self.status = Status::info(self.i18n.format(
                        "status.launch_exit_log",
                        &[
                            ("code", &format!("{exit_code:?}")),
                            ("path", &display_path(&log_path)),
                        ],
                    ));
                }

                // The verdict is built off this thread: 59 rules over half a
                // megabyte of log is not something to run between two frames.
                let instance_name = self
                    .instances
                    .items
                    .iter()
                    .find(|item| item.id == instance_id)
                    .map_or_else(
                        || {
                            instance_directory
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("instance")
                                .to_owned()
                        },
                        |item| item.name.clone(),
                    );
                let started_at_epoch = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_secs())
                    .unwrap_or(0)
                    .saturating_sub(play_time_seconds);
                self.last_run = Some(crate::core::diagnose::RunRecord {
                    instance_id,
                    instance_name,
                    instance_dir: instance_directory,
                    log_path,
                    crash_report,
                    exit_code,
                    duration_secs: play_time_seconds,
                    started_at_epoch,
                    launch_command: Some(launch_command),
                    log_scope: crate::core::diagnose::LogScope::Run,
                });
                self.launch_seq += 1;
                let seq = self.launch_seq;
                if let Some(run) = self.last_run.clone() {
                    self.start_diagnosis(seq, run);
                }
                // The process is gone: hand the row back to the disk instead of
                // leaving `运行中` behind. The helper only rewrites rows still
                // marked live, so it cannot touch an idle instance.
                self.sync_launch_status();
            }
            AppEvent::LaunchAborted { error } => {
                self.active_launch = None;
                self.sync_launch_status();
                self.record_launcher_log(&error);
                self.status = Status::error(error);
                // The launch may have retired a credential that cannot be
                // traded again (`Error::AccountExpired`); the account page has
                // to show `需重新登录` now, not after the next full reload.
                let _ = self.reload_accounts();
            }
        }
    }

    pub(in crate::tui::app) fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if self.modal.is_some() {
            self.handle_modal_key(key);
            return;
        }

        let command = crate::tui::command::from_key_with_task(
            self.mode(),
            self.page,
            self.task_key_state(),
            key,
        );
        self.run_command(command);
    }

    fn handle_modal_key(&mut self, key: KeyEvent) {
        let result = self.modal.as_mut().expect("modal exists").handle_key(key);
        self.settle_modal_result(result);
    }

    /// Applies one ModalResult to the app. Both the keyboard and the pointer
    /// end here, so a dialog action means the same thing whichever way it came.
    pub(in crate::tui::app) fn settle_modal_result(&mut self, result: ModalResult) {
        match result {
            ModalResult::Continue => {}
            ModalResult::BrowseDirectory(initial) => {
                self.directory_browse_request = Some(initial);
            }
            ModalResult::BrowseFile(initial) => {
                self.modpack_browse_request = Some(initial);
            }
            ModalResult::Cancel | ModalResult::Reject => {
                let (resource_confirmation, modpack_staging) = match self.modal.as_ref() {
                    Some(Modal::Form {
                        action: FormAction::InstallResource,
                        ..
                    }) => (true, None),
                    // Only a downloaded archive lives at this path; a locally
                    // picked one is the player's own file, and its form action
                    // (`ImportLocalModpack`) deliberately falls through below.
                    Some(Modal::Form {
                        action: FormAction::InstallModpack,
                        values,
                        ..
                    }) => (false, values.first().cloned()),
                    _ => (false, None),
                };
                self.modal = None;
                self.filter_open = false;
                self.version_filter_open = false;
                if resource_confirmation {
                    self.pending_resource_file = None;
                }
                if let Some(staging) = modpack_staging {
                    let _ = std::fs::remove_file(staging);
                    self.status = Status::info(self.i18n.text("status.modpack_cancelled"));
                }
            }
            ModalResult::Submit(action, values) => match self.submit_form(action, values) {
                Ok(()) => {
                    self.modal = None;
                }
                Err(error) => self.status = Status::error(error.to_string()),
            },
            ModalResult::OpenSelectionPicker { field } => {
                self.open_selection_picker(field);
            }
            ModalResult::RetrySelectionPicker { field } => {
                self.retry_selection_picker(field);
            }
            ModalResult::SelectionChanged { field, .. } => {
                if let Some((minecraft_version, kind)) = self.create_selection_changed(field) {
                    self.spawn_loader_version_fetch(minecraft_version, kind);
                }
            }
            ModalResult::Confirm(action) => {
                self.modal = None;
                if let Err(error) = self.confirm_action(action) {
                    self.status = Status::error(error.to_string());
                }
            }
            ModalResult::FilterRetry => {
                self.retry_filter_dialog();
            }
            ModalResult::FilterApply(selected) => {
                if self.version_filter_open {
                    self.version_filter_open = false;
                    self.modal = None;
                    self.filter_open = false;
                    self.apply_version_filter(&selected);
                    return;
                }
                self.download_center.categories = selected;
                self.download_center.page = 0;
                self.modal = None;
                self.filter_open = false;
                self.spawn_resource_search();
            }
        }
    }

    pub(in crate::tui::app) fn run_command(&mut self, command: crate::tui::command::Command) {
        use crate::tui::command::Command;
        match command {
            Command::NextTab => self.next_page(),
            Command::PrevTab => self.previous_page(),
            Command::Quit => self.quit = true,
            Command::Help => {
                self.help_open = true;
                self.help_scroll = 0;
            }
            Command::Diagnose => self.toggle_diagnosis(),
            // `\0` is Enter: run the first remedy the rule offers.
            Command::Remedy(cap) => self.run_remedy_cap(cap),
            Command::ExportDiagnosis => self.begin_diagnosis_export(),
            Command::LocateInLog => self.locate_in_log(),
            Command::Back => self.go_back(),
            Command::PrevRow => self.scroll_vertical(-1, false),
            Command::NextRow => self.scroll_vertical(1, false),
            Command::PrevPage => {
                let rows = if self.help_open {
                    self.help_page_rows()
                } else if self.diagnosis_open {
                    self.diagnosis_page_rows()
                } else {
                    8
                };
                self.scroll_vertical(-rows, true);
            }
            Command::NextPage => {
                let rows = if self.help_open {
                    self.help_page_rows()
                } else if self.diagnosis_open {
                    self.diagnosis_page_rows()
                } else {
                    8
                };
                self.scroll_vertical(rows, true);
            }
            Command::Top => self.jump_vertical(true),
            Command::Bottom => self.jump_vertical(false),
            Command::Activate => self.activate_current(),
            Command::Launch => {
                if self.workspace_is_open() {
                    self.launch_workspace_instance();
                } else if self.page == Page::Instances {
                    // The cursor is only meaningful where it is drawn; on every
                    // other page the launch target is the instance marked 当前.
                    self.launch_selected_instance();
                } else {
                    self.launch_current_instance();
                }
            }
            Command::SetCurrent => self.set_focused_as_current(),
            Command::Create => self.open_create_form(),
            Command::Import => {
                if self.workspace_has_content_category() {
                    self.open_workspace_import();
                } else {
                    self.open_import_form();
                }
            }
            Command::ImportModpack => self.open_local_modpack_form(),
            Command::Rename => self.page_refresh_or_rename(),
            Command::Delete => {
                if self.workspace_has_content_category() {
                    self.delete_workspace_content();
                } else {
                    self.delete_item();
                }
            }
            Command::OpenFolder => self.open_workspace_folder(),
            Command::ExportScript => self.export_launch_script(),
            Command::OpenDatapacks => self.open_workspace_datapacks(),
            Command::CheckModUpdates => self.check_mod_updates(),
            Command::UpdateModpack => self.open_modpack_update_form(),
            Command::NewAccount => self.open_offline_account_form(),
            Command::MicrosoftLogin => self.start_microsoft_login(),
            Command::Search => self.start_search(),
            Command::Source => self.switch_download_source(),
            Command::Sort => {
                if self.workspace_is_open() {
                    self.cycle_workspace_sort();
                } else {
                    self.cycle_resource_sort();
                }
            }
            Command::Target => self.open_download_target_picker(),
            Command::FilterCategories => self.open_filter_dialog(),
            Command::PrevBlock if self.workspace_is_open() => self.move_workspace_category(-1),
            Command::NextBlock if self.workspace_is_open() => self.move_workspace_category(1),
            Command::PrevBlock => match self.page {
                Page::Instances => self.cycle_instance_filter(-1),
                Page::Tasks => self.change_task_filter(-1),
                Page::Settings => self.change_settings_category(-1),
                Page::DownloadCenter => self.change_download_category(-1),
                _ => {}
            },
            Command::NextBlock => match self.page {
                Page::Instances => self.cycle_instance_filter(1),
                Page::Tasks => self.change_task_filter(1),
                Page::Settings => self.change_settings_category(1),
                Page::DownloadCenter => self.change_download_category(1),
                _ => {}
            },
            Command::Refresh => self.page_refresh_or_rename(),
            Command::RescanWorkspace => self.rescan_visible_workspace_content(),
            Command::ToggleVersionCategory(index) => self.toggle_version_category(index),
            Command::Pause => self.pause_selected_task(),
            Command::Resume => self.resume_selected_task(),
            Command::Retry => self.retry_selected_task(),
            Command::ToggleFollow => {
                if self.workspace_has_content_category() {
                    self.toggle_workspace_content();
                } else if self.workspace_is_open() {
                    self.toggle_workspace_follow();
                } else {
                    self.toggle_log_follow();
                }
            }
            Command::ResetOverrides => self.reset_workspace_overrides(),
            Command::ReorderContent(direction) => {
                self.reorder_workspace_content(isize::from(direction));
            }
            Command::BackupWorld => self.backup_workspace_world(),
            Command::RestoreWorld => self.open_restore_picker(),
            Command::CloneWorld => self.clone_workspace_world(),
            Command::FollowBottom => self.follow_log_bottom(),
            Command::Clear => self.clear_logs(),
            Command::ActivateSetting => self.activate_settings_selection(),
            Command::AdjustSetting(direction) => {
                if self.workspace_is_open() {
                    self.step_workspace_value(direction);
                } else if self.page == Page::Settings {
                    self.adjust_current_setting(direction);
                }
            }
            Command::SettingAction(character) => self.run_setting_shortcut(character),
            Command::Input(character) => self.push_search_input(character),
            Command::Backspace => self.pop_search_input(),
            Command::CursorLeft => self.move_search_cursor(-1),
            Command::CursorRight => self.move_search_cursor(1),
            Command::CursorHome => self.move_search_cursor(isize::MIN / 2),
            Command::CursorEnd => self.move_search_cursor(isize::MAX / 2),
            Command::DeleteForward => self.delete_search_input(),
            Command::Submit => self.submit_search_input(),
            Command::Cancel => self.cancel_search_input(),
            Command::Nothing => {}
        }
    }

    fn activate_current(&mut self) {
        if self.workspace_is_open() {
            self.activate_workspace_row();
            return;
        }
        if self.detail_open {
            if self.page == Page::DownloadCenter {
                if self.resource_detail.is_some() {
                    self.submit_selected_resource_task();
                } else if let Some(version) = self.selected_version().map(|v| v.id.clone()) {
                    // Never stack the creation form on top of the details popup.
                    self.detail_open = false;
                    self.open_create_form_with_version(version);
                }
            }
            return;
        }
        self.activate_selection();
    }

    /// Where one vertical move goes.
    ///
    /// The overlay wins over the page underneath it: with the help card open the
    /// arrows scroll the reference. Checking the workspace first would move the
    /// hidden list behind the card instead.
    fn scroll_vertical(&mut self, rows: isize, page: bool) {
        if self.help_open {
            self.scroll_help(rows);
        } else if self.diagnosis_open {
            self.scroll_diagnosis(rows);
        } else if self.workspace_is_open() {
            self.move_workspace_row(rows);
        } else if self.detail_open {
            self.move_detail_scroll(rows);
        } else if page {
            self.scroll_active_page(rows.signum());
        } else {
            self.move_selection(rows);
        }
    }

    /// `Home` / `End`: the top or the bottom of whatever owns the arrows now.
    fn jump_vertical(&mut self, top: bool) {
        if self.help_open {
            self.help_scroll = if top { 0 } else { self.help_max_scroll() };
        } else if self.diagnosis_open {
            self.diagnosis_scroll = if top { 0 } else { self.diagnosis_max_scroll() };
        } else if self.workspace_is_open() {
            self.jump_workspace_edge(top);
        } else {
            self.jump_active_edge(top);
        }
    }

    fn scroll_active_page(&mut self, direction: isize) {
        if self.detail_open {
            self.move_detail_scroll(direction * 8);
            return;
        }
        if self.page == Page::DownloadCenter {
            self.page_download_center(direction);
            return;
        }
        let step = direction * 10;
        match self.page {
            Page::Overview | Page::Instances => self.move_instance_focus(step),
            Page::Tasks => {
                let count = self.visible_task_indices().len();
                let selected = move_bounded(self.task_selected, count, step);
                self.set_task_selection(selected);
            }
            Page::Accounts => {
                self.accounts.selected =
                    move_bounded(self.accounts.selected, self.accounts.accounts.len(), step);
            }
            Page::Settings => self.move_settings_page(direction),
            Page::Logs => {
                let max = self.log_max_scroll_offset();
                self.logs.scroll(step, max);
            }
            Page::DownloadCenter => {}
        }
    }

    fn move_settings_page(&mut self, direction: isize) {
        if self.settings.category == SettingsCategory::Java {
            self.java_selected = move_bounded(self.java_selected, self.java.len(), direction * 10);
            return;
        }
        let rows = self.settings_row_count();
        self.settings.move_selection(rows, direction * 10);
    }

    fn jump_active_edge(&mut self, top: bool) {
        if self.detail_open {
            self.detail_scroll = if top { 0 } else { self.detail_max_scroll() };
            return;
        }
        if self.page != Page::DownloadCenter {
            match self.page {
                Page::Overview => {
                    let count = self.instances.items.len();
                    if count > 0 {
                        self.instances.focused = if top { 0 } else { count - 1 };
                    }
                }
                Page::Instances => {
                    let visible = self.visible_instance_indices();
                    if let Some(index) = if top {
                        visible.first().copied()
                    } else {
                        visible.last().copied()
                    } {
                        self.instances.focused = index;
                    }
                }
                Page::Tasks => {
                    let count = self.visible_task_indices().len();
                    if count > 0 {
                        self.set_task_selection(if top { 0 } else { count - 1 });
                    }
                }
                Page::Accounts => {
                    let count = self.accounts.accounts.len();
                    if count > 0 {
                        self.accounts.selected = if top { 0 } else { count - 1 };
                    }
                }
                Page::Settings => {
                    if self.settings.category == SettingsCategory::Java {
                        if !self.java.is_empty() {
                            self.java_selected = if top { 0 } else { self.java.len() - 1 };
                        }
                    } else {
                        let rows = self.settings_row_count();
                        if rows > 0 {
                            self.settings
                                .move_selection(rows, if top { -1000 } else { 1000 });
                        }
                    }
                }
                Page::Logs => {
                    let max = self.log_max_scroll_offset();
                    if top {
                        self.logs.scroll(-(max as isize), max);
                    } else {
                        self.logs.set_follow(true, max);
                    }
                }
                Page::DownloadCenter => {}
            }
            return;
        }
        if self.download_center.category == DownloadCategory::GameVersions {
            let count = self.filtered_version_indices().len();
            self.version_selected = if top { 0 } else { count.saturating_sub(1) };
            self.keep_version_selected_visible();
            if top {
                self.reset_detail_scroll();
            }
        } else if top && self.download_center.page != 0 {
            self.download_center.page = 0;
            self.resource_projects.clear();
            self.resource_files.clear();
            self.resource_detail = None;
            self.resource_detail_project = None;
            self.spawn_resource_search();
        }
    }

    fn delete_item(&mut self) {
        if let Some(category) = self.workspace.as_ref().map(|workspace| workspace.category) {
            use crate::tui::pages::workspace::WorkspaceCategory;
            match category {
                WorkspaceCategory::Install => {
                    self.uninstall_workspace_loader();
                    return;
                }
                WorkspaceCategory::Manage => {
                    // The list cursor may point at another instance; the
                    // confirmation must name the one the workspace shows.
                    self.open_workspace_remove_confirmation();
                    return;
                }
                // Content categories are routed to the trash before this point;
                // the read-only screens have no delete action.
                _ => return,
            }
        }
        match self.page {
            Page::Tasks => self.delete_selected_task(),
            Page::Settings if self.settings.category.is_management() => {
                self.open_remove_confirmation();
            }
            _ => self.open_remove_confirmation(),
        }
    }

    pub(in crate::tui::app) fn start_search(&mut self) {
        match self.page {
            // Both card shells keep the search field on screen; focusing it is
            // enough. Only `classic` still needs the prompt dialog, because it
            // has no inline field to type into.
            Page::DownloadCenter if self.preset() != UiPreset::Classic => {
                self.focus_download_search();
            }
            Page::DownloadCenter => self.open_download_search(),
            Page::Logs => {
                self.log_filter = Some(self.log_filter.take().unwrap_or_default());
                self.search = Some(SearchState::new(SearchTarget::Logs));
            }
            Page::Instances if self.workspace_has_content_category() => {
                // The field keeps whatever it already holds: clicking it to
                // refine a query must not wipe the list first.
                let length = self
                    .workspace
                    .as_ref()
                    .map_or(0, |workspace| workspace.search.chars().count());
                self.search = Some(SearchState {
                    cursor: length,
                    ..SearchState::new(SearchTarget::Workspace)
                });
            }
            Page::Settings => {
                self.settings_search = Some(String::new());
                self.settings.selected = 0;
                self.search = Some(SearchState::new(SearchTarget::Settings));
            }
            _ => {}
        }
    }

    fn push_search_input(&mut self, character: char) {
        let Some(target) = self.search.as_ref().map(|state| state.target) else {
            return;
        };
        let cursor = self.search.as_ref().map_or(0, |state| state.cursor);
        match target {
            SearchTarget::Logs => {
                let value = self.log_filter.get_or_insert_with(String::new);
                *value = crate::tui::input::insert_at(value, cursor, character);
            }
            SearchTarget::VersionFilter => {
                let value = crate::tui::input::insert_at(
                    self.download_center.version_filter.search(),
                    cursor,
                    character,
                );
                self.download_center.version_filter.set_search(&value);
                self.reset_version_selection();
            }
            SearchTarget::Resources => {
                self.download_center.search =
                    crate::tui::input::insert_at(&self.download_center.search, cursor, character);
            }
            SearchTarget::Settings => {
                let value = self.settings_search.get_or_insert_with(String::new);
                *value = crate::tui::input::insert_at(value, cursor, character);
                self.settings.selected = 0;
            }
            SearchTarget::Workspace => {
                if let Some(workspace) = self.workspace.as_mut() {
                    workspace.search =
                        crate::tui::input::insert_at(&workspace.search, cursor, character);
                    workspace.selected = 0;
                }
            }
        }
        if let Some(state) = self.search.as_mut() {
            state.cursor = cursor.saturating_add(1);
        }
    }

    fn move_search_cursor(&mut self, delta: isize) {
        let Some(state) = self.search.as_ref() else {
            return;
        };
        let target = state.target;
        let length = match target {
            SearchTarget::Logs => self
                .log_filter
                .as_deref()
                .map_or(0, |value| value.chars().count()),
            SearchTarget::Resources => self.download_center.search.chars().count(),
            SearchTarget::Settings => self
                .settings_search
                .as_deref()
                .map_or(0, |value| value.chars().count()),
            SearchTarget::Workspace => self
                .workspace
                .as_ref()
                .map_or(0, |workspace| workspace.search.chars().count()),
            SearchTarget::VersionFilter => {
                self.download_center.version_filter.search().chars().count()
            }
        };
        let cursor = state.cursor.min(length);
        let next = if delta < 0 {
            cursor.saturating_sub(delta.unsigned_abs())
        } else {
            cursor.saturating_add(delta as usize).min(length)
        };
        if let Some(state) = self.search.as_mut() {
            state.cursor = next;
        }
    }

    /// Forward delete: removes the character under the cursor, leaving the
    /// insertion point where it is.
    fn delete_search_input(&mut self) {
        let Some(target) = self.search.as_ref().map(|state| state.target) else {
            return;
        };
        let cursor = self.search.as_ref().map_or(0, |state| state.cursor);
        match target {
            SearchTarget::Logs => {
                if let Some(value) = self.log_filter.as_mut() {
                    *value = crate::tui::input::remove_at(value, cursor);
                }
            }
            SearchTarget::VersionFilter => {
                let value = crate::tui::input::remove_at(
                    self.download_center.version_filter.search(),
                    cursor,
                );
                self.download_center.version_filter.set_search(&value);
                self.reset_version_selection();
            }
            SearchTarget::Resources => {
                self.download_center.search =
                    crate::tui::input::remove_at(&self.download_center.search, cursor);
            }
            SearchTarget::Settings => {
                if let Some(value) = self.settings_search.as_mut() {
                    *value = crate::tui::input::remove_at(value, cursor);
                }
                self.settings.selected = 0;
            }
            SearchTarget::Workspace => {
                if let Some(workspace) = self.workspace.as_mut() {
                    workspace.search = crate::tui::input::remove_at(&workspace.search, cursor);
                    workspace.selected = 0;
                }
            }
        }
    }

    fn pop_search_input(&mut self) {
        let Some(target) = self.search.as_ref().map(|state| state.target) else {
            return;
        };
        let cursor = self.search.as_ref().map_or(0, |state| state.cursor);
        match target {
            SearchTarget::Logs => {
                if let Some(value) = self.log_filter.as_mut() {
                    *value = crate::tui::input::remove_before(value, cursor);
                }
            }
            SearchTarget::VersionFilter => {
                let value = crate::tui::input::remove_before(
                    self.download_center.version_filter.search(),
                    cursor,
                );
                self.download_center.version_filter.set_search(&value);
                self.reset_version_selection();
            }
            SearchTarget::Resources => {
                self.download_center.search =
                    crate::tui::input::remove_before(&self.download_center.search, cursor);
            }
            SearchTarget::Settings => {
                if let Some(value) = self.settings_search.as_mut() {
                    *value = crate::tui::input::remove_before(value, cursor);
                }
                self.settings.selected = 0;
            }
            SearchTarget::Workspace => {
                if let Some(workspace) = self.workspace.as_mut() {
                    workspace.search = crate::tui::input::remove_before(&workspace.search, cursor);
                    workspace.selected = 0;
                }
            }
        }
        if let Some(state) = self.search.as_mut() {
            state.cursor = cursor.saturating_sub(1);
        }
    }

    pub(in crate::tui::app) fn submit_search_input(&mut self) {
        let target = self.search.take().map(|state| state.target);
        self.search_backup = None;
        if target == Some(SearchTarget::Settings) {
            self.jump_to_settings_search_hit();
            return;
        }
        let Some(SearchTarget::Resources) = target else {
            return;
        };
        // Enter runs the search from the first page; the provider never saw the
        // text typed since the last submit, so trimming it here is safe.
        let query = self.download_center.search.trim().to_owned();
        self.download_center.search = query.clone();
        self.resource_projects.clear();
        self.resource_files.clear();
        self.resource_detail = None;
        self.resource_detail_project = None;
        self.download_center.total = 0;
        self.download_center.page = 0;
        self.download_center.selected = 0;
        self.reset_detail_scroll();
        self.status = Status::info(if query.is_empty() {
            self.i18n.text("status.search_cleared")
        } else {
            self.i18n
                .format("status.search_applied", &[("query", &query)])
        });
        self.spawn_resource_search();
    }

    /// Leaving the field with the mouse commits what was typed.
    ///
    /// `Esc` is a cancel and throws the query away; a click elsewhere means "I
    /// am done here", not "forget it". Settings is the exception: there `Enter`
    /// jumps to the first hit and a stray click must not move the selection, so
    /// the field is dropped untouched.
    pub(in crate::tui::app) fn blur_search(&mut self) {
        let Some(target) = self.search.as_ref().map(|state| state.target) else {
            return;
        };
        if target == SearchTarget::Settings {
            self.search = None;
            self.search_backup = None;
            return;
        }
        self.submit_search_input();
    }

    fn cancel_search_input(&mut self) {
        let target = self.search.take().map(|state| state.target);
        let backup = self.search_backup.take();
        match target {
            Some(SearchTarget::Logs) => self.log_filter = None,
            Some(SearchTarget::VersionFilter) => {
                self.download_center
                    .version_filter
                    .set_search(backup.as_deref().unwrap_or_default());
                self.reset_version_selection();
            }
            Some(SearchTarget::Workspace) => {
                if let Some(workspace) = self.workspace.as_mut() {
                    workspace.search.clear();
                    workspace.selected = 0;
                }
            }
            // Cancelled text never reached the provider, so restore the query
            // the visible results were produced with.
            Some(SearchTarget::Resources) => {
                if let Some(backup) = backup {
                    self.download_center.search = backup;
                }
            }
            Some(SearchTarget::Settings) => self.settings_search = None,
            None => {}
        }
    }

    fn clear_logs(&mut self) {
        self.logs.clear();
        self.status = Status::success(self.i18n.text("status.logs_cleared"));
    }
}
