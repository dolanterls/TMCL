//! Instance workspace: category rail plus a section card and a description
//! line. The second-level screen keeps the same vocabulary as the pages.
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::action::{Action, PageAct};
use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{self, card, card_inner, fit_cell, right_cell};
use crate::tui::input::SearchTarget;
use crate::tui::pages::workspace::{Workspace, WorkspaceCategory};
use crate::tui::widgets::rows::{RowLayout, RowWindow};

use crate::tui::app::state::data_format::format_size;

use super::{App, Page};

const RAIL_WIDTH: u16 = 18;
const RAIL_MIN_WIDTH: u16 = 78;

impl App {
    pub(in crate::tui::app) fn draw_nightlight_workspace(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let inner = area.inner(Margin {
            horizontal: 1,
            vertical: 1,
        });
        let folded = inner.width < RAIL_MIN_WIDTH;
        let (rail, body) = if folded {
            (
                Rect::new(inner.x, inner.y, inner.width, 1),
                Rect::new(
                    inner.x,
                    inner.y + 2,
                    inner.width,
                    inner.height.saturating_sub(2),
                ),
            )
        } else {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(RAIL_WIDTH), Constraint::Min(0)])
                .spacing(1)
                .split(inner);
            // The rail has no card of its own, so it starts on the row where the
            // list beside it starts, below that card's title.
            (
                Rect {
                    y: columns[0]
                        .y
                        .saturating_add(crate::tui::app::view::nightlight::widgets::CARD_PAD_TOP),
                    height: columns[0]
                        .height
                        .saturating_sub(crate::tui::app::view::nightlight::widgets::CARD_PAD_TOP),
                    ..columns[0]
                },
                columns[1],
            )
        };
        self.workspace_rail(frame, rail, workspace, folded, pal);
        self.workspace_body(frame, body, workspace, pal);
    }

    fn workspace_rail(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        folded: bool,
        pal: &Palette,
    ) {
        if area.is_empty() {
            return;
        }
        if folded {
            // One line: the active category and the fold hint.
            let label = self.i18n.text(workspace.category.title_key());
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        format!(" {label}"),
                        Style::default()
                            .fg(pal.accent)
                            .bg(pal.bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(
                            "  ·  {}/{}",
                            workspace.category.index() + 1,
                            WorkspaceCategory::ALL.len()
                        ),
                        Style::default().fg(pal.faint).bg(pal.bg),
                    ),
                ]))
                .style(Style::default().bg(pal.bg)),
                area,
            );
            return;
        }
        for (index, category) in WorkspaceCategory::ALL.into_iter().enumerate() {
            let y = area.y.saturating_add(index as u16);
            if y >= area.bottom() {
                break;
            }
            let active = category == workspace.category;
            let background = if active { pal.accent } else { pal.bg };
            let label = self.i18n.text(category.title_key());
            let count = category
                .content_kind()
                .map(|kind| workspace.content.count(kind))
                .unwrap_or(0);
            let mut spans = vec![
                Span::styled(" ", Style::default().bg(background)),
                Span::styled(
                    widgets::pad_right(
                        &label,
                        area.width.saturating_sub(if count > 0 { 4 } else { 1 }),
                    ),
                    Style::default()
                        .fg(if active { pal.bg } else { pal.muted })
                        .bg(background)
                        .add_modifier(if active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ];
            if count > 0 {
                spans.push(Span::styled(
                    count.to_string(),
                    Style::default()
                        .fg(if active { pal.bg } else { pal.faint })
                        .bg(background),
                ));
            }
            let rect = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                rect,
            );
            self.hitmap.add(rect, Action::workspace_category(index));
        }
    }

    /// The section card's title row also carries the filter field, the way the
    /// download page's header does: one band that says where you are and what
    /// you are looking for. Only content categories have a list to filter.
    fn workspace_card_title(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        pal: &Palette,
    ) {
        let title = self.i18n.text(workspace.category.title_key());
        let title_y = area.y.saturating_add(widgets::CARD_TITLE_Y);
        let title_x = area.x.saturating_add(widgets::CARD_PAD_LEFT);
        let title_width = u16::try_from(widgets::width(&title)).unwrap_or(u16::MAX);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                title.clone(),
                Style::default()
                    .fg(pal.accent)
                    .bg(pal.surface)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect::new(title_x, title_y, title_width.min(area.width), 1),
        );
        if workspace.category.content_kind().is_none() {
            return;
        }
        let field_x = title_x.saturating_add(title_width).saturating_add(3);
        let field = Rect::new(
            field_x,
            title_y,
            area.right()
                .saturating_sub(widgets::CARD_PAD_RIGHT)
                .saturating_sub(field_x),
            1,
        );
        if field.width < 12 {
            return;
        }
        self.hitmap.add(
            field,
            Action::Page {
                page: Page::Instances,
                act: PageAct::Search(SearchTarget::Workspace),
            },
        );
        let focused = matches!(
            self.search.as_ref(),
            Some(state) if state.target == SearchTarget::Workspace
        );
        let cursor = self
            .search
            .as_ref()
            .filter(|state| state.target == SearchTarget::Workspace)
            .map_or_else(|| workspace.search.chars().count(), |state| state.cursor);
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
        spans.extend(
            crate::tui::text::render(
                &workspace.search,
                cursor,
                field.width.saturating_sub(3),
                focused,
                crate::tui::text::blink_on(self.anim_time()),
                style,
                &self.i18n.text("workspace.search_placeholder"),
            )
            .spans,
        );
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(pal.surface)),
            field,
        );
    }

    fn workspace_body(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        pal: &Palette,
    ) {
        if area.is_empty() {
            return;
        }
        card(frame, area, "", pal);
        self.workspace_card_title(frame, area, workspace, pal);
        let mut inner = card_inner(area);
        if inner.is_empty() {
            return;
        }
        // A list that cannot work as it stands says so above itself, and still
        // shows the files: hiding them would read as a failed scan.
        if let Some(banner) = self.workspace_banner(workspace) {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" ! {banner}"),
                    Style::default().fg(pal.err).bg(pal.surface),
                ))),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            inner = Rect {
                y: inner.y.saturating_add(1),
                height: inner.height.saturating_sub(1),
                ..inner
            };
        }
        // A scan in flight must not wipe what is already on screen: the other
        // two presets only show "scanning" when there is nothing to show. Doing
        // it the other way round makes every refresh look like a 20-second
        // freeze, even though the entries were already cached.
        let scanning = workspace
            .category
            .content_kind()
            .is_some_and(|kind| workspace.loading && workspace.content.count(kind) == 0);
        if scanning {
            frame.render_widget(
                Paragraph::new(self.i18n.text("workspace.scanning"))
                    .style(Style::default().fg(pal.faint).bg(pal.surface)),
                inner,
            );
            return;
        }
        // The last two rows are the note strip shared with the other presets:
        // what the selection means, then the keys that work here.
        let strip = inner.height.min(2);
        let list = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(strip),
        );
        if let Some(kind) = workspace.category.content_kind() {
            if workspace.visible_entries(kind).is_empty() {
                self.workspace_empty(frame, list, workspace, kind, pal);
                return;
            }
            self.workspace_content_list(frame, list, workspace, kind, pal);
        } else {
            if workspace.category == WorkspaceCategory::Launch {
                self.workspace_launch_list(frame, list, workspace, pal);
                return;
            }
            let rows = self.workspace_rows(workspace);
            if rows.is_empty() {
                self.workspace_empty_rows(frame, list, pal);
                return;
            }
            self.workspace_value_list(frame, list, workspace, &rows, pal);
        }
        self.workspace_strip(
            frame,
            Rect::new(
                inner.x,
                inner.bottom().saturating_sub(strip),
                inner.width,
                strip,
            ),
            workspace,
            pal,
        );
    }

    /// Why a content list is unusable right now, if it is.
    fn workspace_banner(&self, workspace: &Workspace) -> Option<String> {
        use crate::core::content::ContentKind;
        match workspace.category.content_kind() {
            Some(ContentKind::Shader) if workspace.shader_loader.is_none() => {
                Some(self.i18n.text("workspace.shaders.no_loader_banner"))
            }
            Some(ContentKind::ResourcePack) if self.workspace_instance_running() => {
                Some(self.i18n.text("workspace.packs.running_banner"))
            }
            _ => None,
        }
    }

    fn workspace_empty(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        kind: crate::core::content::ContentKind,
        pal: &Palette,
    ) {
        if area.is_empty() {
            return;
        }
        self.hitmap
            .scroll(area, crate::tui::action::ScrollTarget::WorkspaceRows);
        let (message, hint) = self.workspace_empty_text(workspace, kind);
        let mut lines = vec![Line::from(Span::styled(
            message,
            Style::default().fg(pal.muted).bg(pal.surface),
        ))];
        if area.height > 1 {
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(pal.faint).bg(pal.surface),
            )));
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn workspace_empty_rows(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        frame.render_widget(
            Paragraph::new(self.i18n.text("settings.empty"))
                .style(Style::default().fg(pal.muted).bg(pal.surface)),
            area,
        );
    }

    /// A content list: enable mark, name, version, size and the shared trailing
    /// note. The columns come from the same fields the other presets read, so a
    /// version or a world date cannot be "forgotten" by this renderer.
    fn workspace_content_list(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        kind: crate::core::content::ContentKind,
        pal: &Palette,
    ) {
        if area.is_empty() {
            return;
        }
        let entries = workspace.visible_entries(kind);
        let width = usize::from(area.width);
        let size_width = 9usize;
        let version_width = 12usize;
        let note_reserve = entries
            .iter()
            .filter_map(|entry| self.trailing_note(entry))
            .map(|(text, _)| widgets::width(&text) + 1)
            .max()
            .unwrap_or(0)
            .min(width / 3);
        let name_width = width
            .saturating_sub(3 + version_width + size_width + note_reserve)
            .clamp(12, 34);
        let selected = workspace.selected.min(entries.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Workspace,
            RowLayout::list_bare(area, entries.len(), selected),
        );
        self.hitmap
            .scroll(area, crate::tui::action::ScrollTarget::WorkspaceRows);
        for (visual, index) in (layout.offset()..entries.len())
            .take(layout.visible_rows())
            .enumerate()
        {
            let entry = &entries[index];
            let y = area.y.saturating_add(visual as u16);
            let active = index == selected;
            let background = if active { pal.selection } else { pal.surface };
            let name_color = if active { pal.text } else { pal.muted };
            let mut spans = vec![
                Span::styled(
                    if active { "\u{258e} " } else { "  " },
                    Style::default().fg(pal.accent).bg(background),
                ),
                Span::styled(
                    if entry.enabled {
                        "\u{25cf} "
                    } else {
                        "\u{25cb} "
                    },
                    Style::default()
                        .fg(if entry.enabled { pal.ok } else { pal.faint })
                        .bg(background),
                ),
                Span::styled(
                    fit_cell(&entry.name, name_width),
                    Style::default().fg(name_color).bg(background),
                ),
                Span::styled(
                    fit_cell(&entry.version, version_width),
                    Style::default().fg(pal.muted).bg(background),
                ),
                Span::styled(
                    right_cell(&format_size(entry.size), size_width),
                    Style::default().fg(pal.faint).bg(background),
                ),
            ];
            if let Some((note, warning)) = self.trailing_note(entry) {
                let room = width.saturating_sub(3 + name_width + version_width + size_width);
                spans.push(Span::styled(
                    format!(" {}", fit_cell(&note, room)),
                    Style::default()
                        .fg(if warning { pal.err } else { pal.accent2 })
                        .bg(background),
                ));
            }
            let rect = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                rect,
            );
            self.hitmap.add(rect, Action::workspace_row(index));
            // The mark is its own target: clicking it toggles instead of
            // selecting, which is what a checkbox-looking glyph promises.
            self.hitmap.add(
                Rect::new(rect.x.saturating_add(2), y, 1, 1),
                Action::workspace_toggle(index),
            );
        }
    }

    /// Launch settings: label, then `\u{2039} value \u{203a}` on the rows whose
    /// value the arrow keys can change. The arrows are drawn and registered
    /// from the same calculation, so they cannot drift apart.
    fn workspace_launch_list(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        pal: &Palette,
    ) {
        use crate::tui::app::view::workspace::LaunchFieldKind;

        if area.is_empty() {
            return;
        }
        let rows = self.launch_rows();
        if rows.is_empty() {
            return;
        }
        let selected = workspace.selected.min(rows.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Workspace,
            RowLayout::list_bare(area, rows.len(), selected),
        );
        self.hitmap
            .scroll(area, crate::tui::action::ScrollTarget::WorkspaceRows);
        let label_width = 18usize.min(usize::from(area.width) / 2);
        for (visual, index) in (layout.offset()..rows.len())
            .take(layout.visible_rows())
            .enumerate()
        {
            let row = &rows[index];
            let y = area.y.saturating_add(visual as u16);
            let active = index == selected;
            let background = if active { pal.selection } else { pal.surface };
            let mut spans = vec![
                Span::styled(
                    if active { "\u{258e} " } else { "  " },
                    Style::default().fg(pal.accent).bg(background),
                ),
                Span::styled(
                    fit_cell(&row.label, label_width),
                    Style::default()
                        .fg(if active { pal.text } else { pal.muted })
                        .bg(background),
                ),
            ];
            let value = if row.enabled {
                row.value.clone()
            } else {
                row.disabled_reason.clone()
            };
            let adjustable = row.enabled && matches!(row.kind, LaunchFieldKind::Number { .. });
            let room = usize::from(area.width)
                .saturating_sub(2 + label_width)
                .saturating_sub(if adjustable { 4 } else { 0 });
            if adjustable {
                spans.push(Span::styled(
                    "\u{2039} ",
                    Style::default().fg(pal.faint).bg(background),
                ));
                spans.push(Span::styled(
                    crate::tui::palette::truncate(&value, room),
                    Style::default()
                        .fg(if row.following {
                            pal.muted
                        } else {
                            pal.accent2
                        })
                        .bg(background),
                ));
                spans.push(Span::styled(
                    " \u{203a}",
                    Style::default().fg(pal.faint).bg(background),
                ));
            } else {
                spans.push(Span::styled(
                    crate::tui::palette::truncate(&value, room),
                    Style::default()
                        .fg(if row.enabled { pal.muted } else { pal.faint })
                        .bg(background),
                ));
            }
            let rect = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                rect,
            );
            self.hitmap.add(rect, Action::workspace_row(index));
            if adjustable {
                let value_width =
                    crate::tui::palette::width(&crate::tui::palette::truncate(&value, room));
                let left = rect.x.saturating_add(2 + label_width as u16);
                self.hitmap
                    .add(Rect::new(left, y, 2, 1), Action::workspace_step(index, -1));
                self.hitmap.add(
                    Rect::new(left.saturating_add(2 + value_width as u16), y, 2, 1),
                    Action::workspace_step(index, 1),
                );
            }
        }
    }

    fn workspace_value_list(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        rows: &[(String, String)],
        pal: &Palette,
    ) {
        if area.is_empty() {
            return;
        }
        let selected = workspace.selected.min(rows.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Workspace,
            RowLayout::list_bare(area, rows.len(), selected),
        );
        self.hitmap
            .scroll(area, crate::tui::action::ScrollTarget::WorkspaceRows);
        // As wide as the longest label on screen needs, capped at half the card:
        // a fixed column truncates rows like "Repair missing files" for no
        // reason on categories whose values are empty.
        let label_width = rows
            .iter()
            .map(|(label, _)| widgets::width(label) + 2)
            .max()
            .unwrap_or(18)
            .clamp(12, usize::from(area.width) / 2) as u16;
        for (visual, index) in (layout.offset()..rows.len())
            .take(layout.visible_rows())
            .enumerate()
        {
            let (label, value) = &rows[index];
            let y = area.y.saturating_add(visual as u16);
            let active = index == selected;
            let background = if active { pal.selection } else { pal.surface };
            let mut spans = vec![
                Span::styled(
                    if active { "\u{258e} " } else { "  " },
                    Style::default().fg(pal.accent).bg(background),
                ),
                Span::styled(
                    widgets::pad_right(label, label_width),
                    Style::default().fg(pal.muted).bg(background),
                ),
            ];
            let value_width = usize::from(area.width).saturating_sub(2 + usize::from(label_width));
            spans.push(Span::styled(
                crate::tui::palette::truncate(value, value_width),
                Style::default()
                    .fg(if active { pal.text } else { pal.muted })
                    .bg(background),
            ));
            let rect = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                rect,
            );
            // The manage list is a menu: a click runs the command (destructive
            // ones still go through their confirmation). Every other list is a
            // selection, where a click only moves the cursor.
            let action = if workspace.category == WorkspaceCategory::Manage {
                Action::Page {
                    page: Page::Instances,
                    act: PageAct::WorkspaceRun(index),
                }
            } else {
                Action::workspace_row(index)
            };
            self.hitmap.add(rect, action);
        }
    }

    /// The shared note strip: the meaning of the selection, then the keys that
    /// act on this screen.
    fn workspace_strip(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        pal: &Palette,
    ) {
        if area.is_empty() {
            return;
        }
        let (note, warning) = self.workspace_note(workspace);
        let actions = self.workspace_action_line(workspace);
        let width = usize::from(area.width).saturating_sub(2);
        // The key line is pinned to the bottom of the card. When there is no
        // description it stays there and the row above stays empty, instead of
        // the whole strip sliding up and leaving a gap underneath.
        if !note.is_empty() && area.height >= 2 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("  {}", crate::tui::palette::truncate(&note, width)),
                    Style::default()
                        .fg(if warning { pal.err } else { pal.faint })
                        .bg(pal.surface),
                ))),
                Rect::new(area.x, area.bottom().saturating_sub(2), area.width, 1),
            );
        }
        if !actions.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("  {}", crate::tui::palette::truncate(&actions, width)),
                    Style::default().fg(pal.faint).bg(pal.surface),
                ))),
                Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
            );
        }
    }

    /// Label/value rows for the current category.
    ///
    /// Everything except the content lists comes straight from the shared
    /// model: `workspace_display_rows` is what the other two presets render,
    /// so a row that appears there appears here, and one that does not cannot
    /// be invented on this side. Content lists are built here because they
    /// carry the enable mark and the size, which the shared model does not
    /// format.
    fn workspace_rows(&self, workspace: &Workspace) -> Vec<(String, String)> {
        let Some(kind) = workspace.category.content_kind() else {
            return self.workspace_display_rows();
        };
        workspace
            .visible_entries(kind)
            .into_iter()
            .map(|entry| {
                let mark = if entry.enabled { "●" } else { "○" };
                let badge = entry.badge.clone().unwrap_or_default();
                let value = if badge.is_empty() {
                    format!("{mark} {}", format_size(entry.size))
                } else {
                    format!("{mark} {badge}  {}", format_size(entry.size))
                };
                (entry.name.clone(), value)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::tui::app::state::data_format::format_size;

    #[test]
    fn sizes_use_a_readable_unit() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    }
}
