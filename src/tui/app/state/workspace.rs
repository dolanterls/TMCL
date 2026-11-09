//! Instance workspace actions.
//!
//! The workspace is a second-level screen under the instances tab: `Enter`
//! opens it, `Esc` closes it, and while it is open the row/category commands
//! address the workspace instead of the page behind it.

use super::*;
use crate::tui::pages::workspace::Workspace;

impl App {
    pub(in crate::tui::app) fn open_workspace(&mut self) {
        let Some(instance) = self.instances.selected_item().cloned() else {
            self.status = Status::warning(self.i18n.text("status.create_instance_first"));
            return;
        };
        // A missing file is the normal case for an instance another launcher
        // created; a file that exists and does not parse is a real fallback and
        // must not be silent.
        let metadata_path = instance.path.join("tmcl.json");
        let corrupt = metadata_path.exists()
            && crate::core::config::storage::read_json::<
                crate::core::instance::models::InstanceConfig,
            >(&metadata_path)
            .is_err();
        self.workspace = Some(Workspace::open(&instance));
        self.pane_scroll = 0;
        if corrupt {
            self.status = Status::warning(self.i18n.text("workspace.status.metadata_fallback"));
        }
        // The rail counts every category, so all four are scanned on open
        // instead of only the one the player happens to land on.
        for kind in [
            crate::core::content::ContentKind::Mod,
            crate::core::content::ContentKind::ResourcePack,
            crate::core::content::ContentKind::Shader,
            crate::core::content::ContentKind::World,
        ] {
            self.refresh_workspace_content(kind);
        }
        self.spawn_workspace_size();
    }

    /// Measure the instance directory off the UI thread: a large `mods/` tree
    /// is visible as a frozen frame otherwise.
    fn spawn_workspace_size(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let root = workspace.root.clone();
        let instance = workspace.instance;
        let sender = self.sender.clone();
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn_blocking(move || {
            let bytes = crate::core::content::scan::tree_size(&root);
            let _ = sender.send(crate::tui::event::AppEvent::WorkspaceSize { instance, bytes });
        });
    }

    /// Report a finished size measurement, ignoring an answer for an instance
    /// the workspace no longer shows.
    pub(in crate::tui::app) fn apply_workspace_size(&mut self, instance: uuid::Uuid, bytes: u64) {
        if let Some(workspace) = self.workspace.as_mut()
            && workspace.instance == instance
        {
            workspace.size_bytes = Some(bytes);
        }
    }

    pub(in crate::tui::app) fn close_workspace(&mut self) {
        self.workspace = None;
    }

    pub(in crate::tui::app) fn workspace_is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(in crate::tui::app) fn move_workspace_row(&mut self, direction: isize) {
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        let count = workspace.row_count();
        workspace.move_selection(count, direction);
    }

    pub(in crate::tui::app) fn move_workspace_category(&mut self, direction: isize) {
        if let Some(workspace) = self.workspace.as_mut() {
            workspace.move_category(direction);
            workspace.search.clear();
        }
        self.refresh_current_workspace_content();
    }

    /// Jump straight to a rail entry (a click on the category list).
    pub(in crate::tui::app) fn select_workspace_category(&mut self, index: usize) {
        if index >= crate::tui::pages::workspace::WorkspaceCategory::ALL.len() {
            return;
        }
        let category = crate::tui::pages::workspace::WorkspaceCategory::from_index(index);
        let changed = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.category != category);
        if let Some(workspace) = self.workspace.as_mut() {
            workspace.category = category;
            workspace.selected = 0;
            workspace.scroll = 0;
            workspace.search.clear();
        }
        if changed {
            self.refresh_current_workspace_content();
        }
    }

    /// Put the cursor on `index`, clamped to the rows that exist now.
    pub(in crate::tui::app) fn set_workspace_selection(&mut self, index: usize) {
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        let count = workspace.row_count();
        if count > 0 {
            workspace.selected = index.min(count - 1);
        }
    }

    /// A click on a row's control: the follow marker of a launch row, or the
    /// enable mark of a content row.
    pub(in crate::tui::app) fn toggle_workspace_current(&mut self) {
        if self.workspace_has_content_category() {
            self.toggle_workspace_content();
        } else {
            self.toggle_workspace_follow();
        }
    }

    /// Kick off a scan for the category on screen, if it lists files.
    pub(in crate::tui::app) fn refresh_current_workspace_content(&mut self) {
        if let Some(kind) = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.category.content_kind())
        {
            self.refresh_workspace_content(kind);
        }
    }

    /// `Enter` inside the workspace. The manage category owns the destructive
    /// actions; the other categories get their rows in later stages.
    pub(in crate::tui::app) fn activate_workspace_row(&mut self) {
        let Some(category) = self.workspace.as_ref().map(|workspace| workspace.category) else {
            return;
        };
        match category {
            crate::tui::pages::workspace::WorkspaceCategory::Manage => {
                let index = self
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.selected)
                    .unwrap_or(0);
                if !self.focus_workspace_instance() {
                    return;
                }
                match index {
                    0 => self.page_refresh_or_rename(),
                    1 => self.repair_workspace_instance(),
                    2 => self.open_workspace_export(),
                    3 => self.open_workspace_purge_confirmation(),
                    _ => self.open_workspace_remove_confirmation(),
                }
            }
            crate::tui::pages::workspace::WorkspaceCategory::Launch => {
                self.open_workspace_editor();
            }
            crate::tui::pages::workspace::WorkspaceCategory::Install => {
                self.activate_workspace_install();
            }
            crate::tui::pages::workspace::WorkspaceCategory::Overview => {
                self.status = Status::info(self.i18n.text("hint.launch_key"));
            }
            other if other.content_kind().is_some() => {
                // An empty content list has no entry to inspect; the useful
                // action is fetching something into it. `Enter` takes the
                // player to the download centre with this instance already
                // selected as the install target.
                let kind = other.content_kind().expect("content category");
                let empty = self
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.content.count(kind) == 0);
                if empty {
                    self.open_workspace_downloads();
                } else {
                    self.open_workspace_detail();
                }
            }
            _ => {}
        }
    }

    /// Jump to the download centre with this instance as the install target.
    ///
    /// The workspace already knows which category the player came from, so the
    /// matching resource tab is selected and the search re-runs with this
    /// instance's version and loader, as it does when the target is picked by
    /// hand.
    pub(in crate::tui::app) fn open_workspace_downloads(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let instance = workspace.instance;
        let kind = workspace.category.content_kind();
        let target = match kind {
            Some(crate::core::content::ContentKind::Mod) => {
                DownloadCategory::Resource(ResourceKind::Mod)
            }
            Some(crate::core::content::ContentKind::ResourcePack) => {
                DownloadCategory::Resource(ResourceKind::ResourcePack)
            }
            Some(crate::core::content::ContentKind::Shader) => {
                DownloadCategory::Resource(ResourceKind::Shader)
            }
            Some(crate::core::content::ContentKind::World) => {
                DownloadCategory::Resource(ResourceKind::World)
            }
            // Data packs live inside one world and have no download category;
            // the list is filled by importing a zip into the world.
            Some(crate::core::content::ContentKind::Datapack) => return,
            None => DownloadCategory::Resource(ResourceKind::Mod),
        };
        // Set the target first: every request below is filtered by it. World
        // downloads go through a save-as dialog, so they have no target.
        if kind != Some(crate::core::content::ContentKind::World) {
            self.download_target_id = Some(instance);
        }
        let current = DownloadCategory::ALL
            .iter()
            .position(|category| *category == self.download_center.category)
            .unwrap_or(0);
        let next = DownloadCategory::ALL
            .iter()
            .position(|category| *category == target)
            .unwrap_or(0);
        if next != current {
            self.change_download_category(next as isize - current as isize);
        } else {
            // Same tab, different instance: the visible results belong to the
            // old target, so drop them and re-query instead of leaving a list
            // that installs into the wrong place.
            self.resource_projects.clear();
            self.resource_files.clear();
            self.resource_detail = None;
            self.resource_detail_project = None;
            self.download_center.selected = 0;
            self.download_center.page = 0;
            self.download_center.total = 0;
            self.spawn_resource_search();
        }
        self.set_page(Page::DownloadCenter);
        let message = if kind == Some(crate::core::content::ContentKind::World) {
            "workspace.download.opened"
        } else {
            "workspace.download.jump"
        };
        self.status = Status::info(self.i18n.text(message));
    }

    /// `Enter` on the export row: ask where the archive should go.
    fn open_workspace_export(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        // Beside the instance directory, so the archive does not end up inside
        // the tree it is packaging.
        let file_name = crate::core::content::export::suggested_archive_name(&workspace.name);
        let default = workspace
            .root
            .parent()
            .map(|parent| parent.join(&file_name))
            .unwrap_or_else(|| workspace.root.join(&file_name));
        self.modal = Some(Modal::prompt(
            self.i18n.text("workspace.export.title"),
            self.i18n.text("workspace.export.label"),
            self.i18n.text("workspace.export.help"),
            crate::core::instance::path::display_path(&default),
            crate::tui::modal::FormAction::ExportModpack,
        ));
    }

    /// Run the export off the UI thread; a large `mods/` must not freeze the
    /// screen while it is zipped.
    pub(in crate::tui::app) fn export_workspace_instance(
        &mut self,
        target: &std::path::Path,
    ) -> Result<()> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Ok(());
        };
        let root = workspace.root.clone();
        let instance = workspace.draft.clone();
        let target = crate::core::content::export::normalize_export_target(target, &instance.name)?;
        if target.exists() {
            self.status = Status::warning(self.i18n.text("workspace.export.exists"));
            return Ok(());
        }
        if target.starts_with(&root) {
            self.status = Status::warning(self.i18n.text("workspace.export.inside"));
            return Ok(());
        }
        let sender = self.sender.clone();
        self.status = Status::info(self.i18n.text("workspace.export.running"));
        tokio::task::spawn_blocking(move || {
            let result = crate::core::content::export::export_instance(&root, &instance, &target)
                .map_err(|error| error.to_string());
            let _ = sender.send(crate::tui::event::AppEvent::ModpackExported(result));
        });
        Ok(())
    }

    /// Report a finished export. The workspace may already be closed, so this
    /// only touches the status line.
    pub(in crate::tui::app) fn finish_workspace_export(
        &mut self,
        result: std::result::Result<std::path::PathBuf, String>,
    ) {
        match result {
            Ok(path) => {
                self.status = Status::success(self.i18n.format(
                    "workspace.export.done",
                    &[("path", &crate::core::instance::path::display_path(&path))],
                ));
            }
            Err(error) => {
                self.status = Status::error(
                    self.i18n
                        .format("workspace.export.failed", &[("error", &error)]),
                );
            }
        }
    }

    /// Point the instance list at the workspace instance so the existing
    /// per-selection actions (rename, remove) address the right one.
    pub(in crate::tui::app) fn focus_workspace_instance(&mut self) -> bool {
        let Some(id) = self.workspace.as_ref().map(|workspace| workspace.instance) else {
            return false;
        };
        match self.instances.items.iter().position(|item| item.id == id) {
            Some(index) => {
                self.instances.focused = index;
                true
            }
            None => false,
        }
    }

    fn repair_workspace_instance(&mut self) {
        let Some(instance) = self.instances.selected_item().cloned() else {
            return;
        };
        match self.launch_blockers(&instance) {
            Ok(missing) if missing.is_empty() => {
                self.status = Status::info(self.i18n.text("workspace.status.repair_ok"));
            }
            Ok(missing) => self.start_instance_repair(&instance, missing),
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }

    /// `d` / the manage row: ask before deleting the instance the workspace
    /// shows. The action carries its id, so it cannot fall back to whichever
    /// row the list happens to have focused.
    pub(in crate::tui::app) fn open_workspace_remove_confirmation(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let action = crate::tui::modal::ConfirmAction::RemoveInstance(workspace.instance);
        self.modal = Some(Modal::confirm(
            self.i18n
                .format("confirm.remove_instance", &[("name", &workspace.name)]),
            action,
        ));
    }

    /// Emptying the trash is permanent, so it goes through a confirmation.
    fn open_workspace_purge_confirmation(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let action = crate::tui::modal::ConfirmAction::PurgeInstanceTrash(workspace.instance);
        self.modal = Some(Modal::confirm(
            self.i18n.format(
                "workspace.confirm.purge_trash",
                &[("name", &workspace.name)],
            ),
            action,
        ));
    }

    pub(in crate::tui::app) fn jump_workspace_edge(&mut self, top: bool) {
        if let Some(workspace) = self.workspace.as_mut() {
            let count = workspace.row_count();
            workspace.selected = if top { 0 } else { count.saturating_sub(1) };
        }
    }

    /// Launch the instance this workspace belongs to, not the list selection.
    pub(in crate::tui::app) fn launch_workspace_instance(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let id = workspace.instance;
        let Some(index) = self.instances.items.iter().position(|item| item.id == id) else {
            self.status = Status::warning(self.i18n.text("status.create_instance_first"));
            return;
        };
        self.instances.focused = index;
        self.launch_selected_instance();
    }
}

impl App {
    /// The launch row under the cursor, if the launch category is showing.
    pub(in crate::tui::app) fn current_launch_row(
        &self,
    ) -> Option<crate::tui::app::view::workspace::LaunchRow> {
        let workspace = self.workspace.as_ref()?;
        if workspace.category != crate::tui::pages::workspace::WorkspaceCategory::Launch {
            return None;
        }
        self.launch_rows().into_iter().nth(workspace.selected)
    }

