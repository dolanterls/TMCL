//! `shelf` settings: a category line, one column of rows, a footnote.
//!
//! Values sit on the right edge; the ones that answer to `←/→` are wrapped in
//! `‹ ›` so the glyphs and the click targets are the same two cells. The
//! footnote explains the highlighted row and prints its key, because a hint
//! inside the row would move with the selection.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::tone::Palette;
use super::widgets;
use super::*;
use crate::core::instance::path::display_path;
use crate::tui::action::Action;
use crate::tui::app::state::data::normalize_instance_path;
use crate::tui::hit::RowKind;
use crate::tui::input::SearchTarget;
use crate::tui::palette;
use crate::tui::widgets::rows::{RowLayout, RowWindow};
use crate::tui::widgets::settings::{SettingKind, SettingRow};

/// Rows the head takes: the category line and its rule.
const HEAD_ROWS: u16 = 2;
/// Rows pinned under the list: a hairline and the footnote.
const FOOT_ROWS: u16 = 2;
/// Columns between the selection bar and the text.
const TEXT_INSET: u16 = 2;

impl App {
    pub(in crate::tui::app) fn draw_shelf_settings(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        if area.width < 24 || area.height < 6 {
            return;
        }
        if self.settings_search_active() {
            self.shelf_settings_search(frame, area, pal);
            return;
        }
        self.shelf_settings_categories(frame, Rect::new(area.x, area.y, area.width, 1), pal);
        widgets::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
            pal,
        );

