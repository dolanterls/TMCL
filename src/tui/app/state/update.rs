//! AppEvent handling and background result application.

use super::*;

impl App {
    pub(in crate::tui::app) fn apply_task_snapshots(&mut self, snapshots: Vec<TaskSnapshot>) {
        let previous = self.task_snapshots.clone();
        self.set_task_snapshots(snapshots);
        // A task that started or ended changes whether an instance is complete.
        self.sync_install_statuses();

        // Speed is UI state: sample it when a snapshot arrives, forget it the
        // moment the task stops running so a finished row never shows a rate.
        let now = std::time::Instant::now();
        for snapshot in &self.task_snapshots {
            if snapshot.state == TaskState::Running {
                self.speed_tracker
                    .observe(snapshot.id, snapshot.completed, now);
            } else {
                self.speed_tracker.forget(snapshot.id);
            }
        }

        let newly_succeeded = self
            .task_snapshots
            .iter()
            .filter(|snapshot| snapshot.state == TaskState::Succeeded)
            .filter(|snapshot| {
                previous
                    .iter()
                    .find(|old| old.id == snapshot.id)
                    .is_none_or(|old| old.state != TaskState::Succeeded)
            })
            .cloned()
            .collect::<Vec<_>>();

        for snapshot in newly_succeeded {
            match self.task_engine.operation(snapshot.id) {
                Some(TaskOperation::JavaRuntime(spec)) => {
                    let executable =
                        crate::core::java::runtime::runtime_root(&spec.data_root, spec.major).join(
                            if cfg!(windows) {
                                "bin/java.exe"
                            } else {
                                "bin/java"
                            },
                        );
                    let mut next = self.config.clone();
                    next.java.selected_path = Some(executable.clone());
                    if let Err(error) = self.commit_config(next) {
                        self.status = Status::error(self.i18n.format(
                            "status.java_save_failed",
                            &[
                                ("major", &spec.major.to_string()),
                                ("error", &error.to_string()),
                            ],
                        ));
                    } else {
                        // Silent: the install message below outranks a count.
                        self.start_java_scan();
                        self.status = Status::info(self.i18n.format(
                            "status.java_installed",
                            &[
                                ("major", &spec.major.to_string()),
                                ("path", &display_path(&executable)),
                            ],
                        ));
                    }
                }
                Some(TaskOperation::InstanceInstall(spec)) => {
                    // The install rewrites the client json; the workspace draft
                    // is a copy read before that, so it must be refreshed.
                    if self
                        .workspace
                        .as_ref()
                        .is_some_and(|workspace| workspace.instance == spec.instance_id)
                    {
                        self.refresh_workspace_draft();
                    }
                    // Same job, later step: a modpack's artwork landed during
                    // `prepare` and the row may already hold a placeholder.
                    self.refresh_cover(&spec.instance_directory);
                    if let Err(error) = self.reload() {
                        self.status = Status::error(self.i18n.format(
                            "status.task_refresh_failed",
                            &[("error", &error.to_string())],
                        ));
                    } else if self
                        .instances
                        .items
                        .iter()
                        .any(|item| item.id == spec.instance_id)
                    {
                        self.instances.set_current(Some(spec.instance_id));
                        self.config.state.current_instance_id = Some(spec.instance_id.to_string());
                        if let Err(error) = self.save_config() {
                            self.status = Status::error(self.i18n.format(
                                "status.install_save_failed",
                                &[("error", &error.to_string())],
                            ));
                        } else {
                            self.download_target_id = Some(spec.instance_id);
                            // A modpack install finishes its loader step long
                            // before the pack itself is on disk. Reporting the
                            // whole install as done there told the player the
                            // install was over while a gigabyte was still moving.
                            let job_pending = snapshot.job.is_some_and(|job| {
                                self.task_snapshots
                                    .iter()
                                    .any(|other| other.job == Some(job) && other.state.is_active())
                            });
                            self.status = if job_pending {
                                Status::info(self.i18n.text("status.modpack_files_pending"))
                            } else {
                                Status::success(self.i18n.text("status.minecraft_install_done"))
                            };
                        }
                    }
                }
                Some(TaskOperation::ResourceInstall(_))
                | Some(TaskOperation::ResourceSaveAs(_)) => {
                    self.status = Status::success(self.i18n.text("status.resource_completed"));
                }
                Some(TaskOperation::ModpackInstall(spec)) => {
                    self.finish_modpack_install(spec);
                }
                Some(TaskOperation::ModpackFiles(_)) => {
                    self.status = Status::success(self.i18n.text("status.modpack_files_done"));
                }
                Some(TaskOperation::InstanceRepair(spec)) => {
                    if let Err(error) = self.reload() {
                        self.pending_launch = None;
                        self.status = Status::error(self.i18n.format(
                            "status.task_refresh_failed",
                            &[("error", &error.to_string())],
                        ));
                    } else {
                        self.finish_repair(spec.instance_id, true);
                    }
                }
                Some(TaskOperation::Unrecoverable { .. }) | None => {}
            }
        }

        let failed_repairs = self
            .task_snapshots
            .iter()
            .filter(|snapshot| {
                matches!(snapshot.state, TaskState::Failed | TaskState::Cancelled)
                    && previous
                        .iter()
                        .find(|old| old.id == snapshot.id)
                        .is_none_or(|old| {
                            !matches!(old.state, TaskState::Failed | TaskState::Cancelled)
                        })
            })
            .map(|snapshot| (snapshot.id, snapshot.failure.clone()))
            .collect::<Vec<_>>();
        for (id, detail) in failed_repairs {
            if let Some(TaskOperation::InstanceRepair(spec)) = self.task_engine.operation(id) {
                self.finish_repair_with_detail(spec.instance_id, false, detail.as_deref());
            }
        }
    }