    /// `Space`: follow the global value, or freeze a custom one.
    pub(in crate::tui::app) fn toggle_workspace_follow(&mut self) {
        let Some(row) = self.current_launch_row() else {
            return;
        };
        // Memory bounds are disabled while the mode is automatic, but switching
        // them to a custom value is exactly what makes the mode manual, so they
        // stay reachable here.
        let memory_bound = matches!(
            row.field,
            crate::tui::pages::workspace::LaunchField::MemoryMin
                | crate::tui::pages::workspace::LaunchField::MemoryMax
        );
        if !row.enabled && !memory_bound {
            self.status = Status::warning(row.disabled_reason.clone());
            return;
        }
        if row.kind == crate::tui::app::view::workspace::LaunchFieldKind::Picker {
            self.open_workspace_java_picker();
            return;
        }
        let global = self.config.launch.clone();
        let global_server = self.config.launcher.server_address.clone();
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        let overrides = &mut workspace.draft.overrides;
        match row.field {
            crate::tui::pages::workspace::LaunchField::MemoryMode => {
                overrides.memory_mode = match overrides.memory_mode {
                    Some(_) => None,
                    None => Some(global.memory_mode),
                };
            }
            crate::tui::pages::workspace::LaunchField::MemoryMin => {
                overrides.min_memory_mb = match overrides.min_memory_mb {
                    Some(_) => None,
                    None => Some(global.min_memory_mb),
                };
            }
            crate::tui::pages::workspace::LaunchField::MemoryMax => {
                overrides.max_memory_mb = match overrides.max_memory_mb {
                    Some(_) => None,
                    None => Some(global.max_memory_mb),
                };
            }
            crate::tui::pages::workspace::LaunchField::WindowMode => {
                overrides.fullscreen = match overrides.fullscreen {
                    Some(_) => None,
                    None => Some(global.fullscreen),
                };
            }
            crate::tui::pages::workspace::LaunchField::WindowSize => {
                if overrides.width.is_none() && overrides.height.is_none() {
                    overrides.width = Some(global.width);
                    overrides.height = Some(global.height);
                } else {
                    overrides.width = None;
                    overrides.height = None;
                }
            }
            crate::tui::pages::workspace::LaunchField::JvmArgs => {
                overrides.jvm_args = match overrides.jvm_args {
                    Some(_) => None,
                    None => Some(global.jvm_args.clone()),
                };
            }
            crate::tui::pages::workspace::LaunchField::GameArgs => {
                overrides.game_args = match overrides.game_args {
                    Some(_) => None,
                    None => Some(global.game_args.clone()),
                };
            }
            crate::tui::pages::workspace::LaunchField::Server => {
                overrides.server_address = match overrides.server_address {
                    Some(_) => None,
                    None => Some(global_server.unwrap_or_default()),
                };
            }
            crate::tui::pages::workspace::LaunchField::Java => {}
        }
        self.persist_workspace_overrides();
    }

    /// `Left`/`Right`: step a numeric custom value.
    ///
    /// The window size is one row for two fields, so stepping it keeps a 16:9
    /// ratio instead of letting width and height drift apart.
    pub(in crate::tui::app) fn step_workspace_value(&mut self, direction: isize) {
        let Some(row) = self.current_launch_row() else {
            return;
        };
        if !row.enabled {
            self.status = Status::warning(row.disabled_reason.clone());
            return;
        }
        let crate::tui::app::view::workspace::LaunchFieldKind::Number { step, min, max } = row.kind
        else {
            return;
        };
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        let overrides = &mut workspace.draft.overrides;
        let delta = step as i64 * direction as i64;
        let apply =
            |value: u64| -> u64 { (value as i64 + delta).clamp(min as i64, max as i64) as u64 };
        match row.field {
            crate::tui::pages::workspace::LaunchField::MemoryMin => {
                let Some(current) = overrides.min_memory_mb else {
                    self.status =
                        Status::warning(self.i18n.text("workspace.status.customize_first"));
                    return;
                };
                overrides.min_memory_mb = Some(apply(u64::from(current)) as u32);
            }
            crate::tui::pages::workspace::LaunchField::MemoryMax => {
                let Some(current) = overrides.max_memory_mb else {
                    self.status =
                        Status::warning(self.i18n.text("workspace.status.customize_first"));
                    return;
                };
                let next = apply(u64::from(current)) as u32;
                overrides.max_memory_mb = Some(next);
                // The maximum can never drop below the minimum in manual mode.
                if let Some(minimum) = overrides.min_memory_mb
                    && next < minimum
                {
                    overrides.min_memory_mb = Some(next);
                }
            }
            crate::tui::pages::workspace::LaunchField::WindowSize => {
                let Some(width) = overrides.width else {
                    self.status =
                        Status::warning(self.i18n.text("workspace.status.customize_first"));
                    return;
                };
                let width = apply(u64::from(width)) as u32;
                overrides.width = Some(width);
                overrides.height = Some(((width * 9) / 16).clamp(240, 4320));
            }
            _ => return,
        }
        self.persist_workspace_overrides();
    }

    /// `Enter`: open the editor the row kind asks for.
    pub(in crate::tui::app) fn open_workspace_editor(&mut self) {
        let Some(row) = self.current_launch_row() else {
            return;
        };
        if !row.enabled {
            self.status = Status::warning(row.disabled_reason.clone());
            return;
        }
        use crate::tui::pages::workspace::LaunchField;
        match row.kind {
            crate::tui::app::view::workspace::LaunchFieldKind::Picker => {
                self.open_workspace_java_picker();
            }
            crate::tui::app::view::workspace::LaunchFieldKind::Text => {
                let Some(workspace) = self.workspace.as_ref() else {
                    return;
                };
                let overrides = &workspace.draft.overrides;
                let current = match row.field {
                    LaunchField::JvmArgs => overrides
                        .jvm_args
                        .clone()
                        .unwrap_or_else(|| self.config.launch.jvm_args.clone())
                        .join(" "),
                    LaunchField::GameArgs => overrides
                        .game_args
                        .clone()
                        .unwrap_or_else(|| self.config.launch.game_args.clone())
                        .join(" "),
                    LaunchField::Server => overrides
                        .server_address
                        .clone()
                        .or_else(|| self.config.launcher.server_address.clone())
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                let help = match row.field {
                    LaunchField::JvmArgs => "settings.help.jvm_args",
                    LaunchField::GameArgs => "settings.help.game_args",
                    LaunchField::Server => "settings.help.server_address",
                    _ => "settings.help.jvm_args",
                };
                self.modal = Some(Modal::prompt(
                    row.label.clone(),
                    self.i18n.text("workspace.value"),
                    self.i18n.text(help),
                    current,
                    crate::tui::modal::FormAction::LaunchOverride(row.field),
                ));
            }
            _ => self.toggle_workspace_follow(),
        }
    }

    /// Java picker: the same `label|path` convention as the download target.
    pub(in crate::tui::app) fn open_workspace_java_picker(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let auto = self.i18n.text("workspace.auto");
        let mut options = vec![format!("{auto}|")];
        // Marking the runtimes that cannot run this version saves the player
        // from picking one and only learning about it at launch.
        let required =
            crate::core::java::minimum_java_major(&workspace.draft.minecraft_version).unwrap_or(8);
        options.extend(self.java.iter().map(|java| {
            let short = if java.major < required {
                format!(
                    " · {}",
                    self.i18n.format(
                        "workspace.launch.java_short",
                        &[("major", &required.to_string())],
                    )
                )
            } else {
                String::new()
            };
            format!(
                "Java {} · {}{short}|{}",
                java.major,
                java.version,
                java.executable.display()
            )
        }));
        let current = workspace.draft.overrides.java_path.clone();
        let selected = current
            .as_ref()
            .and_then(|path| {
                options
                    .iter()
                    .position(|option| option.ends_with(&path.display().to_string()))
            })
            .unwrap_or(0);
        let modal = Modal::form(
            self.i18n.text("workspace.launch.java"),
            &[self.i18n.text("workspace.value")],
            &[options[selected].clone()],
            crate::tui::modal::FormAction::LaunchOverride(
                crate::tui::pages::workspace::LaunchField::Java,
            ),
        )
        .with_selection_picker(0, options);
        self.modal = Some(modal);
    }

    /// Apply a value typed or picked in a dialog.
    pub(in crate::tui::app) fn apply_launch_override(
        &mut self,
        field: crate::tui::pages::workspace::LaunchField,
        value: &str,
    ) {
        use crate::tui::pages::workspace::LaunchField;
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        let overrides = &mut workspace.draft.overrides;
        match field {
            LaunchField::JvmArgs => overrides.jvm_args = Some(split_args(value)),
            LaunchField::GameArgs => overrides.game_args = Some(split_args(value)),
            LaunchField::Server => {
                overrides.server_address = Some(value.trim().to_owned());
            }
            LaunchField::Java => {
                let path = value
                    .rsplit_once('|')
                    .map(|(_, path)| path.trim())
                    .unwrap_or_default();
                overrides.java_path = (!path.is_empty()).then(|| std::path::PathBuf::from(path));
            }
            _ => return,
        }
        self.persist_workspace_overrides();
    }

    /// `R`: drop every override and follow the global config again.
    pub(in crate::tui::app) fn reset_workspace_overrides(&mut self) {
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        workspace.draft.overrides = crate::core::instance::overrides::LaunchOverrides::default();
        self.persist_workspace_overrides();
        self.status = Status::success(self.i18n.text("workspace.status.reset"));
    }

    /// Write the draft overrides back into the instance metadata.
    ///
    /// Read-modify-write so `extra` keys and statistics written by other code
    /// survive; a failure keeps the in-memory draft untouched and reports why.
    fn persist_workspace_overrides(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let path = workspace.root.join("tmcl.json");
        let mut config = crate::core::config::storage::read_json::<InstanceConfig>(&path)
            .unwrap_or_else(|_| workspace.draft.clone());
        // What is on disk, so a rejected write can be undone.
        let previous = config.overrides.clone();
        config.overrides = workspace.draft.overrides.clone();
        if let Err(error) = crate::core::config::storage::write_json_atomic(&path, &config) {
            // The screen must not keep showing a value the disk rejected.
            if let Some(workspace) = self.workspace.as_mut() {
                workspace.draft.overrides = previous;
            }
            self.status = Status::error(error.to_string());
        }
    }
}

impl App {
    /// Entry under the cursor in a content category.
    pub(in crate::tui::app) fn current_workspace_entry(
        &self,
    ) -> Option<crate::core::content::ContentEntry> {
        let workspace = self.workspace.as_ref()?;
        let index = workspace.selected;
        self.workspace_content_entries().into_iter().nth(index)
    }

