//! `paper` download centre: a catalogue page with a detail column.
//!
//! The chapter row switches category, the search row says what is being
//! filtered, and the list takes one of three shapes: versions, projects, files.
//! No table headers; the columns are recognisable by their contents.

use super::*;
use crate::tui::hit::RowKind;
use crate::tui::input::SearchTarget;
use crate::tui::palette;
use crate::tui::widgets::rows::{RowLayout, RowWindow};

/// Rows the head takes: categories, rule, search, soft rule. The search sits
/// on the rule's next row: a blank line above a control reads as a hole, not as
/// breathing room.
///
/// `pub(super)` so the tests can rebuild the list/column split without copying
/// the number: the split must be measured where the painter measures it.
pub(super) const HEAD_ROWS: u16 = 4;
/// Margin before the name column, matching the instance list.
const MARGIN: u16 = 3;

impl App {
    pub(in crate::tui::app) fn draw_paper_downloads(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
    ) {
        if area.width < 24 || area.height < 7 {
            return;
        }
        self.paper_download_categories(frame, area, pal);
        let search = Rect::new(area.x, area.y.saturating_add(2), area.width, 1);
        self.paper_download_search(frame, search, pal);
        kit::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(3), area.width, 1),
            pal,
            pal.rule_soft,
        );

        let body = Rect::new(
            area.x,
            area.y.saturating_add(HEAD_ROWS),
            area.width,
            area.height.saturating_sub(HEAD_ROWS),
        );
        if body.height == 0 {
            return;
        }
        if self.download_center.loading {
            self.paper_message(frame, body, pal, "common.loading");
            return;
        }
        if let Some(error) = &self.download_center.error {
            self.paper_message(frame, body, pal, error);
            return;
        }

        let split = self.download_detail_areas(body);
        let list = split.map_or(body, |(list, _)| list);
        match self.download_center.category {
            DownloadCategory::GameVersions => self.paper_download_versions(frame, list, pal),
            _ if self.resource_detail.is_some() => {
                self.paper_download_files(frame, list, pal);
            }
            _ => self.paper_download_projects(frame, list, pal),
        }
        if let Some((_, pane)) = split {
            self.paper_download_detail(frame, pane, pal);
        }
    }

    /// The six categories as a line of running text, with the page number on
    /// the right edge.
    fn paper_download_categories(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        const GAP: u16 = 3;
        let page = self.paper_download_page_label();
        let page_width = palette::width(&page) as u16;
        // One blank column between the last label and the page number, which
        // sits flush with the right margin. Drawing the number at `label_right`
        // spends that column on the number and leaves the page one short.
        let label_right = area.right().saturating_sub(page_width).saturating_sub(1);
        let entries = DownloadCategory::ALL
            .iter()
            .enumerate()
            .map(|(index, category)| {
                let label = self.i18n.text(category.label_key());
                let width = palette::width(&label) as u16;
                (index, *category, label, width)
            })
            .collect::<Vec<_>>();
        // A narrow terminal cannot hold six names. Start at the current one so
        // the reader can still see which category the list belongs to, instead
        // of dropping the tail and hiding the selection.
        let required = entries
            .iter()
            .map(|(_, _, _, width)| *width)
            .sum::<u16>()
            .saturating_add(
                GAP.saturating_mul(u16::try_from(entries.len().saturating_sub(1)).unwrap_or(0)),
            );
        let available = label_right.saturating_sub(area.x);
        let start = if required > available {
            let current = entries
                .iter()
                .position(|(_, category, _, _)| *category == self.download_center.category)
                .unwrap_or(0);
            // Fill to the left of the current name: a row that ends at the
            // selection still tells the reader where they are in the list.
            let mut start = current;
            while start > 0 {
                let candidate = start - 1;
                let width =
                    entries[candidate..]
                        .iter()
                        .map(|(_, _, _, width)| *width)
                        .sum::<u16>()
                        .saturating_add(GAP.saturating_mul(
                            u16::try_from(entries.len() - candidate - 1).unwrap_or(0),
                        ));
                if width > available {
                    break;
                }
                start = candidate;
            }
            start
        } else {
            0
        };
        let mut x = area.x;
        for (index, category, label, width) in entries.into_iter().skip(start) {
            if x.saturating_add(width) > label_right {
                break;
            }
            let rect = Rect::new(x, area.y, width, 1);
            let style = if category == self.download_center.category {
                ink(pal, pal.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                ink(pal, pal.dim)
            };
            frame.render_widget(Paragraph::new(Line::from(Span::styled(label, style))), rect);
            self.hitmap
                .add(rect, Action::filter(Page::DownloadCenter, index));
            x = x.saturating_add(width).saturating_add(GAP);
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(page, ink(pal, pal.faint))))
                .alignment(Alignment::Right),
            Rect::new(
                area.right().saturating_sub(page_width),
                area.y,
                page_width,
                1,
            ),
        );
        kit::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
            pal,
            pal.rule,
        );
    }

    /// Position over total: the row's place in a version list, or the page in a
    /// paged resource list.
    fn paper_download_page_label(&self) -> String {
        if self.download_center.category == DownloadCategory::GameVersions {
            let total = self.filtered_version_indices().len();
            // No versions means no position: `1 / 0` reads as a bug, `0 / 0`
            // reads as an empty list.
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
    fn paper_download_search(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
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
        let mut right = String::new();
        if !versions {
            right = format!(
                "{} · {}",
                localize_resource_source(self.download_center.source, &self.i18n),
                localize_resource_sort(self.download_center.sort, &self.i18n)
            );
        }
        let right_width = palette::width(&right) as u16;
        let budget = area
            .width
            .saturating_sub(palette::width(&label) as u16 + 2)
            .saturating_sub(right_width.saturating_add(2));

        let mut spans = vec![Span::styled(format!("{label} "), ink(pal, pal.dim))];
        let style = if focused {
            ink(pal, pal.ink).add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
        } else if query.is_empty() {
            ink(pal, pal.faint)
        } else {
            ink(pal, pal.soft)
        };
        let shown = if query.is_empty() {
            "…".to_owned()
        } else {
            palette::truncate(query, usize::from(budget.saturating_sub(1)))
        };
        spans.push(Span::styled(shown, style));
        if focused && crate::tui::text::blink_on(self.anim_time()) {
            spans.push(Span::styled("▏", ink(pal, pal.accent)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, area.y, area.width.saturating_sub(right_width), 1),
        );
        if !right.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(right, ink(pal, pal.faint))))
                    .alignment(Alignment::Right),
                area,
            );
        }
        self.hitmap.add(area, Action::search(target));
    }

    fn paper_message(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette, text: &str) {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(text, usize::from(area.width)),
                ink(pal, pal.faint),
            ))),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }

    fn paper_download_versions(&self, frame: &mut Frame<'_>, list: Rect, pal: &PaperPalette) {
        let indices = self.filtered_version_indices();
        let selected = self.version_selected.min(indices.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Versions,
            RowLayout::list_bare(list, indices.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::Versions);
        if indices.is_empty() {
            self.paper_message(frame, list, pal, &self.i18n.text("download.no_results"));
            return;
        }
        // `kit::columns` spans the whole list, so a row's columns are its x
        // and width with the row's own y. Passing the tall rects straight to
        // the paragraph paints every row's values into the first row.
        let columns = kit::columns(list, &[10, 10], 2);
        let (date_x, kind_x) = (columns[0].x, columns[1].x);
        let (date_width, kind_width) = (columns[0].width, columns[1].width);
        for (index, rect) in layout.rows() {
            let Some(version) = indices.get(index).and_then(|at| self.versions.get(*at)) else {
                continue;
            };
            let active = index == selected;
            if active {
                kit::selection(frame, rect, pal);
            }
            let name_width = kind_x.saturating_sub(rect.x.saturating_add(MARGIN + 2));
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&version.id, usize::from(name_width)),
                    if active {
                        ink_marked(pal)
                    } else {
                        ink(pal, pal.soft)
                    },
                ))),
                Rect::new(rect.x.saturating_add(MARGIN), rect.y, name_width, 1),
            );
            let quiet = if active {
                ink(pal, pal.soft)
            } else {
                ink(pal, pal.faint)
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&self.version_kind_label(&version.kind), 10),
                    quiet,
                ))),
                Rect::new(kind_x, rect.y, kind_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::fit(
                        version
                            .release_time
                            .get(..10)
                            .unwrap_or(&version.release_time),
                        10,
                    ),
                    quiet,
                ))),
                Rect::new(date_x, rect.y, date_width, 1),
            );
        }
    }

    /// Project rows carry the title only.
    ///
    /// `classic` and `nightlight` do the same: the summary is the first line of
    /// the description, and the description is printed in full in the detail
    /// column. Showing it in both places made the same sentence appear twice,
    /// three columns apart.
    fn paper_download_projects(&self, frame: &mut Frame<'_>, list: Rect, pal: &PaperPalette) {
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
            self.paper_message(frame, list, pal, &self.i18n.text("download.no_results"));
            return;
        }
        for (index, rect) in layout.rows() {
            let Some(project) = self.resource_projects.get(index) else {
                continue;
            };
            let active = index == selected;
            if active {
                kit::selection(frame, rect, pal);
            }
            let width = rect.width.saturating_sub(MARGIN + 1);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&project.title, usize::from(width)),
                    if active {
                        ink_marked(pal)
                    } else {
                        ink(pal, pal.soft)
                    },
                ))),
                Rect::new(rect.x.saturating_add(MARGIN), rect.y, width, 1),
            );
        }
    }

    /// The second level: one project's files.
    ///
    /// The columns match the other presets' file table (channel, dependency
    /// count, compatibility). The detail column belongs to the project index,
    /// not to the file list. This is the preset's only table head, and it is
    /// here because `兼容性` reads as a word while the count beside it does not.
    fn paper_download_files(&self, frame: &mut Frame<'_>, list: Rect, pal: &PaperPalette) {
        let selected = self
            .download_center
            .selected
            .min(self.resource_files.len().saturating_sub(1));
        // A table head needs a row to sit in; below that the list keeps the
        // whole area and the head goes away with it.
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
            self.paper_message(frame, list, pal, &self.i18n.text("download.no_results"));
            return;
        }

        let targeted = self.download_center.category.targets_instance();
        // The head has to fit as well: `Compatibility` is one cell longer than
        // the widest status word, and a cut header reads as a cut word.
        let status_width = self
            .compatibility_column_width()
            .max(palette::width(&self.i18n.text("download.compatibility")) as u16);
        let (compatibility_width, dependencies_width) = if targeted {
            (status_width, self.dependencies_column_width())
        } else {
            // A modpack builds an instance of its own: there is no target to
            // compare against and no dependency list to count.
            (0, 0)
        };
        let channel_width = self.channel_column_width();
        // Same as the version list: these rects are list-tall, so each row
        // re-anchors them to its own y before drawing.
        let columns = kit::columns(
            body,
            &[compatibility_width, dependencies_width, channel_width],
            2,
        );
        let (compatibility_x, dependencies_x, channel_x) =
            (columns[0].x, columns[1].x, columns[2].x);
        let (compatibility_width, dependencies_width, channel_width) =
            (columns[0].width, columns[1].width, columns[2].width);

        if head {
            let quiet = ink(pal, pal.faint);
            let name_width = channel_x.saturating_sub(list.x.saturating_add(MARGIN + 2));
            let mut cells = vec![(
                Rect::new(list.x.saturating_add(MARGIN), list.y, name_width, 1),
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
                kit::selection(frame, rect, pal);
            }
            let name_width = channel_x.saturating_sub(rect.x.saturating_add(MARGIN + 2));
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&file.name, usize::from(name_width)),
                    if active {
                        ink_marked(pal)
                    } else {
                        ink(pal, pal.soft)
                    },
                ))),
                Rect::new(rect.x.saturating_add(MARGIN), rect.y, name_width, 1),
            );
            let quiet = if active {
                ink(pal, pal.soft)
            } else {
                ink(pal, pal.faint)
            };
            let channel = self.release_channel_label(file.channel);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&channel, usize::from(channel_width)),
                    quiet,
                ))),
                Rect::new(channel_x, rect.y, channel_width, 1),
            );
            if dependencies_width > 0 {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        file.dependencies.len().to_string(),
                        quiet,
                    )))
                    .alignment(Alignment::Right),
                    Rect::new(dependencies_x, rect.y, dependencies_width, 1),
                );
            }
            if compatibility_width > 0 {
                let state =
                    crate::core::resource::compatibility::file_compatibility(file, target.as_ref());
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        localize_compatibility(state, &self.i18n),
                        ink(pal, self.paper_compatibility_color(state, pal)),
                    )))
                    .alignment(Alignment::Right),
                    Rect::new(compatibility_x, rect.y, compatibility_width, 1),
                );
            }
        }
    }

    fn paper_compatibility_color(
        &self,
        state: crate::core::resource::model::Compatibility,
        pal: &PaperPalette,
    ) -> Color {
        use crate::core::resource::model::Compatibility;
        match state {
            Compatibility::Compatible => pal.ok,
            Compatibility::Unknown => pal.faint,
            Compatibility::Incompatible => pal.accent,
        }
    }

    /// The detail column: the same record the detail sheet prints, on the page
    /// instead of on a slip.
    ///
    /// The content is `detail_lines()`, the single source `classic` and
    /// `nightlight` also print. Opening a project swaps the record for the
    /// selected file, as it does in the other presets. A reduced card of its own
    /// would say less than the slip behind the same page.
    fn paper_download_detail(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        if area.width < 8 || area.height < 4 {
            return;
        }
        let body = kit::section(frame, area, pal, &self.i18n.text("common.details"), None);
        if body.height == 0 {
            return;
        }
        let paragraph =
            Paragraph::new(self.paper_record_lines(pal, false)).wrap(Wrap { trim: false });
        let total = paragraph.line_count(body.width);
        let max_scroll = total.saturating_sub(usize::from(body.height));
        self.pane_max_scroll.set(max_scroll);
        let scroll = self.pane_scroll.min(max_scroll);
        frame.render_widget(
            paragraph.scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
            body,
        );
        // A record longer than the column says so, as the other presets' panels
        // do: a scrollbar in the page margin with an accent thumb. Without it a
        // long description stops at the bottom edge with no sign that the wheel
        // continues it.
        self.paper_scrollbar(frame, body, total, scroll, pal, pal.bg);
        self.hitmap
            .scroll(area, crate::tui::action::ScrollTarget::Pane);
    }
}
