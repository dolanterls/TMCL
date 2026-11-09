//! Download center page.

use super::*;

impl App {
    pub(in crate::tui::app) fn downloads_hints(&self) -> Vec<crate::tui::chrome::Hint> {
        let mut hints = vec![
            self.hint("Enter", "action.open_install"),
            self.hint("/", "action.search"),
            self.hint("s", "action.source"),
            self.hint("o", "action.sort"),
        ];
        // A game version or a modpack becomes a new instance, so neither has a
        // target instance to pick.
        if self.download_center.category.targets_instance() {
            hints.push(self.hint("t", "action.target"));
        }
        hints.push(self.hint("f", "action.filter"));
        hints.push(self.hint("[ ]", "action.category"));
        hints
    }

    /// Focuses the inline search bar drawn above the category pills. Only the
    /// `creeper` preset has one; `classic` keeps the prompt dialog.
    pub(in crate::tui::app) fn focus_download_search(&mut self) {
        // A click on an already focused field must not snapshot the half-typed
        // text as the query Esc would restore.
        if self.search.is_some() {
            return;
        }
        if self.download_center.category == DownloadCategory::GameVersions {
            let query = self.download_center.version_filter.search().to_owned();
            self.search_backup = Some(query.clone());
            self.search = Some(SearchState {
                target: SearchTarget::VersionFilter,
                cursor: query.chars().count(),
            });
            return;
        }
        let query = self.download_center.search.clone();
        self.search_backup = Some(query.clone());
        self.search = Some(SearchState {
            target: SearchTarget::Resources,
            cursor: query.chars().count(),
        });
    }

    #[cfg(test)]
    pub(in crate::tui::app) fn download_page_sections(&self, area: Rect) -> Option<[Rect; 5]> {
        self.current_download_page_sections(area)
    }

    pub(in crate::tui::app) fn current_download_page_sections(
        &self,
        area: Rect,
    ) -> Option<[Rect; 5]> {
        let creeper = self.preset() == UiPreset::Creeper;
        let narrow = self.is_narrow();
        // Resource pages keep one row for the info/category line in both
        // presets; the versions page stays exactly as it was.
        let info_rows = u16::from(
            (creeper && !narrow) || self.download_center.category != DownloadCategory::GameVersions,
        );
        Self::download_page_sections_with_filter_rows(
            area,
            if creeper && !narrow {
                3
            } else {
                u16::from(creeper)
            },
            info_rows,
        )
    }