    /// `Space`: switch the entry on or off.
    ///
    /// Mods use the file name, resource packs a line in `options.txt`, shaders
    /// the loader's own config: the same key in three different stores.
    pub(in crate::tui::app) fn toggle_workspace_content(&mut self) {
        let Some(entry) = self.current_workspace_entry() else {
            return;
        };
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let Some(kind) = workspace.category.content_kind() else {
            return;
        };
        if kind == crate::core::content::ContentKind::World {
            return;
        }
        if self.workspace_instance_running() {
            self.status = Status::warning(self.i18n.text("workspace.status.running_locked"));
            return;
        }
        if kind == crate::core::content::ContentKind::Shader && workspace.shader_loader.is_none() {
            self.status = Status::warning(self.i18n.text("workspace.status.shader_need_loader"));
            return;
        }
        let root = workspace.root.clone();
        let loader = workspace.shader_loader;
        let file = entry.display_file_name();
        let turning_on = !entry.enabled;

        // `Some(path)` when the store is the file system and the name moved.
        let result = match kind {
            crate::core::content::ContentKind::Mod
            | crate::core::content::ContentKind::Datapack => {
                crate::core::content::toggle::set_enabled(&entry.path, turning_on).map(Some)
            }
            crate::core::content::ContentKind::ResourcePack => {
                crate::core::content::toggle::set_pack_enabled(&root, &file, turning_on)
                    .map(|()| None)
            }
            crate::core::content::ContentKind::Shader => {
                crate::core::content::shader::set_selected(
                    &root,
                    loader,
                    if turning_on { &file } else { "" },
                )
                .map(|()| None)
            }
            crate::core::content::ContentKind::World => return,
        };

        match result {
            Ok(moved) => {
                // The rescan below is a background pass, and until its answer
                // lands the list still describes the file as it was. A second
                // `Space` inside that window would read the old name and the old
                // state, repeat the first intent verbatim and collide with the
                // file it renamed (`cannot rename … already exists`). Pointing
                // the cached entry at the new name makes a rapid press mean what
                // it says.
                self.retarget_content_entry(kind, &entry.path, moved.as_deref(), turning_on);
                self.status = Status::info(self.i18n.format(
                    "workspace.status.toggled",
                    &[
                        ("name", &file),
                        (
                            "state",
                            &if entry.enabled {
                                self.i18n.text("workspace.state.disabled")
                            } else {
                                self.i18n.text("workspace.state.enabled")
                            },
                        ),
                    ],
                ));
                self.rescan_workspace_content(kind);
            }
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }

    /// Point the cached list at the file the toggle produced.
    ///
    /// Only the file-system kinds move (`new_path`); packs and shaders record
    /// their state in a config file, so their name stays and only `enabled`
    /// changes. The entry is matched by the path it had, which is what the
    /// caller acted on.
    fn retarget_content_entry(
        &mut self,
        kind: crate::core::content::ContentKind,
        old_path: &std::path::Path,
        new_path: Option<&std::path::Path>,
        enabled: bool,
    ) {
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        let mut entries = workspace.content.entries(kind).to_vec();
        let Some(slot) = entries.iter_mut().find(|entry| entry.path == old_path) else {
            return;
        };
        slot.enabled = enabled;
        if let Some(path) = new_path {
            slot.path = path.to_path_buf();
        }
        workspace.content.set(kind, entries);
    }

    /// `J` / `K`: move the selected resource pack up or down the stack.
    ///
    /// Only enabled packs have a position: an entry that is off is not in the
    /// array at all, so there is nothing to swap.
    pub(in crate::tui::app) fn reorder_workspace_content(&mut self, direction: isize) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let Some(kind) = workspace.category.content_kind() else {
            return;
        };
        if kind != crate::core::content::ContentKind::ResourcePack {
            return;
        }
        if self.workspace_instance_running() {
            self.status = Status::warning(self.i18n.text("workspace.status.running_locked"));
            return;
        }
        let entries = workspace.visible_entries(kind);
        let Some(current) = entries.get(workspace.selected) else {
            return;
        };
        if !current.enabled {
            self.status = Status::warning(self.i18n.text("workspace.status.enable_first"));
            return;
        }
        let target = if direction < 0 {
            workspace.selected.checked_sub(1)
        } else {
            Some(workspace.selected + 1).filter(|index| *index < entries.len())
        };
        let Some(target) = target else {
            return;
        };
        let Some(neighbour) = entries.get(target) else {
            return;
        };
        if !neighbour.enabled {
            self.status = Status::info(self.i18n.text("workspace.status.order_edge"));
            return;
        }
        let moving = current.display_file_name();
        let other = neighbour.display_file_name();
        let root = workspace.root.clone();
        match crate::core::content::toggle::swap_packs(&root, &moving, &other) {
            Ok(()) => {
                self.status = Status::info(self.i18n.text("workspace.status.order_saved"));
                if let Some(workspace) = self.workspace.as_mut() {
                    workspace.selected = target;
                }
                self.rescan_workspace_content(kind);
            }
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }

    /// True when the workspace's own instance is the one the game is running.
    ///
    /// Minecraft rewrites its config on exit, so an edit made while it runs is
    /// either lost or overwrites the game's own write.
    pub(in crate::tui::app) fn workspace_instance_running(&self) -> bool {
        let Some(workspace) = self.workspace.as_ref() else {
            return false;
        };
        self.active_launch
            .as_ref()
            // By id, not by name: two game directories can hold versions with
            // the same folder name, and locking the wrong instance's content is
            // a data-loss bug, not a cosmetic one.
            .is_some_and(|launch| launch.instance_id == workspace.instance)
    }

    /// `(installed, required)` when the instance pins a Java runtime older than
    /// its game version needs. One helper feeds the row colour and the note, so
    /// the two cannot tell different stories.
    pub(in crate::tui::app) fn workspace_java_mismatch(&self) -> Option<(u32, u32)> {
        let workspace = self.workspace.as_ref()?;
        let path = workspace.draft.overrides.java_path.as_ref()?;
        let java = self.java.iter().find(|java| &java.executable == path)?;
        let required =
            crate::core::java::minimum_java_major(&workspace.draft.minecraft_version).ok()?;
        (java.major < required).then_some((java.major, required))
    }

    /// `d`: ask, then move the entry into the instance trash.
    pub(in crate::tui::app) fn delete_workspace_content(&mut self) {
        let Some(entry) = self.current_workspace_entry() else {
            return;
        };
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let Some(kind) = workspace.category.content_kind() else {
            return;
        };
        let action = crate::tui::modal::ConfirmAction::DeleteContent {
            instance: workspace.instance,
            kind,
            index: workspace.selected,
        };
        self.modal = Some(Modal::confirm(
            self.i18n.format(
                "workspace.confirm.delete",
                &[("name", &entry.display_file_name())],
            ),
            action,
        ));
    }

    /// `u` in the mod list: ask Modrinth for newer versions of every jar.
    pub(in crate::tui::app) fn check_mod_updates(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if workspace.category != crate::tui::pages::workspace::WorkspaceCategory::Mods {
            return;
        }
        let instance_id = workspace.instance;
        let root = workspace.root.clone();
        let Some(summary) = self
            .instances
            .items
            .iter()
            .find(|item| item.id == instance_id)
            .cloned()
        else {
            return;
        };
        let Ok(mut instance) = crate::core::config::storage::read_json::<
            crate::core::instance::models::InstanceConfig,
        >(&summary.path.join("tmcl.json")) else {
            self.status = Status::warning(self.i18n.text("workspace.status.metadata_fallback"));
            return;
        };
        instance.path = summary.path.clone();
        let game_version = instance.minecraft_version.clone();
        let loader = crate::core::resource::compatibility::loader_filter(&instance.loader);
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let sender = self.sender.clone();
        self.status = Status::info(self.i18n.text("status.mod_updates_checking"));
        handle.spawn(async move {
            let result = async {
                let client = crate::core::version::http::HttpClient::new()?;
                crate::core::content::update::check_modrinth(
                    &client,
                    &root.join("mods"),
                    &game_version,
                    loader,
                )
                .await
            }
            .await
            .map_err(|error| error.to_string());
            let _ = sender.send(crate::tui::event::AppEvent::ModUpdatesChecked {
                instance: instance_id,
                result,
            });
        });
    }

    /// Handles the background answer; ignored when the workspace moved on.
    pub(in crate::tui::app) fn apply_mod_updates_checked(
        &mut self,
        instance: uuid::Uuid,
        result: std::result::Result<crate::core::content::update::UpdateReport, String>,
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
            Ok(report) if report.updates.is_empty() => {
                self.status = Status::info(self.i18n.format(
                    "status.mod_updates_none",
                    &[
                        ("count", &report.checked.to_string()),
                        ("unknown", &report.unknown.to_string()),
                    ],
                ));
            }
            Ok(report) => {
                let sample = report
                    .updates
                    .iter()
                    .take(3)
                    .map(|update| format!("{} → {}", update.file_name, update.new_version))
                    .collect::<Vec<_>>()
                    .join("; ");
                let count = report.updates.len().to_string();
                let unknown = report.unknown.to_string();
                self.pending_mod_updates = report.updates;
                self.modal = Some(crate::tui::modal::Modal::confirm(
                    self.i18n.format(
                        "confirm.mod_updates",
                        &[
                            ("count", &count),
                            ("sample", &sample),
                            ("unknown", &unknown),
                        ],
                    ),
                    crate::tui::modal::ConfirmAction::ApplyModUpdates,
                ));
                self.status = Status::info(self.i18n.format(
                    "status.mod_updates_found",
                    &[("count", &count), ("unknown", &unknown)],
                ));
            }
        }
    }

    /// Moves each outdated jar to the trash and queues its replacement.
    ///
    /// The old file goes to the trash, not to `/dev/null`: if the download of
    /// the new version fails, the previous jar is one restore away.
    pub(in crate::tui::app) fn apply_pending_mod_updates(
        &mut self,
    ) -> crate::core::error::Result<()> {
        let updates = std::mem::take(&mut self.pending_mod_updates);
        let Some(workspace) = self.workspace.as_ref() else {
            return Ok(());
        };
        let instance = workspace.instance;
        let root = workspace.root.clone();
        let mut queued = 0usize;
        let mut problems = Vec::new();
        for update in updates {
            if let Err(error) = crate::core::content::trash::move_to_trash(&root, &update.path) {
                problems.push(format!("{}: {error}", update.file_name));
                continue;
            }
            let file = update.file.clone();
            let name = file.name.clone();
            match self.queue_resource_install_on(
                file,
                name,
                instance,
                crate::core::resource::model::ResourceKind::Mod,
            ) {
                Ok(_) => queued += 1,
                Err(error) => problems.push(format!("{}: {error}", update.file_name)),
            }
        }
        if problems.is_empty() {
            self.status = Status::info(self.i18n.format(
                "status.mod_updates_applied",
                &[("count", &queued.to_string())],
            ));
        } else {
            let sample = problems
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            self.status = Status::warning(self.i18n.format(
                "status.mod_updates_partial",
                &[
                    ("count", &queued.to_string()),
                    ("failed", &problems.len().to_string()),
                    ("sample", &sample),
                ],
            ));
        }
        self.rescan_workspace_content(crate::core::content::ContentKind::Mod);
        Ok(())
    }

    /// `p` on a world: list that world's data packs.
    ///
    /// A data pack is not an instance-level category, so the list reuses the
    /// content view with the world carried in `datapack_world`; Esc returns to
    /// the world list instead of closing the workspace.
    pub(in crate::tui::app) fn open_workspace_datapacks(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if workspace.category != crate::tui::pages::workspace::WorkspaceCategory::Worlds {
            return;
        }
        let Some(entry) = workspace
            .content
            .entries(crate::core::content::ContentKind::World)
            .get(workspace.selected)
        else {
            self.status = Status::warning(self.i18n.text("workspace.status.no_world_selected"));
            return;
        };
        if !entry.path.is_dir() {
            return;
        }
        let Some(world) = entry
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            return;
        };
        let name = entry.name.clone();

        if let Some(workspace) = self.workspace.as_mut() {
            workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Datapacks;
            workspace.datapack_world = Some(world);
            workspace.selected = 0;
            workspace.scroll = 0;
            workspace.detail = None;
            // The stamp cache is keyed by kind, and another world's packs must
            // not be mistaken for this world's.
            workspace
                .scan_stamps
                .remove(&crate::core::content::ContentKind::Datapack);
            workspace
                .content
                .set(crate::core::content::ContentKind::Datapack, Vec::new());
        }
        self.status = Status::info(
            self.i18n
                .format("workspace.status.datapacks_of", &[("name", &name)]),
        );
        self.refresh_current_workspace_content();
    }

    /// Esc inside a world's data packs: go back to the world list.
    pub(in crate::tui::app) fn leave_workspace_datapacks(&mut self) -> bool {
        let Some(workspace) = self.workspace.as_mut() else {
            return false;
        };
        if workspace.category != crate::tui::pages::workspace::WorkspaceCategory::Datapacks {
            return false;
        }
        workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Worlds;
        workspace.datapack_world = None;
        workspace.selected = 0;
        workspace.scroll = 0;
        workspace.detail = None;
        workspace
            .scan_stamps
            .remove(&crate::core::content::ContentKind::Datapack);
        // Listed worlds come from the cache, so no rescan is needed here.
        true
    }

    /// `i`: import one or more paths into the current content directory.
    pub(in crate::tui::app) fn open_workspace_import(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let Some(kind) = workspace.category.content_kind() else {
            return;
        };
        self.modal = Some(Modal::prompt(
            self.i18n.text("workspace.import.title"),
            self.i18n.text("workspace.import.label"),
            self.i18n.text("workspace.import.help"),
            String::new(),
            crate::tui::modal::FormAction::ImportContent(kind),
        ));
    }

    /// Copy the typed paths into the instance content directory.
    pub(in crate::tui::app) fn import_workspace_content(
        &mut self,
        kind: crate::core::content::ContentKind,
        input: &str,
    ) {
        let Some(directory) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.content_directory())
        else {
            return;
        };
        let mut imported = 0usize;
        let mut skipped = 0usize;
        for candidate in split_import_input(input) {
            let source = std::path::PathBuf::from(&candidate);
            // The import rules (name check, kind check, collision) live in
            // `core` so the CLI refuses the same files for the same reasons.
            match crate::core::content::import::copy_in(&directory, kind, &source) {
                Ok(_) => imported += 1,
                Err(_) => skipped += 1,
            }
        }
        self.status = Status::info(self.i18n.format(
            "workspace.status.imported",
            &[
                ("imported", &imported.to_string()),
                ("skipped", &skipped.to_string()),
            ],
        ));
        self.refresh_current_workspace_content();
    }

    /// `o`: reveal the content directory in the file manager.
    pub(in crate::tui::app) fn open_workspace_folder(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        // A world is the directory; the other categories are a directory of
        // files, so opening the folder of the selected world is what the user
        // means by "open" there.
        let selected_world = (workspace.category
            == crate::tui::pages::workspace::WorkspaceCategory::Worlds)
            .then(|| self.current_workspace_entry())
            .flatten();
        let directory = match (workspace.category.content_kind(), selected_world) {
            (Some(_), Some(entry)) => entry.path,
            (Some(_), None) => workspace.content_directory(),
            (None, _) => workspace.root.clone(),
        };
        let _ = std::fs::create_dir_all(&directory);
        if let Err(error) = crate::platform::windows::open_path(&directory) {
            self.status = Status::error(error.to_string());
        }
    }

    /// `s`: cycle the content ordering.
    pub(in crate::tui::app) fn cycle_workspace_sort(&mut self) {
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        if workspace.category.content_kind().is_none() {
            return;
        }
        workspace.sort = workspace.sort.next();
        workspace.selected = 0;
        let key = crate::tui::pages::workspace::sort_label_key(workspace.sort);
        self.status = Status::info(self.i18n.text(key));
    }

    /// `Enter`: open the shared detail popup with this entry's metadata.
    pub(in crate::tui::app) fn open_workspace_detail(&mut self) {
        let Some(entry) = self.current_workspace_entry() else {
            return;
        };
        if let Some(workspace) = self.workspace.as_mut() {
            workspace.detail = Some(entry);
        }
        self.detail_open = true;
        self.detail_scroll = 0;
    }
}

