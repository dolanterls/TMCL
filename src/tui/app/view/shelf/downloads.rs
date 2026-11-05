//! `shelf` download centre: a full-width catalogue with a record column.
//!
//! The head is four rows — categories with the position on the right, a rule,
//! the search line, a second rule — and the list below it is one of three
//! shapes: game versions, the project index, or one project's files. Only the
//! project index keeps a record column: the file table already says what the
//! record would (channel, dependencies, compatibility), and the version rows
//! carry their own date and type.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::tone::Palette;
use super::widgets;
use super::*;
use crate::core::resource::model::Compatibility;
use crate::tui::action::{Action, ScrollTarget};
use crate::tui::hit::RowKind;
use crate::tui::input::SearchTarget;
use crate::tui::pages::download_center::DownloadCategory;
use crate::tui::palette;
use crate::tui::widgets::rows::{RowLayout, RowWindow};

/// Rows the head takes: categories, rule, search, rule.
pub(in crate::tui::app) const HEAD_ROWS: u16 = 4;
/// Columns between the selection bar and the row's text.
const TEXT_INSET: u16 = 2;

/// The list body of the download page. Painting, the wheel window and the
/// tests all measure this one rectangle.
pub(in crate::tui::app) fn download_body(area: Rect) -> Rect {
    let y = area.y.saturating_add(HEAD_ROWS);
    Rect::new(area.x, y, area.width, area.bottom().saturating_sub(y))
}