    /// `[search, category tabs, (unused), result info, result list]`.
    ///
    /// The search panel and the result-info line are `creeper` affordances;
    /// `classic` keeps its prompt dialog, so both rows collapse to zero height
    /// and every caller (render, mouse, detail geometry) shares the offsets.
    /// The third slot keeps its place so existing indices stay valid; the
    /// version chips it held are gone from both presets.
    pub(in crate::tui::app) fn download_page_sections_with_filter_rows(
        area: Rect,
        search_rows: u16,
        info_rows: u16,
    ) -> Option<[Rect; 5]> {
        let inner = area;
        (inner.height > 0).then(|| {
            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(search_rows),
                    Constraint::Length(1),
                    Constraint::Length(0),
                    Constraint::Length(info_rows),
                    Constraint::Min(0),
                ])
                .split(inner);
            [
                sections[0],
                sections[1],
                sections[2],
                sections[3],
                sections[4],
            ]
        })
    }

    pub(in crate::tui::app) fn download_tabs(&self) -> TabBar {
        TabBar::new(self.download_pill_labels())
    }

    /// Localized name of a manifest version kind (`release`, `snapshot`,
    /// `old_beta`, `old_alpha`). Unknown kinds fall through unchanged so a new
    /// Mojang type still renders something.
    pub(in crate::tui::app) fn version_kind_label(&self, kind: &str) -> String {
        let key = match kind {
            "release" => "download.version_kind.release",
            "snapshot" => "download.version_kind.snapshot",
            "old_beta" | "old_alpha" => "download.version_kind.legacy",
            _ => return kind.to_owned(),
        };
        self.i18n.text(key)
    }

    /// Localized release channel of a project file. Printing the enum through
    /// `Debug` showed `Stable` and `Beta` in English to a Chinese UI and put the
    /// variant's spelling into the layout.
    pub(in crate::tui::app) fn release_channel_label(
        &self,
        channel: crate::core::resource::model::ReleaseChannel,
    ) -> String {
        use crate::core::resource::model::ReleaseChannel;

        let key = match channel {
            ReleaseChannel::Release => "download.channel.release",
            ReleaseChannel::Beta => "download.channel.beta",
            ReleaseChannel::Alpha => "download.channel.alpha",
            ReleaseChannel::Unknown => "download.channel.unknown",
        };
        self.i18n.text(key)
    }

    /// Name of the instance the current category installs into.
    ///
    /// Only meaningful when [`DownloadCategory::targets_instance`] is true; the
    /// callers that print it check that first.
    pub(in crate::tui::app) fn download_target_name(&self) -> String {
        self.download_target_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
            .map_or_else(|| self.i18n.text("common.none"), |item| item.name.clone())
    }

    /// Caption key for the page/sort/source line.
    ///
    /// A game version or a modpack builds its own instance and a world is
    /// saved wherever the player points the dialog, so those captions must not
    /// name a target: the placeholder would either be missing (a broken
    /// format) or name an instance the download never touches.
    pub(in crate::tui::app) fn download_info_key(&self) -> &'static str {
        match self.download_center.category.destination() {
            DownloadDestination::Instance => "download.info_resources",
            DownloadDestination::NewInstance => "download.info_resources_new_instance",
            DownloadDestination::SavePath => "download.info_resources_save_path",
        }
    }

    /// Fields for [`Self::download_info_key`]: page, sort, source, total, and
    /// the target name when the category has one.
    pub(in crate::tui::app) fn download_info_fields(&self) -> Vec<(&'static str, String)> {
        let pages = resource_page_count(self.download_center.total, RESOURCE_PAGE_SIZE);
        let mut fields = vec![
            ("page", (self.download_center.page + 1).to_string()),
            ("pages", pages.to_string()),
            (
                "sort",
                localize_resource_sort(self.download_center.sort, &self.i18n),
            ),
            (
                "source",
                localize_resource_source(self.download_center.source, &self.i18n),
            ),
            ("total", self.download_center.total.to_string()),
        ];
        if self.download_center.category.targets_instance() {
            fields.push(("target", self.download_target_name()));
        }
        fields
    }

    /// Category chip labels shared by the `creeper` renderer and its hit test.
    pub(in crate::tui::app) fn download_pill_labels(&self) -> Vec<String> {
        DownloadCategory::ALL
            .iter()
            .map(|category| self.i18n.text(category.label_key()))
            .collect()
    }

    /// Terminal width the category tabs need on one row.
    pub(in crate::tui::app) fn download_tabs_width(&self) -> usize {
        let labels = self.download_pill_labels();
        labels
            .iter()
            .map(|label| crate::tui::widgets::kit::pill_width(label))
            .sum::<usize>()
            + labels.len().saturating_sub(1)
    }

    /// Text of the right-hand filter entry on the category row: the resource
    /// category summary, or how many version kinds are shown.
    pub(in crate::tui::app) fn download_filter_entry(&self) -> String {
        if self.download_center.category == DownloadCategory::GameVersions {
            let filters = self.download_center.version_filter.enabled_count();
            self.i18n.format(
                "download.info_versions",
                &[("filters", &filters.to_string())],
            )
        } else {
            self.download_filter_label()
        }
    }

    /// Width of that entry, including the trailing arrow.
    pub(in crate::tui::app) fn download_filter_entry_width(&self) -> u16 {
        u16::try_from(crate::tui::palette::width(&self.download_filter_entry()) + 2)
            .unwrap_or(u16::MAX)
    }

    /// Right-aligned rectangle of the entry on the category row, shared by both
    /// presets so their hit boxes cannot drift from the drawn text.
    pub(in crate::tui::app) fn download_filter_entry_rect(&self, row: Rect) -> Rect {
        let width = self.download_filter_entry_width().min(row.width);
        Rect::new(row.right().saturating_sub(width), row.y, width, 1)
    }

    /// Text after `类别：` for the current selection (`全部` when empty).
    pub(in crate::tui::app) fn download_filter_summary(&self) -> String {
        if self.download_center.categories.is_empty() {
            return self.i18n.text("download.filter_all");
        }
        self.download_center
            .categories
            .iter()
            .map(|id| {
                self.category_options()
                    .iter()
                    .find(|category| &category.id == id)
                    .map(|category| self.category_label(category))
                    .unwrap_or_else(|| id.clone())
            })
            .collect::<Vec<_>>()
            .join("+")
    }

    pub(in crate::tui::app) fn download_filter_label(&self) -> String {
        self.i18n.format(
            "download.filter_label",
            &[("value", &self.download_filter_summary())],
        )
    }

    pub(in crate::tui::app) fn filtered_version_indices(&self) -> Vec<usize> {
        self.download_center
            .version_filter
            .visible_indices(&self.versions)
    }

    pub(in crate::tui::app) fn selected_version(&self) -> Option<&VersionSummary> {
        self.filtered_version_indices()
            .get(self.version_selected)
            .and_then(|index| self.versions.get(*index))
    }

    pub(in crate::tui::app) fn reset_version_selection(&mut self) {
        self.version_selected = 0;
        self.version_scroll = 0;
        self.reset_detail_scroll();
    }

    pub(in crate::tui::app) fn version_visible_rows(&self) -> usize {
        let list_area = self
            .layout_chrome(Rect::new(0, 0, self.terminal_width, self.terminal_height))
            .main;
        let list_area = self.download_list_area(list_area);
        let count = self.filtered_version_indices().len();
        self.list_layout(list_area, count, 0).visible_rows().max(1)
    }

    pub(in crate::tui::app) fn page_versions(&mut self, direction: isize) {
        let version_count = self.filtered_version_indices().len();
        if version_count == 0 {
            return;
        }
        let page_size = self.version_visible_rows();
        self.version_selected = if direction < 0 {
            self.version_scroll.saturating_sub(page_size)
        } else {
            self.version_scroll
                .saturating_add(page_size)
                .min(version_count - 1)
        };
        self.version_scroll = self.version_selected;
        self.keep_version_selected_visible();
    }

    pub(in crate::tui::app) fn page_download_center(&mut self, direction: isize) {
        if self.download_center.category == DownloadCategory::GameVersions {
            self.page_versions(direction);
            return;
        }
        if self.download_center.loading {
            return;
        }
        let next = if direction < 0 {
            self.download_center.page.saturating_sub(1)
        } else {
            if u64::from(self.download_center.page.saturating_add(1))
                * u64::from(RESOURCE_PAGE_SIZE)
                >= self.download_center.total
            {
                return;
            }
            self.download_center.page.saturating_add(1)
        };
        if next == self.download_center.page {
            return;
        }
        self.download_center.page = next;
        self.resource_projects.clear();
        self.resource_files.clear();
        self.resource_detail = None;
        self.resource_detail_project = None;
        self.download_center.selected = 0;
        self.spawn_resource_search();
    }

    pub(in crate::tui::app) fn cycle_resource_sort(&mut self) {
        if self.download_center.category == DownloadCategory::GameVersions {
            self.status = Status::warning(self.i18n.text("download.game_versions_sort_hint"));
            return;
        }
        let sort = match self.download_center.sort {
            ResourceSort::Relevance => ResourceSort::Downloads,
            ResourceSort::Downloads => ResourceSort::Updated,
            ResourceSort::Updated => ResourceSort::Newest,
            ResourceSort::Newest => ResourceSort::Relevance,
            ResourceSort::Follows => ResourceSort::Relevance,
        };
        self.download_center.sort = sort;
        self.resource_projects.clear();
        self.resource_files.clear();
        self.resource_detail = None;
        self.resource_detail_project = None;
        self.spawn_resource_search();
    }

    pub(in crate::tui::app) fn keep_version_selected_visible(&mut self) {
        let version_count = self.filtered_version_indices().len();
        if version_count == 0 {
            self.version_selected = 0;
            self.version_scroll = 0;
            return;
        }

        self.version_selected = self.version_selected.min(version_count - 1);
        let visible_rows = self.version_visible_rows();
        if self.version_selected < self.version_scroll {
            self.version_scroll = self.version_selected;
        } else if self.version_selected >= self.version_scroll + visible_rows {
            self.version_scroll = self.version_selected + 1 - visible_rows;
        }
        self.version_scroll = self
            .version_scroll
            .min(version_count.saturating_sub(visible_rows));
    }

    pub(in crate::tui::app) fn toggle_version_category(&mut self, index: usize) {
        if self.download_center.category != DownloadCategory::GameVersions {
            return;
        }
        if let Some(category) = VersionCategory::ALL.get(index).copied() {
            self.download_center.version_filter.toggle(category);
            self.reset_version_selection();
        }
    }

    pub(in crate::tui::app) fn refresh_versions(&mut self) {
        self.refresh_versions_with_context(false);
    }

    pub(in crate::tui::app) fn refresh_versions_for_picker(&mut self) {
        self.refresh_versions_with_context(true);
    }

    pub(in crate::tui::app) fn refresh_versions_with_context(&mut self, for_picker: bool) {
        let cache_path = self.paths.cache_dir.join("version_manifest_v2.json");
        let sender = self.sender.clone();
        self.status = Status::info(self.i18n.text("status.refreshing_versions"));
        tokio::spawn(async move {
            let result = async {
                let catalog = list_versions(
                    &HttpClient::new()?,
                    &[DownloadSource::Official, DownloadSource::Bmclapi],
                    &VersionCache::new(cache_path),
                )
                .await?;
                Ok::<_, Error>((catalog.manifest.versions, catalog.from_cache))
            }
            .await;
            match result {
                Ok((versions, from_cache)) => {
                    let _ = sender.send(AppEvent::VersionsLoaded {
                        versions,
                        from_cache,
                    });
                }
                Err(error) => {
                    let _ = sender.send(AppEvent::VersionsLoadFailed {
                        for_picker,
                        error: error.to_string(),
                    });
                }
            }
        });
    }

    pub(in crate::tui::app) fn refresh_download_center(&mut self) {
        if self.download_center.category == DownloadCategory::GameVersions {
            self.refresh_versions();
        } else {
            self.spawn_resource_search();
        }
    }

    /// Drop everything that belonged to the previous query.
    ///
    /// The page renders `resource_projects` and `resource_detail` directly, so a
    /// stale list is painted under the new search text, and an open detail hides
    /// the list entirely. Every path that changes the category, the source or the
    /// query has to come through here; writing the four fields by hand is how the
    /// diagnosis remedy ended up showing yesterday's results.
    pub(in crate::tui::app) fn reset_resource_view(&mut self) {
        self.search = None;
        self.search_backup = None;
        self.resource_projects.clear();
        self.resource_files.clear();
        self.resource_detail = None;
        self.resource_detail_project = None;
        self.download_center.selected = 0;
        self.download_center.page = 0;
        self.download_center.total = 0;
        self.download_center.loading = false;
        self.download_center.error = None;
        // Bump, do not zero: a response already in flight belongs to the old
        // query and must fail the `request_id ==` check in `dispatch`.
        self.download_center.request_id = self.download_center.request_id.saturating_add(1);
        self.reset_detail_scroll();
    }

    pub(in crate::tui::app) fn change_download_category(&mut self, delta: isize) {
        let current = DownloadCategory::ALL
            .iter()
            .position(|category| *category == self.download_center.category)
            .unwrap_or(0);
        let next = move_bounded(current, DownloadCategory::ALL.len(), delta);
        self.download_center
            .set_category(DownloadCategory::ALL[next]);
        self.reset_resource_view();
        self.version_selected = 0;
        self.version_scroll = 0;
        self.refresh_download_center();
    }

    pub(in crate::tui::app) fn switch_download_source(&mut self) {
        let Some(kind) = self.download_center.category.resource_kind() else {
            self.status = Status::warning(self.i18n.text("download.game_versions_sort_hint"));
            return;
        };
        let source = match self.download_center.source {
            ResourceSource::Modrinth if kind != ResourceKind::World => ResourceSource::CurseForge,
            ResourceSource::CurseForge if kind != ResourceKind::World => ResourceSource::Modrinth,
            ResourceSource::Modrinth => ResourceSource::CurseForge,
            ResourceSource::CurseForge => ResourceSource::CurseForge,
        };
        if source == ResourceSource::CurseForge && option_env!("TMCL_CURSEFORGE_API_KEY").is_none()
        {
            self.status = Status::warning(self.i18n.text("status.curseforge_unavailable"));
            return;
        }
        self.download_center.set_source(source);
        self.reset_resource_view();
        self.spawn_resource_search();
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
        KeyCode::Char('/') if plain => Command::Search,
        KeyCode::Char('s') if plain => Command::Source,
        KeyCode::Char('o') if plain => Command::Sort,
        KeyCode::Char('t') if plain => Command::Target,
        KeyCode::Char('f') if plain => Command::FilterCategories,
        KeyCode::Char('r') if plain => Command::Refresh,
        KeyCode::Char('[') | KeyCode::Char('h') => Command::PrevBlock,
        KeyCode::Char(']') | KeyCode::Char('l') => Command::NextBlock,
        KeyCode::Char(character @ '1'..='4') if plain => {
            Command::ToggleVersionCategory(usize::from(character as u8 - b'1'))
        }
        _ => Command::Nothing,
    }
}
