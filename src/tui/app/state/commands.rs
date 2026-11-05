//! Cross-page commands: activation, selection and refresh.

use super::*;

impl App {
    pub fn set_status(&mut self, status: impl Into<String>) {
        let status = status.into();
        self.record_launcher_log(&status);
        self.status = Status::info(status);
    }

    pub(in crate::tui::app) fn record_launcher_log(&mut self, line: &str) {
        if !self.config.logs.launcher_enabled {
            return;
        }
        self.launcher_logs.append(line.to_owned());
        let _ = std::fs::create_dir_all(&self.paths.launcher_logs_dir);
        let path = self.paths.launcher_logs_dir.join("launcher.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            use std::io::Write;
            let _ = writeln!(file, "{line}");
        }
    }

    pub(in crate::tui::app) fn move_selection(&mut self, delta: isize) {
        if self.detail_open {
            self.move_detail_scroll(delta);
            return;
        }
        match self.page {
            Page::Instances => self.move_instance_focus(delta),
            Page::Accounts => {
                if delta < 0 {
                    self.accounts.select_previous();
                } else {
                    self.accounts.select_next();
                }
            }
            Page::DownloadCenter => {
                if self.download_center.category == DownloadCategory::GameVersions {
                    let version_count = self.filtered_version_indices().len();
                    self.version_selected =
                        move_bounded(self.version_selected, version_count, delta);
                    self.keep_version_selected_visible();
                } else {
                    let count = if self.resource_detail.is_some() {
                        self.resource_files.len()
                    } else {
                        self.resource_projects.len()
                    };
                    self.download_center.selected =
                        move_bounded(self.download_center.selected, count, delta);
                    self.pane_scroll = 0;
                }
            }
            Page::Tasks => {
                let selected =
                    move_bounded(self.task_selected, self.visible_task_indices().len(), delta);
                self.set_task_selection(selected);
            }
            Page::Settings => {
                if self.settings.category == SettingsCategory::Java {
                    self.java_selected = move_bounded(self.java_selected, self.java.len(), delta);
                } else {
                    let row_count = self.settings_row_count();
                    self.settings.move_selection(row_count, delta);
                }
            }
            Page::Overview => self.move_instance_focus(delta),
            Page::Logs => self.logs.scroll(delta, self.log_max_scroll_offset()),
        }
    }

    pub(in crate::tui::app) fn activate_selection(&mut self) {
        match self.page {
            Page::Overview => {
                if self.instances.items.is_empty() {
                    self.set_page(Page::Instances);
                } else {
                    self.launch_current_instance();
                }
            }
            Page::Instances => self.open_workspace(),
            Page::Accounts => self.select_current_account(),
            Page::DownloadCenter => {
                if self.download_center.category == DownloadCategory::GameVersions {
                    // The row already shows the type and release date, so Enter
                    // goes straight to instance creation instead of opening a
                    // details popup that only repeats them.
                    match self.selected_version().map(|version| version.id.clone()) {
                        Some(version) => {
                            self.detail_open = false;
                            self.open_create_form_with_version(version);
                        }
                        None => {
                            self.status =
                                Status::warning(self.i18n.text("status.refresh_versions_first"));
                        }
                    }
                } else if self.resource_detail.is_some() {
                    self.submit_selected_resource_task();
                } else if let Some(project) = self
                    .resource_projects
                    .get(self.download_center.selected)
                    .cloned()
                {
                    self.open_resource_project(project);
                } else {
                    self.status = Status::warning(self.i18n.text("status.refresh_resources_first"));
                }
            }
            Page::Tasks => self.open_details(),
            Page::Logs => {}
            Page::Settings => self.activate_settings_selection(),
        }
    }

    pub(in crate::tui::app) fn page_refresh_or_rename(&mut self) {
        // Renaming from the workspace must address the instance the workspace
        // shows, not whichever row the list happens to have focused.
        if self.workspace_is_open() && !self.focus_workspace_instance() {
            // The workspace instance is gone from the list; renaming whatever
            // row is focused now would rename the wrong instance.
            self.status = Status::warning(self.i18n.text("status.select_instance"));
            return;
        }
        match self.page {
            Page::Instances => {
                let Some(instance) = self.instances.selected_item() else {
                    self.status = Status::warning(self.i18n.text("status.select_instance"));
                    return;
                };
                self.modal = Some(Modal::form(
                    self.i18n.text("form.rename_instance"),
                    &[self.i18n.text("form.new_name")],
                    std::slice::from_ref(&instance.name),
                    FormAction::RenameInstance,
                ));
            }
            Page::Settings if self.settings.category == SettingsCategory::Java => self.scan_java(),
            Page::DownloadCenter => self.refresh_download_center(),
            _ => {}
        }
    }
}

pub(crate) fn accounts_command(
    _mode: crate::tui::command::Mode,
    key: crossterm::event::KeyEvent,
) -> crate::tui::command::Command {
    use crate::tui::command::Command;
    use crossterm::event::KeyCode;

    let plain = key.modifiers.is_empty();
    match key.code {
        KeyCode::Char('n') if plain => Command::NewAccount,
        KeyCode::Char('m') if plain => Command::MicrosoftLogin,
        KeyCode::Char('d') if plain => Command::Delete,
        _ => Command::Nothing,
    }
}

pub(crate) fn logs_command(
    _mode: crate::tui::command::Mode,
    key: crossterm::event::KeyEvent,
) -> crate::tui::command::Command {
    use crate::tui::command::Command;
    use crossterm::event::KeyCode;

    let plain = key.modifiers.is_empty();
    match key.code {
        KeyCode::Char('/') if plain => Command::Search,
        KeyCode::Char('f') if plain => Command::ToggleFollow,
        KeyCode::Char('g') if plain => Command::FollowBottom,
        KeyCode::Char('c') if plain => Command::Clear,
        _ => Command::Nothing,
    }
}

pub(crate) fn overview_command(
    _mode: crate::tui::command::Mode,
    key: crossterm::event::KeyEvent,
) -> crate::tui::command::Command {
    use crate::tui::command::Command;
    use crossterm::event::KeyCode;

    let plain = key.modifiers.is_empty();
    match key.code {
        KeyCode::Char('n') if plain => Command::Create,
        KeyCode::Char('i') if plain => Command::Import,
        _ => Command::Nothing,
    }
}