impl App {
    pub(in crate::tui::app) fn draw_shelf_downloads(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        if area.width < 24 || area.height < 6 {
            return;
        }
        self.shelf_download_categories(frame, Rect::new(area.x, area.y, area.width, 1), pal);
        widgets::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
            pal,
        );
        self.shelf_download_search(
            frame,
            Rect::new(area.x, area.y.saturating_add(2), area.width, 1),
            pal,
        );
        widgets::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(3), area.width, 1),
            pal,
        );

        let body = download_body(area);
        if body.height == 0 {
            return;
        }
        if self.download_center.loading {
            let text = self.i18n.text("common.loading");
            shelf_message(frame, body, pal, &text);
            return;
        }
        if let Some(error) = &self.download_center.error {
            shelf_message(frame, body, pal, error);
            return;
        }

        let split = self.download_detail_areas(body);
        let list = split.map_or(body, |(list, _)| list);
        match self.download_center.category {
            DownloadCategory::GameVersions => self.shelf_download_versions(frame, list, pal),
            _ if self.resource_detail.is_some() => self.shelf_download_files(frame, list, pal),
            _ => self.shelf_download_projects(frame, list, pal),
        }
        if let Some((_, pane)) = split {
            self.shelf_download_record(frame, pane, pal);
        }
    }

    /// Categories on the left, the category-filter entry and the position on
    /// the right.
    fn shelf_download_categories(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        let labels = DownloadCategory::ALL
            .iter()
            .map(|category| self.i18n.text(category.label_key()))
            .collect::<Vec<_>>();
        let current = DownloadCategory::ALL
            .iter()
            .position(|category| *category == self.download_center.category)
            .unwrap_or(0);

        let count = self.shelf_download_count_label();
        let count_width = palette::width(&count) as u16;
        let count_x = area.right().saturating_sub(count_width);
        // The names get the row first: a category the reader cannot see is
        // worse than a chip they can still reach with `f`.
        let limit = count_x.saturating_sub(2);
        let entries = widgets::fit_labels(area, &labels, current, limit);
        for (index, rect) in &entries {
            let style = if *index == current {
                Style::default()
                    .fg(pal.accent_light)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(pal.dim).bg(pal.bg)
            };
            let Some(label) = labels.get(*index) else {
                continue;
            };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(label, usize::from(rect.width)),
                    style,
                )),
                *rect,
            );
            self.hitmap
                .add(*rect, Action::filter(Page::DownloadCenter, *index));
        }

        // The filter entry is the pointer's way into the category dialog; it
        // only appears when it does not push a name off the row. It sits with
        // the position on the right, so the row reads as names | status.
        let entry = self.download_filter_entry();
        let entry_width = self.download_filter_entry_width();
        let last_label = entries.last().map_or(area.x, |(_, rect)| rect.right());
        let entry_x = count_x.saturating_sub(entry_width.saturating_add(2));
        if !entry.is_empty() && entry_x >= last_label.saturating_add(2) {
            let rect = Rect::new(entry_x, area.y, entry_width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        palette::truncate(&entry, usize::from(entry_width.saturating_sub(2))),
                        Style::default().fg(pal.faint).bg(pal.bg),
                    ),
                    Span::styled(" ›", Style::default().fg(pal.accent).bg(pal.bg)),
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

        frame.render_widget(
            Paragraph::new(Span::styled(
                count,
                Style::default().fg(pal.faint).bg(pal.bg),
            ))
            .alignment(Alignment::Right),
            Rect::new(count_x, area.y, count_width, 1),
        );
    }

    /// Position over total: the selected row in a version list, or the page in
    /// a paged resource list.
    fn shelf_download_count_label(&self) -> String {
        if self.download_center.category == DownloadCategory::GameVersions {
            let total = self.filtered_version_indices().len();
            // No versions means no position: `1 / 0` reads as a bug.
            let position = if total == 0 {
                0
            } else {
                self.version_selected.saturating_add(1).min(total)
            };
            format!("{position} / {total}")
        } else {
            let pages = resource_page_count(self.download_center.total, RESOURCE_PAGE_SIZE).max(1);
            format!("{} / {pages}", self.download_center.page.saturating_add(1))
        }
    }

    /// `搜索 …` on the left, source and sort on the right.
    ///
    /// The query itself lives in the state layer; this row only paints it and
    /// registers the click that focuses it, so `/` and a click land in the same
    /// place.
    fn shelf_download_search(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        let versions = self.download_center.category == DownloadCategory::GameVersions;
        let target = if versions {
            SearchTarget::VersionFilter
        } else {
            SearchTarget::Resources
        };
        let focused = matches!(self.search.as_ref(), Some(state) if state.target == target);
        let query = if versions {
            self.download_center.version_filter.search()
        } else {
            self.download_center.search.as_str()
        };
        let label = self.i18n.text("common.search");
        let right = if versions {
            String::new()
        } else {
            format!(
                "{} · {}",
                localize_resource_source(self.download_center.source, &self.i18n),
                localize_resource_sort(self.download_center.sort, &self.i18n)
            )
        };
        let right_width = palette::width(&right) as u16;
        let budget = area
            .width
            .saturating_sub(palette::width(&label) as u16 + 2)
            .saturating_sub(right_width.saturating_add(2));

        let shown = if query.is_empty() {
            "…".to_owned()
        } else {
            palette::truncate(query, usize::from(budget.saturating_sub(1)))
        };
        let style = if focused {
            Style::default()
                .fg(pal.text)
                .bg(pal.bg)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else if query.is_empty() {
            Style::default().fg(pal.faint).bg(pal.bg)
        } else {
            Style::default().fg(pal.text).bg(pal.bg)
        };
        let mut spans = vec![
            Span::styled(format!("{label} "), Style::default().fg(pal.dim).bg(pal.bg)),
            Span::styled(shown, style),
        ];
        if focused && crate::tui::text::blink_on(self.anim_time()) {
            spans.push(Span::styled(
                "▏",
                Style::default().fg(pal.accent).bg(pal.bg),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, area.y, area.width.saturating_sub(right_width), 1),
        );
        if !right.is_empty() {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    right,
                    Style::default().fg(pal.faint).bg(pal.bg),
                ))
                .alignment(Alignment::Right),
                area,
            );
        }
        self.hitmap.add(area, Action::search(target));
    }

    /// Version list: id, kind, release date. Drawn whole and windowed by
    /// `RowWindow::Versions`, exactly like `paper` — pre-slicing with
    /// `version_scroll` puts the wheel and the painter on two different rows.
    fn shelf_download_versions(&self, frame: &mut Frame<'_>, list: Rect, pal: &Palette) {
        let indices = self.filtered_version_indices();
        let selected = self.version_selected.min(indices.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Versions,
            RowLayout::list_bare(list, indices.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::Versions);
        if indices.is_empty() {
            let text = self.i18n.text("download.no_results");
            shelf_message(frame, list, pal, &text);
            return;
        }

        let date_width = 10u16;
        let kind_width = 10u16;
        let date_x = list.right().saturating_sub(date_width);
        let kind_x = date_x.saturating_sub(kind_width + 2);
        let name_x = list.x.saturating_add(TEXT_INSET);
        let name_width = kind_x.saturating_sub(name_x + 1);

        for (index, rect) in layout.rows() {
            let Some(version) = indices.get(index).and_then(|at| self.versions.get(*at)) else {
                continue;
            };
            let active = index == selected;
            if active {
                widgets::selection_band(frame, rect, pal);
            }
            let primary = if active { pal.text } else { pal.dim };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&version.id, usize::from(name_width)),
                    Style::default()
                        .fg(primary)
                        .bg(if active { pal.surface } else { pal.bg })
                        .add_modifier(if active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )),
                Rect::new(name_x, rect.y, name_width, 1),
            );
            let quiet = if active { pal.dim } else { pal.faint };
            let bg = if active { pal.surface } else { pal.bg };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&self.version_kind_label(&version.kind), 10),
                    Style::default().fg(quiet).bg(bg),
                )),
                Rect::new(kind_x, rect.y, kind_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::fit(
                        version
                            .release_time
                            .get(..10)
                            .unwrap_or(&version.release_time),
                        usize::from(date_width),
                    ),
                    Style::default().fg(quiet).bg(bg),
                )),
                Rect::new(date_x, rect.y, date_width, 1),
            );
        }
    }

    /// Project index: titles only. Author, id, downloads, versions, loaders,
    /// tags and the description belong to the record column, which prints them
    /// once instead of saying the first line twice.
    fn shelf_download_projects(&self, frame: &mut Frame<'_>, list: Rect, pal: &Palette) {
        let selected = self
            .download_center
            .selected
            .min(self.resource_projects.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::ResourceProjects,
            RowLayout::list_bare(list, self.resource_projects.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::ResourceProjects);
        if self.resource_projects.is_empty() {
            let text = self.i18n.text("download.no_results");
            shelf_message(frame, list, pal, &text);
            return;
        }
        for (index, rect) in layout.rows() {
            let Some(project) = self.resource_projects.get(index) else {
                continue;
            };
            let active = index == selected;
            if active {
                widgets::selection_band(frame, rect, pal);
            }
            let name_x = rect.x.saturating_add(TEXT_INSET);
            let width = rect.right().saturating_sub(name_x + 1);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&project.title, usize::from(width)),
                    Style::default()
                        .fg(if active { pal.text } else { pal.dim })
                        .bg(if active { pal.surface } else { pal.bg })
                        .add_modifier(if active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )),
                Rect::new(name_x, rect.y, width, 1),
            );
        }
    }

    /// The second level: one project's files.
    ///
    /// The columns are the other presets' file table — channel, dependency
    /// count, compatibility. The compatibility column keeps its colour on the
    /// selected row, which is why a row here is banded rather than filled with
    /// the accent: `ERROR` on `ACCENT` is about 1.6:1.
    fn shelf_download_files(&self, frame: &mut Frame<'_>, list: Rect, pal: &Palette) {
        let selected = self
            .download_center
            .selected
            .min(self.resource_files.len().saturating_sub(1));
        let head = !self.resource_files.is_empty() && list.height > 2;
        let body = if head {
            Rect::new(
                list.x,
                list.y.saturating_add(1),
                list.width,
                list.height.saturating_sub(1),
            )
        } else {
            list
        };
        let layout = self.row_windows.resolve(
            RowWindow::ResourceFiles,
            RowLayout::list_bare(body, self.resource_files.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::ResourceFiles);
        if self.resource_files.is_empty() {
            let text = self.i18n.text("download.no_results");
            shelf_message(frame, list, pal, &text);
            return;
        }

        let targeted = self.download_center.category.targets_instance();
        let compatibility_width = self
            .compatibility_column_width()
            .max(palette::width(&self.i18n.text("download.compatibility")) as u16);
        let dependencies_width = if targeted {
            self.dependencies_column_width()
        } else {
            0
        };
        let channel_width = self.channel_column_width();

        let compatibility_x = list.right().saturating_sub(compatibility_width);
        let dependencies_x = compatibility_x
            .saturating_sub(dependencies_width)
            .saturating_sub(u16::from(targeted) * 2);
        let channel_x = if targeted {
            dependencies_x.saturating_sub(channel_width + 2)
        } else {
            compatibility_x.saturating_sub(channel_width + 2)
        };
        let name_x = list.x.saturating_add(TEXT_INSET);
        let name_width = channel_x.saturating_sub(name_x + 1);

        if head {
            let quiet = Style::default().fg(pal.faint).bg(pal.bg);
            let mut cells = vec![(
                Rect::new(name_x, list.y, name_width, 1),
                self.i18n.text("download.file"),
                false,
            )];
            cells.push((
                Rect::new(channel_x, list.y, channel_width, 1),
                self.i18n.text("download.channel"),
                false,
            ));
            if dependencies_width > 0 {
                cells.push((
                    Rect::new(dependencies_x, list.y, dependencies_width, 1),
                    self.i18n.text("download.dependencies"),
                    true,
                ));
            }
            if compatibility_width > 0 {
                cells.push((
                    Rect::new(compatibility_x, list.y, compatibility_width, 1),
                    self.i18n.text("download.compatibility"),
                    true,
                ));
            }
            for (rect, label, right) in cells {
                let mut paragraph = Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&label, usize::from(rect.width)),
                    quiet,
                )));
                if right {
                    paragraph = paragraph.alignment(Alignment::Right);
                }
                frame.render_widget(paragraph, rect);
            }
        }

        let target = self.resource_target_context();
        for (index, rect) in layout.rows() {
            let Some(file) = self.resource_files.get(index) else {
                continue;
            };
            let active = index == selected;
            if active {
                widgets::selection_band(frame, rect, pal);
            }
            let bg = if active { pal.surface } else { pal.bg };
            let quiet = if active { pal.dim } else { pal.faint };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&file.name, usize::from(name_width)),
                    Style::default()
                        .fg(if active { pal.text } else { pal.dim })
                        .bg(bg)
                        .add_modifier(if active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )),
                Rect::new(name_x, rect.y, name_width, 1),
            );
            let channel = self.release_channel_label(file.channel);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&channel, usize::from(channel_width)),
                    Style::default().fg(quiet).bg(bg),
                )),
                Rect::new(channel_x, rect.y, channel_width, 1),
            );
            if dependencies_width > 0 {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        file.dependencies.len().to_string(),
                        Style::default().fg(quiet).bg(bg),
                    ))
                    .alignment(Alignment::Right),
                    Rect::new(dependencies_x, rect.y, dependencies_width, 1),
                );
            }
            let state =
                crate::core::resource::compatibility::file_compatibility(file, target.as_ref());
            let colour = match state {
                Compatibility::Compatible => pal.ok,
                Compatibility::Unknown => pal.faint,
                Compatibility::Incompatible => pal.error,
            };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    localize_compatibility(state, &self.i18n),
                    Style::default().fg(colour).bg(bg),
                ))
                .alignment(Alignment::Right),
                Rect::new(compatibility_x, rect.y, compatibility_width, 1),
            );
        }
    }

    /// The record column: the same `detail_lines()` the detail sheet prints, so
    /// author, project id, downloads, versions, loaders, tags and the summary
    /// are all here — a reduced card of its own would quietly lose half of
    /// them.
    fn shelf_download_record(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.width < 8 || area.height < 4 {
            return;
        }
        let inner = widgets::panel(frame, area, &self.i18n.text("common.details"), pal);
        if inner.height == 0 {
            return;
        }
        let paragraph =
            Paragraph::new(self.styled_detail_lines(self.shelf_theme())).wrap(Wrap { trim: false });
        let total = paragraph.line_count(inner.width);
        let max_scroll = total.saturating_sub(usize::from(inner.height));
        self.pane_max_scroll.set(max_scroll);
        let scroll = self.pane_scroll.min(max_scroll);
        frame.render_widget(
            paragraph.scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
            inner,
        );
        crate::tui::app::view::margin_scrollbar(
            frame, inner, total, scroll, pal.faint, pal.accent, pal.bg,
        );
        self.hitmap.scroll(area, ScrollTarget::Pane);
    }
}

/// A one-line note at the top of `area`, for empty and error states.
fn shelf_message(frame: &mut Frame<'_>, area: Rect, pal: &Palette, text: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Span::styled(
            palette::truncate(text, usize::from(area.width)),
            Style::default().fg(pal.faint).bg(pal.bg),
        )),
        Rect::new(area.x, area.y, area.width, 1),
    );
}
