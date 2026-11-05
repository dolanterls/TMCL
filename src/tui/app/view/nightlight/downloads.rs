//! Download centre: search card, category chips, count line and the result
//! table (game versions) or project index (everything else).
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::action::{Action, PageAct, ScrollTarget};
use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{
    self, card, card_inner, fit_cell, right_cell, table_head,
};
use crate::tui::hit::RowKind;
use crate::tui::input::SearchTarget;
use crate::tui::pages::download_center::{DownloadCategory, DownloadDestination};
use crate::tui::palette::{fit, truncate};
use crate::tui::widgets::rows::{RowLayout, RowWindow};

use super::{App, Page};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::app) struct DownloadLayout {
    pub search: Rect,
    pub categories: Rect,
    pub info: Rect,
    pub results: Rect,
}

pub(in crate::tui::app) fn layout(area: Rect) -> DownloadLayout {
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .spacing(1)
        .split(inner);
    DownloadLayout {
        search: rows[0],
        categories: rows[1],
        info: rows[2],
        results: rows[3],
    }
}

impl App {
    pub(in crate::tui::app) fn draw_nightlight_downloads(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        let layout = layout(area);
        self.nightlight_download_search(frame, layout.search, pal);
        let labels = self.download_pill_labels();
        let active = DownloadCategory::ALL
            .iter()
            .position(|category| *category == self.download_center.category)
            .unwrap_or(0);
        let entry = self.download_filter_entry();
        let entry_width = self.download_filter_entry_width();
        let row = layout.categories;
        // The entry is right-aligned inside its rect, so its last glyph lands on
        // the rect's final column. Inset the rect by one column so the value
        // keeps a gap from the page margin instead of hugging the edge.
        const EDGE_INSET: u16 = 1;
        let chips = Rect {
            width: row
                .width
                .saturating_sub(entry_width.saturating_add(2 + EDGE_INSET)),
            ..row
        };
        self.draw_pill_row(frame, chips, &labels, active, Page::DownloadCenter, pal);
        // The filter control belongs on the tab row: it filters those tabs, and
        // reading it from the far end of the line below is a hunt.
        let entry_rect = Rect::new(
            row.right()
                .saturating_sub(entry_width.saturating_add(EDGE_INSET)),
            row.y,
            entry_width,
            1,
        );
        // Same affordance as the other two presets: the chevron is what says
        // "this opens something". The rect is `text + 2` wide for exactly
        // `" ›"`, so drawing only the value left two dead cells on the left and
        // no hint that the control was clickable.
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("{entry} ›"),
                Style::default().fg(pal.accent).bg(pal.bg),
            ))
            .alignment(ratatui::layout::Alignment::Right),
            entry_rect,
        );
        self.hitmap.add(
            entry_rect,
            Action::Page {
                page: Page::DownloadCenter,
                act: PageAct::FilterCategories,
            },
        );
        self.download_info(frame, layout.info, pal);
        self.download_results(frame, layout.results, pal);
    }

    fn nightlight_download_search(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.is_empty() {
            return;
        }
        let target = if self.download_center.category == DownloadCategory::GameVersions {
            SearchTarget::VersionFilter
        } else {
            SearchTarget::Resources
        };
        // One band: the page name and the query share the title row, so the
        // card reads as a title bar with a field in it. Stacking the input
        // under the wordmark made the header look like two unrelated blocks.
        card(frame, area, "", pal);
        self.hitmap.add(
            area,
            Action::Page {
                page: Page::DownloadCenter,
                act: PageAct::Search(target),
            },
        );
        let title = self.i18n.text("page.download");
        let title_cells = u16::try_from(widgets::width(&title)).unwrap_or(u16::MAX);
        let row_y = area.y.saturating_add(widgets::CARD_TITLE_Y);
        let title_x = area.x.saturating_add(widgets::CARD_PAD_LEFT);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                title,
                Style::default()
                    .fg(pal.accent)
                    .bg(pal.surface)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect::new(title_x, row_y, title_cells.min(area.width), 1),
        );
        // Three cells of air after the name, then the rest of the card: the
        // query is the widest thing here and gets the space.
        let field_x = title_x.saturating_add(title_cells).saturating_add(3);
        let field = Rect::new(
            field_x,
            row_y,
            area.right()
                .saturating_sub(widgets::CARD_PAD_RIGHT)
                .saturating_sub(field_x),
            1,
        );
        if field.width < 8 {
            return;
        }
        let query = if self.download_center.category == DownloadCategory::GameVersions {
            self.download_center.version_filter.search()
        } else {
            self.download_center.search.as_str()
        };
        let focused = matches!(
            self.search.as_ref(),
            Some(state) if state.target == target
        );
        let cursor = self
            .search
            .as_ref()
            .filter(|state| state.target == target)
            .map_or(query.chars().count(), |state| state.cursor);
        let style = crate::tui::text::FieldStyle {
            text: pal.text,
            dim: pal.muted,
            bg: pal.surface,
            cursor_fg: pal.bg,
            cursor_bg: pal.accent,
        };
        let mut spans = vec![Span::styled(
            " ⌕ ",
            Style::default()
                .fg(if focused { pal.accent } else { pal.muted })
                .bg(pal.surface),
        )];
        if query.is_empty() && !focused {
            spans.push(Span::styled(
                truncate(
                    &self.i18n.text("download.search_placeholder"),
                    usize::from(field.width).saturating_sub(4),
                ),
                Style::default().fg(pal.faint).bg(pal.surface),
            ));
        } else {
            spans.extend(
                crate::tui::text::render(
                    query,
                    cursor,
                    field.width.saturating_sub(4),
                    focused,
                    crate::tui::text::blink_on(self.anim_time()),
                    style,
                    "",
                )
                .spans,
            );
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(pal.surface)),
            field,
        );
    }

    fn download_info(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.is_empty() {
            return;
        }
        let target = self
            .download_target_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
            .map(|item| item.name.clone())
            .unwrap_or_else(|| self.i18n.text("common.none"));
        // Where the download goes is the one thing the result page cannot show
        // anywhere else. Only a category that installs into an existing
        // instance can name one; the others say what will happen instead.
        let text = if self.download_center.category == DownloadCategory::GameVersions {
            format!(
                " {}",
                self.i18n.format(
                    "download.version_total",
                    &[("count", &self.filtered_version_indices().len().to_string())],
                )
            )
        } else {
            let destination = match self.download_center.category.destination() {
                DownloadDestination::Instance => {
                    format!("{} {target}", self.i18n.text("action.target"))
                }
                DownloadDestination::NewInstance => self.i18n.text("download.creates_instance"),
                DownloadDestination::SavePath => self.i18n.text("download.saves_to_path"),
            };
            format!(
                " {}  \u{00b7}  {destination}",
                self.download_filter_summary()
            )
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate(&text, usize::from(area.width)),
                Style::default().fg(pal.faint).bg(pal.bg),
            ))),
            area,
        );
    }

    /// `Results · Mods`: the card names the category it is showing.
    ///
    /// At 80 columns the chip row cannot always keep every category, so the
    /// title is the one place that always says where the user is.
    fn results_title(&self) -> String {
        format!(
            "{} · {}",
            self.i18n.text("download.results"),
            self.i18n.text(self.download_center.category.label_key())
        )
    }

    fn download_results(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.is_empty() {
            return;
        }
        if self.download_center.loading {
            card(frame, area, &self.results_title(), pal);
            let inner = card_inner(area);
            frame.render_widget(
                Paragraph::new(self.i18n.text("download.searching"))
                    .style(Style::default().fg(pal.muted).bg(pal.surface)),
                inner,
            );
            return;
        }
        if let Some(error) = self.download_center.error.as_deref() {
            card(frame, area, &self.i18n.text("download.error"), pal);
            let inner = card_inner(area);
            frame.render_widget(
                Paragraph::new(truncate(error, usize::from(inner.width)))
                    .style(Style::default().fg(pal.err).bg(pal.surface)),
                inner,
            );
            return;
        }
        let split = self.download_detail_areas(area);
        let list_area = split.map_or(area, |(list, _)| list);
        if self.download_center.category == DownloadCategory::GameVersions {
            self.version_table(frame, list_area, pal);
        } else if self.resource_detail.is_some() {
            self.resource_file_table(frame, list_area, pal);
        } else {
            self.project_index(frame, list_area, pal);
        }
        if let Some((_, pane)) = split {
            self.hitmap.scroll(pane, ScrollTarget::Pane);
            self.download_detail_card(frame, pane, pal);
        }
    }

    fn version_table(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        card(frame, area, &self.results_title(), pal);
        let inner = card_inner(area);
        if inner.height < 3 {
            return;
        }
        let indices = self.filtered_version_indices();
        let selected = self.version_selected.min(indices.len().saturating_sub(1));
        let widths = column_widths(inner.width);
        // Same "N · n/N" line as the card lists: the table is the one place
        // where the position matters most (thousands of versions) and it had no
        // counter at all. An empty table says 0/0, never 1/0.
        let counter = if indices.is_empty() {
            format!(" {}  ·  0/0 ", indices.len())
        } else {
            format!(" {}  ·  {}/{} ", indices.len(), selected + 1, indices.len())
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                counter,
                Style::default().fg(pal.faint).bg(pal.surface),
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new(table_head(
                &[
                    &self.i18n.text("overview.version"),
                    &self.i18n.text("common.type"),
                    &self.i18n.text("download.released"),
                ],
                &widths,
                pal,
            )),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
        let rows = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2),
        );
        let layout = self.row_windows.resolve(
            RowWindow::Versions,
            RowLayout::list_bare(rows, indices.len(), selected),
        );
        // Register the scroll target before the empty-state return: an empty
        // table still owns the wheel over its body, or the page swallows the
        // notch and the next list appears deaf.
        self.hitmap.rows(layout, RowKind::Versions);
        if indices.is_empty() {
            frame.render_widget(
                Paragraph::new(truncate(
                    &self.i18n.text("download.no_results"),
                    usize::from(rows.width),
                ))
                .style(Style::default().fg(pal.muted).bg(pal.surface)),
                rows,
            );
            return;
        }
        let start = layout.offset();
        for (visual, position) in indices
            .iter()
            .skip(start)
            .take(layout.visible_rows())
            .enumerate()
        {
            let Some(version) = self.versions.get(*position) else {
                continue;
            };
            let y = rows.y.saturating_add(visual as u16);
            let active = start + visual == selected;
            let background = if active { pal.selection } else { pal.surface };
            let mut spans = vec![
                Span::styled(
                    fit(&version.id, usize::from(widths[0])),
                    Style::default()
                        .fg(if active { pal.text } else { pal.muted })
                        .bg(background),
                ),
                Span::styled(
                    fit(
                        &self.version_kind_label(&version.kind),
                        usize::from(widths[1]),
                    ),
                    Style::default().fg(pal.accent2).bg(background),
                ),
                Span::styled(
                    truncate(
                        version
                            .release_time
                            .get(..10)
                            .unwrap_or(&version.release_time),
                        usize::from(widths[2]),
                    ),
                    Style::default().fg(pal.faint).bg(background),
                ),
            ];
            if active {
                spans = spans
                    .into_iter()
                    .map(|span| {
                        Span::styled(
                            span.content.into_owned(),
                            span.style.bg(pal.selection).fg(pal.text),
                        )
                    })
                    .collect();
            }
            let rect = Rect::new(rows.x, y, rows.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                rect,
            );
        }
    }

    fn project_index(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        card(frame, area, &self.results_title(), pal);
        let inner = card_inner(area);
        if inner.height < 2 {
            return;
        }
        let total = self.resource_projects.len();
        let selected = self.download_center.selected.min(total.saturating_sub(1));
        // `min(total.max(1))` turned an empty index into "1/0": a position that
        // cannot exist. Empty lists say 0/0 and explain themselves below.
        let counter = if total == 0 {
            format!(" {total}  ·  0/0")
        } else {
            format!(" {total}  ·  {}/{}", selected + 1, total)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                counter,
                Style::default().fg(pal.faint).bg(pal.surface),
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let rows = Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            inner.height.saturating_sub(1),
        );
        let layout = self.row_windows.resolve(
            RowWindow::ResourceProjects,
            RowLayout::list_bare(rows, total, selected),
        );
        self.hitmap.rows(layout, RowKind::ResourceProjects);
        if total == 0 {
            frame.render_widget(
                Paragraph::new(truncate(
                    &self.i18n.text("download.no_results"),
                    usize::from(inner.width),
                ))
                .style(Style::default().fg(pal.muted).bg(pal.surface)),
                rows,
            );
            return;
        }
        let start = layout.offset();
        for (visual, index) in (start..total).take(layout.visible_rows()).enumerate() {
            let Some(project) = self.resource_projects.get(index) else {
                continue;
            };
            let y = rows.y.saturating_add(visual as u16);
            let active = index == selected;
            let background = if active { pal.selection } else { pal.surface };
            // No download count here: the detail pane already prints it, and
            // repeating it cost the title a dozen columns in a list this
            // narrow. Names win.
            let spans = vec![Span::styled(
                truncate(&project.title, usize::from(inner.width)),
                Style::default()
                    .fg(if active { pal.text } else { pal.muted })
                    .bg(background),
            )];
            let rect = Rect::new(rows.x, y, rows.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                rect,
            );
        }
    }

    /// The second level of the resource flow: one project's files with their
    /// channel, plus - when the download lands in an existing instance - its
    /// compatibility and dependency count. Enter on a row starts the install;
    /// Esc returns to the index.
    fn resource_file_table(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        let title = format!(
            "{} \u{00b7} {}",
            self.i18n.text("download.resource_files"),
            self.resource_detail.as_deref().unwrap_or_default()
        );
        let inner = card(frame, area, &title, pal);
        if inner.height < 2 || inner.width < 24 {
            return;
        }
        // A modpack builds the instance it installs into, so a compatibility
        // badge and a dependency count here would be measured against an
        // unrelated instance; only the file and its channel are meaningful.
        let targeted = self.download_center.category.targets_instance();
        // Each fixed column must fit its own header, or the table shows a
        // truncated caption over a full value.
        let compat_width = 16u16.min(inner.width / 3);
        let channel_width = 11u16.min(inner.width / 4);
        let deps_width = 12u16.min(inner.width / 5);
        let file_width = if targeted {
            inner
                .width
                .saturating_sub(compat_width + channel_width + deps_width)
        } else {
            inner.width.saturating_sub(channel_width)
        };
        let file = self.i18n.text("download.file");
        let channel = self.i18n.text("download.channel");
        let (headers, widths) = if targeted {
            (
                vec![
                    self.i18n.text("download.compatibility"),
                    file,
                    channel,
                    self.i18n.text("download.dependencies"),
                ],
                [compat_width, file_width, channel_width, deps_width],
            )
        } else {
            (vec![file, channel], [file_width, channel_width, 0, 0])
        };
        let headers = headers.iter().map(String::as_str).collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(table_head(&headers, &widths, pal)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let rows = Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            inner.height.saturating_sub(1),
        );
        let total = self.resource_files.len();
        if total == 0 {
            frame.render_widget(
                Paragraph::new(self.i18n.text("download.no_results"))
                    .style(Style::default().fg(pal.muted).bg(pal.surface)),
                rows,
            );
            return;
        }
        let selected = self.download_center.selected.min(total.saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::ResourceFiles,
            RowLayout::list_bare(rows, total, selected),
        );
        self.hitmap.rows(layout, RowKind::ResourceFiles);
        let target = self.resource_target_context();
        for (visual, index) in (layout.offset()..total)
            .take(layout.visible_rows())
            .enumerate()
        {
            let Some(file) = self.resource_files.get(index) else {
                continue;
            };
            let y = rows.y.saturating_add(visual as u16);
            let active = index == selected;
            let background = if active { pal.selection } else { pal.surface };
            let mut spans = vec![Span::styled(
                if active { "\u{258e} " } else { "  " },
                Style::default().fg(pal.accent).bg(background),
            )];
            if targeted {
                let compatibility =
                    crate::core::resource::compatibility::file_compatibility(file, target.as_ref());
                let color = match compatibility {
                    crate::core::resource::model::Compatibility::Compatible => pal.ok,
                    crate::core::resource::model::Compatibility::Unknown => pal.accent2,
                    crate::core::resource::model::Compatibility::Incompatible => pal.err,
                };
                let label =
                    crate::tui::app::state::data::localize_compatibility(compatibility, &self.i18n);
                spans.push(Span::styled(
                    fit_cell(&label, usize::from(compat_width).saturating_sub(2)),
                    Style::default().fg(color).bg(background),
                ));
            }
            // The leading marker owns two cells of the file column when there
            // is no compatibility column to hold them.
            let name_width = if targeted {
                file_width
            } else {
                file_width.saturating_sub(2)
            };
            spans.push(Span::styled(
                fit_cell(&file.name, usize::from(name_width)),
                Style::default()
                    .fg(if active { pal.text } else { pal.muted })
                    .bg(background),
            ));
            spans.push(Span::styled(
                fit_cell(
                    &self.release_channel_label(file.channel),
                    usize::from(channel_width),
                ),
                Style::default().fg(pal.faint).bg(background),
            ));
            if targeted {
                spans.push(Span::styled(
                    right_cell(
                        &file.dependencies.len().to_string(),
                        usize::from(deps_width),
                    ),
                    Style::default().fg(pal.faint).bg(background),
                ));
            }
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                Rect::new(rows.x, y, rows.width, 1),
            );
        }
    }

    fn download_detail_card(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        card(frame, area, &self.i18n.text("hint.details"), pal);
        let inner = card_inner(area);
        if inner.is_empty() {
            return;
        }
        // The clamp used by the wheel lives on `App`; a pane that never sets it
        // swallows every notch because the maximum stays at zero.
        let max = widgets::detail_pane(frame, inner, self.detail_lines(), self.pane_scroll, pal);
        self.pane_max_scroll.set(max);
    }
}

fn column_widths(width: u16) -> [u16; 3] {
    // The date is shown as year-month-day; twelve cells cut it and left the
    // column showing an ellipsis instead of a date.
    let released = 14.min(width / 4);
    let kind = 10.min(width / 5);
    [width.saturating_sub(released + kind), kind, released]
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{column_widths, layout};

    #[test]
    fn the_download_page_stacks_four_rows() {
        let grid = layout(Rect::new(0, 0, 93, 38));
        assert_eq!(grid.search.height, 3);
        assert_eq!(grid.categories.height, 1);
        assert_eq!(grid.info.height, 1);
        assert_eq!(grid.results.bottom(), 37);
        assert!(grid.results.height > 20);
    }

    #[test]
    fn version_columns_share_the_available_width() {
        let widths = column_widths(80);
        assert_eq!(
            usize::from(widths[0]) + usize::from(widths[1]) + usize::from(widths[2]),
            80
        );
        assert!(widths[0] > widths[1]);
    }
}
