//! `classic` download centre: the version table and its info card.
//!
//! Page geometry, filtering and the resource flows live in
//! `app::state::downloads` and its siblings.
use super::*;
use crate::tui::hit::RowKind;

impl App {
    /// Classic's info row: page/sort/source/total plus the category chip.
    fn draw_classic_download_info(&self, frame: &mut Frame<'_>, area: Rect) {
        if area.is_empty() || self.download_center.category == DownloadCategory::GameVersions {
            return;
        }
        let fields = self.download_info_fields();
        let params = fields
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect::<Vec<_>>();
        let base = self.i18n.format(self.download_info_key(), &params);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ◆ ", self.theme.accent_style()),
                // " ◆ " already eats three cells: truncating to the full row
                // would hard-clip the text against the panel border and lose
                // the ellipsis that tells the player it was cut.
                Span::styled(
                    crate::tui::palette::truncate(&base, usize::from(area.width.saturating_sub(3))),
                    self.theme.dim(),
                ),
            ]))
            .style(self.theme.base()),
            area,
        );
    }

    pub(super) fn draw_version_page(&self, frame: &mut Frame<'_>, area: Rect) {
        let Some(sections) = self.current_download_page_sections(area) else {
            return;
        };
        let current = self
            .instances
            .current_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
            .map(|item| item.name.clone())
            .unwrap_or_else(|| self.i18n.text("common.none"));
        let target = self
            .download_target_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
            .map(|item| item.name.clone())
            .unwrap_or_else(|| self.i18n.text("common.none"));
        let tabs = self.download_tabs();
        // The category filter entry sits at the right end of this row; the tab
        // bar gives up exactly the space it needs. One column of padding keeps
        // the entry off the panel border.
        let row = Rect {
            width: sections[1].width.saturating_sub(1),
            ..sections[1]
        };
        let entry_width = self.download_filter_entry_width();
        let used = tabs
            .rects(row)
            .last()
            .map_or(0, |rect| usize::from(rect.right().saturating_sub(row.x)));
        let entry_fits = usize::from(row.width) >= used + 2 + usize::from(entry_width);
        let tabs_area = Rect {
            width: if entry_fits {
                row.width.saturating_sub(entry_width + 2)
            } else {
                row.width
            },
            ..row
        };
        frame.render_widget(
            tabs.widget()
                .select(
                    DownloadCategory::ALL
                        .iter()
                        .position(|category| *category == self.download_center.category)
                        .unwrap_or(0),
                )
                .highlight_style(self.theme.accent_style())
                .style(Style::default().fg(self.theme.label)),
            tabs_area,
        );
        for (index, rect) in tabs.rects(tabs_area).iter().enumerate() {
            self.hitmap
                .add(*rect, Action::filter(Page::DownloadCenter, index));
        }
        if entry_fits {
            let entry = self.download_filter_entry_rect(row);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(self.download_filter_entry(), self.theme.dim()),
                    Span::styled(" ›", self.theme.accent_style()),
                ])),
                entry,
            );
            self.hitmap.add(
                entry,
                Action::Page {
                    page: Page::DownloadCenter,
                    act: crate::tui::action::PageAct::FilterCategories,
                },
            );
        }
        if !sections[3].is_empty() {
            self.draw_classic_download_info(frame, sections[3]);
        }

        let full_area = sections[4];
        if full_area.is_empty() {
            return;
        }
        let split = self.download_detail_areas(full_area);
        let list_area = split.map_or(full_area, |(table, _)| table);
        if self.download_center.loading {
            crate::tui::widgets::empty::render_loading_with_title(
                frame,
                list_area,
                &self.i18n.text("common.loading"),
                &self.i18n.text("download.loading_metadata"),
                self.theme,
            );
            return;
        }
        if let Some(error) = &self.download_center.error {
            crate::tui::widgets::empty::render(
                frame,
                list_area,
                &self.i18n.text("download.error"),
                error,
                self.theme,
            );
            return;
        }

        let (title, headers, rows, item_count, selected) = if self.download_center.category
            == DownloadCategory::GameVersions
        {
            let visible_indices = self.filtered_version_indices();
            let versions = visible_indices
                .iter()
                .skip(self.version_scroll)
                .take(
                    list_area
                        .height
                        .saturating_sub(if self.is_narrow() { 2 } else { 3 })
                        as usize,
                )
                .map(|index| {
                    let version = &self.versions[*index];
                    vec![
                        version.id.clone(),
                        self.version_kind_label(&version.kind),
                        version
                            .release_time
                            .get(..10)
                            .unwrap_or(&version.release_time)
                            .to_owned(),
                    ]
                })
                .collect::<Vec<_>>();
            (
                self.i18n.text("download.minecraft_versions"),
                vec![
                    self.i18n.text("java.version"),
                    self.i18n.text("common.type"),
                    self.i18n.text("download.released"),
                ],
                versions,
                visible_indices.len(),
                self.version_selected.saturating_sub(self.version_scroll),
            )
        } else if self.resource_detail.is_some() {
            let targeted = self.download_center.category.targets_instance();
            let target = self.resource_target_context();
            let files = self
                .resource_files
                .iter()
                .map(|file| {
                    let channel = self.release_channel_label(file.channel);
                    if !targeted {
                        // A modpack version only needs its name and channel:
                        // compatibility and dependencies describe an instance
                        // this download creates, not one it edits.
                        return vec![file.name.clone(), channel];
                    }
                    let compatibility = crate::core::resource::compatibility::file_compatibility(
                        file,
                        target.as_ref(),
                    );
                    let compatibility_badge = crate::tui::widgets::status::badge_with_theme(
                        match compatibility {
                            crate::core::resource::model::Compatibility::Compatible => {
                                StatusKind::Ready
                            }
                            crate::core::resource::model::Compatibility::Unknown => {
                                StatusKind::Info
                            }
                            crate::core::resource::model::Compatibility::Incompatible => {
                                StatusKind::Failed
                            }
                        },
                        localize_compatibility(compatibility, &self.i18n),
                        self.theme,
                    );
                    vec![
                        compatibility_badge.to_string(),
                        file.name.clone(),
                        channel,
                        format!("deps:{}", file.dependencies.len()),
                    ]
                })
                .collect::<Vec<_>>();
            let headers = if targeted {
                vec![
                    self.i18n.text("download.compatibility"),
                    self.i18n.text("download.file"),
                    self.i18n.text("download.channel"),
                    self.i18n.text("download.dependencies"),
                ]
            } else {
                vec![
                    self.i18n.text("download.file"),
                    self.i18n.text("download.channel"),
                ]
            };
            (
                self.i18n.text("download.resource_files"),
                headers,
                files,
                self.resource_files.len(),
                self.download_center.selected,
            )
        } else {
            // Author and project id are detail-pane information; the list only
            // needs the title so it stays readable next to that pane.
            let projects = self
                .resource_projects
                .iter()
                .map(|project| vec![project.title.clone()])
                .collect::<Vec<_>>();
            (
                self.i18n.text("download.resources"),
                vec![self.i18n.text("download.title")],
                projects,
                self.resource_projects.len(),
                self.download_center.selected,
            )
        };
        if rows.is_empty() {
            crate::tui::widgets::empty::render(
                frame,
                list_area,
                &title,
                &self.i18n.text("download.no_results"),
                self.theme,
            );
            return;
        }
        if self.is_narrow() {
            let items = rows.iter().map(|row| row.join(" · ")).collect::<Vec<_>>();
            let layout = self.render_compact_list(
                frame,
                list_area,
                &title,
                &items,
                selected.min(item_count.saturating_sub(1)),
                self.download_row_window(),
            );
            self.register_download_rows(layout);
            return;
        }
        let widths = if self.download_center.category == DownloadCategory::GameVersions {
            vec![
                Constraint::Length(16),
                Constraint::Length(10),
                Constraint::Min(1),
            ]
        } else if self.resource_detail.is_some() {
            if self.download_center.category.targets_instance() {
                vec![
                    Constraint::Length(16),
                    Constraint::Percentage(42),
                    Constraint::Length(12),
                    Constraint::Min(1),
                ]
            } else {
                vec![Constraint::Percentage(50), Constraint::Min(1)]
            }
        } else {
            vec![Constraint::Min(1)]
        };
        let version_target = self.download_center.category == DownloadCategory::GameVersions;
        let query = if version_target {
            self.download_center.version_filter.search().to_owned()
        } else {
            self.download_center.search.clone()
        };
        // The query lives in the panel title: the border stays a border, and the
        // active filter is still visible after the search form closes.
        // A game version or a modpack becomes a new instance: there is no
        // target to show, and the current instance is irrelevant here.
        let mut title = if version_target || !self.download_center.category.targets_instance() {
            title
        } else {
            format!("{title} · {current} → {target}")
        };
        if !query.trim().is_empty() {
            title.push_str(&format!(" · {}: {query}", self.i18n.text("common.search")));
        }
        title.push_str(&format!(
            " · {} · {}",
            localize_resource_source(self.download_center.source, &self.i18n),
            localize_resource_sort(self.download_center.sort, &self.i18n)
        ));
        // The game-version list is already a window of its own — `version_scroll`
        // pre-slices it — so it keeps the plain derivation. The resource lists
        // are drawn whole and remember where they sat.
        let selected = selected.min(item_count.saturating_sub(1));
        let mut layout = RowLayout::table(list_area, rows.len(), selected);
        if !version_target {
            layout = self.row_windows.resolve(self.download_row_window(), layout);
        }
        crate::tui::widgets::table::render_layout(
            layout,
            frame,
            list_area,
            title,
            headers,
            rows,
            Some(selected),
            &widths,
            true,
            self.theme,
        );
        self.register_download_rows(layout);

        let item_count = if self.download_center.category == DownloadCategory::GameVersions {
            self.filtered_version_indices().len()
        } else if self.resource_detail.is_some() {
            self.resource_files.len()
        } else {
            self.resource_projects.len()
        };
        if item_count > 0 {
            let position = if self.download_center.category == DownloadCategory::GameVersions {
                self.version_scroll
            } else {
                self.download_center.selected
            };
            let mut scrollbar_state = ScrollbarState::new(item_count)
                .position(position)
                .viewport_content_length(list_area.height as usize);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("↑"))
                    .end_symbol(Some("↓")),
                list_area,
                &mut scrollbar_state,
            );
        }
        if let Some((_, pane)) = split {
            self.hitmap
                .scroll(pane, crate::tui::action::ScrollTarget::Pane);
            self.draw_detail_pane(frame, pane);
        }
    }

    /// Registers the result rows of the classic download surface.
    ///
    /// The version list draws a window that starts at `version_scroll`, so its
    /// hits carry that offset while project and file lists start at zero.
    fn register_download_rows(&self, layout: RowLayout) {
        let versions = self.download_center.category == DownloadCategory::GameVersions;
        let kind = if versions {
            RowKind::Versions
        } else if self.resource_detail.is_some() {
            RowKind::ResourceFiles
        } else {
            RowKind::ResourceProjects
        };
        let base = if versions { self.version_scroll } else { 0 };
        self.hitmap.rows_at(layout, kind, base);
    }
}
