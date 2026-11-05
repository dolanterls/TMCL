//! Download centre: category pills, filter chips and the result rows.

use crate::tui::app::view::sheet::Sheet;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::*;
use crate::tui::hit::RowKind;
use crate::tui::palette;
use crate::tui::widgets::kit::{self as widgets, RowData};

impl App {
    pub(in crate::tui::app) fn draw_creeper_downloads(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        t: f64,
    ) {
        let pal = self.creeper_palette();
        let Some(sections) = self.current_download_page_sections(area) else {
            return;
        };
        self.download_search(frame, sections[0], t);
        self.download_category_row(frame, sections[1]);
        self.download_info_line(frame, sections[3]);

        let list_area = sections[4];
        if list_area.is_empty() {
            return;
        }
        if self.download_center.loading {
            widgets::empty_card(
                frame,
                list_area,
                &self.i18n.text("common.loading"),
                &self.i18n.text("download.loading_metadata"),
                pal.cyan,
                t,
            );
            return;
        }
        if let Some(error) = self.download_center.error.as_deref() {
            widgets::empty_card(
                frame,
                list_area,
                &self.i18n.text("download.error"),
                error,
                pal.red,
                t,
            );
            return;
        }

        let split = self.download_detail_areas(list_area);
        let rows_area = split.map_or(list_area, |(table, _)| table);
        // Projects are three-line cards like the reference browse screen;
        // game versions and a project's files keep the compact table.
        let projects = self.download_center.category != DownloadCategory::GameVersions
            && self.resource_detail.is_none();
        let rendered = if projects {
            self.download_projects(frame, rows_area, t)
        } else {
            self.download_rows(frame, rows_area, t)
        };
        if !rendered {
            return;
        }
        if let Some((_, pane)) = split {
            self.hitmap
                .scroll(pane, crate::tui::action::ScrollTarget::Pane);
            self.creeper_detail_pane(frame, pane, t);
        }
    }

    /// Search surface: a titled panel with the input row inside, matching the
    /// browse screen of the reference launcher. A narrow terminal drops the
    /// frame to one compact line so the result list keeps its rows.
    fn download_search(&self, frame: &mut Frame<'_>, area: Rect, t: f64) {
        let pal = self.creeper_palette();
        if area.is_empty() {
            return;
        }
        let target = if self.download_center.category == DownloadCategory::GameVersions {
            SearchTarget::VersionFilter
        } else {
            SearchTarget::Resources
        };
        self.hitmap.add(area, Action::search(target));
        let focused = self.download_search_focused();
        if area.height >= 3 {
            let block = widgets::card(
                &self.download_search_title(),
                pal.cyan,
                focused,
                t,
                pal.surface,
            );
            let inner = block.inner(area);
            frame.render_widget(block, area);
            if inner.is_empty() {
                return;
            }
            self.download_search_input(frame, inner, focused, t);
        } else {
            frame.render_widget(
                Block::default().style(Style::default().bg(pal.surface)),
                area,
            );
            self.download_search_input(frame, area, focused, t);
        }
    }

