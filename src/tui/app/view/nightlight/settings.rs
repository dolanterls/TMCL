//! Settings page: category rail on the left, one-line rows on the right, and
//! the description of the selected row pinned to the bottom.
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::tui::action::{Action, PageAct};
use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{self, card, card_inner, table_head};
use crate::tui::hit::RowKind;
use crate::tui::pages::settings::SettingsCategory;
use crate::tui::palette::{fit, truncate};
use crate::tui::widgets::rows::RowLayout;
use crate::tui::widgets::settings::SettingKind;

use super::{App, Page};

const RAIL_WIDTH: u16 = 22;
const RAIL_MIN_WIDTH: u16 = 88;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::app) struct SettingsLayout {
    pub rail: Rect,
    pub rows: Rect,
    pub description: Rect,
}

pub(in crate::tui::app) fn layout(area: Rect) -> SettingsLayout {
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    if area.width < RAIL_MIN_WIDTH {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .spacing(1)
            .split(inner);
        return SettingsLayout {
            rail: Rect::default(),
            rows: rows[0],
            description: rows[1],
        };
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(RAIL_WIDTH), Constraint::Min(0)])
        .spacing(1)
        .split(inner);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .spacing(1)
        .split(columns[1]);
    SettingsLayout {
        rail: columns[0],
        rows: rows[0],
        description: rows[1],
    }
}

impl App {
    pub(in crate::tui::app) fn draw_nightlight_settings(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        let layout = layout(area);
        if !layout.rail.is_empty() {
            self.nightlight_settings_rail(frame, layout.rail, pal);
        }
        match self.settings.category {
            SettingsCategory::Java => self.nightlight_java_table(frame, layout.rows, pal),
            SettingsCategory::GameDirectories => {
                self.nightlight_directory_table(frame, layout.rows, pal);
            }
            _ => self.settings_rows_panel(frame, layout.rows, pal),
        }
        self.settings_description(frame, layout.description, pal);
    }

    fn nightlight_settings_rail(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.is_empty() {
            return;
        }
        frame.render_widget(Block::default().style(Style::default().bg(pal.bg)), area);
        // The rail has no card of its own, but its first entry must line up with
        // the first row of the settings card beside it, which starts below that
        // card's title.
        let area = Rect {
            y: area
                .y
                .saturating_add(crate::tui::app::view::nightlight::widgets::CARD_PAD_TOP),
            height: area
                .height
                .saturating_sub(crate::tui::app::view::nightlight::widgets::CARD_PAD_TOP),
            ..area
        };
        let selected = self.settings.category.index();
        for (index, category) in SettingsCategory::ALL.into_iter().enumerate() {
            let y = area.y.saturating_add(index as u16);
            if y >= area.bottom() {
                break;
            }
            // The shared counter knows that the Java and directory categories
            // are lists of detected things, not static setting rows.
            let count = self.settings_category_count(category);
            let label = self.i18n.text(category.title_key());
            let active = index == selected;
            let background = if active { pal.accent } else { pal.bg };
            let mut spans = vec![
                Span::styled(" ", Style::default().bg(background)),
                Span::styled(
                    truncate(&label, usize::from(area.width).saturating_sub(6)),
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
            let counter = count.to_string();
            let used = 2 + widgets::width(&label) + widgets::width(&counter);
            if usize::from(area.width) > used {
                spans.push(Span::styled(
                    " ".repeat(usize::from(area.width) - used),
                    Style::default().bg(background),
                ));
            }
            spans.push(Span::styled(
                counter,
                Style::default()
                    .fg(if active { pal.bg } else { pal.faint })
                    .bg(background),
            ));
            let rect = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                rect,
            );
            self.hitmap.add(rect, Action::category(index));
        }
    }

    /// `Java runtimes`: the same list the other two presets render, in the
    /// nightlight table style. The category cannot use the plain settings rows,
    /// because those come from `settings_rows()` and do not include it.
    fn nightlight_java_table(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        let inner = card(frame, area, &self.i18n.text("java.runtime"), pal);
        if inner.is_empty() {
            return;
        }
        if self.java.is_empty() {
            frame.render_widget(
                Paragraph::new(self.i18n.text("java.empty"))
                    .style(Style::default().fg(pal.muted).bg(pal.surface)),
                inner,
            );
            return;
        }
        let widths = column_widths(inner.width, 16, 8, 14);
        frame.render_widget(
            Paragraph::new(table_head(
                &[
                    &self.i18n.text("overview.status"),
                    &self.i18n.text("java.major"),
                    &self.i18n.text("java.version"),
                    &self.i18n.text("java.path"),
                ],
                &widths,
                pal,
            )),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let rows = Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            inner.height.saturating_sub(1),
        );
        let selected = self.java_selected.min(self.java.len().saturating_sub(1));
        let layout = RowLayout::list_bare(rows, self.java.len(), selected);
        self.hitmap.rows(layout, RowKind::Java);
        for (visual, index) in (layout.offset()..self.java.len())
            .take(layout.visible_rows())
            .enumerate()
        {
            let Some(java) = self.java.get(index) else {
                continue;
            };
            let y = rows.y.saturating_add(visual as u16);
            let active = index == selected;
            let background = if active { pal.selection } else { pal.surface };
            let current = self.config.java.selected_path.as_ref() == Some(&java.executable);
            let status = if current {
                self.i18n.text("java.selected")
            } else {
                self.i18n.text("java.available")
            };
            let mark = if current || java.is_jdk {
                "\u{25cf}"
            } else {
                "\u{25cb}"
            };
            let mark_color = if current || java.is_jdk {
                pal.ok
            } else {
                pal.faint
            };
            let mut spans = vec![
                Span::styled(
                    if active { "\u{258e} " } else { "  " },
                    Style::default().fg(pal.accent).bg(background),
                ),
                Span::styled(
                    format!("{mark} "),
                    Style::default().fg(mark_color).bg(background),
                ),
                Span::styled(
                    fit(&status, usize::from(widths[0]).saturating_sub(2)),
                    Style::default()
                        .fg(if active { pal.text } else { pal.muted })
                        .bg(background),
                ),
                Span::styled(
                    fit(&format!("Java {}", java.major), usize::from(widths[1])),
                    Style::default().fg(pal.accent2).bg(background),
                ),
                Span::styled(
                    fit(&java.version, usize::from(widths[2])),
                    Style::default().fg(pal.muted).bg(background),
                ),
                Span::styled(
                    truncate(
                        &crate::core::instance::path::display_path(&java.executable),
                        usize::from(widths[3]),
                    ),
                    Style::default().fg(pal.faint).bg(background),
                ),
            ];
            let rect = Rect::new(rows.x, y, rows.width, 1);
            spans.shrink_to_fit();
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                rect,
            );
        }
    }

