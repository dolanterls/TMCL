//! Instances page.

use super::*;

impl App {
    pub(in crate::tui::app) fn instances_hints(&self) -> Vec<crate::tui::chrome::Hint> {
        vec![
            self.hint("Enter", "action.details"),
            self.hint("l", "action.launch"),
            self.hint("n", "action.new"),
            self.hint("i", "action.import"),
            self.hint("I", "action.import_modpack"),
            self.hint("r", "action.rename"),
            self.hint("d", "action.remove"),
            self.hint("x", "action.export_script").with_priority(11),
            // `Space` marks the current instance; the overview and every launch
            // without a row selection follow it. It sits after the row actions
            // and yields first, so a narrow footer keeps `d` and `x` instead.
            self.hint("Space", "action.set_current").with_priority(12),
            self.hint("[ ]", "action.change_filter"),
        ]
    }

    pub(in crate::tui::app) const INSTANCE_FILTERS: [&'static str; 6] =
        ["", "vanilla", "fabric", "forge", "neoforge", "quilt"];

    pub(in crate::tui::app) fn instance_filter_labels(&self) -> Vec<String> {
        let mut labels = vec![self.i18n.text("common.all")];
        labels.extend(
            ["Vanilla", "Fabric", "Forge", "NeoForge", "Quilt"]
                .into_iter()
                .map(|loader| self.loader_label(loader)),
        );
        labels
    }

    /// Display name of a loader. Only Vanilla has a translated name — the rest
    /// are brand names — but the mapping belongs in one place so the filter
    /// tabs, the list rows and the detail pane cannot drift apart.
    ///
    /// The comparison stays on the canonical English spelling, which is also
    /// what the loader filter matches against.
    pub(in crate::tui::app) fn loader_label(&self, loader: &str) -> String {
        if loader.eq_ignore_ascii_case("vanilla") {
            self.i18n.text("loader.vanilla")
        } else {
            loader.to_owned()
        }
    }

    pub(in crate::tui::app) fn visible_instance_indices(&self) -> Vec<usize> {
        let filter = Self::INSTANCE_FILTERS
            .get(self.instance_filter)
            .copied()
            .unwrap_or("");
        self.instances
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| filter.is_empty() || item.loader.eq_ignore_ascii_case(filter))
            .map(|(index, _)| index)
            .collect()
    }

    pub(in crate::tui::app) fn set_instance_filter(&mut self, index: usize) {
        self.instance_filter = index.min(Self::INSTANCE_FILTERS.len() - 1);
        let visible = self.visible_instance_indices();
        if !visible.contains(&self.instances.focused) {
            self.instances.focused = visible.first().copied().unwrap_or(0);
        }
    }

    pub(in crate::tui::app) fn cycle_instance_filter(&mut self, delta: isize) {
        let count = Self::INSTANCE_FILTERS.len();
        let next = if delta < 0 {
            (self.instance_filter + count - 1) % count
        } else {
            (self.instance_filter + 1) % count
        };
        self.set_instance_filter(next);
    }

    /// The instance marked 当前: what the overview shows and what its play
    /// control acts on. Distinct from [`Self::selected_item`], which is only
    /// the row the cursor is on.
    pub(in crate::tui::app) fn current_instance(&self) -> Option<&InstanceSummary> {
        let id = self.instances.current_id?;
        self.instances.items.iter().find(|item| item.id == id)
    }

    pub(in crate::tui::app) fn move_instance_focus(&mut self, delta: isize) {
        self.pane_scroll = 0;
        let visible = if self.page == Page::Overview {
            (0..self.instances.items.len()).collect::<Vec<_>>()
        } else {
            self.visible_instance_indices()
        };
        if visible.is_empty() {
            return;
        }
        let position = visible
            .iter()
            .position(|index| *index == self.instances.focused)
            .unwrap_or(0);
        let next = if delta < 0 {
            position.saturating_sub(delta.unsigned_abs())
        } else {
            position
                .saturating_add(delta as usize)
                .min(visible.len() - 1)
        };
        self.instances.focused = visible[next];
    }

    pub(in crate::tui::app) fn open_create_form(&mut self) {
        let version = self
            .versions
            .iter()
            .find(|version| version.kind == "release")
            .map(|version| version.id.clone())
            .unwrap_or_default();
        self.open_create_form_with_version(version);
    }

