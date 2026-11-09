//! `paper` settings: a chapter directory beside a list of entries.
//!
//! Values sit on the right edge of the column, adjustable ones between `‹ ›`.
//! The category rail is a directory, not tabs: no boxes, no counts, only the
//! current chapter underlined.

use super::*;
use crate::tui::hit::RowKind;
use crate::tui::palette;
use crate::tui::widgets::rows::{RowLayout, RowWindow};
use crate::tui::widgets::settings::SettingKind;

const MARGIN: u16 = 3;
/// Directory width on a wide terminal; a narrower one gets a thinner rail.
fn rail_width(area: Rect) -> u16 {
    if area.width >= 100 { 18 } else { 14 }
}

impl App {
    pub(in crate::tui::app) fn draw_paper_settings(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
    ) {
        if area.width < 24 || area.height < 4 {
            return;
        }
        if self.settings_search_active() {
            self.paper_settings_search(frame, area, pal);
            return;
        }
        if self.is_narrow() {
            // One folded line stands in for the rail: the categories stay
            // reachable by mouse without a column of their own.
            let strip = Rect::new(area.x, area.y, area.width, 1);
            self.paper_settings_strip(frame, strip, pal);
            kit::rule(
                frame,
                Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
                pal,
                pal.rule,
            );
            let body = Rect::new(
                area.x,
                area.y.saturating_add(3),
                area.width,
                area.height.saturating_sub(3),
            );
            self.paper_settings_content(frame, body, pal);
            return;
        }
        let rail = Rect::new(
            area.x,
            area.y,
            rail_width(area).min(area.width.saturating_sub(12)),
            area.height,
        );
        self.paper_settings_rail(frame, rail, pal);
        // A column rule, the way a printed page separates a margin index from
        // the body text.
        let rule_x = rail.right().saturating_add(1);
        for y in rail.y..rail.bottom() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled("│", ink(pal, pal.rule)))),
                Rect::new(rule_x, y, 1, 1),
            );
        }
        let body = Rect::new(
            rule_x.saturating_add(2),
            area.y,
            area.right().saturating_sub(rule_x.saturating_add(2)),
            area.height,
        );
        self.paper_settings_content(frame, body, pal);
    }

    fn paper_settings_rail(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        for (index, category) in SettingsCategory::ALL.iter().enumerate() {
            let y = area.y.saturating_add(index as u16);
            if y >= area.bottom() {
                break;
            }
            let active = *category == self.settings.category;
            let style = if active {
                ink(pal, pal.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                ink(pal, pal.dim)
            };
            let rect = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    self.i18n.text(category.title_key()),
                    style,
                ))),
                rect,
            );
            self.hitmap.add(rect, Action::category(index));
        }
    }

    fn paper_settings_strip(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        let entries = SettingsCategory::ALL
            .iter()
            .enumerate()
            .map(|(index, category)| {
                let label = self.i18n.text(category.title_key());
                let width = palette::width(&label) as u16;
                (index, *category, label, width)
            })
            .collect::<Vec<_>>();
        let required = entries
            .iter()
            .map(|(_, _, _, width)| *width)
            .sum::<u16>()
            .saturating_add(u16::try_from(entries.len().saturating_sub(1)).unwrap_or(0));
        // Seven names do not fit every terminal. Start at the current one rather
        // than showing the first four and hiding the category the reader picked.
        let start = if required > area.width {
            let current = entries
                .iter()
                .position(|(_, category, _, _)| *category == self.settings.category)
                .unwrap_or(0);
            // Take as many earlier names as still fit, so the row reads as a
            // run of categories ending at the current one instead of a lone
            // word in the corner.
            let mut start = current;
            while start > 0 {
                let candidate = start - 1;
                let width = entries[candidate..]
                    .iter()
                    .map(|(_, _, _, width)| *width)
                    .sum::<u16>()
                    .saturating_add(u16::try_from(entries.len() - candidate - 1).unwrap_or(0));
                if width > area.width {
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
            // The separator belongs to the label that follows it: drawing it
            // first leaves a dangling `·` when the next name does not fit.
            if index > start {
                // Separator plus label: the pair fits only if the label's last
                // cell stays inside the row.
                if x.saturating_add(1).saturating_add(width) > area.right() {
                    break;
                }
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled("·", ink(pal, pal.faint)))),
                    Rect::new(x, area.y, 1, 1),
                );
                x = x.saturating_add(1);
            }
            if x.saturating_add(width) > area.right() {
                break;
            }
            let active = category == self.settings.category;
            let style = if active {
                ink(pal, pal.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                ink(pal, pal.dim)
            };
            let rect = Rect::new(x, area.y, width, 1);
            frame.render_widget(Paragraph::new(Line::from(Span::styled(label, style))), rect);
            self.hitmap.add(rect, Action::category(index));
            x = x.saturating_add(width);
        }
    }

    fn paper_settings_content(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        match self.settings.category {
            SettingsCategory::Java => self.paper_java_rows(frame, area, pal),
            SettingsCategory::GameDirectories => self.paper_directory_rows(frame, area, pal),
            _ => self.paper_setting_rows(frame, area, pal),
        }
    }

    fn paper_setting_rows(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        let rows = self.settings_rows();
        let title = self.i18n.text(self.settings.category.title_key());
        let selected = self.settings.selected.min(rows.len().saturating_sub(1));
        let counter = if rows.is_empty() {
            None
        } else {
            Some(format!("{}/{}", selected.saturating_add(1), rows.len()))
        };
        let content = kit::section(frame, area, pal, &title, counter.as_deref());
        let layout = self.row_windows.resolve(
            RowWindow::Settings,
            RowLayout::list_bare(content, rows.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::Settings);
        if rows.is_empty() {
            self.paper_message_at(frame, content, pal, "settings.empty");
            return;
        }
        for (index, rect) in layout.rows() {
            let Some(row) = rows.get(index) else {
                continue;
            };
            self.paper_setting_row(frame, rect, pal, row, index == selected, index);
        }
    }

    fn paper_setting_row(
        &self,
        frame: &mut Frame<'_>,
        row: Rect,
        pal: &PaperPalette,
        setting: &crate::tui::widgets::settings::SettingRow,
        active: bool,
        index: usize,
    ) {
        if active {
            kit::selection(frame, row, pal);
        }
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
        let full_width = {
            let text = palette::width(&value) as u16;
            if adjustable {
                text.saturating_add(4)
            } else {
                text
            }
        };
        let label_x = row.x.saturating_add(MARGIN);
        // The label keeps at least this much room; a long value is what gives
        // way, not the name of the setting.
        const MIN_LABEL: u16 = 12;
        // A value also stops at 60% of the row. Without the cap a log directory
        // ate the label down to `Log directo…` on an 80-column terminal, even
        // though the path was being truncated anyway.
        let wanted = full_width.min(row.width.saturating_mul(3) / 5);
        let block_x = row
            .right()
            .saturating_sub(wanted)
            .max(label_x.saturating_add(MIN_LABEL).saturating_add(2));
        // A long value (a log directory, a JVM argument) must be cut to the
        // row: `block_x` gets clamped to keep the label readable, and without
        // this clamp the value would then run past the right edge and panic in
        // the buffer.
        let block_width = wanted.min(row.right().saturating_sub(block_x));
        let value_width = if adjustable {
            block_width.saturating_sub(4)
        } else {
            block_width
        };
        let label_width = block_x.saturating_sub(label_x).saturating_sub(2);

        let label_style = if !setting.enabled {
            ink(pal, pal.faint)
        } else {
            ink(pal, pal.dim)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(&setting.title, usize::from(label_width)),
                label_style,
            ))),
            Rect::new(label_x, row.y, label_width, 1),
        );

        let value_style = if !setting.enabled {
            ink(pal, pal.faint)
        } else if active {
            ink_marked(pal)
        } else if setting.kind == SettingKind::Action {
            ink(pal, pal.accent)
        } else {
            ink(pal, pal.ink)
        };
        if !adjustable {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&value, usize::from(block_width)),
                    value_style,
                )))
                .alignment(Alignment::Right),
                Rect::new(block_x, row.y, block_width, 1),
            );
            return;
        }

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("‹", ink(pal, pal.accent)))),
            Rect::new(block_x, row.y, 1, 1),
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(&value, usize::from(value_width)),
                value_style,
            ))),
            Rect::new(block_x.saturating_add(2), row.y, value_width, 1),
        );
        let close_x = block_x.saturating_add(3).saturating_add(value_width);
        if close_x < row.right() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled("›", ink(pal, pal.accent)))),
                Rect::new(close_x, row.y, 1, 1),
            );
        }
        self.hitmap
            .add(Rect::new(block_x, row.y, 1, 1), Action::step(index, -1));
        if close_x < row.right() {
            self.hitmap
                .add(Rect::new(close_x, row.y, 1, 1), Action::step(index, 1));
        }
    }

    fn paper_java_rows(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        let title = self.i18n.text("java.runtime");
        let counter = self.java.len().to_string();
        let content = kit::section(frame, area, pal, &title, Some(&counter));
        let selected = self.java_selected.min(self.java.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Java,
            RowLayout::list_bare(content, self.java.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::Java);
        if self.java.is_empty() {
            self.paper_message_at(frame, content, pal, "java.empty");
            return;
        }
        // The rects span the list, so each row re-anchors them to its own y.
        let columns = kit::columns(content, &[10, 16], 2);
        let (status_x, version_x) = (columns[0].x, columns[1].x);
        let (status_width, version_width) = (columns[0].width, columns[1].width);
        let path_width = version_x.saturating_sub(content.x.saturating_add(2));
        for (index, rect) in layout.rows() {
            let Some(java) = self.java.get(index) else {
                continue;
            };
            let active = index == selected;
            if active {
                kit::selection(frame, rect, pal);
            }
            let current = self.config.java.selected_path.as_ref() == Some(&java.executable);
            let (word, colour) = if current {
                (self.i18n.text("java.selected"), pal.ok)
            } else if java.is_jdk {
                (self.i18n.text("java.available"), pal.ok)
            } else {
                (self.i18n.text("java.available"), pal.warn)
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&word, usize::from(status_width)),
                    ink(pal, colour),
                ))),
                Rect::new(status_x, rect.y, status_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&format!("Java {}", java.version), 16),
                    ink(pal, if active { pal.soft } else { pal.faint }),
                ))),
                Rect::new(version_x, rect.y, version_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&display_path(&java.executable), usize::from(path_width)),
                    ink(pal, if active { pal.soft } else { pal.faint }),
                ))),
                Rect::new(content.x.saturating_add(2), rect.y, path_width, 1),
            );
        }
    }

    fn paper_directory_rows(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        let title = self.i18n.text("settings.category.game_directories");
        let count = (2 + self.config.game_directories.len()).to_string();
        let content = kit::section(frame, area, pal, &title, Some(&count));
        let total = 2 + self.config.game_directories.len();
        let selected = self.settings.selected.min(total.saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Directories,
            RowLayout::list_bare(content, total, selected),
        );
        self.hitmap.rows(layout, RowKind::Directories);
        // Same as the Java list: list-tall rects, one row at a time.
        let columns = kit::columns(content, &[10, 12], 2);
        let (current_x, instances_x) = (columns[0].x, columns[1].x);
        let (current_width, instances_width) = (columns[0].width, columns[1].width);
        let path_width = instances_x.saturating_sub(content.x.saturating_add(2));
        for (index, rect) in layout.rows() {
            let active = index == selected;
            if active {
                kit::selection(frame, rect, pal);
            }
            if index < 2 {
                let (mark, key) = if index == 0 {
                    ("＋", "settings.add_game_directory")
                } else {
                    ("⇣", "settings.import_game_directory")
                };
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!("{mark} {}", self.i18n.text(key)),
                        if active {
                            ink_marked(pal)
                        } else {
                            ink(pal, pal.ink)
                        },
                    ))),
                    Rect::new(content.x.saturating_add(2), rect.y, content.width, 1),
                );
                continue;
            }
            let Some(path) = self.config.game_directories.get(index - 2) else {
                continue;
            };
            let current_dir = self.config.game_directories.first().is_some_and(|first| {
                normalize_instance_path(first) == normalize_instance_path(path)
            });
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&display_path(path), usize::from(path_width)),
                    if active {
                        ink_marked(pal)
                    } else {
                        ink(pal, pal.ink)
                    },
                ))),
                Rect::new(content.x.saturating_add(2), rect.y, path_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    self.i18n.format(
                        "settings.directory_instances",
                        &[(
                            "count",
                            &self.game_directory_instance_count(path).to_string(),
                        )],
                    ),
                    ink(pal, pal.faint),
                ))),
                Rect::new(instances_x, rect.y, instances_width, 1),
            );
            if current_dir {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        self.i18n.text("settings.current_directory"),
                        ink(pal, pal.accent),
                    ))),
                    Rect::new(current_x, rect.y, current_width, 1),
                );
            }
        }
    }

    fn paper_settings_search(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        let hits = self.settings_search_results();
        let selected = self.settings.selected.min(hits.len().saturating_sub(1));
        let title = self.i18n.text("settings.search_title");
        let counter = self.i18n.format(
            "settings.search_results",
            &[("count", &hits.len().to_string())],
        );
        let content = kit::section(frame, area, pal, &title, Some(&counter));
        let field = Rect::new(content.x, content.y, content.width, 1);
        self.paper_search_field(frame, field, pal);
        let list = Rect::new(
            content.x,
            content.y.saturating_add(2),
            content.width,
            content.height.saturating_sub(2),
        );
        if list.height == 0 {
            return;
        }
        let layout = self.row_windows.resolve(
            RowWindow::Settings,
            RowLayout::list_bare(list, hits.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::Settings);
        if hits.is_empty() {
            self.paper_message_at(frame, list, pal, "settings.empty");
            return;
        }
        for (index, rect) in layout.rows() {
            let Some(hit) = hits.get(index) else {
                continue;
            };
            let active = index == selected;
            if active {
                kit::selection(frame, rect, pal);
            }
            let label = format!(
                "{} · {}",
                self.i18n.text(hit.category.title_key()),
                hit.row.title
            );
            let value_width = palette::width(&hit.row.value) as u16 + 2;
            let label_width = rect.width.saturating_sub(MARGIN + value_width);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&label, usize::from(label_width)),
                    if active {
                        ink_marked(pal)
                    } else {
                        ink(pal, pal.soft)
                    },
                ))),
                Rect::new(rect.x.saturating_add(MARGIN), rect.y, label_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&hit.row.value, usize::from(value_width)),
                    ink(pal, pal.faint),
                )))
                .alignment(Alignment::Right),
                rect,
            );
        }
    }

    fn paper_search_field(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        let focused = matches!(
            self.search.as_ref(),
            Some(state) if state.target == crate::tui::input::SearchTarget::Settings
        );
        let query = self.settings_search.as_deref().unwrap_or_default();
        let label = self.i18n.text("settings.search_placeholder");
        let style = if focused {
            ink(pal, pal.ink).add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
        } else if query.is_empty() {
            ink(pal, pal.faint)
        } else {
            ink(pal, pal.soft)
        };
        let shown = if query.is_empty() {
            palette::truncate(&label, usize::from(area.width.saturating_sub(4)))
        } else {
            palette::truncate(query, usize::from(area.width.saturating_sub(4)))
        };
        let mut spans = vec![Span::styled("⌕ ", ink(pal, pal.dim))];
        spans.push(Span::styled(shown, style));
        if focused {
            spans.push(Span::styled("▏", ink(pal, pal.accent)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        self.hitmap.add(
            area,
            Action::search(crate::tui::input::SearchTarget::Settings),
        );
    }
}
