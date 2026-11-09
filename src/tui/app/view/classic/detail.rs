//! Detail panel content and scrolling.

use super::*;

/// Narrowest download list area that still leaves the resource detail card a
/// usable width. Title-only rows make this much smaller than the old
/// three-column table needed: a 100-column terminal can show both.
const DOWNLOAD_DETAIL_MIN_WIDTH: u16 = 76;

/// Marks a detail row that is free-form prose. Resource descriptions may
/// contain `": "` themselves, so they must never be parsed as `label: value`.
impl App {
    pub(in crate::tui::app) fn open_details(&mut self) {
        self.detail_scroll = 0;
        self.detail_open = true;
    }

    pub(super) fn draw_detail_popup(&self, frame: &mut Frame<'_>) {
        self.draw_detail_overlay(frame, self.theme);
    }

    pub(in crate::tui::app) fn instance_detail_areas(&self, content: Rect) -> Option<(Rect, Rect)> {
        if self.is_narrow() || content.width < 110 {
            return None;
        }
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(58),
                Constraint::Length(1),
                Constraint::Min(30),
            ])
            .split(content);
        Some((columns[0], columns[2]))
    }

    /// Result table plus the resource detail card, on the project index only.
    ///
    /// The second level is the file table, which already carries what the card
    /// would say: compatibility, channel and dependency count, one row per file.
    /// A card beside it repeated the selected row and took a third of the width,
    /// so the file list keeps its full width. Project rows are title-only
    /// (author and id live in the card), which lets 120 columns show both.
    pub(in crate::tui::app) fn download_detail_areas(
        &self,
        list_area: Rect,
    ) -> Option<(Rect, Rect)> {
        if self.is_narrow()
            || self.resource_detail.is_some()
            || list_area.width < DOWNLOAD_DETAIL_MIN_WIDTH
            || self.download_center.category == DownloadCategory::GameVersions
            || (self.resource_projects.is_empty() && self.resource_detail_project.is_none())
        {
            return None;
        }
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(58),
                Constraint::Length(1),
                Constraint::Min(30),
            ])
            .split(list_area);
        Some((columns[0], columns[2]))
    }

    pub(in crate::tui::app) fn scroll_detail_pane(&mut self, delta: isize) {
        let next = if delta < 0 {
            self.pane_scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.pane_scroll.saturating_add(delta as usize)
        };
        // Without the clamp an overscroll keeps accumulating while the content
        // stays at the end, so the next scroll back appears to do nothing.
        self.pane_scroll = next.min(self.pane_max_scroll.get());
    }

    pub(super) fn draw_detail_pane(&self, frame: &mut Frame<'_>, area: Rect) {
        let block = crate::tui::chrome::panel(&self.i18n.text("common.details"), false, self.theme);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        let paragraph = Paragraph::new(self.styled_detail_lines(self.theme, self.theme.fg))
            .wrap(Wrap { trim: false });
        let total = paragraph.line_count(inner.width);
        let max_scroll = total.saturating_sub(usize::from(inner.height));
        self.pane_max_scroll.set(max_scroll);
        let scroll = self.pane_scroll.min(max_scroll);
        frame.render_widget(
            paragraph.scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
            inner,
        );
        if total > usize::from(inner.height) {
            let mut scrollbar = pane_scrollbar_state(total, usize::from(inner.height), scroll);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("↑"))
                    .end_symbol(Some("↓")),
                inner,
                &mut scrollbar,
            );
        }
    }

    fn detail_popup_height(&self) -> u16 {
        self.terminal_height.saturating_sub(2).clamp(6, 16)
    }

    /// Width of the detail body, per preset.
    ///
    /// The shared popup reserves one cell of border on each side; the
    /// nightlight card draws no border and reserves two columns on the left and
    /// one on the right. Using one number for both wrapped the text one column
    /// wider than it was painted, so the last row never appeared.
    fn detail_inner_width(&self) -> u16 {
        let popup = self.terminal_width.saturating_sub(4).clamp(20, 76);
        let pad = if self.preset() == UiPreset::Nightlight {
            3
        } else {
            2
        };
        popup.saturating_sub(pad)
    }

    /// Rows the detail body needs at `width`, including wrapped continuations.
    fn detail_line_count(&self, width: u16) -> usize {
        let lines = if self.preset() == UiPreset::Nightlight {
            // `modals::detail` paints the unstyled rows in a card.
            self.detail_lines()
        } else {
            self.styled_detail_lines(self.theme, self.theme.fg)
        };
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .line_count(width)
    }

    pub(in crate::tui::app) fn detail_max_scroll(&self) -> usize {
        // `paper` draws a right-hand column (or the whole page), not the
        // centred popup: its width and height come from the same two functions
        // the painter uses. Deriving them here separately would strand the last
        // rows exactly like the log page did.
        if self.preset() == UiPreset::Paper {
            let area = Rect::new(0, 0, self.terminal_width, self.terminal_height);
            let body = Self::paper_detail_body(self.paper_detail_rect(area));
            let lines = self.paper_detail_lines(&self.paper_palette());
            return Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .line_count(body.width.max(1))
                .saturating_sub(usize::from(body.height));
        }
        let width = self.detail_inner_width().max(1);
        let height = self.detail_popup_height().saturating_sub(2);
        self.detail_line_count(width)
            .saturating_sub(usize::from(height))
    }

    pub(in crate::tui::app) fn move_detail_scroll(&mut self, delta: isize) {
        let max_scroll = self.detail_max_scroll();
        self.detail_scroll = if delta < 0 {
            self.detail_scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.detail_scroll
                .saturating_add(delta as usize)
                .min(max_scroll)
        };
        self.detail_scroll = self.detail_scroll.min(max_scroll);
    }

    pub(in crate::tui::app) fn reset_detail_scroll(&mut self) {
        self.detail_scroll = 0;
    }

    pub(in crate::tui::app) fn detail_lines(&self) -> Vec<Line<'static>> {
        match self.page {
            Page::Overview => vec![
                Line::from(self.i18n.text("detail.launch_workspace")),
                Line::from(self.i18n.text("detail.enter_launch")),
                Line::from(self.i18n.text("detail.sidebar_context")),
            ],
            Page::Instances
                if self
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.detail.is_some()) =>
            {
                let entry = self
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.detail.clone())
                    .expect("checked above");
                self.content_detail_lines(&entry)
            }
            Page::Instances => self
                .instances
                .focused_item()
                .map(|item| {
                    vec![
                        Line::from(item.name.clone()),
                        Line::from(
                            self.i18n
                                .format("detail.version", &[("value", &item.version)]),
                        ),
                        Line::from(self.i18n.format(
                            "detail.loader",
                            &[(
                                "value",
                                &format!(
                                    "{}{}",
                                    self.loader_label(&item.loader),
                                    item.loader_version
                                        .as_ref()
                                        .map_or(String::new(), |version| format!(" {version}"))
                                ),
                            )],
                        )),
                        Line::from(self.i18n.format(
                            "detail.status",
                            &[("value", &localize_instance_status(&item.status, &self.i18n))],
                        )),
                        Line::from(self.i18n.format(
                            "detail.java",
                            &[(
                                "value",
                                &self.config.java.selected_path.as_ref().map_or_else(
                                    || self.i18n.text("settings.java.auto"),
                                    |path| display_path(path),
                                ),
                            )],
                        )),
                        Line::from(self.i18n.format(
                            "detail.account",
                            &[(
                                "value",
                                &self.current_account().map_or_else(
                                    || self.i18n.text("common.none"),
                                    |account| account.username,
                                ),
                            )],
                        )),
                        Line::from(self.i18n.format(
                            "detail.launches",
                            &[("value", &item.launch_count.to_string())],
                        )),
                        Line::from(self.i18n.format(
                            "detail.play_time",
                            &[("value", &format_play_time(item.play_time_seconds))],
                        )),
                        Line::from(
                            self.i18n
                                .format("detail.path", &[("value", &display_path(&item.path))]),
                        ),
                    ]
                })
                .unwrap_or_else(|| vec![Line::from(self.i18n.text("detail.no_instance"))]),
            Page::Accounts => {
                let mut lines = self
                    .accounts
                    .accounts
                    .get(self.accounts.selected)
                    .map(|account| {
                        vec![
                            Line::from(account.username.clone()),
                            Line::from(self.i18n.format(
                                "detail.type",
                                &[("value", &format!("{:?}", account.kind))],
                            )),
                            Line::from(
                                self.i18n
                                    .format("detail.uuid", &[("value", &account.uuid.to_string())]),
                            ),
                        ]
                    })
                    .unwrap_or_else(|| vec![Line::from(self.i18n.text("detail.no_account"))]);
                if let Some(device_code) = &self.accounts.device_code {
                    lines.extend([
                        Line::from(
                            self.i18n
                                .format("detail.code", &[("value", &device_code.user_code)]),
                        ),
                        Line::from(
                            self.i18n
                                .format("detail.open", &[("value", &device_code.verification_uri)]),
                        ),
                        Line::from(self.i18n.format(
                            "detail.remaining",
                            &[
                                ("status", &device_code.status),
                                ("seconds", &device_code.remaining_seconds.to_string()),
                            ],
                        )),
                    ]);
                }
                lines
            }
            Page::Tasks => self
                .selected_task()
                .map(|task| {
                    vec![
                        Line::from(task.label.clone()),
                        Line::from(
                            self.i18n
                                .format("detail.task_id", &[("value", &task.id.to_string())]),
                        ),
                        Line::from(self.i18n.format(
                            "detail.state",
                            &[("value", &localize_task_state(task.state, &self.i18n))],
                        )),
                        Line::from(
                            self.i18n
                                .format("detail.attempt", &[("value", &task.attempt.to_string())]),
                        ),
                        Line::from(self.i18n.format(
                            "detail.task_progress",
                            &[
                                ("completed", &task.completed.to_string()),
                                (
                                    "total",
                                    &task.total.map_or("?".into(), |total| total.to_string()),
                                ),
                            ],
                        )),
                        Line::from(
                            self.speed_tracker
                                .speed(task.id, std::time::Instant::now())
                                .and_then(|speed| {
                                    crate::tui::app::state::data_format::format_transfer(
                                        task.completed,
                                        task.total,
                                        speed,
                                    )
                                })
                                .unwrap_or_default(),
                        ),
                        Line::from(if task.retryable {
                            self.i18n.text("detail.actions_retry")
                        } else {
                            self.i18n.text("detail.actions_pause")
                        }),
                        task.failure
                            .as_ref()
                            .map(|failure| {
                                Line::from(self.i18n.format("detail.error", &[("value", failure)]))
                            })
                            .unwrap_or_else(|| Line::from(self.i18n.text("detail.no_error"))),
                    ]
                })
                .unwrap_or_else(|| vec![Line::from(self.i18n.text("detail.no_task"))]),
            Page::DownloadCenter => {
                // File level answers a different question than project level, so
                // it does not depend on the project being loaded: the file is
                // what is about to be installed.
                if self.resource_detail.is_some() {
                    if let Some(file) = self.resource_files.get(self.download_center.selected) {
                        // A modpack version builds its own instance; comparing
                        // it with the selected one and listing its "deps" says
                        // nothing about what Enter will do.
                        let targeted = self.download_center.category.targets_instance();
                        let mut lines = vec![
                            Line::from(file.name.clone()),
                            Line::from(format!(
                                "{}  {}",
                                self.i18n.text("download.channel"),
                                self.release_channel_label(file.channel)
                            )),
                        ];
                        if targeted {
                            let compatibility =
                                crate::core::resource::compatibility::file_compatibility(
                                    file,
                                    self.resource_target_context().as_ref(),
                                );
                            lines.push(Line::from(format!(
                                "{}  {}",
                                self.i18n.text("download.compatibility"),
                                crate::tui::app::state::data::localize_compatibility(
                                    compatibility,
                                    &self.i18n,
                                )
                            )));
                            lines.push(Line::from(""));
                            lines.push(Line::from(self.i18n.text("download.dependencies")));
                            if file.dependencies.is_empty() {
                                lines.push(Line::from(format!(
                                    "  \u{00b7} {}",
                                    self.i18n.text("download.no_dependencies")
                                )));
                            } else {
                                for dependency in &file.dependencies {
                                    let key = match dependency.kind {
                                        crate::core::resource::model::DependencyKind::Required => {
                                            "download.dep.required"
                                        }
                                        crate::core::resource::model::DependencyKind::Optional => {
                                            "download.dep.optional"
                                        }
                                        _ => "download.dep.other",
                                    };
                                    lines.push(Line::from(format!(
                                        "  \u{00b7} {} ({})",
                                        dependency.name,
                                        self.i18n.text(key)
                                    )));
                                }
                            }
                        }
                        lines.push(Line::from(""));
                        lines.push(Line::from(format!(
                            "\u{25b8} {}",
                            self.i18n.text("download.install_file")
                        )));
                        return lines;
                    }
                }
                if let Some(project) = self
                    .resource_detail_project
                    .as_ref()
                    .or_else(|| self.resource_projects.get(self.download_center.selected))
                {
                    // Everything the result list also shows, printed once here:
                    // downloads, merged loaders, tags without loaders, and the
                    // full description.
                    let loaders = project_loaders(project);
                    let tags = project_tags(project, &loaders);
                    let unknown = self.i18n.text("common.unknown");
                    let downloads = project
                        .downloads
                        .map_or_else(|| unknown.clone(), format_count);
                    let versions = if project.game_versions.is_empty() {
                        unknown.clone()
                    } else {
                        project.game_versions.join(", ")
                    };
                    let loader_text = if loaders.is_empty() {
                        unknown.clone()
                    } else {
                        loaders.join(", ")
                    };
                    let tag_text = if tags.is_empty() {
                        unknown.clone()
                    } else {
                        tags.iter()
                            .map(|tag| self.category_name_label(tag))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    let lines = vec![
                        Line::from(project.title.clone()),
                        Line::from(
                            self.i18n
                                .format("download.author", &[("value", &project.author)]),
                        ),
                        Line::from(
                            self.i18n
                                .format("download.project_id", &[("value", &project.project_id)]),
                        ),
                        Line::from(
                            self.i18n
                                .format("download.downloads", &[("value", &downloads)]),
                        ),
                        Line::from(self.i18n.format("detail.versions", &[("value", &versions)])),
                        Line::from(
                            self.i18n
                                .format("detail.loaders", &[("value", &loader_text)]),
                        ),
                        Line::from(self.i18n.format("download.tags", &[("value", &tag_text)])),
                        Line::from(""),
                        Line::from(format!("{PARAGRAPH_MARK}{}", project.description)),
                        Line::from(format!("▸ {}", self.i18n.text("download.install_file"))),
                    ];
                    lines
                } else if let Some(version) = self.selected_version() {
                    vec![
                        Line::from(
                            self.i18n
                                .format("download.minecraft", &[("id", &version.id)]),
                        ),
                        Line::from(self.i18n.format(
                            "download.type",
                            &[("value", &self.version_kind_label(&version.kind))],
                        )),
                        Line::from(
                            self.i18n
                                .format("download.published", &[("value", &version.release_time)]),
                        ),
                        Line::from(self.i18n.text("download.open_version")),
                    ]
                } else {
                    vec![
                        Line::from(self.i18n.text("download.choose_category")),
                        Line::from(self.i18n.text("download.background_tasks")),
                    ]
                }
            }
            Page::Logs => vec![
                Line::from(self.i18n.text("detail.live_log")),
                Line::from(self.i18n.format(
                    "detail.buffered_lines",
                    &[("count", &self.logs.len().to_string())],
                )),
                Line::from(self.i18n.format(
                    "detail.follow",
                    &[(
                        "value",
                        &self.i18n.text(if self.logs.follow() {
                            "common.on"
                        } else {
                            "common.off"
                        }),
                    )],
                )),
            ],
            Page::Settings => vec![
                Line::from(self.i18n.text("detail.settings_forms")),
                Line::from(self.i18n.text("detail.config_saved")),
                Line::from(self.i18n.format(
                    "detail.directories",
                    &[("count", &self.config.game_directories.len().to_string())],
                )),
                Line::from(self.i18n.format(
                    "detail.launch",
                    &[
                        ("min", &self.config.launch.min_memory_mb.to_string()),
                        ("max", &self.config.launch.max_memory_mb.to_string()),
                        ("width", &self.config.launch.width.to_string()),
                        ("height", &self.config.launch.height.to_string()),
                    ],
                )),
                Line::from(self.i18n.format(
                    "detail.downloads",
                    &[
                        (
                            "version",
                            source_mode_label(self.config.downloads.version_list_source),
                        ),
                        (
                            "file",
                            source_mode_label(self.config.downloads.file_download_source),
                        ),
                    ],
                )),
                Line::from(self.i18n.format(
                    "detail.retention",
                    &[("days", &self.config.logs.retention_days.to_string())],
                )),
            ],
        }
    }
}