    pub(in crate::tui::app) fn open_create_form_with_version(&mut self, version: String) {
        let game_directory = self
            .config
            .game_directories
            .first()
            .map(|path| display_path(path))
            .unwrap_or_default();
        let mut modal = Modal::form(
            self.i18n.text("form.create_instance"),
            &[
                self.i18n.text("form.game_directory"),
                self.i18n.text("form.instance_name"),
                self.i18n.text("form.minecraft_version"),
                self.i18n.text("form.loader"),
                self.i18n.text("form.loader_version"),
            ],
            &[
                game_directory,
                String::new(),
                version,
                "vanilla".into(),
                String::new(),
            ],
            FormAction::CreateInstance,
        )
        .with_directory_picker(0)
        .with_version_picker(CREATE_VERSION_FIELD, self.versions.clone())
        .with_selection_picker(
            CREATE_LOADER_FIELD,
            ["vanilla", "fabric", "quilt", "forge", "neoforge"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )
        .with_selection_picker(CREATE_LOADER_VERSION_FIELD, Vec::new());
        modal.set_selection_unavailable(
            CREATE_LOADER_VERSION_FIELD,
            self.i18n.text("form.vanilla_no_loader_version"),
        );
        self.modal = Some(modal);
    }

    pub(in crate::tui::app) fn open_import_form(&mut self) {
        let game_directory = self
            .config
            .game_directories
            .first()
            .map(|path| display_path(path))
            .unwrap_or_default();
        self.modal = Some(
            Modal::form(
                self.i18n.text("form.import_instances"),
                &[self.i18n.text("form.game_directory")],
                &[game_directory],
                FormAction::ImportInstance,
            )
            .with_directory_picker(0),
        );
    }

    /// Create an instance from a modpack archive picked from disk: the same
    /// three answers as a downloaded modpack, except field 0 is the archive
    /// itself rather than a cache path we own.
    pub(in crate::tui::app) fn open_local_modpack_form(&mut self) {
        let game_directory = self
            .config
            .game_directories
            .first()
            .map(|path| display_path(path))
            .unwrap_or_default();
        let modal = Modal::form(
            self.i18n.text("form.import_modpack"),
            &[
                self.i18n.text("form.modpack_archive"),
                self.i18n.text("form.game_directory"),
                self.i18n.text("form.new_instance_name"),
            ],
            &[String::new(), game_directory, String::new()],
            FormAction::ImportLocalModpack,
        )
        .with_file_picker(0)
        .with_directory_picker(1);
        self.modal = Some(modal);
        self.status = Status::info(self.i18n.text("form.import_modpack_hint"));
    }

    pub(in crate::tui::app) fn set_focused_as_current(&mut self) {
        let Some(instance) = self.instances.focused_item().cloned() else {
            self.status = Status::warning(self.i18n.text("status.create_instance_first"));
            return;
        };
        let previous = self.instances.current_id;
        self.instances.set_current(Some(instance.id));
        self.config.state.current_instance_id = Some(instance.id.to_string());
        if self.download_target_id.is_none() || self.download_target_id == previous {
            self.download_target_id = Some(instance.id);
        }
        match self.save_config() {
            Ok(()) => {
                if self.download_center.category.resource_kind().is_some() {
                    self.resource_projects.clear();
                    self.resource_files.clear();
                    self.resource_detail = None;
                    self.resource_detail_project = None;
                    self.spawn_resource_search();
                }
                self.status = Status::info(
                    self.i18n
                        .format("status.current_instance", &[("name", &instance.name)]),
                );
            }
            Err(error) => {
                self.status = Status::error(self.i18n.format(
                    "status.current_save_failed",
                    &[("error", &error.to_string())],
                ));
            }
        }
    }
}

pub(crate) fn command(
    _mode: crate::tui::command::Mode,
    key: crossterm::event::KeyEvent,
) -> crate::tui::command::Command {
    use crate::tui::command::Command;
    use crossterm::event::KeyCode;

    let plain = key.modifiers.is_empty();
    match key.code {
        KeyCode::Char('l') if plain => Command::Launch,
        KeyCode::Char(' ') => Command::SetCurrent,
        KeyCode::Char('n') if plain => Command::Create,
        KeyCode::Char('i') if plain => Command::Import,
        // `i` already imports instances from a game directory, so the modpack
        // import takes the shifted key. Both spellings are accepted: terminals
        // differ on whether Shift+i arrives as `I` or as `i` + SHIFT.
        KeyCode::Char('i')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::SHIFT) =>
        {
            Command::ImportModpack
        }
        KeyCode::Char('I') => Command::ImportModpack,
        KeyCode::Char('r') if plain => Command::Rename,
        KeyCode::Char('d') if plain => Command::Delete,
        KeyCode::Char('x') if plain => Command::ExportScript,
        KeyCode::Char('[') | KeyCode::Char('h') | KeyCode::Left => Command::PrevBlock,
        KeyCode::Char(']') | KeyCode::Right => Command::NextBlock,
        _ => Command::Nothing,
    }
}