    /// `⌕  <query or hint>  N 个结果` — the single content row of the panel.
    fn download_search_input(&self, frame: &mut Frame<'_>, area: Rect, focused: bool, t: f64) {
        let pal = self.creeper_palette();
        let bg = pal.surface;
        let count = self.download_search_count();
        let count_width = u16::try_from(palette::width(&count)).unwrap_or(u16::MAX);
        let input_width = area
            .width
            .saturating_sub(4)
            .saturating_sub(count_width.saturating_add(2));
        let query = self.download_search_query();
        let placeholder = self.i18n.text("download.search_placeholder");

        let mut spans = vec![Span::styled(
            " ⌕  ",
            Style::default()
                .fg(if focused { pal.cyan } else { pal.text_dim })
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )];
        // One renderer for every field: the block follows the real insertion
        // point instead of always sitting on the last character.
        let cursor = self.download_search_cursor().min(query.chars().count());
        let style = crate::tui::text::FieldStyle {
            text: palette::mix(pal.cyan, pal.text, 0.3),
            dim: pal.text_faint,
            bg,
            cursor_fg: pal.bg,
            cursor_bg: pal.cyan,
        };
        spans.extend(
            crate::tui::text::render(
                query,
                cursor,
                input_width,
                focused,
                crate::tui::text::blink_on(t),
                style,
                &placeholder,
            )
            .spans,
        );
        if !count.is_empty() {
            spans.push(Span::styled("  ", Style::default().bg(bg)));
            spans.push(Span::styled(
                count,
                Style::default()
                    .fg(if self.download_center.loading {
                        pal.gold
                    } else {
                        pal.text_faint
                    })
                    .bg(bg),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }

    fn download_search_title(&self) -> String {
        let kind = self.i18n.text(self.download_center.category.label_key());
        let key = if self.download_search_query().trim().is_empty() {
            "download.panel_browse"
        } else {
            "download.panel_search"
        };
        self.i18n.format(key, &[("kind", &kind)])
    }

    fn download_search_query(&self) -> &str {
        if self.download_center.category == DownloadCategory::GameVersions {
            self.download_center.version_filter.search()
        } else {
            self.download_center.search.as_str()
        }
    }

    /// Insertion point of the inline search, when it is the focused field.
    fn download_search_cursor(&self) -> usize {
        let target = if self.download_center.category == DownloadCategory::GameVersions {
            SearchTarget::VersionFilter
        } else {
            SearchTarget::Resources
        };
        self.search
            .as_ref()
            .filter(|state| state.target == target)
            .map_or_else(
                || self.download_search_query().chars().count(),
                |state| state.cursor,
            )
    }

    fn download_search_focused(&self) -> bool {
        let target = if self.download_center.category == DownloadCategory::GameVersions {
            SearchTarget::VersionFilter
        } else {
            SearchTarget::Resources
        };
        self.search.is_some_and(|state| state.target == target)
    }

    /// Result count shown at the end of the search row.
    fn download_search_count(&self) -> String {
        if self.download_center.loading {
            return self.i18n.text("download.searching");
        }
        if self.download_center.category == DownloadCategory::GameVersions {
            return self.i18n.format(
                "download.version_count",
                &[("count", &self.filtered_version_indices().len().to_string())],
            );
        }
        if self.resource_detail.is_some() {
            return self.i18n.format(
                "download.file_count",
                &[("count", &self.resource_files.len().to_string())],
            );
        }
        if self.resource_projects.is_empty() {
            return String::new();
        }
        self.i18n.format(
            "download.result_count",
            &[("count", &self.resource_projects.len().to_string())],
        )
    }

    /// Caption between the category row and the result card:
    /// `◆ 第 1/3 页 · 排序 相关性 · 来源 MODRINTH · 共 120 个 · 目标 xxx`.
    /// The filter entry lives on the category row, not here.
    fn download_info_line(&self, frame: &mut Frame<'_>, area: Rect) {
        let pal = self.creeper_palette();
        if area.is_empty() {
            return;
        }
        let text = if self.download_center.category == DownloadCategory::GameVersions {
            // A version installs into a brand-new instance, so this page has
            // no target and the caption does not pretend otherwise.
            self.i18n.format(
                "download.version_total",
                &[("count", &self.filtered_version_indices().len().to_string())],
            )
        } else {
            let fields = self.download_info_fields();
            let params = fields
                .iter()
                .map(|(key, value)| (*key, value.as_str()))
                .collect::<Vec<_>>();
            self.i18n.format(self.download_info_key(), &params)
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " ◆ ",
                    Style::default()
                        .fg(pal.cyan)
                        .bg(pal.surface)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    palette::truncate(&text, usize::from(area.width.saturating_sub(3))),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                ),
            ]))
            .style(Style::default().bg(pal.surface)),
            area,
        );
    }

    /// Category row: bare tabs on the left, the filter entry pinned to the
    /// right, both on the same surface as the cards above and below.
    ///
    /// When the tabs need the whole row (a short terminal), the entry steps
    /// aside: `f` still opens the same dialog and the footer says so.
    fn download_category_row(&self, frame: &mut Frame<'_>, area: Rect) {
        let pal = self.creeper_palette();
        if area.is_empty() {
            return;
        }
        frame.render_widget(
            Block::default().style(Style::default().bg(pal.surface)),
            area,
        );

        let entry_width = self.download_filter_entry_width();
        let fits =
            usize::from(area.width) >= self.download_tabs_width() + 2 + usize::from(entry_width);
        // Two columns of breathing room keep the last tab off the entry.
        let tabs = Rect {
            width: if fits {
                area.width.saturating_sub(entry_width + 2)
            } else {
                area.width
            },
            ..area
        };
        self.download_pills(frame, tabs);
        if !fits {
            return;
        }

        let rect = self.download_filter_entry_rect(area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    self.download_filter_entry(),
                    Style::default().fg(pal.text_dim).bg(pal.surface),
                ),
                Span::styled(" ›", Style::default().fg(pal.cyan).bg(pal.surface)),
            ])),
            rect,
        );
        self.hitmap.add(
            rect,
            Action::Page {
                page: Page::DownloadCenter,
                act: crate::tui::action::PageAct::FilterCategories,
            },
        );
    }

    /// One row per project: everything else (description, tags, downloads,
    /// loaders, author and id) is consolidated in the detail card, so the list
    /// stays a compact, scannable index.
    fn download_projects(&self, frame: &mut Frame<'_>, area: Rect, t: f64) -> bool {
        let pal = self.creeper_palette();
        let count = self.resource_projects.len();
        let kind = self.i18n.text(self.download_center.category.label_key());
        let title = format!("{} · {kind}", self.i18n.text("download.results"));
        if count == 0 {
            widgets::empty_card(
                frame,
                area,
                &title,
                &self.i18n.text("download.no_results"),
                pal.cyan,
                t,
            );
            return false;
        }
        let selected = self.download_center.selected.min(count - 1);
        let layout = self.row_windows.resolve(
            RowWindow::ResourceProjects,
            RowLayout::list(area, count, selected),
        );
        self.hitmap.rows(layout, RowKind::ResourceProjects);
        let block = widgets::with_counter(
            widgets::card(&title, pal.cyan, true, t, pal.surface),
            selected + 1,
            count,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.width < 8 || inner.height == 0 {
            return true;
        }
        let text_width = usize::from(inner.width.saturating_sub(4));
        for (visual, project) in self
            .resource_projects
            .iter()
            .skip(layout.offset())
            .take(layout.visible_rows())
            .enumerate()
        {
            let y = inner.y + visual as u16;
            let active = layout.offset() + visual == selected;
            let bg = if active {
                pal.fill(pal.cyan, 0.13)
            } else {
                pal.surface
            };
            frame.render_widget(
                Block::default().style(Style::default().bg(bg)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        " ◆ ",
                        Style::default()
                            .fg(if active { pal.cyan } else { pal.text_faint })
                            .bg(bg),
                    ),
                    Span::styled(
                        palette::one_line(&project.title, text_width),
                        Style::default()
                            .fg(if active {
                                palette::mix(pal.cyan, pal.text, 0.25)
                            } else {
                                pal.text
                            })
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        if active { " ›" } else { "" },
                        Style::default()
                            .fg(pal.cyan)
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]))
                .style(Style::default().bg(bg)),
                Rect::new(inner.x, y, inner.width, 1),
            );
        }
        true
    }

    fn download_pills(&self, frame: &mut Frame<'_>, area: Rect) {
        let pal = self.creeper_palette();
        let labels = self.download_pill_labels();
        let selected = DownloadCategory::ALL
            .iter()
            .position(|category| *category == self.download_center.category);
        let spans = widgets::pill_row(&labels, selected, pal.cyan, pal.surface);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(pal.surface)),
            area,
        );
        for (index, rect) in widgets::pill_rects(&labels, area).iter().enumerate() {
            self.hitmap
                .add(*rect, Action::filter(Page::DownloadCenter, index));
        }
    }

    /// Draws the version/file table; returns false when the page already
    /// rendered an empty state. Project results use [`Self::download_projects`].
    fn download_rows(&self, frame: &mut Frame<'_>, area: Rect, t: f64) -> bool {
        let pal = self.creeper_palette();
        let is_versions = self.download_center.category == DownloadCategory::GameVersions;
        let (title, headers, widths, rows, count) = if is_versions {
            let indices = self.filtered_version_indices();
            let rows = indices
                .iter()
                .map(|index| {
                    let version = &self.versions[*index];
                    RowData::new([
                        version.id.clone(),
                        self.version_kind_label(&version.kind),
                        version
                            .release_time
                            .get(..10)
                            .unwrap_or(&version.release_time)
                            .to_owned(),
                    ])
                    .mark(0, palette::mix(pal.text, pal.bg, 0.05))
                    .mark(1, pal.cyan)
                })
                .collect::<Vec<_>>();
            (
                self.i18n.text("download.minecraft_versions"),
                vec![
                    self.i18n.text("java.version"),
                    self.i18n.text("common.type"),
                    self.i18n.text("download.released"),
                ],
                vec![
                    Constraint::Length(18),
                    Constraint::Length(12),
                    Constraint::Min(1),
                ],
                rows,
                indices.len(),
            )
        } else {
            let targeted = self.download_center.category.targets_instance();
            let target = self.resource_target_context();
            let rows = self
                .resource_files
                .iter()
                .map(|file| {
                    let channel = self.release_channel_label(file.channel);
                    if !targeted {
                        // A modpack version creates its own instance, so a
                        // compatibility badge and a dependency count would be
                        // measured against an unrelated instance.
                        return RowData::new([file.name.clone(), channel]);
                    }
                    let compatibility = crate::core::resource::compatibility::file_compatibility(
                        file,
                        target.as_ref(),
                    );
                    let (kind, label, color) = match compatibility {
                        crate::core::resource::model::Compatibility::Compatible => (
                            StatusKind::Ready,
                            localize_compatibility(compatibility, &self.i18n),
                            pal.green,
                        ),
                        crate::core::resource::model::Compatibility::Unknown => (
                            StatusKind::Info,
                            localize_compatibility(compatibility, &self.i18n),
                            pal.cyan,
                        ),
                        crate::core::resource::model::Compatibility::Incompatible => (
                            StatusKind::Failed,
                            localize_compatibility(compatibility, &self.i18n),
                            pal.red,
                        ),
                    };
                    RowData::new([
                        format!("{} {}", widgets::status_glyph(kind), label),
                        file.name.clone(),
                        channel,
                        format!("deps:{}", file.dependencies.len()),
                    ])
                    .mark(0, color)
                })
                .collect::<Vec<_>>();
            let (headers, widths) = if targeted {
                (
                    vec![
                        self.i18n.text("download.compatibility"),
                        self.i18n.text("download.file"),
                        self.i18n.text("download.channel"),
                        self.i18n.text("download.dependencies"),
                    ],
                    vec![
                        Constraint::Length(18),
                        Constraint::Percentage(44),
                        Constraint::Length(12),
                        Constraint::Min(1),
                    ],
                )
            } else {
                (
                    vec![
                        self.i18n.text("download.file"),
                        self.i18n.text("download.channel"),
                    ],
                    vec![Constraint::Percentage(60), Constraint::Min(1)],
                )
            };
            (
                format!(
                    "{} · {}",
                    self.i18n.text("download.resource_files"),
                    self.resource_detail.as_deref().unwrap_or_default()
                ),
                headers,
                widths,
                rows,
                self.resource_files.len(),
            )
        };
        if rows.is_empty() || count == 0 {
            widgets::empty_card(
                frame,
                area,
                &title,
                &self.i18n.text("download.no_results"),
                pal.cyan,
                t,
            );
            return false;
        }
        let selected = if is_versions {
            self.version_selected
        } else {
            self.download_center.selected
        };
        let window = if is_versions {
            RowWindow::Versions
        } else {
            RowWindow::ResourceFiles
        };
        let layout = self.row_windows.resolve(
            window,
            RowLayout::table(area, count, selected.min(count.saturating_sub(1))),
        );
        self.hitmap.rows(
            layout,
            if is_versions {
                RowKind::Versions
            } else {
                RowKind::ResourceFiles
            },
        );
        let offset = layout.offset();
        let visible = layout.visible_rows().max(1);
        let window = rows
            .into_iter()
            .skip(offset)
            .take(visible)
            .collect::<Vec<_>>();
        widgets::render_rows(
            frame,
            area,
            &title,
            pal.cyan,
            &headers,
            &window,
            Some(selected.min(count.saturating_sub(1)).saturating_sub(offset)),
            &widths,
            t,
            0,
            Some((selected.min(count.saturating_sub(1)) + 1, count)),
        );
        true
    }
}