/// Loader names that providers sometimes report only as category tags.
const LOADER_TAGS: [&str; 8] = [
    "fabric",
    "forge",
    "neoforge",
    "quilt",
    "liteloader",
    "rift",
    "iris",
    "optifine",
];

/// Merges declared loaders with loader-looking tags so the card never says
/// "unknown" while the tags already name a loader.
fn project_loaders(project: &ResourceProject) -> Vec<String> {
    let mut loaders: Vec<String> = Vec::new();
    for loader in &project.loaders {
        let value = loader.trim().to_ascii_lowercase();
        if !value.is_empty() && !loaders.contains(&value) {
            loaders.push(value);
        }
    }
    for tag in &project.categories {
        let value = tag.trim().to_ascii_lowercase();
        if LOADER_TAGS.contains(&value.as_str()) && !loaders.contains(&value) {
            loaders.push(value);
        }
    }
    loaders
}

/// Categories minus the ones already shown as loaders, so no value repeats
/// between the two fields.
fn project_tags(project: &ResourceProject, loaders: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in &project.categories {
        let trimmed = tag.trim();
        if trimmed.is_empty()
            || loaders
                .iter()
                .any(|loader| loader.eq_ignore_ascii_case(trimmed))
        {
            continue;
        }
        if tags
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(trimmed))
        {
            continue;
        }
        tags.push(trimmed.to_owned());
    }
    tags
}

