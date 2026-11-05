//! World actions: backup, restore, clone.
//!
//! All three touch whole directories that can be hundreds of megabytes, so each
//! one hands the work to a blocking thread and reports through
//! [`AppEvent::WorldJob`](crate::tui::event::AppEvent::WorldJob). The status line
//! says what is happening while it happens; there is no progress bar, but nothing
//! in the UI freezes either.

use super::*;

impl App {
    /// The world entry under the cursor, if the worlds category is showing.
    pub(in crate::tui::app) fn current_workspace_world(
        &self,
    ) -> Option<crate::core::content::ContentEntry> {
        let workspace = self.workspace.as_ref()?;
        if workspace.category != crate::tui::pages::workspace::WorkspaceCategory::Worlds {
            return None;
        }
        self.current_workspace_entry()
    }

    /// Refuse world jobs while this instance is running: the game writes into
    /// the same directory, so a backup would capture a half-written world.
    fn world_job_blocked(&mut self) -> bool {
        if self.workspace_instance_running() {
            self.status = Status::warning(self.i18n.text("workspace.status.running_locked"));
            return true;
        }
        false
    }

    pub(in crate::tui::app) fn backup_workspace_world(&mut self) {
        let Some(entry) = self.current_workspace_world() else {
            return;
        };
        if self.world_job_blocked() {
            return;
        }
        let Some(root) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.root.clone())
        else {
            return;
        };
        let sender = self.sender.clone();
        let world = entry.path.clone();
        self.status = Status::info(self.i18n.text("workspace.world.backing_up"));
        tokio::task::spawn_blocking(move || {
            let outcome = crate::core::content::backup::backup_world(&root, &world)
                .map(crate::tui::event::WorldJobOutcome::BackedUp)
                .map_err(|error| error.to_string());
            let _ = sender.send(crate::tui::event::AppEvent::WorldJob(outcome));
        });
    }

    pub(in crate::tui::app) fn clone_workspace_world(&mut self) {
        let Some(entry) = self.current_workspace_world() else {
            return;
        };
        if self.world_job_blocked() {
            return;
        }
        let sender = self.sender.clone();
        let world = entry.path.clone();
        let suffix = self.i18n.text("workspace.world.clone_suffix");
        self.status = Status::info(self.i18n.text("workspace.world.cloning"));
        tokio::task::spawn_blocking(move || {
            let outcome = crate::core::content::backup::clone_world(&world, &suffix)
                .map(crate::tui::event::WorldJobOutcome::Cloned)
                .map_err(|error| error.to_string());
            let _ = sender.send(crate::tui::event::AppEvent::WorldJob(outcome));
        });
    }

    /// `r`: list the backups of this world and let the player pick one.
    pub(in crate::tui::app) fn open_restore_picker(&mut self) {
        let Some(entry) = self.current_workspace_world() else {
            return;
        };
        let Some(root) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.root.clone())
        else {
            return;
        };
        let world_name = entry.display_file_name();
        let backups = crate::core::content::backup::list_backups(&root, &world_name);
        if backups.is_empty() {
            self.status = Status::warning(self.i18n.text("workspace.world.no_backups"));
            return;
        }
        // `label|path`: the label is what the player reads, the path is what the
        // submit handler needs, and neither has to survive a round trip through
        // the list index.
        let options = backups
            .iter()
            .map(|backup| {
                format!(
                    "{} · {}|{}",
                    backup.stamp,
                    format_size(backup.size),
                    backup.path.display()
                )
            })
            .collect::<Vec<_>>();
        let modal = Modal::form(
            self.i18n.text("workspace.world.pick_backup"),
            &[self.i18n.text("workspace.world.pick_backup_label")],
            &[options[0].clone()],
            crate::tui::modal::FormAction::RestoreWorld,
        )
        .with_selection_picker(0, options);
        self.modal = Some(modal);
    }

    /// Run the restore the picker returned.
    pub(in crate::tui::app) fn restore_workspace_world(&mut self, archive: std::path::PathBuf) {
        let Some(entry) = self.current_workspace_world() else {
            return;
        };
        if self.world_job_blocked() {
            return;
        }
        let Some(root) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.root.clone())
        else {
            return;
        };
        let sender = self.sender.clone();
        let world = entry.path.clone();
        self.status = Status::info(self.i18n.text("workspace.world.restoring"));
        tokio::task::spawn_blocking(move || {
            let outcome = crate::core::content::backup::restore_world(&root, &world, &archive)
                .map(|safety| crate::tui::event::WorldJobOutcome::Restored { safety })
                .map_err(|error| error.to_string());
            let _ = sender.send(crate::tui::event::AppEvent::WorldJob(outcome));
        });
    }

    /// Report a finished world job and refresh the list it changed.
    pub(in crate::tui::app) fn finish_world_job(
        &mut self,
        result: std::result::Result<crate::tui::event::WorldJobOutcome, String>,
    ) {
        match &result {
            Ok(outcome) => {
                let key = match outcome {
                    crate::tui::event::WorldJobOutcome::BackedUp(_) => "workspace.world.backed_up",
                    crate::tui::event::WorldJobOutcome::Restored { .. } => {
                        "workspace.world.restored"
                    }
                    crate::tui::event::WorldJobOutcome::Cloned(_) => "workspace.world.cloned",
                };
                self.status = Status::success(self.i18n.format(key, &[("name", &outcome.name())]));
            }
            Err(error) => {
                self.status = Status::error(
                    self.i18n
                        .format("workspace.world.job_failed", &[("error", error)]),
                );
            }
        }
        self.rescan_workspace_content(crate::core::content::ContentKind::World);
    }
}