    /// Applies the result of an `InstanceRepair` task.
    ///
    /// A repair the user did not trigger from a launch leaves the selection
    /// alone; one that was blocking a launch continues that launch.
    pub(in crate::tui::app) fn finish_repair(&mut self, instance_id: Uuid, succeeded: bool) {
        self.finish_repair_with_detail(instance_id, succeeded, None);
    }

    /// Same as [`Self::finish_repair`], but keeps the task's failure text so the
    /// status bar can name the files and the launcher log keeps the full detail.
    pub(in crate::tui::app) fn finish_repair_with_detail(
        &mut self,
        instance_id: Uuid,
        succeeded: bool,
        detail: Option<&str>,
    ) {
        let waiting = self.pending_launch == Some(instance_id);
        self.pending_launch = None;
        if !succeeded {
            if let Some(detail) = detail.filter(|value| !value.is_empty()) {
                self.record_launcher_log(&format!("Instance repair failed: {detail}"));
                let offline = detail.contains(crate::core::launch::failure::OFFLINE_MARKER);
                let message = detail
                    .strip_prefix(crate::core::launch::failure::OFFLINE_MARKER)
                    .map(str::trim)
                    .unwrap_or(detail);
                self.status = if offline {
                    Status::error(
                        self.i18n
                            .format("status.repair_offline", &[("error", message)]),
                    )
                } else {
                    Status::error(
                        self.i18n
                            .format("status.repair_failed_detail", &[("error", detail)]),
                    )
                };
            } else {
                self.status = Status::error(self.i18n.text("status.repair_failed"));
            }
            return;
        }
        self.status = Status::success(self.i18n.text("status.repair_done"));
        if waiting {
            self.instances.set_current(Some(instance_id));
            self.launch_after_repair = true;
            self.launch_current_instance();
        }
    }

    pub(in crate::tui::app) fn set_task_snapshots(&mut self, snapshots: Vec<TaskSnapshot>) {
        let previous_id = self.task_selected_id;
        let previous_index = self.task_selected;
        self.task_snapshots = snapshots;
        let visible = self.visible_task_indices();
        let selected_index = previous_id
            .and_then(|id| {
                visible.iter().position(|index| {
                    self.task_snapshots
                        .get(*index)
                        .is_some_and(|snapshot| snapshot.id == id)
                })
            })
            .unwrap_or_else(|| previous_index.min(visible.len().saturating_sub(1)));
        self.task_selected = selected_index;
        self.task_selected_id = self
            .task_snapshots
            .get(visible.get(selected_index).copied().unwrap_or(usize::MAX))
            .map(|snapshot| snapshot.id);
    }

    pub(crate) fn subscribe_task_snapshots(
        &self,
    ) -> tokio::sync::watch::Receiver<Vec<TaskSnapshot>> {
        self.task_engine.subscribe()
    }

    /// Pulls the current task snapshots out of the engine and applies them.
    ///
    /// The watch channel is used as a wake-up signal, not as the data path.
    /// Reading the channel hands out a `Ref` that holds the value's read lock,
    /// and publishers take that same lock while holding the engine state
    /// mutex, so the loop must not keep a `Ref` around while it calls back
    /// into the engine. Asking for a fresh copy sidesteps the whole class of
    /// problem - and it is what HMCL does, where a finished task notifies the
    /// UI and the UI reads the task's own properties afterwards.
    pub(crate) fn refresh_task_snapshots(&mut self) {
        let snapshots = self.task_engine.snapshots();
        self.apply_task_snapshots(snapshots);
    }

    pub(in crate::tui::app) fn download_snapshot(
        &self,
    ) -> crate::core::download::DownloadSourceSnapshot {
        self.config.download_snapshot(&self.paths)
    }
}