impl App {
    /// Detail lines for a workspace content entry.
    fn content_detail_lines(
        &self,
        entry: &crate::core::content::ContentEntry,
    ) -> Vec<Line<'static>> {
        use crate::core::content::ContentMeta;
        let text = |key: &str| self.i18n.text(key);
        let mut lines = vec![Line::from(entry.name.clone())];
        if !entry.version.is_empty() {
            // `detail.version` is a template ("版本：{value}"), not a label: a
            // bare `format!` printed the placeholder into the record.
            lines.push(Line::from(
                self.i18n
                    .format("detail.version", &[("value", &entry.version)]),
            ));
        }
        lines.push(Line::from(format!(
            "{}: {}",
            text("workspace.detail.file"),
            entry.display_file_name()
        )));
        lines.push(Line::from(format!(
            "{}: {}",
            text("workspace.detail.size"),
            format_size(entry.size)
        )));
        if !entry.enabled {
            lines.push(Line::from(text("workspace.detail.disabled")));
        }
        if let Some(warning) = entry.warning.as_deref() {
            lines.push(Line::from(format!(
                "{}: {warning}",
                text("workspace.detail.warning")
            )));
        }
        match &entry.meta {
            ContentMeta::Mod {
                id,
                authors,
                description,
                depends,
                ..
            } => {
                lines.push(Line::from(format!("ID: {id}")));
                if !authors.is_empty() {
                    lines.push(Line::from(format!(
                        "{}: {}",
                        text("workspace.detail.authors"),
                        authors.join(", ")
                    )));
                }
                if !depends.is_empty() {
                    lines.push(Line::from(format!(
                        "{}: {}",
                        text("workspace.detail.depends"),
                        depends.join(", ")
                    )));
                }
                if !description.is_empty() {
                    lines.push(Line::from(String::new()));
                    lines.push(Line::from(description.clone()));
                }
            }
            ContentMeta::ResourcePack {
                description,
                pack_format,
            } => {
                if let Some(format) = pack_format {
                    lines.push(Line::from(format!(
                        "{}: {format}",
                        text("workspace.detail.pack_format")
                    )));
                }
                if !description.is_empty() {
                    lines.push(Line::from(description.clone()));
                }
            }
            ContentMeta::Shader { description } => {
                if !description.is_empty() {
                    lines.push(Line::from(description.clone()));
                }
            }
            ContentMeta::World { last_played, .. } => {
                if let Some(millis) = last_played {
                    lines.push(Line::from(format!(
                        "{}: {millis}",
                        text("workspace.detail.last_played")
                    )));
                }
            }
            ContentMeta::Unknown => {}
        }
        lines.push(Line::from(String::new()));
        lines.push(Line::from(crate::core::instance::path::display_path(
            &entry.path,
        )));
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::{project_loaders, project_tags};
    use crate::core::resource::model::{ResourceProject, ResourceSource};