    /// `Game directories`: the add/import actions first, then the configured
    /// roots with their instance counts. Same empty-card problem as the Java
    /// list above.
    fn nightlight_directory_table(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        let inner = card(
            frame,
            area,
            &self.i18n.text("settings.category.game_directories"),
            pal,
        );
        if inner.is_empty() {
            return;
        }
        let widths = column_widths(inner.width, 0, 10, 12);
        frame.render_widget(
            Paragraph::new(table_head(
                &[
                    &self.i18n.text("settings.directory"),
                    &self.i18n.text("settings.instances"),
                    &self.i18n.text("settings.state"),
                ],
                &[widths[0], widths[1], widths[2], 0],
                pal,
            )),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let mut rows: Vec<(String, String, String)> = vec![
            (
                format!("\u{ff0b} {}", self.i18n.text("settings.add_game_directory")),
                String::new(),
                String::new(),
            ),
            (
                format!(
                    "\u{21e3} {}",
                    self.i18n.text("settings.import_game_directory")
                ),
                String::new(),
                String::new(),
            ),
        ];
        for (index, path) in self.config.game_directories.iter().enumerate() {
            let count = self.game_directory_instance_count(path);
            rows.push((
                crate::core::instance::path::display_path(path),
                self.i18n.format(
                    "settings.directory_instances",
                    &[("count", &count.to_string())],
                ),
                if index == 0 {
                    format!("\u{25cf} {}", self.i18n.text("settings.current_directory"))
                } else {
                    String::new()
                },
            ));
        }
        let list = Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            inner.height.saturating_sub(1),
        );
        let selected = self.settings.selected.min(rows.len().saturating_sub(1));
        let layout = RowLayout::list_bare(list, rows.len(), selected);
        self.hitmap.rows(layout, RowKind::Directories);
        for (visual, index) in (layout.offset()..rows.len())
            .take(layout.visible_rows())
            .enumerate()
        {
            let (path, count, state) = &rows[index];
            let y = list.y.saturating_add(visual as u16);
            let active = index == selected;
            let background = if active { pal.selection } else { pal.surface };
            let mut spans = vec![
                Span::styled(
                    if active { "\u{258e} " } else { "  " },
                    Style::default().fg(pal.accent).bg(background),
                ),
                Span::styled(
                    fit(path, usize::from(widths[0]).saturating_sub(2)),
                    Style::default()
                        .fg(if index < 2 {
                            pal.accent
                        } else if active {
                            pal.text
                        } else {
                            pal.muted
                        })
                        .bg(background),
                ),
                Span::styled(
                    fit(count, usize::from(widths[1])),
                    Style::default().fg(pal.faint).bg(background),
                ),
                Span::styled(
                    fit(state, usize::from(widths[2])),
                    Style::default().fg(pal.ok).bg(background),
                ),
            ];
            spans.shrink_to_fit();
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                Rect::new(list.x, y, list.width, 1),
            );
        }
    }

    fn settings_rows_panel(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.is_empty() {
            return;
        }
        if self.settings_search_active() {
            self.settings_search_panel(frame, area, pal);
            return;
        }
        card(
            frame,
            area,
            &self.i18n.text(self.settings.category.title_key()),
            pal,
        );
        let inner = card_inner(area);
        if inner.is_empty() {
            return;
        }
        let rows = self.settings_rows();
        let total = rows.len();
        // Keep the selection inside the window that fits.
        let visible = usize::from(inner.height).max(1);
        self.hitmap.scroll(
            inner,
            crate::tui::action::ScrollTarget::Rows(RowKind::Settings),
        );
        let offset = self
            .settings
            .selected
            .saturating_sub(visible.saturating_sub(1));
        for (visual, index) in (offset..total).take(visible).enumerate() {
            let Some(row) = rows.get(index) else {
                break;
            };
            let y = inner.y.saturating_add(visual as u16);
            let active = index == self.settings.selected;
            let background = if active { pal.selection } else { pal.surface };
            let value_width = widgets::width(&row.value) + 4;
            let title_width = usize::from(inner.width)
                .saturating_sub(value_width + 3)
                .max(6);
            let mut spans = vec![
                Span::styled(
                    if active { "▎ " } else { "  " },
                    Style::default().fg(pal.accent).bg(background),
                ),
                Span::styled(
                    truncate(&row.title, title_width),
                    Style::default()
                        .fg(if !row.enabled {
                            pal.faint
                        } else if active {
                            pal.text
                        } else {
                            pal.muted
                        })
                        .bg(background),
                ),
            ];
            // Same hierarchy as the other two presets: the value is the
            // actionable part, so it carries the accent and the label stays
            // secondary. Writing the value in `bg` hid it inside the row.
            let value_color = if !row.enabled {
                pal.faint
            } else if active {
                pal.text
            } else {
                pal.accent
            };
            let value = value_text(row, pal, value_color);
            let value_cells = u16::try_from(widgets::width(&value)).unwrap_or(0);
            push_right(
                &mut spans,
                Span::styled(
                    value.clone(),
                    Style::default().fg(value_color).bg(background),
                ),
                usize::from(inner.width),
                background,
            );
            let rect = Rect::new(inner.x, y, inner.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                rect,
            );
            self.hitmap.add(rect, Action::row(Page::Settings, index));
            if row.kind == SettingKind::Choice && row.enabled && value_cells > 0 {
                // Push-right aligns the value with the row's right edge, so the
                // arrows are the first and last cells of that string. Deriving
                // the hit boxes the same way the text is placed keeps them on
                // the glyphs instead of a guess at the column.
                let left = rect.right().saturating_sub(value_cells);
                for (by, x) in [(-1i8, left), (1, left + value_cells - 1)] {
                    self.hitmap.add(
                        Rect::new(x, y, 1, 1),
                        Action::Page {
                            page: Page::Settings,
                            act: PageAct::Step { row: index, by },
                        },
                    );
                }
            }
        }
    }

    /// Search surface: the query row on top, cross-category hits underneath.
    ///
    /// `settings_row_count()` already reports `settings_search_results().len()`
    /// while search is active, so the keyboard index walks the hits. Painting
    /// the current category instead pointed the highlight at rows that were not
    /// on screen; this panel draws exactly the list the index moves through.
    fn settings_search_panel(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        card(frame, area, &self.i18n.text("settings.search_title"), pal);
        let inner = card_inner(area);
        if inner.is_empty() {
            return;
        }
        let query = self.settings_search.as_deref().unwrap_or_default();
        let focused = matches!(
            self.search.as_ref(),
            Some(state) if state.target == crate::tui::input::SearchTarget::Settings
        );
        let cursor = self
            .search
            .as_ref()
            .filter(|state| state.target == crate::tui::input::SearchTarget::Settings)
            .map_or_else(|| query.chars().count(), |state| state.cursor);
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
                    &self.i18n.text("settings.search_placeholder"),
                    usize::from(inner.width).saturating_sub(4),
                ),
                Style::default().fg(pal.faint).bg(pal.surface),
            ));
        } else {
            spans.extend(
                crate::tui::text::render(
                    query,
                    cursor,
                    inner.width.saturating_sub(4),
                    focused,
                    crate::tui::text::blink_on(self.anim_time()),
                    style,
                    "",
                )
                .spans,
            );
        }
        let query_area = Rect::new(inner.x, inner.y, inner.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(pal.surface)),
            query_area,
        );
        self.hitmap.add(
            query_area,
            Action::search(crate::tui::input::SearchTarget::Settings),
        );

        let list = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2),
        );
        if list.is_empty() {
            return;
        }
        self.hitmap.scroll(
            list,
            crate::tui::action::ScrollTarget::Rows(RowKind::Settings),
        );
        let results = self.settings_search_results();
        if results.is_empty() {
            frame.render_widget(
                Paragraph::new(truncate(
                    &self.i18n.text("settings.empty"),
                    usize::from(list.width),
                ))
                .style(Style::default().fg(pal.muted).bg(pal.surface)),
                list,
            );
            return;
        }
        let selected = self.settings.selected.min(results.len() - 1);
        let layout = RowLayout::list_bare(list, results.len(), selected);
        let start = layout.offset();
        for (visual, index) in (start..results.len())
            .take(layout.visible_rows())
            .enumerate()
        {
            let Some(hit) = results.get(index) else {
                continue;
            };
            let y = list.y.saturating_add(visual as u16);
            let active = index == selected;
            let background = if active { pal.selection } else { pal.surface };
            let title = format!(
                "{} · {}",
                self.i18n.text(hit.category.title_key()),
                hit.row.title
            );
            let value_color = if hit.row.enabled {
                if active { pal.text } else { pal.accent }
            } else {
                pal.faint
            };
            let value = value_text(&hit.row, pal, value_color);
            let mut spans = vec![
                Span::styled(
                    if active { "▎ " } else { "  " },
                    Style::default().fg(pal.accent).bg(background),
                ),
                Span::styled(
                    truncate(&title, usize::from(list.width).saturating_sub(4)),
                    Style::default()
                        .fg(if active { pal.text } else { pal.muted })
                        .bg(background),
                ),
            ];
            push_right(
                &mut spans,
                Span::styled(value, Style::default().fg(value_color).bg(background)),
                usize::from(list.width),
                background,
            );
            let rect = Rect::new(list.x, y, list.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
                rect,
            );
            self.hitmap.add(rect, Action::row(Page::Settings, index));
        }
    }

    fn settings_description(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.is_empty() {
            return;
        }
        frame.render_widget(Block::default().style(Style::default().bg(pal.bg)), area);
        // While searching, the selection indexes the cross-category hits, not
        // the current category's rows: describe the row that is highlighted.
        let row = if self.settings_search_active() {
            self.settings_search_results()
                .get(self.settings.selected)
                .map(|hit| hit.row.clone())
        } else {
            self.current_setting_row()
        };
        let Some(row) = row else {
            return;
        };
        let text = if row.enabled {
            row.description.clone()
        } else {
            row.disabled_reason.clone()
        };
        frame.render_widget(
            Paragraph::new(truncate(&text, usize::from(area.width)))
                .style(Style::default().fg(pal.muted).bg(pal.bg)),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }
}

