//! Settings page: category rail on the left, inline-editable rows on the
//! right, and a description band under the selected row.

use crate::tui::app::view::sheet::Sheet;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::*;
use crate::tui::hit::RowKind;
use crate::tui::palette;
use crate::tui::widgets::kit::{self as widgets, RowData};
use crate::tui::widgets::settings::{SettingKind, SettingRow};

/// Separator + two description lines under the row list.
const DESCRIPTION_ROWS: u16 = 3;

impl App {
    /// Rail and content rectangles. Narrow layouts skip the rail entirely.
    pub(in crate::tui::app) fn creeper_settings_areas(&self, main: Rect) -> (Rect, Rect) {
        if main.width < 76 {
            return (Rect::default(), main);
        }
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(24),
                Constraint::Length(1),
                Constraint::Min(24),
            ])
            .split(main);
        (columns[0], columns[2])
    }

    pub(in crate::tui::app) fn settings_category_count(&self, category: SettingsCategory) -> usize {
        match category {
            SettingsCategory::Java => self.java.len(),
            SettingsCategory::GameDirectories => self.config.game_directories.len(),
            // Counted through the same filtered list the page draws, so a row
            // this preset hides cannot leave the rail claiming one more item
            // than the list shows.
            _ => self.settings_rows_for(category).len(),
        }
    }

    pub(in crate::tui::app) fn draw_creeper_settings(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        t: f64,
    ) {
        let (rail, content) = self.creeper_settings_areas(area);
        if rail.width > 0 {
            self.settings_rail(frame, rail, t);
        }
        if self.settings_search_active() {
            self.settings_search_rows(frame, content, t);
            return;
        }
        match self.settings.category {
            SettingsCategory::Java => self.java_rows(frame, content, t),
            SettingsCategory::GameDirectories => self.directory_rows(frame, content, t),
            _ => self.setting_rows(frame, content, t),
        }
    }

    fn settings_inner(&self, content: Rect) -> Rect {
        let pal = self.creeper_palette();
        widgets::card("", pal.blue, true, 0.0, pal.surface).inner(content)
    }

    fn settings_list_rect(&self, inner: Rect) -> Rect {
        let description = if inner.height >= 8 {
            DESCRIPTION_ROWS.min(inner.height.saturating_sub(1))
        } else {
            0
        };
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(description),
        )
    }

    fn management_offset(&self, count: usize, height: u16) -> usize {
        let selected = match self.settings.category {
            SettingsCategory::Java => self.java_selected,
            _ => self.settings.selected,
        };
        management_offset(count, selected, height)
    }

    fn settings_rail(&self, frame: &mut Frame<'_>, area: Rect, t: f64) {
        let pal = self.creeper_palette();
        let block = widgets::card(
            &self.i18n.text("page.settings"),
            pal.blue,
            false,
            t,
            pal.surface,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        for (index, category) in SettingsCategory::ALL.iter().enumerate() {
            let y = inner.y.saturating_add(index as u16);
            if y >= inner.bottom() {
                break;
            }
            let active = *category == self.settings.category;
            let bg = if active {
                pal.fill(pal.blue, 0.2)
            } else {
                pal.surface
            };
            let count = self.settings_category_count(*category);
            let count = if count == 0 {
                String::new()
            } else {
                count.to_string()
            };
            let count_width = palette::width(&count);
            let label_width = usize::from(inner.width)
                .saturating_sub(count_width + 4)
                .max(4);
            let label = palette::truncate(&self.i18n.text(category.title_key()), label_width);
            let pad = " ".repeat(label_width.saturating_sub(palette::width(&label)));
            let row = Rect::new(inner.x, y, inner.width, 1);
            self.hitmap.add(row, Action::category(index));
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        if active { "▌ " } else { "  " },
                        Style::default()
                            .fg(palette::mix(pal.blue, pal.text, 0.15))
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{label}{pad} "),
                        Style::default()
                            .fg(if active { pal.text } else { pal.text_dim })
                            .bg(bg)
                            .add_modifier(if active {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(count, Style::default().fg(pal.text_faint).bg(bg)),
                ]))
                .style(Style::default().bg(bg)),
                row,
            );
        }
        if inner.height > SettingsCategory::ALL.len() as u16 + 1 && !self.settings_search_active() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(
                        " {}",
                        palette::truncate(
                            &self.i18n.text("settings.rail_hint"),
                            usize::from(inner.width).saturating_sub(2)
                        )
                    ),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                ))),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            );
        }
    }

    /// Key/value rows: one line per setting, inline value, selected-row hint
    /// and a description band at the bottom of the card.
    fn setting_rows(&self, frame: &mut Frame<'_>, area: Rect, t: f64) {
        let pal = self.creeper_palette();
        let rows = self.settings_rows();
        if rows.is_empty() {
            widgets::empty_card(
                frame,
                area,
                &self.i18n.text(self.settings.category.title_key()),
                &self.i18n.text("settings.empty"),
                pal.blue,
                t,
            );
            return;
        }
        let selected = self.settings.selected.min(rows.len() - 1);
        let title = self.i18n.text(self.settings.category.title_key());
        let mut block = widgets::card(&title, pal.blue, true, t, pal.surface)
            .title_bottom(Line::from(format!("{}/{}", selected + 1, rows.len())).right_aligned());
        let _ = &mut block;
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }

        let list = self.settings_list_rect(inner);
        let (offset, visible) = settings_window(rows.len(), selected, list.height);
        for (visual, row) in rows
            .iter()
            .skip(offset)
            .take(visible.min(usize::from(list.height)))
            .enumerate()
        {
            let index = offset + visual;
            let area = Rect::new(list.x, list.y + visual as u16, list.width, 1);
            let value = self.render_setting_row(frame, area, row, index == selected);
            self.hitmap.add(area, Action::row(Page::Settings, index));
            // `‹ ` and ` ›` occupy the two ends of the value area; only rows
            // that offer a choice advertise them.
            if row.enabled && matches!(row.kind, SettingKind::Choice | SettingKind::Number { .. }) {
                self.hitmap.add(
                    Rect::new(value.x, value.y, 2.min(value.width), 1),
                    Action::step(index, -1),
                );
                self.hitmap.add(
                    Rect::new(
                        value.x.saturating_add(value.width.saturating_sub(2)),
                        value.y,
                        2.min(value.width),
                        1,
                    ),
                    Action::step(index, 1),
                );
            }
        }
        if list.height > 0 && rows.len() > visible {
            let mut state =
                crate::tui::app::view::pane_scrollbar_state(rows.len(), visible, offset);
            frame.render_stateful_widget(
                ratatui::widgets::Scrollbar::new(
                    ratatui::widgets::ScrollbarOrientation::VerticalRight,
                )
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .style(Style::default().fg(pal.text_faint).bg(pal.surface)),
                list,
                &mut state,
            );
        }

        if inner.height >= 8 && list.height < inner.height {
            let sep_y = list.bottom();
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "─".repeat(usize::from(inner.width)),
                    Style::default()
                        .fg(palette::mix(pal.surface, pal.text_faint, 0.35))
                        .bg(pal.surface),
                ))),
                Rect::new(inner.x, sep_y, inner.width, 1),
            );
            let mut lines = Vec::new();
            if let Some(row) = rows.get(selected) {
                lines.push(Line::from(vec![
                    Span::styled("◆ ", Style::default().fg(pal.blue).bg(pal.surface)),
                    Span::styled(
                        palette::truncate(
                            &row.description,
                            usize::from(inner.width).saturating_sub(2),
                        ),
                        Style::default().fg(pal.text_dim).bg(pal.surface),
                    ),
                ]));
                lines.push(if !row.enabled {
                    Line::from(vec![
                        Span::styled("! ", Style::default().fg(pal.gold).bg(pal.surface)),
                        Span::styled(
                            palette::truncate(
                                &row.disabled_reason,
                                usize::from(inner.width).saturating_sub(2),
                            ),
                            Style::default().fg(pal.gold).bg(pal.surface),
                        ),
                    ])
                } else {
                    let hint = if row.kind == SettingKind::Action {
                        self.i18n.text("settings.enter_run_hint")
                    } else {
                        self.i18n.text("settings.autosave")
                    };
                    Line::from(vec![
                        Span::styled("  ", Style::default().bg(pal.surface)),
                        Span::styled(
                            palette::truncate(&hint, usize::from(inner.width).saturating_sub(2)),
                            Style::default().fg(pal.text_faint).bg(pal.surface),
                        ),
                    ])
                });
            }
            frame.render_widget(
                Paragraph::new(lines).style(Style::default().bg(pal.surface)),
                Rect::new(inner.x, sep_y.saturating_add(1), inner.width, 2),
            );
        }
    }

    /// Draws one settings row and returns the value-area rectangle, so the
    /// caller can register the `‹ ›` step targets without repeating the
    /// column arithmetic.
    fn render_setting_row(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        row: &SettingRow,
        selected: bool,
    ) -> Rect {
        let pal = self.creeper_palette();
        let bg = if selected {
            pal.fill(pal.blue, 0.18)
        } else {
            pal.surface
        };
        frame.render_widget(Block::default().style(Style::default().bg(bg)), area);
        let title_width = 24.min(usize::from(area.width).saturating_sub(18));
        let hint_width = if selected {
            palette::width(&row.hint).min(usize::from(area.width) / 3)
        } else {
            0
        };
        let value_width = usize::from(area.width)
            .saturating_sub(title_width + 2 + hint_width + 2)
            .max(6);
        let mut spans = vec![
            Span::styled(
                if selected { "▌ " } else { "  " },
                Style::default()
                    .fg(palette::mix(pal.blue, pal.text, 0.15))
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                palette::pad(&palette::truncate(&row.title, title_width), title_width),
                Style::default()
                    .fg(if selected {
                        pal.text
                    } else if row.enabled {
                        pal.text_dim
                    } else {
                        pal.text_faint
                    })
                    .bg(bg)
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled("  ", Style::default().bg(bg)),
        ];
        spans.extend(self.setting_value_spans(row, selected, bg, value_width));
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        if selected && row.enabled && !row.hint.is_empty() {
            spans.push(Span::styled(
                palette::truncate(&row.hint, hint_width),
                Style::default().fg(pal.text_faint).bg(bg),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
            area,
        );
        let value_x = area
            .x
            .saturating_add(u16::try_from(2 + title_width + 2).unwrap_or(u16::MAX));
        // Only the `‹ value ›` span is a target; returning the whole value
        // field would put the right arrow far away from the text.
        let control_width = if row.enabled
            && matches!(row.kind, SettingKind::Choice | SettingKind::Number { .. })
        {
            2 + palette::width(&palette::truncate(
                &row.value,
                value_width.saturating_sub(4),
            )) + 2
        } else {
            value_width
        };
        Rect::new(
            value_x,
            area.y,
            u16::try_from(control_width).unwrap_or(u16::MAX),
            1,
        )
    }

    fn setting_value_spans(
        &self,
        row: &SettingRow,
        selected: bool,
        bg: ratatui::style::Color,
        width: usize,
    ) -> Vec<Span<'static>> {
        let pal = self.creeper_palette();
        if !row.enabled {
            return vec![Span::styled(
                palette::truncate(&row.value, width),
                Style::default().fg(pal.text_faint).bg(bg),
            )];
        }
        let value_style = Style::default()
            .fg(if selected {
                palette::mix(pal.blue, pal.text, 0.25)
            } else {
                pal.text_dim
            })
            .bg(bg)
            .add_modifier(if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        match row.kind {
            SettingKind::Toggle => {
                let on = row.value_on.unwrap_or(false);
                vec![
                    Span::styled(
                        if on { "● " } else { "○ " },
                        Style::default()
                            .fg(if on { pal.green } else { pal.text_faint })
                            .bg(bg),
                    ),
                    Span::styled(
                        palette::truncate(&row.value, width.saturating_sub(2)),
                        Style::default()
                            .fg(if on { pal.green } else { pal.text_faint })
                            .bg(bg),
                    ),
                ]
            }
            SettingKind::Choice | SettingKind::Number { .. } => vec![
                Span::styled("‹ ", Style::default().fg(pal.text_faint).bg(bg)),
                Span::styled(
                    palette::truncate(&row.value, width.saturating_sub(4)),
                    value_style,
                ),
                Span::styled(" ›", Style::default().fg(pal.text_faint).bg(bg)),
            ],
            SettingKind::Action => vec![
                Span::styled("▶ ", Style::default().fg(pal.green).bg(bg)),
                Span::styled(
                    palette::truncate(&row.value, width.saturating_sub(2)),
                    Style::default()
                        .fg(if selected { pal.text } else { pal.text_dim })
                        .bg(bg),
                ),
            ],
            SettingKind::Text => vec![Span::styled(
                palette::truncate(&row.value, width),
                value_style,
            )],
        }
    }

    /// Flat cross-category results with a live query bar. Enter jumps to the
    /// highlighted row and leaves search mode.
    fn settings_search_rows(&self, frame: &mut Frame<'_>, area: Rect, t: f64) {
        let pal = self.creeper_palette();
        let hits = self.settings_search_results();
        let selected = self.settings.selected.min(hits.len().saturating_sub(1));
        let count = hits.len();
        let title = self.i18n.text("settings.search_title");
        let block = widgets::card(&title, pal.cyan, true, t, pal.surface).title_bottom(
            Line::from(if count == 0 {
                String::new()
            } else {
                format!("{}/{}", selected + 1, count)
            })
            .right_aligned(),
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }

        let query = self.settings_search.as_deref().unwrap_or_default();
        let shown = if query.trim().is_empty() {
            self.i18n.text("settings.search_placeholder")
        } else {
            query.to_owned()
        };
        let result_label = self
            .i18n
            .format("settings.search_results", &[("count", &count.to_string())]);
        let right_width = palette::width(&result_label)
            .min(usize::from(inner.width) / 2)
            .min(usize::from(inner.width));
        let left_width = usize::from(inner.width).saturating_sub(right_width + 1);
        // The `⌕` bar is the inline search field of this page: clicking it puts
        // the pointer back on the query, exactly like `/`.
        self.hitmap.add(
            Rect::new(inner.x, inner.y, left_width as u16, 1),
            Action::search(SearchTarget::Settings),
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("⌕ ", Style::default().fg(pal.cyan).bg(pal.surface)),
                Span::styled(
                    palette::truncate(&shown, left_width.saturating_sub(2)),
                    Style::default()
                        .fg(if query.trim().is_empty() {
                            pal.text_faint
                        } else {
                            pal.text
                        })
                        .bg(pal.surface),
                ),
            ]))
            .style(Style::default().bg(pal.surface)),
            Rect::new(inner.x, inner.y, left_width as u16, 1),
        );
        if right_width > 0 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&result_label, right_width),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                )))
                .alignment(ratatui::layout::Alignment::Right)
                .style(Style::default().bg(pal.surface)),
                Rect::new(
                    inner.right().saturating_sub(right_width as u16),
                    inner.y,
                    right_width as u16,
                    1,
                ),
            );
        }

        let list = settings_search_list_rect(inner);
        if hits.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&self.i18n.text("settings.empty"), usize::from(list.width)),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                )))
                .style(Style::default().bg(pal.surface)),
                Rect::new(list.x, list.y, list.width, 1),
            );
        } else {
            let (offset, visible) = settings_window(count, selected, list.height);
            for (visual, hit) in hits
                .iter()
                .skip(offset)
                .take(visible.min(usize::from(list.height)))
                .enumerate()
            {
                let index = offset + visual;
                let mut row = hit.row.clone();
                row.hint.clear();
                row.title = format!(
                    "{} · {}",
                    self.i18n.text(hit.category.title_key()),
                    row.title
                );
                let area = Rect::new(list.x, list.y + visual as u16, list.width, 1);
                let _ = self.render_setting_row(frame, area, &row, index == selected);
                // Search results are rows of their own list; the `‹ ›` targets
                // belong to the category list, so they are not registered here.
                self.hitmap.add(area, Action::row(Page::Settings, index));
            }
            if count > visible {
                let mut state = crate::tui::app::view::pane_scrollbar_state(count, visible, offset);
                frame.render_stateful_widget(
                    ratatui::widgets::Scrollbar::new(
                        ratatui::widgets::ScrollbarOrientation::VerticalRight,
                    )
                    .begin_symbol(Some("↑"))
                    .end_symbol(Some("↓"))
                    .track_symbol(Some("│"))
                    .thumb_symbol("█")
                    .style(Style::default().fg(pal.text_faint).bg(pal.surface)),
                    list,
                    &mut state,
                );
            }
        }

        if inner.height >= 8 && list.height < inner.height {
            let sep_y = list.bottom();
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "─".repeat(usize::from(inner.width)),
                    Style::default()
                        .fg(palette::mix(pal.surface, pal.text_faint, 0.35))
                        .bg(pal.surface),
                ))),
                Rect::new(inner.x, sep_y, inner.width, 1),
            );
            let text = hits
                .get(selected)
                .map(|hit| hit.row.description.clone())
                .unwrap_or_else(|| self.i18n.text("settings.search_jump"));
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("◆ ", Style::default().fg(pal.cyan).bg(pal.surface)),
                    Span::styled(
                        palette::truncate(&text, usize::from(inner.width).saturating_sub(2)),
                        Style::default().fg(pal.text_dim).bg(pal.surface),
                    ),
                ]))
                .style(Style::default().bg(pal.surface)),
                Rect::new(inner.x, sep_y.saturating_add(1), inner.width, 1),
            );
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("  ", Style::default().bg(pal.surface)),
                    Span::styled(
                        palette::truncate(
                            &self.i18n.text("settings.search_jump"),
                            usize::from(inner.width).saturating_sub(2),
                        ),
                        Style::default().fg(pal.text_faint).bg(pal.surface),
                    ),
                ]))
                .style(Style::default().bg(pal.surface)),
                Rect::new(inner.x, sep_y.saturating_add(2), inner.width, 1),
            );
        }
    }

    fn java_rows(&self, frame: &mut Frame<'_>, area: Rect, t: f64) {
        let pal = self.creeper_palette();
        if self.java.is_empty() {
            widgets::empty_card(
                frame,
                area,
                &self.i18n.text("java.runtime"),
                &self.i18n.text("java.empty"),
                pal.gold,
                t,
            );
            return;
        }
        let rows = self
            .java
            .iter()
            .map(|java| {
                let selected = self.config.java.selected_path.as_ref() == Some(&java.executable);
                let kind = if selected {
                    StatusKind::Current
                } else if java.is_jdk {
                    StatusKind::Ready
                } else {
                    StatusKind::Warning
                };
                RowData::new([
                    format!(
                        "{} {}",
                        widgets::status_glyph(kind),
                        if selected {
                            self.i18n.text("java.selected")
                        } else {
                            self.i18n.text("java.available")
                        }
                    ),
                    format!("Java {}", java.major),
                    java.version.clone(),
                    display_path(&java.executable),
                ])
                .mark(0, widgets::status_color(kind))
            })
            .collect::<Vec<_>>();
        let inner = self.settings_inner(area);
        let body_height = inner.height.saturating_sub(1);
        let offset = self.management_offset(rows.len(), body_height);
        widgets::render_rows(
            frame,
            area,
            &self.i18n.text("java.runtime"),
            pal.gold,
            &[
                self.i18n.text("overview.status"),
                self.i18n.text("java.major"),
                self.i18n.text("java.version"),
                self.i18n.text("java.path"),
            ],
            &rows,
            Some(self.java_selected.min(rows.len().saturating_sub(1))),
            &[
                Constraint::Length(14),
                Constraint::Length(9),
                Constraint::Length(14),
                Constraint::Min(10),
            ],
            t,
            offset,
            Some((
                self.java_selected.min(rows.len().saturating_sub(1)) + 1,
                rows.len(),
            )),
        );
        // `render_rows` draws a border, one header row and then the window; the
        // shared table layout describes exactly those rows.
        self.hitmap.rows(
            RowLayout::table(
                area,
                rows.len(),
                self.java_selected.min(rows.len().saturating_sub(1)),
            ),
            RowKind::Java,
        );
    }

    fn directory_rows(&self, frame: &mut Frame<'_>, area: Rect, t: f64) {
        let pal = self.creeper_palette();
        let mut rows = vec![
            RowData::new([
                format!("＋ {}", self.i18n.text("settings.add_game_directory")),
                String::new(),
                String::new(),
            ])
            .mark(0, pal.green),
            RowData::new([
                format!("⇣ {}", self.i18n.text("settings.import_game_directory")),
                String::new(),
                String::new(),
            ])
            .mark(0, pal.cyan),
        ];
        for (index, path) in self.config.game_directories.iter().enumerate() {
            let count = self.game_directory_instance_count(path);
            let current = index == 0;
            rows.push(
                RowData::new([
                    display_path(path),
                    self.i18n.format(
                        "settings.directory_instances",
                        &[("count", &count.to_string())],
                    ),
                    if current {
                        format!("● {}", self.i18n.text("settings.current_directory"))
                    } else {
                        String::new()
                    },
                ])
                .mark(2, if current { pal.green } else { pal.text_faint }),
            );
        }
        let inner = self.settings_inner(area);
        let body_height = inner.height.saturating_sub(1);
        let offset = self.management_offset(rows.len(), body_height);
        widgets::render_rows(
            frame,
            area,
            &self.i18n.text("settings.category.game_directories"),
            pal.cyan,
            &[
                self.i18n.text("settings.directory"),
                self.i18n.text("settings.instances"),
                self.i18n.text("settings.state"),
            ],
            &rows,
            Some(self.settings.selected.min(rows.len().saturating_sub(1))),
            &[
                Constraint::Min(18),
                Constraint::Length(10),
                Constraint::Length(12),
            ],
            t,
            offset,
            Some((
                self.settings.selected.min(rows.len().saturating_sub(1)) + 1,
                rows.len(),
            )),
        );
        self.hitmap.rows(
            RowLayout::table(
                area,
                rows.len(),
                self.settings.selected.min(rows.len().saturating_sub(1)),
            ),
            RowKind::Directories,
        );
    }
}

/// Visible window for a one-row list: keeps the selection on screen.
pub(in crate::tui::app) fn settings_window(
    count: usize,
    selected: usize,
    height: u16,
) -> (usize, usize) {
    let visible = usize::from(height.max(1)).min(count.max(1));
    if count == 0 {
        return (0, visible);
    }
    let offset = selected
        .min(count - 1)
        .saturating_sub(visible.saturating_sub(1))
        .min(count.saturating_sub(visible));
    (offset, visible)
}

fn management_offset(count: usize, selected: usize, height: u16) -> usize {
    settings_window(count, selected, height).0
}

fn settings_search_list_rect(inner: Rect) -> Rect {
    let description = if inner.height >= 10 {
        DESCRIPTION_ROWS.min(inner.height.saturating_sub(2))
    } else {
        0
    };
    Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(1 + description),
    )
}