    fn project(loaders: &[&str], categories: &[&str]) -> ResourceProject {
        ResourceProject {
            source: ResourceSource::Modrinth,
            project_id: "id".into(),
            title: "title".into(),
            author: "author".into(),
            description: String::new(),
            icon_url: None,
            categories: categories.iter().map(|tag| (*tag).to_owned()).collect(),
            game_versions: Vec::new(),
            loaders: loaders.iter().map(|loader| (*loader).to_owned()).collect(),
            downloads: None,
            updated_at: None,
        }
    }

    #[test]
    fn loader_tags_are_merged_into_the_loader_field() {
        let project = project(&[], &["forge", "combat", "Forge"]);
        let loaders = project_loaders(&project);
        assert_eq!(loaders, vec!["forge".to_string()]);
        // The tag that became a loader is not repeated in the tag list.
        assert_eq!(project_tags(&project, &loaders), vec!["combat".to_string()]);
    }

    #[test]
    fn declared_loaders_win_and_duplicates_are_dropped() {
        let project = project(
            &["fabric", "Fabric", "quilt"],
            &["fabric", "quilt", "optimization"],
        );
        let loaders = project_loaders(&project);
        assert_eq!(loaders, vec!["fabric".to_string(), "quilt".to_string()]);
        assert_eq!(
            project_tags(&project, &loaders),
            vec!["optimization".to_string()]
        );
    }
}