/// Column widths for a table: `head` when it is non-zero, otherwise whatever
/// the fixed columns leave over.
fn column_widths(width: u16, head: u16, second: u16, third: u16) -> [u16; 4] {
    let fixed = second.saturating_add(third);
    let first = if head == 0 {
        width.saturating_sub(fixed)
    } else {
        head
    };
    [
        first,
        second,
        third,
        width.saturating_sub(first.saturating_add(fixed)),
    ]
}

fn value_text(
    row: &crate::tui::widgets::settings::SettingRow,
    _pal: &Palette,
    _color: ratatui::style::Color,
) -> String {
    match row.kind {
        SettingKind::Choice => format!("‹ {} ›", row.value),
        SettingKind::Toggle => format!(
            "{} {}",
            if row.value_on == Some(true) {
                "●"
            } else {
                "○"
            },
            row.value
        ),
        SettingKind::Action => format!("{} ▶", row.value),
        _ => row.value.clone(),
    }
}

fn push_right(
    spans: &mut Vec<Span<'static>>,
    right: Span<'static>,
    width: usize,
    bg: ratatui::style::Color,
) {
    let used = spans
        .iter()
        .map(|span| widgets::width(&span.content))
        .sum::<usize>()
        + widgets::width(&right.content);
    if width > used {
        spans.push(Span::styled(
            " ".repeat(width - used),
            Style::default().bg(bg),
        ));
    }
    spans.push(right);
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::layout;

    #[test]
    fn the_settings_page_splits_rail_rows_and_description() {
        let grid = layout(Rect::new(0, 0, 93, 38));
        assert_eq!(grid.rail.width, 22);
        assert!(grid.rows.x > grid.rail.right());
        assert_eq!(grid.rows.bottom(), 35);
        assert_eq!(grid.description.height, 1);
        assert_eq!(grid.description.bottom(), 37);
    }

    #[test]
    fn a_narrow_page_drops_the_rail() {
        let grid = layout(Rect::new(0, 0, 80, 24));
        assert!(grid.rail.is_empty());
        assert!(grid.rows.width > 70);
        assert_eq!(grid.description.bottom(), 23);
    }
}