/// Split the import field into paths.
///
/// Windows paths are full of spaces and "Copy as path" wraps them in quotes,
/// so a quoted segment is taken verbatim; anything unquoted is whitespace
/// separated, as the dialog help says.
fn split_import_input(input: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in input.trim().chars() {
        match character {
            '"' => quoted = !quoted,
            value if value.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    paths.push(std::mem::take(&mut current));
                }
            }
            value => current.push(value),
        }
    }
    if !current.is_empty() {
        paths.push(current);
    }
    paths
}

/// Identity of a content directory: modification time plus entry count. Both
/// change when a file is added, removed or replaced, which is exactly when the
/// cached scan is stale.
fn dir_stamp(path: &std::path::Path) -> Option<(std::time::SystemTime, usize)> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let count = std::fs::read_dir(path).ok()?.count();
    Some((modified, count))
}

impl App {
    /// Scan a content category in the background unless the directory stamp
    /// still matches what the cache was built from.
    pub(in crate::tui::app) fn refresh_workspace_content(
        &mut self,
        kind: crate::core::content::ContentKind,
    ) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let root = workspace.root.clone();
        let instance = workspace.instance;
        let directory = workspace.content_directory();
        let world = workspace.datapack_world.clone();
        let stamp = dir_stamp(&directory);
        if workspace.scan_stamps.get(&kind) == Some(&stamp) {
            return;
        }
        // A scan request from a synchronous context (unit tests) has nowhere to
        // run; pretending it is loading would pin the list on "scanning".
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let sender = self.sender.clone();
        if let Some(workspace) = self.workspace.as_mut() {
            workspace.pending_scans = workspace.pending_scans.saturating_add(1);
            workspace.loading = true;
        }
        handle.spawn_blocking(move || {
            let entries = crate::core::content::load::entries(&root, kind, world.as_deref());
            let shader_loader = (kind == crate::core::content::ContentKind::Shader)
                .then(|| crate::core::content::shader::detect(&root.join("mods")))
                .flatten();
            let stamp = dir_stamp(&directory);
            let _ = sender.send(crate::tui::event::AppEvent::WorkspaceScanned {
                instance,
                kind,
                entries,
                stamp,
                shader_loader,
            });
        });
    }

    /// Re-read the category on screen, ignoring the directory stamp.
    ///
    /// The player may have dropped a jar into the folder from Explorer; the
    /// stamp is the cache's friend, not the player's.
    pub(in crate::tui::app) fn rescan_visible_workspace_content(&mut self) {
        let Some(kind) = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.category.content_kind())
        else {
            self.status = Status::info(self.i18n.text("workspace.status.nothing_to_refresh"));
            return;
        };
        self.rescan_workspace_content(kind);
        self.status = Status::info(self.i18n.text("workspace.status.refreshed"));
    }

    /// Rescan even when the directory looks unchanged.
    ///
    /// Enable state lives in `options.txt` / the shader config, and writing
    /// those does not touch the directory mtime the cache keys on.
    pub(in crate::tui::app) fn rescan_workspace_content(
        &mut self,
        kind: crate::core::content::ContentKind,
    ) {
        if let Some(workspace) = self.workspace.as_mut() {
            workspace.scan_stamps.remove(&kind);
        }
        self.refresh_workspace_content(kind);
    }

    /// Apply a scan result for the workspace that is on screen.
    ///
    /// Test-only: the event path names the instance it scanned, so a late
    /// answer for the previous instance cannot land in this one.
    #[cfg(test)]
    pub(in crate::tui::app) fn apply_workspace_scan(
        &mut self,
        kind: crate::core::content::ContentKind,
        entries: Vec<crate::core::content::ContentEntry>,
        stamp: Option<(std::time::SystemTime, usize)>,
        shader_loader: Option<crate::core::content::shader::ShaderLoader>,
    ) {
        let Some(instance) = self.workspace.as_ref().map(|workspace| workspace.instance) else {
            return;
        };
        self.apply_workspace_scan_for(instance, kind, entries, stamp, shader_loader);
    }

    /// Apply a scan result, ignoring one that belongs to another instance.
    pub(in crate::tui::app) fn apply_workspace_scan_for(
        &mut self,
        instance: uuid::Uuid,
        kind: crate::core::content::ContentKind,
        entries: Vec<crate::core::content::ContentEntry>,
        stamp: Option<(std::time::SystemTime, usize)>,
        shader_loader: Option<crate::core::content::shader::ShaderLoader>,
    ) {
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        if workspace.instance != instance {
            return;
        }
        workspace.content.set(kind, entries);
        workspace.scan_stamps.insert(kind, stamp);
        workspace.pending_scans = workspace.pending_scans.saturating_sub(1);
        workspace.loading = workspace.pending_scans > 0;
        if kind == crate::core::content::ContentKind::Shader {
            workspace.shader_loader = shader_loader;
        }
        let count = workspace.content.count(kind);
        if workspace.selected >= count {
            workspace.selected = count.saturating_sub(1);
        }
    }
}

impl App {
    /// True when the workspace is showing a category that lists files.
    pub(in crate::tui::app) fn workspace_has_content_category(&self) -> bool {
        self.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.category.content_kind().is_some())
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_finished_mod_update_check_asks_before_replacing_anything() {
        use crate::core::content::update::{ModUpdate, UpdateReport};
        use crate::core::resource::model::{ReleaseChannel, ResourceFile, ResourceSource};

        let (mut app, root) = app_with_mods();
        let instance = app.workspace.as_ref().expect("workspace").instance;
        let old = root.join("mods/alpha.jar");
        app.apply_mod_updates_checked(
            instance,
            Ok(UpdateReport {
                checked: 2,
                unknown: 1,
                updates: vec![ModUpdate {
                    path: old.clone(),
                    file_name: "alpha.jar".into(),
                    current_version: "1.0".into(),
                    new_version: "1.1".into(),
                    file: ResourceFile {
                        source: ResourceSource::Modrinth,
                        project_id: "alpha".into(),
                        file_id: "v2".into(),
                        name: "alpha-1.1.jar".into(),
                        download_url: "https://example.invalid/alpha-1.1.jar".into(),
                        sha1: Some("abc".into()),
                        size: Some(10),
                        game_versions: Vec::new(),
                        loaders: Vec::new(),
                        channel: ReleaseChannel::Release,
                        dependencies: Vec::new(),
                    },
                }],
            }),
        );

        assert!(
            matches!(
                app.modal.as_ref(),
                Some(crate::tui::modal::Modal::Confirm {
                    action: crate::tui::modal::ConfirmAction::ApplyModUpdates,
                    ..
                })
            ),
            "the check must ask before touching files"
        );
        assert!(old.exists(), "checking must not move the old jar");
        assert_eq!(app.pending_mod_updates.len(), 1);
        let _ = root;
    }