        let body = Rect::new(
            area.x,
            area.y.saturating_add(HEAD_ROWS),
            area.width,
            area.height.saturating_sub(HEAD_ROWS + FOOT_ROWS),
        );
        if body.height == 0 {
            return;
        }
        match self.settings.category {
            SettingsCategory::Java => self.shelf_java_rows(frame, body, pal),
            SettingsCategory::GameDirectories => self.shelf_directory_rows(frame, body, pal),
            _ => self.shelf_setting_rows(frame, body, pal),
        }
        self.shelf_settings_footnote(frame, area, pal);
    }

    /// The category line: seven names, the current one underlined.
    fn shelf_settings_categories(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        let labels = SettingsCategory::ALL
            .iter()
            .map(|category| self.i18n.text(category.title_key()))
            .collect::<Vec<_>>();
        let current = self.settings.category.index();
        for (index, rect) in widgets::fit_labels(area, &labels, current, area.right()) {
            let style = if index == current {
                Style::default()
                    .fg(pal.accent_light)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(pal.dim).bg(pal.bg)
            };
            let Some(label) = labels.get(index) else {
                continue;
            };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(label, usize::from(rect.width)),
                    style,
                )),
                rect,
            );
            self.hitmap.add(rect, Action::category(index));
        }
    }

    fn shelf_setting_rows(&self, frame: &mut Frame<'_>, body: Rect, pal: &Palette) {
        let rows = self.settings_rows();
        let selected = self.settings.selected.min(rows.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Settings,
            RowLayout::list_bare(body, rows.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::Settings);
        if rows.is_empty() {
            let text = self.i18n.text("settings.empty");
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&text, usize::from(body.width)),
                    Style::default().fg(pal.faint).bg(pal.bg),
                )),
                Rect::new(body.x, body.y, body.width, 1),
            );
            return;
        }
        for (index, rect) in layout.rows() {
            let Some(row) = rows.get(index) else {
                continue;
            };
            self.shelf_setting_row(frame, rect, pal, row, index, index == selected);
        }
    }

    fn shelf_setting_row(
        &self,
        frame: &mut Frame<'_>,
        rect: Rect,
        pal: &Palette,
        setting: &SettingRow,
        index: usize,
        active: bool,
    ) {
        if active {
            widgets::selection_band(frame, rect, pal);
        }
        let background = if active { pal.surface } else { pal.bg };
        let adjustable = setting.enabled
            && matches!(
                setting.kind,
                SettingKind::Choice | SettingKind::Number { .. }
            );
        let value = if let Some(on) = setting.value_on {
            format!("{} {}", if on { "●" } else { "○" }, setting.value)
        } else {
            setting.value.clone()
        };
        // `‹ value ›` is four cells wider than the value itself.
        let block_width = palette::width(&value) as u16 + if adjustable { 4 } else { 0 };
        let label_x = rect.x.saturating_add(TEXT_INSET);
        const MIN_LABEL: u16 = 12;
        let block_x = rect
            .right()
            .saturating_sub(block_width.saturating_add(1))
            .max(label_x.saturating_add(MIN_LABEL).saturating_add(2));
        let block_width = block_width.min(rect.right().saturating_sub(block_x));
        let label_width = block_x.saturating_sub(label_x).saturating_sub(2);

        let label_style = if !setting.enabled {
            Style::default().fg(pal.faint).bg(background)
        } else if active {
            Style::default()
                .fg(pal.text)
                .bg(background)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(pal.dim).bg(background)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                palette::truncate(&setting.title, usize::from(label_width)),
                label_style,
            )),
            Rect::new(label_x, rect.y, label_width, 1),
        );

        let value_style = if !setting.enabled {
            Style::default().fg(pal.faint).bg(background)
        } else if active {
            Style::default().fg(pal.text).bg(background)
        } else if setting.kind == SettingKind::Action {
            Style::default().fg(pal.accent).bg(background)
        } else {
            Style::default().fg(pal.dim).bg(background)
        };
        if !adjustable {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&value, usize::from(block_width)),
                    value_style,
                ))
                .alignment(Alignment::Right),
                Rect::new(block_x, rect.y, block_width, 1),
            );
            return;
        }

        let value_width = block_width.saturating_sub(4);
        frame.render_widget(
            Paragraph::new(Span::styled(
                "‹",
                Style::default().fg(pal.accent).bg(background),
            )),
            Rect::new(block_x, rect.y, 1, 1),
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                palette::truncate(&value, usize::from(value_width)),
                value_style,
            )),
            Rect::new(block_x.saturating_add(2), rect.y, value_width, 1),
        );
        self.hitmap
            .add(Rect::new(block_x, rect.y, 1, 1), Action::step(index, -1));
        let close_x = block_x.saturating_add(3).saturating_add(value_width);
        if close_x < rect.right() {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "›",
                    Style::default().fg(pal.accent).bg(background),
                )),
                Rect::new(close_x, rect.y, 1, 1),
            );
            self.hitmap
                .add(Rect::new(close_x, rect.y, 1, 1), Action::step(index, 1));
        }
    }

    /// The footnote: what the highlighted row does, and the key that does it.
    fn shelf_settings_footnote(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.height < FOOT_ROWS {
            return;
        }
        let rule_y = area.bottom().saturating_sub(FOOT_ROWS);
        widgets::rule(frame, Rect::new(area.x, rule_y, area.width, 1), pal);
        let Some((description, hint)) = self.shelf_footnote_text() else {
            return;
        };
        let hint_width = palette::width(&hint) as u16;
        let text_width = area.width.saturating_sub(hint_width.saturating_add(3));
        let row = Rect::new(area.x, rule_y.saturating_add(1), area.width, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(
                palette::truncate(&description, usize::from(text_width)),
                Style::default().fg(pal.faint).bg(pal.bg),
            )),
            row,
        );
        if hint_width > 0 && hint_width < area.width {
            frame.render_widget(
                Paragraph::new(Span::styled(hint, Style::default().fg(pal.dim).bg(pal.bg)))
                    .alignment(Alignment::Right),
                row,
            );
        }
    }

    /// Description and key hint for whatever is highlighted, including the
    /// search hits (whose selection indexes the flat list, not the category).
    fn shelf_footnote_text(&self) -> Option<(String, String)> {
        let row = if self.settings_search_active() {
            self.settings_search_results()
                .get(self.settings.selected)
                .map(|hit| hit.row.clone())
        } else {
            self.current_setting_row()
        };
        if let Some(row) = row {
            let text = if row.enabled {
                row.description.clone()
            } else {
                row.disabled_reason.clone()
            };
            let hint = if row.enabled {
                row.hint.clone()
            } else {
                String::new()
            };
            return Some((text, hint));
        }
        // The Java and directory lists are not setting rows: describe the
        // highlighted item instead of leaving the band empty.
        match self.settings.category {
            SettingsCategory::Java => {
                let java = self.java.get(self.java_selected)?;
                Some((
                    display_path(&java.executable),
                    java_supported_minecraft(java.major),
                ))
            }
            SettingsCategory::GameDirectories => {
                let path = self.config.game_directories.get(self.settings.selected)?;
                Some((
                    display_path(path),
                    self.i18n.format(
                        "settings.directory_instances",
                        &[(
                            "count",
                            &self.game_directory_instance_count(path).to_string(),
                        )],
                    ),
                ))
            }
            _ => None,
        }
    }

    /// `Java 21 · Temurin 21.0.5` with `已选` on the right.
    fn shelf_java_rows(&self, frame: &mut Frame<'_>, body: Rect, pal: &Palette) {
        let selected = self.java_selected.min(self.java.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Java,
            RowLayout::list_bare(body, self.java.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::Java);
        if self.java.is_empty() {
            let text = self.i18n.text("java.empty");
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&text, usize::from(body.width)),
                    Style::default().fg(pal.faint).bg(pal.bg),
                )),
                Rect::new(body.x, body.y, body.width, 1),
            );
            return;
        }
        for (index, rect) in layout.rows() {
            let Some(java) = self.java.get(index) else {
                continue;
            };
            let active = index == selected;
            if active {
                widgets::selection_band(frame, rect, pal);
            }
            let background = if active { pal.surface } else { pal.bg };
            let current = self.config.java.selected_path.as_ref() == Some(&java.executable);
            let status = if current {
                self.i18n.text("java.selected")
            } else {
                self.i18n.text("java.available")
            };
            let status_width = palette::width(&status) as u16;
            let name_x = rect.x.saturating_add(TEXT_INSET);
            let name_width = rect
                .right()
                .saturating_sub(name_x)
                .saturating_sub(status_width)
                .saturating_sub(2);
            let vendor = java
                .vendor
                .clone()
                .unwrap_or_else(|| self.i18n.text("java.unknown_vendor"));
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(
                        &format!("Java {} · {vendor} {}", java.major, java.version),
                        usize::from(name_width),
                    ),
                    Style::default()
                        .fg(if active { pal.text } else { pal.dim })
                        .bg(background)
                        .add_modifier(if active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )),
                Rect::new(name_x, rect.y, name_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Span::styled(
                    status,
                    Style::default()
                        .fg(if current { pal.ok } else { pal.faint })
                        .bg(background),
                ))
                .alignment(Alignment::Right),
                Rect::new(rect.x, rect.y, rect.width, 1),
            );
        }
    }

    /// The two commands at the top, then one line per game directory.
    fn shelf_directory_rows(&self, frame: &mut Frame<'_>, body: Rect, pal: &Palette) {
        let total = 2 + self.config.game_directories.len();
        let selected = self.settings.selected.min(total.saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Directories,
            RowLayout::list_bare(body, total, selected),
        );
        self.hitmap.rows(layout, RowKind::Directories);
        for (index, rect) in layout.rows() {
            let active = index == selected;
            if active {
                widgets::selection_band(frame, rect, pal);
            }
            let background = if active { pal.surface } else { pal.bg };
            if index < 2 {
                let (mark, key) = if index == 0 {
                    ("＋", "settings.add_game_directory")
                } else {
                    ("⇣", "settings.import_game_directory")
                };
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        format!(
                            " {mark} {}",
                            palette::truncate(
                                &self.i18n.text(key),
                                usize::from(rect.width.saturating_sub(4))
                            )
                        ),
                        Style::default()
                            .fg(if active { pal.accent_light } else { pal.text })
                            .bg(background)
                            .add_modifier(if active {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    )),
                    Rect::new(rect.x, rect.y, rect.width, 1),
                );
                continue;
            }
            let Some(path) = self.config.game_directories.get(index - 2) else {
                continue;
            };
            let count = self.i18n.format(
                "settings.directory_instances",
                &[(
                    "count",
                    &self.game_directory_instance_count(path).to_string(),
                )],
            );
            let count_width = palette::width(&count) as u16;
            let current = self.config.game_directories.first().is_some_and(|first| {
                normalize_instance_path(first) == normalize_instance_path(path)
            });
            // The marker gets its own column. Drawing it at the row's left edge
            // painted over the first characters of the path (`Currentmes/…`);
            // the count and the marker both have to be reserved before the path
            // is measured.
            let marker = if current {
                self.i18n.text("settings.current_directory")
            } else {
                String::new()
            };
            let marker_width = palette::width(&marker) as u16;
            // Only reserve the marker column when there is a marker; a plain row
            // keeps two cells of gap.
            let marker_gap = if marker_width > 0 { 2 } else { 0 };
            let name_x = rect.x.saturating_add(TEXT_INSET);
            let name_width = rect
                .right()
                .saturating_sub(name_x)
                .saturating_sub(count_width)
                .saturating_sub(marker_width)
                .saturating_sub(2 + marker_gap);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&display_path(path), usize::from(name_width)),
                    Style::default()
                        .fg(if active { pal.text } else { pal.dim })
                        .bg(background)
                        .add_modifier(if active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )),
                Rect::new(name_x, rect.y, name_width, 1),
            );
            if current && marker_width > 0 {
                let marker_x = rect
                    .right()
                    .saturating_sub(count_width)
                    .saturating_sub(marker_width)
                    .saturating_sub(2);
                if marker_x > name_x {
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            marker,
                            Style::default().fg(pal.accent).bg(background),
                        )),
                        Rect::new(marker_x, rect.y, marker_width, 1),
                    );
                }
            }
            frame.render_widget(
                Paragraph::new(Span::styled(
                    count,
                    Style::default().fg(pal.faint).bg(background),
                ))
                .alignment(Alignment::Right),
                Rect::new(rect.x, rect.y, rect.width, 1),
            );
        }
    }

    /// `/` swaps the page for the cross-category search list.
    fn shelf_settings_search(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        let target = SearchTarget::Settings;
        let focused = matches!(self.search.as_ref(), Some(state) if state.target == target);
        let query = self.settings_search.as_deref().unwrap_or_default();
        let hits = self.settings_search_results();
        let selected = self.settings.selected.min(hits.len().saturating_sub(1));

        let label = self.i18n.text("common.search");
        let count = self.i18n.format(
            "settings.search_results",
            &[("count", &hits.len().to_string())],
        );
        let count_width = palette::width(&count) as u16;
        let budget = area
            .width
            .saturating_sub(palette::width(&label) as u16 + 2 + count_width + 2);
        let mut spans = vec![
            Span::styled(format!("{label} "), Style::default().fg(pal.dim).bg(pal.bg)),
            Span::styled(
                if query.is_empty() {
                    palette::truncate(
                        &self.i18n.text("settings.search_placeholder"),
                        usize::from(budget.saturating_sub(1)),
                    )
                } else {
                    palette::truncate(query, usize::from(budget.saturating_sub(1)))
                },
                if focused {
                    Style::default()
                        .fg(pal.text)
                        .bg(pal.bg)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else if query.is_empty() {
                    Style::default().fg(pal.faint).bg(pal.bg)
                } else {
                    Style::default().fg(pal.text).bg(pal.bg)
                },
            ),
        ];
        if focused && crate::tui::text::blink_on(self.anim_time()) {
            spans.push(Span::styled(
                "▏",
                Style::default().fg(pal.accent).bg(pal.bg),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, area.y, area.width.saturating_sub(count_width), 1),
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                count,
                Style::default().fg(pal.faint).bg(pal.bg),
            ))
            .alignment(Alignment::Right),
            area,
        );
        self.hitmap.add(
            Rect::new(area.x, area.y, area.width, 1),
            Action::search(target),
        );
        widgets::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
            pal,
        );

        let body = Rect::new(
            area.x,
            area.y.saturating_add(HEAD_ROWS),
            area.width,
            area.height.saturating_sub(HEAD_ROWS + FOOT_ROWS),
        );
        if body.height == 0 {
            return;
        }
        let layout = self.row_windows.resolve(
            RowWindow::Settings,
            RowLayout::list_bare(body, hits.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::Settings);
        if hits.is_empty() {
            let text = self.i18n.text("settings.empty");
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&text, usize::from(body.width)),
                    Style::default().fg(pal.faint).bg(pal.bg),
                )),
                Rect::new(body.x, body.y, body.width, 1),
            );
            self.shelf_settings_footnote(frame, area, pal);
            return;
        }
        for (index, rect) in layout.rows() {
            let Some(hit) = hits.get(index) else {
                continue;
            };
            let active = index == selected;
            if active {
                widgets::selection_band(frame, rect, pal);
            }
            let background = if active { pal.surface } else { pal.bg };
            let label = format!(
                "{} · {}",
                self.i18n.text(hit.category.title_key()),
                hit.row.title
            );
            let value_width = palette::width(&hit.row.value) as u16 + 2;
            let label_width = rect.width.saturating_sub(TEXT_INSET + value_width).max(6);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&label, usize::from(label_width)),
                    Style::default()
                        .fg(if active { pal.text } else { pal.dim })
                        .bg(background),
                )),
                Rect::new(rect.x.saturating_add(TEXT_INSET), rect.y, label_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&hit.row.value, usize::from(value_width)),
                    Style::default()
                        .fg(if active { pal.text } else { pal.faint })
                        .bg(background),
                ))
                .alignment(Alignment::Right),
                rect,
            );
        }
        self.shelf_settings_footnote(frame, area, pal);
    }
}