    fn app_with_mods() -> (App, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("tmcl-workspace-mods-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("mods")).expect("create mods dir");
        std::fs::write(root.join("mods/alpha.jar"), b"jar").expect("write alpha");
        std::fs::write(root.join("mods/beta.jar.disabled"), b"jar").expect("write beta");
        let id = uuid::Uuid::new_v4();
        let instance = crate::core::instance::models::InstanceConfig::new(
            &root,
            uuid::Uuid::new_v4(),
            "demo",
            "1.20.1",
        );
        crate::core::config::storage::write_json_atomic(&root.join("tmcl.json"), &instance)
            .expect("write instance metadata");
        let mut app = App::new();
        app.page = Page::Instances;
        app.instances.apply(vec![InstanceSummary {
            id,
            name: "demo".into(),
            version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: None,
            status: "ready".into(),
            path: root.clone(),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
        app.instances.focused = 0;
        app.open_workspace();
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Mods;
        }
        let entries = crate::core::content::scan::scan(
            &root.join("mods"),
            crate::core::content::ContentKind::Mod,
        );
        app.apply_workspace_scan(crate::core::content::ContentKind::Mod, entries, None, None);
        (app, root)
    }

    /// An instance with resource packs (and optionally a shader pack) on disk.
    fn app_with_packs(packs: &[&str], shader: Option<&str>) -> (App, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("tmcl-workspace-packs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("resourcepacks")).expect("create packs dir");
        for pack in packs {
            std::fs::write(root.join("resourcepacks").join(pack), b"zip").expect("write pack");
        }
        if let Some(name) = shader {
            std::fs::create_dir_all(root.join("shaderpacks")).expect("create shader dir");
            std::fs::write(root.join("shaderpacks").join(name), b"zip").expect("write shader");
        }
        let id = uuid::Uuid::new_v4();
        let instance = crate::core::instance::models::InstanceConfig::new(
            &root,
            uuid::Uuid::new_v4(),
            "demo",
            "1.20.1",
        );
        crate::core::config::storage::write_json_atomic(&root.join("tmcl.json"), &instance)
            .expect("write instance metadata");
        let mut app = App::new();
        app.page = Page::Instances;
        app.instances.apply(vec![InstanceSummary {
            id,
            name: "demo".into(),
            version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: None,
            status: "ready".into(),
            path: root.clone(),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
        app.instances.focused = 0;
        app.open_workspace();
        (app, root)
    }

    /// Scan a category the way the background thread would.
    fn scanned(app: &mut App, root: &std::path::Path, kind: crate::core::content::ContentKind) {
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = match kind {
                crate::core::content::ContentKind::ResourcePack => {
                    crate::tui::pages::workspace::WorkspaceCategory::ResourcePacks
                }
                crate::core::content::ContentKind::Shader => {
                    crate::tui::pages::workspace::WorkspaceCategory::Shaders
                }
                _ => crate::tui::pages::workspace::WorkspaceCategory::Mods,
            };
        }
        let mut entries = crate::core::content::scan::scan(&root.join(kind.directory()), kind);
        crate::core::content::scan::apply_saved_state(root, kind, &mut entries);
        let loader = (kind == crate::core::content::ContentKind::Shader)
            .then(|| crate::core::content::shader::detect(&root.join("mods")))
            .flatten();
        app.apply_workspace_scan(kind, entries, None, loader);
    }

    fn options_text(root: &std::path::Path) -> String {
        std::fs::read_to_string(root.join("options.txt")).unwrap_or_default()
    }

    #[test]
    fn content_scan_fills_cache_and_clears_loading() {
        let (mut app, root) = app_with_mods();
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.loading = true;
        }
        let entries = crate::core::content::scan::scan(
            &root.join("mods"),
            crate::core::content::ContentKind::Mod,
        );
        app.apply_workspace_scan(crate::core::content::ContentKind::Mod, entries, None, None);
        let workspace = app.workspace.as_ref().expect("workspace");
        assert_eq!(
            workspace
                .content
                .count(crate::core::content::ContentKind::Mod),
            2
        );
        assert!(!workspace.loading);
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn space_toggles_mod_and_refreshes() {
        let (mut app, root) = app_with_mods();
        let before = crate::core::content::scan::scan(
            &root.join("mods"),
            crate::core::content::ContentKind::Mod,
        );
        assert_eq!(before.len(), 2);
        let first = before
            .iter()
            .find(|entry| entry.enabled)
            .expect("enabled entry")
            .path
            .clone();
        app.toggle_workspace_content();
        assert!(
            !first.exists(),
            "the toggled file must be renamed on disk: {}",
            first.display()
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    /// The rescan is a background scan; until its answer lands the list still
    /// describes the file as it was. Without the retarget, a second `Space`
    /// inside that window reads the old name and the old state, repeats the
    /// first intent and collides with the file it renamed (`cannot rename …
    /// already exists`). Two presses must land back where they started.
    #[tokio::test]
    async fn a_second_space_before_the_rescan_lands_toggles_back() {
        let (mut app, root) = app_with_mods();
        let first = app
            .workspace_content_entries()
            .into_iter()
            .next()
            .expect("an entry");
        let name = first.path.clone();

        app.toggle_workspace_content();
        assert!(
            !name.exists(),
            "the first press must rename the file: {}",
            name.display()
        );

        app.toggle_workspace_content();
        assert!(
            name.exists(),
            "the second press must rename it back instead of colliding with its own move: {}",
            app.status.as_str()
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn delete_moves_mod_to_trash() {
        let (mut app, root) = app_with_mods();
        let entries = app.workspace_content_entries();
        let target = entries.first().expect("entry").path.clone();
        let kind = crate::core::content::ContentKind::Mod;
        app.confirm_action(crate::tui::modal::ConfirmAction::DeleteContent {
            instance: app.workspace.as_ref().expect("workspace").instance,
            kind,
            index: 0,
        })
        .expect("delete content");
        assert!(!target.exists());
        assert!(
            !crate::core::content::trash::list(&root).is_empty(),
            "the deleted entry must be recoverable"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn stale_delete_index_is_ignored() {
        let (mut app, root) = app_with_mods();
        let result = app.confirm_action(crate::tui::modal::ConfirmAction::DeleteContent {
            instance: app.workspace.as_ref().expect("workspace").instance,
            kind: crate::core::content::ContentKind::Mod,
            index: 99,
        });
        assert!(result.is_ok());
        assert_eq!(
            crate::core::content::scan::scan(
                &root.join("mods"),
                crate::core::content::ContentKind::Mod
            )
            .len(),
            2,
            "an out-of-range index must not delete anything"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn import_copies_files_and_reports_skips() {
        let (mut app, root) = app_with_mods();
        let source = root.join("outside.jar");
        std::fs::write(&source, b"jar").expect("write source");
        app.import_workspace_content(
            crate::core::content::ContentKind::Mod,
            &format!(
                "{} {}",
                source.display(),
                root.join("missing.jar").display()
            ),
        );
        assert!(root.join("mods/outside.jar").is_file());
        assert!(app.status.as_str().contains('1'), "{}", app.status.as_str());
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn workspace_search_filters_content() {
        let (mut app, _root) = app_with_mods();
        assert_eq!(app.workspace_content_entries().len(), 2);
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.search = "alpha".into();
        }
        let entries = app.workspace_content_entries();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].name.contains("alpha"));
    }

    #[test]
    fn sort_cycles_name_size_status() {
        use crate::tui::pages::workspace::ContentSort;
        let (mut app, _root) = app_with_mods();
        assert_eq!(
            app.workspace.as_ref().expect("workspace").sort,
            ContentSort::Name
        );
        app.cycle_workspace_sort();
        assert_eq!(
            app.workspace.as_ref().expect("workspace").sort,
            ContentSort::Size
        );
        app.cycle_workspace_sort();
        assert_eq!(
            app.workspace.as_ref().expect("workspace").sort,
            ContentSort::Status
        );
        app.cycle_workspace_sort();
        assert_eq!(
            app.workspace.as_ref().expect("workspace").sort,
            ContentSort::Name
        );
    }

    #[test]
    fn content_detail_opens_and_closes() {
        let (mut app, _root) = app_with_mods();
        app.open_workspace_detail();
        assert!(app.detail_open);
        assert!(app.workspace.as_ref().expect("workspace").detail.is_some());
        let lines = app.detail_lines();
        assert!(!lines.is_empty());
        app.go_back();
        assert!(!app.detail_open);
    }

    use crate::core::config::models::UiPreset;
    use crate::tui::app::{App, Page};
    use crate::tui::pages::instances::InstanceSummary;
    use std::path::PathBuf;

    fn app_with_instance() -> App {
        let mut app = App::new();
        app.page = Page::Instances;
        app.instances.items.push(InstanceSummary {
            id: uuid::Uuid::new_v4(),
            name: "demo".into(),
            version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: Some("47.4.22".into()),
            status: "ready".into(),
            path: PathBuf::from("C:/games/versions/demo"),
            launch_count: 2,
            play_time_seconds: 3_600,
        });
        app.instances.focused = 0;
        app.config.state.ui.preset = UiPreset::Creeper;
        app
    }

    #[test]
    fn enter_opens_workspace_and_escape_returns() {
        let mut app = app_with_instance();
        app.open_workspace();
        assert!(app.workspace_is_open());
        let workspace = app.workspace.as_ref().expect("workspace");
        assert_eq!(workspace.name, "demo");
        assert_eq!(
            workspace.category,
            crate::tui::pages::workspace::WorkspaceCategory::Overview
        );
        app.close_workspace();
        assert!(!app.workspace_is_open());
    }

    #[test]
    fn workspace_without_instances_reports_the_reason() {
        let mut app = App::new();
        app.page = Page::Instances;
        app.open_workspace();
        assert!(!app.workspace_is_open());
        assert!(!app.status.as_str().is_empty());
    }

    #[test]
    fn escape_closes_workspace_before_leaving_page() {
        let mut app = app_with_instance();
        app.open_workspace();
        app.go_back();
        assert!(!app.workspace_is_open());
        assert_eq!(app.page, Page::Instances);
    }

    #[test]
    fn manage_rows_put_destructive_actions_last() {
        use crate::tui::pages::workspace::WorkspaceCategory;
        let mut app = app_with_instance();
        app.open_workspace();
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = WorkspaceCategory::Manage;
        }
        let rows = app.workspace_display_rows();
        assert_eq!(rows.len(), 5);
        assert!(rows[0].0.contains("Rename"), "{rows:?}");
        assert!(rows[2].0.contains("Export"), "{rows:?}");
        assert!(rows[3].0.starts_with('!'), "{rows:?}");
        assert!(rows[4].0.contains("Delete"), "{rows:?}");
        assert!(
            rows[3].0.contains("trash") && rows[4].0.starts_with('!'),
            "both destructive rows must be marked: {rows:?}"
        );
    }

    #[test]
    fn deleting_instance_asks_first_and_then_closes_the_workspace() {
        use crate::tui::pages::workspace::WorkspaceCategory;
        let mut app = app_with_instance();
        app.open_workspace();
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = WorkspaceCategory::Manage;
            workspace.selected = 4;
        }
        app.activate_workspace_row();
        // The workspace stays open behind the dialog: cancelling must return
        // to it, not to a list whose cursor may point elsewhere.
        assert!(app.workspace_is_open());
        let Some(crate::tui::modal::Modal::Confirm { action, .. }) = app.modal else {
            panic!("the removal confirmation must be open");
        };
        assert!(
            matches!(action, crate::tui::modal::ConfirmAction::RemoveInstance(_)),
            "the action must carry the instance id: {action:?}"
        );
    }

    #[test]
    fn delete_key_in_manage_targets_the_workspace_instance() {
        use crate::tui::pages::workspace::WorkspaceCategory;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = app_with_instance();
        let mut second = app.instances.items[0].clone();
        second.id = uuid::Uuid::new_v4();
        second.name = "other".into();
        app.instances.items.push(second);
        app.instances.focused = 1;
        app.open_workspace();
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = WorkspaceCategory::Manage;
        }
        // The list cursor is on a different instance than the workspace.
        app.instances.focused = 0;
        let workspace_id = app.workspace.as_ref().expect("workspace").instance;
        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        let Some(crate::tui::modal::Modal::Confirm { action, .. }) = app.modal else {
            panic!("the confirmation must be open");
        };
        assert_eq!(
            action,
            crate::tui::modal::ConfirmAction::RemoveInstance(workspace_id),
            "the confirmation must address the instance the workspace shows"
        );
    }

    #[test]
    fn emptying_the_trash_asks_first() {
        use crate::tui::pages::workspace::WorkspaceCategory;
        let mut app = app_with_instance();
        app.open_workspace();
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = WorkspaceCategory::Manage;
            workspace.selected = 3;
        }
        app.activate_workspace_row();
        let Some(crate::tui::modal::Modal::Confirm { action, .. }) = app.modal else {
            panic!("emptying the trash must go through a confirmation");
        };
        assert!(matches!(
            action,
            crate::tui::modal::ConfirmAction::PurgeInstanceTrash(_)
        ));
        assert!(
            app.workspace_is_open(),
            "cancelling must return to the workspace it came from"
        );
    }

    fn temp_app(name: &str) -> (App, std::path::PathBuf) {
        // Real instances live under `<game>/versions/<name>`; the delete path
        // now goes through `game_directory_of`, which enforces that layout.
        let game =
            std::env::temp_dir().join(format!("tmcl-workspace-{name}-{}", uuid::Uuid::new_v4()));
        let root = game.join("versions").join(name);
        std::fs::create_dir_all(root.join("mods")).expect("create instance dir");
        let mut app = App::new();
        app.page = Page::Instances;
        app.instances.apply(vec![InstanceSummary {
            id: uuid::Uuid::new_v4(),
            name: name.into(),
            version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: None,
            status: "ready".into(),
            path: root.clone(),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
        app.instances.focused = 0;
        app.open_workspace();
        (app, root)
    }

    #[test]
    fn confirming_the_purge_empties_only_this_instance_trash() {
        let (mut app, root) = temp_app("purge");
        std::fs::create_dir_all(root.join(".tmcl-trash/20260101-000000/mods"))
            .expect("create trash");
        std::fs::write(root.join(".tmcl-trash/20260101-000000/mods/a.jar"), b"jar")
            .expect("write trashed file");
        let id = app.instances.items[0].id;
        app.confirm_action(crate::tui::modal::ConfirmAction::PurgeInstanceTrash(id))
            .expect("purge the trash");
        assert!(
            !root.join(".tmcl-trash").exists(),
            "the trash directory must be gone"
        );
        assert!(root.exists(), "the instance itself must survive");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn removing_an_instance_by_id_accepts_the_workspace_one() {
        let (mut app, root) = temp_app("remove");
        // A second row the cursor can sit on while the workspace shows the first.
        let mut other = app.instances.items[0].clone();
        other.id = uuid::Uuid::new_v4();
        other.name = "other".into();
        other.path = root.join("other");
        app.instances.items.push(other.clone());
        app.instances.focused = 0;
        app.instances.focused = 1;
        let target = app.workspace.as_ref().expect("workspace").instance;
        app.confirm_action(crate::tui::modal::ConfirmAction::RemoveInstance(target))
            .expect("remove the workspace instance");
        assert!(
            app.instances.items.iter().all(|item| item.id != target),
            "the named instance is gone"
        );
        assert!(
            app.instances.items.iter().any(|item| item.id == other.id),
            "the other instance survives"
        );
        assert!(
            !app.workspace_is_open(),
            "the workspace follows the instance it was showing"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn removing_an_instance_above_the_cursor_keeps_the_selection() {
        let (mut app, root) = temp_app("remove-above");
        let mut second = app.instances.items[0].clone();
        second.id = uuid::Uuid::new_v4();
        second.name = "b".into();
        second.path = root.join("b");
        let mut third = second.clone();
        third.id = uuid::Uuid::new_v4();
        third.name = "c".into();
        third.path = root.join("c");
        app.instances.items = vec![app.instances.items[0].clone(), second, third.clone()];
        app.close_workspace();
        app.instances.focused = 2;
        let first_id = app.instances.items[0].id;
        app.confirm_action(crate::tui::modal::ConfirmAction::RemoveInstance(first_id))
            .expect("remove the first instance");
        assert_eq!(
            app.instances.selected_item().map(|item| item.id),
            Some(third.id),
            "the same instance must stay selected"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn l_key_launches_the_workspace_instance() {
        use crate::tui::pages::workspace::WorkspaceCategory;
        let mut app = app_with_instance();
        let mut other = app.instances.items[0].clone();
        other.id = uuid::Uuid::new_v4();
        other.name = "other".into();
        app.instances.items.push(other);
        app.instances.focused = 0;
        app.open_workspace();
        app.workspace.as_mut().expect("workspace").category = WorkspaceCategory::Overview;
        // The cursor sits on the second row; L must still launch the first.
        app.instances.focused = 1;
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('L'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(
            app.instances.focused, 0,
            "the launch must address the instance the workspace shows"
        );
    }

    #[test]
    fn e_opens_rename_for_the_workspace_instance() {
        let mut app = app_with_instance();
        let mut other = app.instances.items[0].clone();
        other.id = uuid::Uuid::new_v4();
        other.name = "other".into();
        app.instances.items.push(other);
        app.instances.focused = 0;
        app.open_workspace();
        app.instances.focused = 1;
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('e'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(
            matches!(
                app.modal,
                Some(crate::tui::modal::Modal::Form {
                    action: crate::tui::modal::FormAction::RenameInstance,
                    ..
                })
            ),
            "e must open the rename form"
        );
        assert_eq!(
            app.instances.focused, 0,
            "the form must name the workspace instance"
        );
    }

    #[tokio::test]
    async fn f5_rescans_the_visible_category() {
        let (mut app, root) = temp_app("rescan");
        app.workspace.as_mut().expect("workspace").category =
            crate::tui::pages::workspace::WorkspaceCategory::Mods;
        let directory = root.join("mods");
        let stamp = super::dir_stamp(&directory);
        // A cached, unchanged directory: a plain refresh would skip it.
        app.apply_workspace_scan(
            crate::core::content::ContentKind::Mod,
            Vec::new(),
            stamp,
            None,
        );
        app.workspace.as_mut().expect("workspace").loading = false;
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::F(5),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(
            app.workspace.as_ref().expect("workspace").loading,
            "F5 must bypass the directory stamp"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn a_finished_scan_keeps_loading_while_others_are_pending() {
        let (mut app, root) = temp_app("pending");
        // Opening the workspace already started the four category scans.
        let before = app.workspace.as_ref().expect("workspace").pending_scans;
        app.refresh_workspace_content(crate::core::content::ContentKind::Mod);
        app.refresh_workspace_content(crate::core::content::ContentKind::ResourcePack);
        assert_eq!(
            app.workspace.as_ref().expect("workspace").pending_scans,
            before + 2
        );
        app.apply_workspace_scan(
            crate::core::content::ContentKind::Mod,
            Vec::new(),
            None,
            None,
        );
        let workspace = app.workspace.as_ref().expect("workspace");
        assert_eq!(workspace.pending_scans, before + 1);
        assert!(
            workspace.loading,
            "one answer must not cancel the scans still running"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn a_scan_result_for_another_instance_is_ignored() {
        use crate::core::content::{ContentEntry, ContentKind, ContentMeta};
        let (mut app, root) = temp_app("scan-other");
        let entry = ContentEntry {
            name: "alpha".into(),
            version: "1.0".into(),
            path: root.join("mods/alpha.jar"),
            size: 1,
            enabled: true,
            order: None,
            badge: None,
            warning: None,
            meta: ContentMeta::Unknown,
        };
        app.apply_workspace_scan(ContentKind::Mod, vec![entry], None, None);
        app.apply_workspace_scan_for(
            uuid::Uuid::new_v4(),
            ContentKind::Mod,
            Vec::new(),
            None,
            None,
        );
        assert_eq!(
            app.workspace
                .as_ref()
                .expect("workspace")
                .content
                .count(ContentKind::Mod),
            1,
            "a late answer for the previous instance must not clear this list"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    // Restoring the flag is the point: the test made the file read-only.
    #[allow(clippy::permissions_set_readonly_false)]
    fn a_rejected_write_rolls_the_draft_back() {
        let (mut app, root) = temp_app("rollback");
        let path = root.join("tmcl.json");
        let mut on_disk = crate::core::instance::models::InstanceConfig::new(
            &root,
            app.instances.items[0].id,
            "rollback",
            "1.20.1",
        );
        on_disk.overrides.max_memory_mb = None;
        crate::core::config::storage::write_json_atomic(&path, &on_disk).expect("write metadata");
        app.workspace
            .as_mut()
            .expect("workspace")
            .draft
            .overrides
            .max_memory_mb = Some(4096);
        let mut permissions = std::fs::metadata(&path).expect("stat").permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).expect("make read-only");
        app.persist_workspace_overrides();
        assert_eq!(
            app.workspace
                .as_ref()
                .expect("workspace")
                .draft
                .overrides
                .max_memory_mb,
            None,
            "a rejected write must not leave the new value on screen"
        );
        let mut permissions = std::fs::metadata(&path).expect("stat").permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).expect("restore permissions");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn opening_the_workspace_scans_every_category() {
        let (app, root) = temp_app("prescan");
        let workspace = app.workspace.as_ref().expect("workspace");
        assert_eq!(
            workspace.pending_scans, 4,
            "all four content categories must be requested on open"
        );
        assert!(workspace.loading, "the list must show that it is scanning");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn a_size_result_for_another_instance_is_ignored() {
        let (mut app, root) = temp_app("size");
        let id = app.workspace.as_ref().expect("workspace").instance;
        app.apply_workspace_size(uuid::Uuid::new_v4(), 42);
        assert!(
            app.workspace
                .as_ref()
                .expect("workspace")
                .size_bytes
                .is_none(),
            "a late answer for another instance must not be shown"
        );
        app.apply_workspace_size(id, 42);
        assert_eq!(
            app.workspace.as_ref().expect("workspace").size_bytes,
            Some(42)
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn overview_reports_size_and_content_summary() {
        use crate::core::content::{ContentEntry, ContentKind, ContentMeta};
        let (mut app, root) = temp_app("overview");
        {
            let workspace = app.workspace.as_mut().expect("workspace");
            workspace.size_bytes = Some(2048);
            workspace.content.set(
                ContentKind::Mod,
                vec![ContentEntry {
                    name: "alpha".into(),
                    version: "1.0".into(),
                    path: root.join("mods/alpha.jar"),
                    size: 1,
                    enabled: true,
                    order: None,
                    badge: None,
                    warning: None,
                    meta: ContentMeta::Unknown,
                }],
            );
        }
        let rows = app.workspace_display_rows();
        assert_eq!(rows.len(), 9, "nine overview rows: {rows:?}");
        assert_eq!(app.workspace.as_ref().expect("workspace").row_count(), 9);
        assert!(
            rows[6].1.contains("KB") || rows[6].1.contains("MB"),
            "the size row must show a formatted size: {rows:?}"
        );
        assert!(
            rows[7].1.contains('1') && rows[7].1.contains("mods"),
            "the contents row must count the cache: {rows:?}"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn optifine_row_says_download_manually() {
        let (app, root) = temp_app("optifine");
        let workspace = app.workspace.as_ref().expect("workspace");
        let rows = app.workspace_install_rows(workspace);
        let value_for = |key: &str| {
            rows.iter()
                .find(|(label, _)| label == &app.i18n.text(key))
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        assert_eq!(
            value_for("workspace.install.optifine"),
            app.i18n.text("workspace.install.optifine_manual")
        );
        assert_eq!(
            value_for("workspace.install.quilt"),
            app.i18n.text("workspace.not_installed"),
            "Quilt is installable now; only OptiFine is manual"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn a_java_runtime_below_the_required_major_is_reported() {
        use crate::tui::pages::workspace::WorkspaceCategory;
        let (mut app, root) = temp_app("java");
        app.java = vec![crate::core::java::JavaInstallation {
            executable: std::path::PathBuf::from("C:/java8/bin/java.exe"),
            version: "1.8.0_392".into(),
            major: 8,
            vendor: None,
            is_jdk: false,
            lts: true,
        }];
        {
            let workspace = app.workspace.as_mut().expect("workspace");
            workspace.category = WorkspaceCategory::Launch;
            workspace.selected = crate::tui::pages::workspace::LaunchField::Java.index();
            workspace.draft.overrides.java_path =
                Some(std::path::PathBuf::from("C:/java8/bin/java.exe"));
        }
        // 1.20.1 needs Java 17.
        assert_eq!(app.workspace_java_mismatch(), Some((8, 17)));
        let (message, warning) = app.workspace_note(app.workspace.as_ref().expect("workspace"));
        assert!(warning, "an unusable runtime is a warning: {message:?}");
        assert!(message.contains("17"), "{message}");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn import_input_keeps_quoted_paths_whole() {
        assert_eq!(
            super::split_import_input("\"C:/a b/x.jar\" C:/y.jar"),
            vec!["C:/a b/x.jar".to_owned(), "C:/y.jar".to_owned()]
        );
        assert_eq!(
            super::split_import_input("  C:/one.jar  "),
            vec!["C:/one.jar".to_owned()]
        );
        assert!(super::split_import_input("   ").is_empty());
    }

    #[test]
    fn a_filter_that_hides_every_row_stops_advertising_row_keys() {
        use crate::core::content::{ContentEntry, ContentKind, ContentMeta};
        use crate::tui::pages::workspace::WorkspaceCategory;
        let (mut app, root) = temp_app("filtered");
        {
            let workspace = app.workspace.as_mut().expect("workspace");
            workspace.category = WorkspaceCategory::Mods;
            workspace.content.set(
                ContentKind::Mod,
                vec![ContentEntry {
                    name: "alpha".into(),
                    version: "1.0".into(),
                    path: root.join("mods/alpha.jar"),
                    size: 1,
                    enabled: true,
                    order: None,
                    badge: None,
                    warning: None,
                    meta: ContentMeta::Unknown,
                }],
            );
            workspace.search = "no-such-mod".into();
        }
        let workspace = app.workspace.as_ref().expect("workspace");
        let line = app.workspace_action_line(workspace);
        assert!(
            !line.contains("Space"),
            "no row is visible, so toggling must not be advertised: {line}"
        );
        assert!(
            !line.contains(&app.i18n.text("action.delete")),
            "no row is visible, so delete must not be advertised: {line}"
        );
        assert!(line.contains("F5"), "category-wide keys still work: {line}");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn a_corrupt_metadata_file_is_reported() {
        let root =
            std::env::temp_dir().join(format!("tmcl-workspace-corrupt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("mods")).expect("create instance dir");
        std::fs::write(root.join("tmcl.json"), b"{ not json").expect("write broken metadata");
        let mut app = App::new();
        app.page = Page::Instances;
        app.instances.apply(vec![InstanceSummary {
            id: uuid::Uuid::new_v4(),
            name: "broken".into(),
            version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: None,
            status: "ready".into(),
            path: root.clone(),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
        app.instances.focused = 0;
        app.open_workspace();
        assert!(
            app.workspace_is_open(),
            "a broken file still opens on the list summary"
        );
        assert_eq!(
            app.status.as_str(),
            app.i18n.text("workspace.status.metadata_fallback"),
            "the fallback must be visible"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn rename_refuses_when_the_workspace_instance_left_the_list() {
        let (mut app, root) = temp_app("rename-gone");
        // Simulate the instance disappearing from the list while the workspace
        // is still open: the rename must not fall back to another row.
        app.instances.items.clear();
        app.instances.focused = 0;
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('e'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(app.modal.is_none(), "no form must open");
        assert_eq!(
            app.status.as_str(),
            app.i18n.text("status.select_instance"),
            "the refusal must say why"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn delete_key_on_a_read_only_workspace_screen_does_nothing() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = app_with_instance();
        app.open_workspace();
        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(app.modal.is_none(), "the overview has nothing to delete");
        assert!(app.workspace_is_open());
    }

    #[tokio::test]
    async fn workspace_download_sets_target_and_switches_page() {
        use crate::core::resource::model::ResourceKind;
        use crate::tui::pages::download_center::DownloadCategory;
        let (mut app, root) = app_with_real_instance();
        let target = app.instances.items[0].id;
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Mods;
        }
        app.activate_workspace_row();
        assert_eq!(app.page, Page::DownloadCenter);
        assert_eq!(app.download_target_id, Some(target));
        assert_eq!(
            app.download_center.category,
            DownloadCategory::Resource(ResourceKind::Mod),
            "the matching resource tab must be selected"
        );
        assert!(
            !app.workspace_is_open(),
            "another page must not inherit the workspace key map"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn workspace_empty_state_mentions_the_download_target() {
        use crate::core::content::ContentKind;
        let (app, root) = app_with_real_instance();
        let workspace = app.workspace.as_ref().expect("workspace");
        let (message, hint) = app.workspace_empty_text(workspace, ContentKind::Mod);
        assert!(message.contains("No mods"), "{message}");
        assert!(
            hint.contains(&app.i18n.text("workspace.desc.target")),
            "{hint}"
        );
        assert!(
            hint.contains(&app.i18n.text("workspace.empty.open_downloads")),
            "{hint}"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn manage_export_row_opens_a_path_prompt() {
        use crate::tui::pages::workspace::WorkspaceCategory;
        let (mut app, root) = app_with_real_instance();
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = WorkspaceCategory::Manage;
            workspace.selected = 2;
        }
        app.activate_workspace_row();
        match app.modal.as_ref().expect("export prompt") {
            crate::tui::modal::Modal::Prompt { action, value, .. } => {
                assert_eq!(*action, crate::tui::modal::FormAction::ExportModpack);
                assert!(value.ends_with(".zip"), "{value}");
            }
            other => panic!("expected an export prompt, got {other:?}"),
        }
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn export_refuses_to_replace_an_existing_archive() {
        let (mut app, root) = app_with_real_instance();
        let target = root.join("taken.zip");
        std::fs::write(&target, b"keep").expect("write existing archive");
        app.export_workspace_instance(&target)
            .expect("refusal is a status, not an error");
        assert_eq!(
            app.status.as_str(),
            app.i18n.text("workspace.export.exists")
        );
        assert_eq!(std::fs::read(&target).expect("read archive"), b"keep");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn renaming_refreshes_the_open_workspace() {
        let (mut app, root) = app_with_real_instance();
        let name = format!("renamed-{}", uuid::Uuid::new_v4());
        app.submit_form(
            crate::tui::modal::FormAction::RenameInstance,
            vec![name.clone()],
        )
        .expect("rename");
        let workspace = app.workspace.as_ref().expect("workspace stays open");
        assert_eq!(workspace.name, name);
        let renamed = root.parent().expect("parent").join(&name);
        assert_eq!(workspace.root, renamed);
        assert_eq!(workspace.draft.path, renamed);
        assert!(renamed.join("tmcl.json").is_file());
        std::fs::remove_dir_all(&renamed).expect("clean instance");
    }

    #[test]
    fn workspace_category_movement_wraps() {
        let mut app = app_with_instance();
        app.open_workspace();
        app.move_workspace_category(-1);
        assert_eq!(
            app.workspace.as_ref().expect("workspace").category,
            crate::tui::pages::workspace::WorkspaceCategory::Manage
        );
        app.move_workspace_category(1);
        assert_eq!(
            app.workspace.as_ref().expect("workspace").category,
            crate::tui::pages::workspace::WorkspaceCategory::Overview
        );
    }

    fn app_with_real_instance() -> (App, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("tmcl-workspace-launch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create instance dir");
        let id = uuid::Uuid::new_v4();
        let instance = crate::core::instance::models::InstanceConfig::new(
            &root,
            uuid::Uuid::new_v4(),
            "demo",
            "1.20.1",
        );
        crate::core::config::storage::write_json_atomic(&root.join("tmcl.json"), &instance)
            .expect("write instance metadata");
        let mut app = App::new();
        app.page = Page::Instances;
        app.instances.apply(vec![InstanceSummary {
            id,
            name: "demo".into(),
            version: "1.20.1".into(),
            loader: "Vanilla".into(),
            loader_version: None,
            status: "ready".into(),
            path: root.clone(),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
        app.instances.focused = 0;
        app.open_workspace();
        (app, root)
    }

    fn select_field(app: &mut App, field: crate::tui::pages::workspace::LaunchField) {
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Launch;
            workspace.selected = field.index();
        }
    }

    fn read_overrides(root: &std::path::Path) -> crate::core::instance::overrides::LaunchOverrides {
        crate::core::config::storage::read_json::<crate::core::instance::models::InstanceConfig>(
            &root.join("tmcl.json"),
        )
        .expect("read instance metadata")
        .overrides
    }

    #[test]
    fn launch_rows_follow_global_values() {
        let (mut app, root) = app_with_real_instance();
        select_field(
            &mut app,
            crate::tui::pages::workspace::LaunchField::MemoryMax,
        );
        let rows = app.launch_rows();
        assert_eq!(rows.len(), 9);
        let max = rows
            .iter()
            .find(|row| row.field == crate::tui::pages::workspace::LaunchField::MemoryMax)
            .expect("memory max row");
        assert!(max.following);
        assert!(max.value.contains("Follow global"), "{}", max.value);
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn launch_rows_mark_overridden_fields() {
        let (mut app, root) = app_with_real_instance();
        select_field(
            &mut app,
            crate::tui::pages::workspace::LaunchField::MemoryMax,
        );
        app.toggle_workspace_follow();
        let rows = app.launch_rows();
        let max = rows
            .iter()
            .find(|row| row.field == crate::tui::pages::workspace::LaunchField::MemoryMax)
            .expect("memory max row");
        assert!(!max.following);
        assert!(max.value.contains("Custom"), "{}", max.value);
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn space_switches_between_follow_and_custom() {
        let (mut app, root) = app_with_real_instance();
        select_field(
            &mut app,
            crate::tui::pages::workspace::LaunchField::WindowMode,
        );
        assert!(read_overrides(&root).fullscreen.is_none());
        app.toggle_workspace_follow();
        assert_eq!(read_overrides(&root).fullscreen, Some(false));
        app.toggle_workspace_follow();
        assert!(read_overrides(&root).fullscreen.is_none());
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn memory_step_is_256_and_clamped() {
        let (mut app, root) = app_with_real_instance();
        select_field(
            &mut app,
            crate::tui::pages::workspace::LaunchField::MemoryMin,
        );
        app.toggle_workspace_follow();
        let before = read_overrides(&root).min_memory_mb.expect("custom minimum");
        app.step_workspace_value(1);
        let after = read_overrides(&root).min_memory_mb.expect("custom minimum");
        assert_eq!(after, before + 256);
        // Clamped at the lower bound instead of going below 256 MB.
        for _ in 0..40 {
            app.step_workspace_value(-1);
        }
        assert_eq!(read_overrides(&root).min_memory_mb, Some(256));
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn window_size_step_keeps_a_ratio() {
        let (mut app, root) = app_with_real_instance();
        select_field(
            &mut app,
            crate::tui::pages::workspace::LaunchField::WindowSize,
        );
        app.toggle_workspace_follow();
        let before = read_overrides(&root);
        let width = before.width.expect("custom width");
        app.step_workspace_value(1);
        let after = read_overrides(&root);
        assert_eq!(after.width, Some(width + 16));
        assert_eq!(
            after.height,
            Some(((after.width.expect("width") * 9) / 16).clamp(240, 4320))
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn following_rows_ignore_stepping() {
        let (mut app, root) = app_with_real_instance();
        select_field(
            &mut app,
            crate::tui::pages::workspace::LaunchField::MemoryMax,
        );
        app.step_workspace_value(1);
        assert!(
            read_overrides(&root).is_empty(),
            "following rows stay untouched"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn launch_override_splits_arguments() {
        let (mut app, root) = app_with_real_instance();
        app.apply_launch_override(
            crate::tui::pages::workspace::LaunchField::JvmArgs,
            "-Dfoo=bar -Xmx4G",
        );
        assert_eq!(
            read_overrides(&root).jvm_args,
            Some(vec!["-Dfoo=bar".to_owned(), "-Xmx4G".to_owned()])
        );
        // An empty prompt clears the arguments on purpose.
        app.apply_launch_override(crate::tui::pages::workspace::LaunchField::JvmArgs, "");
        assert_eq!(read_overrides(&root).jvm_args, Some(Vec::new()));
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn empty_server_address_clears_auto_connect() {
        let (mut app, root) = app_with_real_instance();
        app.apply_launch_override(crate::tui::pages::workspace::LaunchField::Server, "");
        assert_eq!(
            read_overrides(&root).server_address,
            Some(String::new()),
            "an empty address must survive as an explicit override"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn choosing_automatic_clears_java_override() {
        let (mut app, root) = app_with_real_instance();
        app.apply_launch_override(
            crate::tui::pages::workspace::LaunchField::Java,
            "Java 17|C:/Java/jdk-17/bin/java.exe",
        );
        assert_eq!(
            read_overrides(&root).java_path,
            Some(std::path::PathBuf::from("C:/Java/jdk-17/bin/java.exe"))
        );
        app.apply_launch_override(
            crate::tui::pages::workspace::LaunchField::Java,
            "Automatic|",
        );
        assert!(read_overrides(&root).java_path.is_none());
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn setting_change_writes_tmcl_json() {
        let (mut app, root) = app_with_real_instance();
        select_field(
            &mut app,
            crate::tui::pages::workspace::LaunchField::MemoryMax,
        );
        app.toggle_workspace_follow();
        app.step_workspace_value(1);
        let saved: crate::core::instance::models::InstanceConfig =
            crate::core::config::storage::read_json(&root.join("tmcl.json"))
                .expect("read metadata");
        assert_eq!(
            saved.overrides.max_memory_mb,
            Some(saved.overrides.max_memory_mb.expect("value")),
            "the step must reach the file"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn reset_clears_all_overrides() {
        let (mut app, root) = app_with_real_instance();
        select_field(
            &mut app,
            crate::tui::pages::workspace::LaunchField::MemoryMax,
        );
        app.toggle_workspace_follow();
        assert!(!read_overrides(&root).is_empty());
        app.reset_workspace_overrides();
        assert!(read_overrides(&root).is_empty());
        std::fs::remove_dir_all(root).expect("clean instance");
    }
    #[tokio::test]
    async fn resource_pack_toggle_writes_options() {
        let (mut app, root) = app_with_packs(&["alpha.zip", "beta.zip"], None);
        scanned(
            &mut app,
            &root,
            crate::core::content::ContentKind::ResourcePack,
        );
        assert_eq!(app.workspace_content_entries().len(), 2);
        assert!(
            app.workspace_content_entries()
                .iter()
                .all(|entry| !entry.enabled),
            "nothing is in options.txt yet"
        );

        app.workspace.as_mut().expect("workspace").selected = 0;
        app.toggle_workspace_content();
        let text = options_text(&root);
        assert!(
            text.contains("resourcePacks:[\"vanilla\",\"file/alpha.zip\"]"),
            "{text}"
        );

        // Toggling again removes it, leaving only the fallback pack.
        scanned(
            &mut app,
            &root,
            crate::core::content::ContentKind::ResourcePack,
        );
        app.workspace.as_mut().expect("workspace").selected = 0;
        app.toggle_workspace_content();
        assert!(
            options_text(&root).contains("resourcePacks:[\"vanilla\"]"),
            "{}",
            options_text(&root)
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn pack_order_matches_options_array() {
        let (mut app, root) = app_with_packs(&["alpha.zip", "beta.zip", "gamma.zip"], None);
        std::fs::write(
            root.join("options.txt"),
            "resourcePacks:[\"vanilla\",\"file/gamma.zip\",\"file/alpha.zip\"]\n",
        )
        .expect("write options");
        scanned(
            &mut app,
            &root,
            crate::core::content::ContentKind::ResourcePack,
        );

        let names = app
            .workspace_content_entries()
            .iter()
            .map(|entry| (entry.display_file_name(), entry.enabled))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                ("gamma.zip".to_owned(), true),
                ("alpha.zip".to_owned(), true),
                ("beta.zip".to_owned(), false),
            ],
            "enabled packs must follow the saved order, not the alphabet"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn reorder_writes_array_in_new_order() {
        let (mut app, root) = app_with_packs(&["alpha.zip", "beta.zip"], None);
        std::fs::write(
            root.join("options.txt"),
            "resourcePacks:[\"vanilla\",\"file/alpha.zip\",\"file/beta.zip\"]\n",
        )
        .expect("write options");
        scanned(
            &mut app,
            &root,
            crate::core::content::ContentKind::ResourcePack,
        );

        app.workspace.as_mut().expect("workspace").selected = 0;
        app.reorder_workspace_content(1);
        assert!(
            options_text(&root).contains("[\"vanilla\",\"file/beta.zip\",\"file/alpha.zip\"]"),
            "{}",
            options_text(&root)
        );
        assert_eq!(
            app.workspace.as_ref().expect("workspace").selected,
            1,
            "the cursor follows the pack it moved"
        );

        // And back up again.
        scanned(
            &mut app,
            &root,
            crate::core::content::ContentKind::ResourcePack,
        );
        app.workspace.as_mut().expect("workspace").selected = 1;
        app.reorder_workspace_content(-1);
        assert!(
            options_text(&root).contains("[\"vanilla\",\"file/alpha.zip\",\"file/beta.zip\"]"),
            "{}",
            options_text(&root)
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    /// An instance holding one world, so the world jobs have something to act on.
    fn app_with_world() -> (App, std::path::PathBuf) {
        let (sender, _) = tokio::sync::mpsc::unbounded_channel();
        app_with_world_and(sender)
    }

    /// Same fixture, but the caller keeps the receiving end so the background
    /// jobs' `AppEvent`s can be observed.
    fn app_with_world_and(
        sender: tokio::sync::mpsc::UnboundedSender<crate::tui::event::AppEvent>,
    ) -> (App, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("tmcl-workspace-worlds-{}", uuid::Uuid::new_v4()));
        let world = root.join("saves/Castle");
        std::fs::create_dir_all(world.join("region")).expect("create world dir");
        std::fs::write(world.join("level.dat"), b"not nbt").expect("write level.dat");
        std::fs::write(world.join("region/r.0.0.mca"), b"region").expect("write region");
        let id = uuid::Uuid::new_v4();
        let instance = crate::core::instance::models::InstanceConfig::new(
            &root,
            uuid::Uuid::new_v4(),
            "demo",
            "1.20.1",
        );
        crate::core::config::storage::write_json_atomic(&root.join("tmcl.json"), &instance)
            .expect("write instance metadata");
        let mut app = App::with_state(
            crate::core::config::paths::DataPaths::from_appdata(
                std::env::temp_dir().join(format!("TMCL-world-{}", uuid::Uuid::new_v4())),
            ),
            crate::core::config::models::Config::default(),
            sender,
        );
        app.page = Page::Instances;
        app.instances.apply(vec![InstanceSummary {
            id,
            name: "demo".into(),
            version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: None,
            status: "ready".into(),
            path: root.clone(),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
        app.instances.focused = 0;
        app.open_workspace();
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Worlds;
        }
        let mut entries = crate::core::content::scan::scan(
            &root.join("saves"),
            crate::core::content::ContentKind::World,
        );
        crate::core::content::scan::apply_saved_state(
            &root,
            crate::core::content::ContentKind::World,
            &mut entries,
        );
        app.apply_workspace_scan(
            crate::core::content::ContentKind::World,
            entries,
            None,
            None,
        );
        (app, root)
    }

    #[test]
    fn world_jobs_are_ignored_outside_the_worlds_category() {
        let (mut app, root) = app_with_packs(&["alpha.zip"], None);
        scanned(
            &mut app,
            &root,
            crate::core::content::ContentKind::ResourcePack,
        );
        let before = app.status.as_str().to_owned();
        app.backup_workspace_world();
        assert_eq!(app.status.as_str(), before, "only the worlds list backs up");
        assert!(
            !root.join(crate::core::content::backup::BACKUP_DIR).exists(),
            "nothing may be written for another category"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn world_backup_reports_the_archive_name() {
        let (mut app, root) = app_with_world();
        let outcome = crate::tui::event::WorldJobOutcome::BackedUp(
            root.join(crate::core::content::backup::BACKUP_DIR)
                .join("Castle-20260101-120000.zip"),
        );
        app.finish_world_job(Ok(outcome));
        assert!(
            app.status.as_str().contains("Castle-20260101-120000.zip"),
            "{}",
            app.status.as_str()
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn world_job_failure_is_reported() {
        let (mut app, root) = app_with_world();
        app.finish_world_job(Err("disk full".into()));
        assert!(
            app.status.as_str().contains("disk full"),
            "{}",
            app.status.as_str()
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn world_clone_is_refused_while_the_game_runs() {
        let (mut app, root) = app_with_world();
        let running_id = app.workspace.as_ref().expect("workspace").instance;
        app.workspace.as_mut().expect("workspace").selected = 0;
        app.active_launch = Some(crate::tui::app::ActiveLaunch::starting(
            running_id,
            "demo".into(),
        ));
        app.clone_workspace_world();
        assert_eq!(
            app.status.as_str(),
            app.i18n.text("workspace.status.running_locked")
        );
        let copies = std::fs::read_dir(root.join("saves"))
            .expect("read saves")
            .flatten()
            .count();
        assert_eq!(copies, 1, "no copy may be created while the game runs");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    /// Collect the next world-job result, skipping the scans that opening the
    /// workspace kicks off in the same channel.
    async fn next_world_job(
        events: &mut tokio::sync::mpsc::UnboundedReceiver<crate::tui::event::AppEvent>,
    ) -> std::result::Result<crate::tui::event::WorldJobOutcome, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(!left.is_zero(), "no world job result arrived in 30s");
            match tokio::time::timeout(left, events.recv()).await {
                Ok(Some(crate::tui::event::AppEvent::WorldJob(result))) => return result,
                Ok(Some(_)) => continue,
                Ok(None) => panic!("the event channel closed before the job finished"),
                Err(_) => panic!("timed out waiting for the world job"),
            }
        }
    }

    /// A backup must run on the blocking pool and report through the event
    /// channel; the earlier tests called `finish_world_job` directly.
    #[tokio::test]
    async fn backup_runs_in_the_background_and_sends_the_event() {
        let (sender, mut events) = tokio::sync::mpsc::unbounded_channel();
        let (mut app, root) = app_with_world_and(sender);
        app.workspace.as_mut().expect("workspace").selected = 0;
        app.backup_workspace_world();
        assert_eq!(
            app.status.as_str(),
            app.i18n.text("workspace.world.backing_up"),
            "the status must say the job started"
        );
        let outcome = next_world_job(&mut events).await.expect("backup succeeded");
        let crate::tui::event::WorldJobOutcome::BackedUp(archive) = outcome else {
            panic!("expected a backup, got {outcome:?}");
        };
        assert!(
            archive.is_file(),
            "the archive the event names must exist: {}",
            archive.display()
        );
        app.finish_world_job(Ok(crate::tui::event::WorldJobOutcome::BackedUp(
            archive.clone(),
        )));
        assert!(
            app.status.as_str().contains(&outcome_name(&archive)),
            "{}",
            app.status.as_str()
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    fn outcome_name(path: &std::path::Path) -> String {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn clone_runs_in_the_background_and_sends_the_event() {
        let (sender, mut events) = tokio::sync::mpsc::unbounded_channel();
        let (mut app, root) = app_with_world_and(sender);
        app.workspace.as_mut().expect("workspace").selected = 0;
        app.clone_workspace_world();
        let outcome = next_world_job(&mut events).await.expect("clone succeeded");
        let crate::tui::event::WorldJobOutcome::Cloned(copy) = outcome else {
            panic!("expected a clone, got {outcome:?}");
        };
        assert!(
            copy.join("level.dat").is_file(),
            "the copied world must be complete: {}",
            copy.display()
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn restore_runs_in_the_background_and_sends_the_event() {
        let (sender, mut events) = tokio::sync::mpsc::unbounded_channel();
        let (mut app, root) = app_with_world_and(sender);
        let world = root.join("saves/Castle");
        let archive =
            crate::core::content::backup::backup_world(&root, &world).expect("make a backup");
        // Damage the world so a successful restore is observable.
        std::fs::write(world.join("level.dat"), b"damaged").expect("damage world");
        app.workspace.as_mut().expect("workspace").selected = 0;
        app.restore_workspace_world(archive);
        let outcome = next_world_job(&mut events)
            .await
            .expect("restore succeeded");
        let crate::tui::event::WorldJobOutcome::Restored { safety } = outcome else {
            panic!("expected a restore, got {outcome:?}");
        };
        assert!(
            safety.is_file(),
            "a safety backup of the pre-restore world must exist: {}",
            safety.display()
        );
        assert_eq!(
            std::fs::read(world.join("level.dat")).expect("read restored world"),
            b"not nbt",
            "the backup content must be back on disk"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn restore_picker_lists_the_worlds_backups() {
        let (mut app, root) = app_with_world();
        let world = root.join("saves/Castle");
        crate::core::content::backup::backup_world(&root, &world).expect("first backup");
        crate::core::content::backup::backup_world(&root, &world).expect("second backup");
        app.workspace.as_mut().expect("workspace").selected = 0;
        app.open_restore_picker();
        match app.modal.as_ref().expect("picker modal") {
            crate::tui::modal::Modal::Form {
                action,
                selection_options,
                ..
            } => {
                assert_eq!(*action, crate::tui::modal::FormAction::RestoreWorld);
                assert_eq!(selection_options.get(&0).map(Vec::len), Some(2));
            }
            other => panic!("expected a form picker, got {other:?}"),
        }
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn restore_picker_explains_an_empty_backup_list() {
        let (mut app, root) = app_with_world();
        app.workspace.as_mut().expect("workspace").selected = 0;
        app.open_restore_picker();
        assert!(app.modal.is_none(), "no backups means no picker");
        assert_eq!(
            app.status.as_str(),
            app.i18n.text("workspace.world.no_backups")
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    /// An instance whose client json says Forge, with `mods` copied in.
    fn app_with_forge(mods: usize) -> (App, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("tmcl-workspace-install-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("mods")).expect("create mods dir");
        for index in 0..mods {
            std::fs::write(root.join(format!("mods/mod{index}.jar")), b"jar").expect("write mod");
        }
        std::fs::write(
            root.join("demo.json"),
            r#"{"id":"demo","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
                "libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.4.22"}]}"#,
        )
        .expect("write client json");
        let mut instance = crate::core::instance::models::InstanceConfig::new(
            &root,
            uuid::Uuid::new_v4(),
            "demo",
            "1.20.1",
        );
        instance.loader = crate::core::instance::models::InstanceLoader::Forge {
            version: "1.20.1-47.4.22".into(),
        };
        let game_directory = root.parent().expect("parent").to_path_buf();
        crate::core::config::storage::write_json_atomic(&root.join("tmcl.json"), &instance)
            .expect("write instance metadata");
        let id = uuid::Uuid::new_v4();
        let mut app = App::new();
        app.page = Page::Instances;
        app.instances.apply(vec![InstanceSummary {
            id,
            name: "demo".into(),
            version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: Some("47.4.22".into()),
            status: "ready".into(),
            path: root.clone(),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
        app.instances.focused = 0;
        app.config.game_directories = vec![game_directory];
        app.open_workspace();
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Install;
            workspace.draft.loader = instance.loader.clone();
            workspace.draft.game_directory_id =
                crate::tui::app::game_directory_id(root.parent().expect("parent"));
        }
        let entries = crate::core::content::scan::scan(
            &root.join("mods"),
            crate::core::content::ContentKind::Mod,
        );
        app.apply_workspace_scan(crate::core::content::ContentKind::Mod, entries, None, None);
        (app, root)
    }

    #[test]
    fn install_rows_follow_the_client_json() {
        let (app, root) = app_with_forge(0);
        let rows = app.install_statuses();
        assert_eq!(rows.len(), 6);
        assert_eq!(
            rows[1].1,
            crate::core::instance::config::LoaderState::Installed("1.20.1-47.4.22".into()),
            "the forge row reads the client json"
        );
        assert_eq!(
            rows[3].1,
            crate::core::instance::config::LoaderState::Incompatible { current: "Forge" },
            "fabric cannot be added while forge owns the instance"
        );
        assert_eq!(
            rows[4].1,
            crate::core::instance::config::LoaderState::Incompatible { current: "Forge" },
            "Quilt is installable, but not next to Forge"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn loader_switch_requires_confirmation_when_mods_present() {
        let (mut app, root) = app_with_forge(3);
        app.request_loader_install(
            crate::core::instance::config::InstallableLoader::Fabric,
            "0.15.11|0.15.11",
        );
        match app.modal.as_ref().expect("confirmation") {
            crate::tui::modal::Modal::Confirm { action, prompt } => {
                assert_eq!(*action, crate::tui::modal::ConfirmAction::SwitchLoader);
                assert!(prompt.contains('3'), "{prompt}");
            }
            other => panic!("expected a confirm dialog, got {other:?}"),
        }
        assert!(
            app.pending_loader_install.is_some(),
            "the payload waits outside the Copy enum"
        );
        assert!(
            app.task_snapshots.is_empty(),
            "nothing may be queued before the answer"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn loader_switch_without_mods_starts_immediately() {
        let (mut app, root) = app_with_forge(0);
        app.request_loader_install(
            crate::core::instance::config::InstallableLoader::Fabric,
            "0.15.11|0.15.11",
        );
        assert!(app.modal.is_none(), "no mods means no confirmation");
        assert!(
            app.status.as_str().contains("Fabric"),
            "{}",
            app.status.as_str()
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn install_backs_up_client_json() {
        let (mut app, root) = app_with_forge(0);
        app.start_loader_install(
            crate::tui::app::state::workspace_install::PendingLoaderInstall {
                kind: crate::core::instance::config::InstallableLoader::Fabric,
                loader_version: Some("0.15.11".into()),
                minecraft_version: "1.20.1".into(),
            },
        );
        let backups = std::fs::read_dir(&root)
            .expect("read instance")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with("demo.json.bak-"))
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1, "{backups:?}");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn uninstall_dispatches_a_vanilla_install() {
        let (mut app, root) = app_with_forge(0);
        app.uninstall_workspace_loader();
        assert!(matches!(
            app.modal.as_ref(),
            Some(crate::tui::modal::Modal::Confirm {
                action: crate::tui::modal::ConfirmAction::SwitchLoader,
                ..
            })
        ));
        let pending = app
            .pending_loader_install
            .clone()
            .expect("pending uninstall");
        assert_eq!(
            pending.kind,
            crate::core::instance::config::InstallableLoader::Minecraft
        );
        assert_eq!(pending.minecraft_version, "1.20.1");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn uninstall_on_vanilla_only_reports_it() {
        let (mut app, root) = app_with_forge(0);
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.draft.loader = crate::core::instance::models::InstanceLoader::Vanilla;
        }
        app.uninstall_workspace_loader();
        assert!(app.modal.is_none());
        assert_eq!(
            app.status.as_str(),
            app.i18n.text("workspace.install.already_vanilla")
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn deleting_a_pack_forgets_it_in_options() {
        let (mut app, root) = app_with_packs(&["alpha.zip", "beta.zip"], None);
        std::fs::write(
            root.join("options.txt"),
            "resourcePacks:[\"vanilla\",\"file/alpha.zip\",\"file/beta.zip\"]\n",
        )
        .expect("write options");
        scanned(
            &mut app,
            &root,
            crate::core::content::ContentKind::ResourcePack,
        );
        app.workspace.as_mut().expect("workspace").selected = 0;
        let instance = app.workspace.as_ref().expect("workspace").instance;
        app.confirm_action(crate::tui::modal::ConfirmAction::DeleteContent {
            instance,
            kind: crate::core::content::ContentKind::ResourcePack,
            index: 0,
        })
        .expect("delete pack");
        let text = options_text(&root);
        assert!(
            !text.contains("alpha"),
            "the deleted pack must leave options.txt: {text}"
        );
        assert!(text.contains("file/beta.zip"), "{text}");
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn reorder_disabled_row_warns() {
        let (mut app, root) = app_with_packs(&["alpha.zip", "beta.zip"], None);
        scanned(
            &mut app,
            &root,
            crate::core::content::ContentKind::ResourcePack,
        );
        app.workspace.as_mut().expect("workspace").selected = 1;
        app.reorder_workspace_content(1);
        assert_eq!(
            app.status.as_str(),
            app.i18n.text("workspace.status.enable_first")
        );
        assert!(
            !root.join("options.txt").exists(),
            "a rejected reorder must not create the file"
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[test]
    fn shader_category_warns_without_loader() {
        let (mut app, root) = app_with_packs(&[], Some("BSL.zip"));
        scanned(&mut app, &root, crate::core::content::ContentKind::Shader);
        let workspace = app.workspace.as_ref().expect("workspace");
        assert!(workspace.shader_loader.is_none());
        let entries = app.workspace_content_entries();
        assert_eq!(entries.len(), 1);
        assert!(
            !entries[0].enabled,
            "a shader cannot be on without a loader"
        );

        app.toggle_workspace_content();
        assert_eq!(
            app.status.as_str(),
            app.i18n.text("workspace.status.shader_need_loader")
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn shader_toggle_writes_iris_config() {
        let (mut app, root) = app_with_packs(&[], Some("BSL.zip"));
        std::fs::create_dir_all(root.join("mods")).expect("create mods dir");
        std::fs::write(root.join("mods/iris-1.7.0.jar"), b"jar").expect("write iris");
        scanned(&mut app, &root, crate::core::content::ContentKind::Shader);
        assert_eq!(
            app.workspace.as_ref().expect("workspace").shader_loader,
            Some(crate::core::content::shader::ShaderLoader::Iris)
        );

        app.workspace.as_mut().expect("workspace").selected = 0;
        app.toggle_workspace_content();
        assert_eq!(
            crate::core::content::shader::selected(&root).as_deref(),
            Some("BSL.zip")
        );
        std::fs::remove_dir_all(root).expect("clean instance");
    }

    #[tokio::test]
    async fn pack_toggle_is_refused_while_the_game_runs() {
        let (mut app, root) = app_with_packs(&["alpha.zip"], None);
        scanned(
            &mut app,
            &root,
            crate::core::content::ContentKind::ResourcePack,
        );
        let running_id = app.workspace.as_ref().expect("workspace").instance;
        app.active_launch = Some(crate::tui::app::ActiveLaunch::starting(
            running_id,
            "demo".into(),
        ));
        app.workspace.as_mut().expect("workspace").selected = 0;
        app.toggle_workspace_content();
        assert_eq!(
            app.status.as_str(),
            app.i18n.text("workspace.status.running_locked")
        );
        assert!(
            !root.join("options.txt").exists(),
            "the game rewrites options.txt on exit, so the write must not happen"
        );

        // A different instance running is not this instance's problem.
        app.active_launch = Some(crate::tui::app::ActiveLaunch::starting(
            uuid::Uuid::new_v4(),
            "other".into(),
        ));
        app.toggle_workspace_content();
        assert!(root.join("options.txt").exists());
        std::fs::remove_dir_all(root).expect("clean instance");
    }
}
